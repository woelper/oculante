#![windows_subsystem = "windows"]

use clap::Arg;
use clap::Command;
// use fluent_uri::Uri;
use log::debug;
use log::error;
use log::info;
use log::trace;
use log::warn;
use nalgebra::Vector2;
use notan::app::Event;
use notan::draw::*;
use notan::egui::{self, *};
use notan::prelude::*;
use shortcuts::key_pressed;
use shortcuts::MouseWheelEvent;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
pub mod cache;
pub mod scrubber;
pub mod settings;
pub mod shortcuts;
#[cfg(feature = "turbo")]
use crate::image_editing::lossless_tx;
use crate::scrubber::find_first_image_in_directory;
use crate::settings::set_system_theme;
use crate::settings::ColorTheme;
use crate::shortcuts::InputEvent::*;
mod utils;
use utils::*;
mod appstate;
mod image_loader;
use appstate::*;
#[cfg(not(feature = "file_open"))]
mod filebrowser;

pub mod ktx2_loader;
// mod events;
#[cfg(target_os = "macos")]
mod mac;
mod net;
use net::*;
#[cfg(test)]
mod tests;
mod ui;
#[cfg(feature = "update")]
mod update;
use ui::*;

use crate::image_editing::EditState;

mod image_editing;
pub mod paint;

pub const FONT: &[u8; 309828] = include_bytes!("../res/fonts/Inter-Regular.ttf");

#[notan_main]
fn main() -> Result<(), String> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    // on debug builds, override log level
    #[cfg(debug_assertions)]
    {
        std::env::set_var("RUST_LOG", "debug");
    }
    let _ = env_logger::try_init();

    let icon_data = include_bytes!("../icon.ico");

    let mut window_config = WindowConfig::new()
        .set_title(&format!("Oculante | {}", env!("CARGO_PKG_VERSION")))
        .set_size(1026, 600) // window's size
        .set_resizable(true) // window can be resized
        .set_window_icon_data(Some(icon_data))
        .set_taskbar_icon_data(Some(icon_data))
        .set_multisampling(0)
        .set_app_id("oculante");

    #[cfg(target_os = "windows")]
    {
        window_config = window_config
            .set_lazy_loop(true) // don't redraw every frame on windows
            .set_vsync(true)
            .set_high_dpi(true);
    }

    #[cfg(target_os = "linux")]
    {
        window_config = window_config
            .set_lazy_loop(true)
            .set_vsync(true)
            .set_high_dpi(true);
    }

    #[cfg(target_os = "netbsd")]
    {
        window_config = window_config.set_lazy_loop(true).set_vsync(true);
    }

    #[cfg(target_os = "macos")]
    {
        window_config = window_config
            .set_lazy_loop(true)
            .set_vsync(true)
            .set_high_dpi(true);
    }

    #[cfg(target_os = "macos")]
    {
        // MacOS needs an incredible dance performed just to open a file
        let _ = mac::launch();
    }

    // Unfortunately we need to load the persistent settings here, too - the window settings need
    // to be set before window creation
    match settings::PersistentSettings::load() {
        Ok(settings) => {
            window_config.vsync = settings.vsync;
            window_config.lazy_loop = !settings.force_redraw;
            window_config.decorations = !settings.borderless;
            if settings.window_geometry != Default::default() {
                window_config.width = settings.window_geometry.1 .0 as u32;
                window_config.height = settings.window_geometry.1 .1 as u32;
            }
            debug!("Loaded settings.");
            if settings.zen_mode {
                let mut title_string = window_config.title.clone();
                title_string.push_str(&format!(
                    "          '{}' to disable zen mode",
                    shortcuts::lookup(&settings.shortcuts, &shortcuts::InputEvent::ZenMode)
                ));
                window_config = window_config.set_title(&title_string);
            }
        }
        Err(e) => {
            error!("Could not load settings: {e}");
        }
    }
    window_config.always_on_top = true;
    window_config.min_size = Some((1, 1));
    window_config.max_size = None;

    debug!("Starting oculante.");
    notan::init_with(init)
        .add_config(window_config)
        .add_config(EguiConfig)
        .add_config(DrawConfig)
        .event(event)
        .update(update)
        .draw(drawe)
        .build()
}

fn init(app: &mut App, gfx: &mut Graphics, plugins: &mut Plugins) -> OculanteState {
    debug!("Now matching arguments {:?}", std::env::args());
    // Filter out strange mac args
    let args: Vec<String> = std::env::args().filter(|a| !a.contains("psn_")).collect();

    let matches = Command::new("Oculante")
        .arg(Arg::new("INPUT").help("Display this image").index(1))
        .arg(
            Arg::new("l")
                .short('l')
                .help("Listen on port")
                .takes_value(true),
        )
        .arg(
            Arg::new("stdin")
                .short('s')
                .id("stdin")
                .takes_value(false)
                .help("Load data from STDIN"),
        )
        .arg(
            Arg::new("chainload")
                .required(false)
                .takes_value(false)
                .short('c')
                .help("Chainload on Mac"),
        )
        .get_matches_from(args);

    debug!("Completed argument parsing.");

    // #[cfg(target_os = "windows")]
    let maybe_img_location = matches.value_of("INPUT").map(PathBuf::from);

    // #[cfg(not(target_os = "windows"))]
    // let maybe_img_location = matches
    //     .value_of("INPUT")
    //     .map(Uri::parse)
    //     .and_then(|uri_result| {
    //         uri_result
    //             .ok()
    //             .map(|uri| PathBuf::from(uri.path().as_str()))
    //     });

    let mut state = OculanteState {
        texture_channel: mpsc::channel(),
        // current_path: maybe_img_location.cloned(/),
        ..Default::default()
    };

    match settings::PersistentSettings::load() {
        Ok(settings) => {
            state.persistent_settings = settings;
            info!("Successfully loaded previous settings.")
        }
        Err(e) => {
            warn!("Settings failed to load: {e}. This may happen after application updates. Generating a fresh file.");
            state.persistent_settings = Default::default();
            state.persistent_settings.save_blocking();
        }
    }

    state.player = Player::new(
        state.texture_channel.0.clone(),
        state.persistent_settings.max_cache,
        gfx.limits().max_texture_size,
    );

    debug!("Image is: {:?}", maybe_img_location);

    if let Some(ref location) = maybe_img_location {
        // Check if path is a directory or a file (and that it even exists)
        let mut start_img_location: Option<PathBuf> = None;

        if let Ok(maybe_location_metadata) = location.metadata() {
            if maybe_location_metadata.is_dir() {
                // Folder - Pick first image from the folder...
                if let Ok(first_img_location) = find_first_image_in_directory(location) {
                    start_img_location = Some(first_img_location);
                }
            } else if is_ext_compatible(location) {
                // Image File with a usable extension
                start_img_location = Some(location.clone());
            } else {
                // Unsupported extension
                state.send_message_err(&format!("ERROR: Unsupported file: {} - Open Github issue if you think this should not happen.", location.display()));
            }
        } else {
            // Not a valid path, or user doesn't have permission to access?
            state.send_message_err(&format!("ERROR: Can't open file: {}", location.display()));
        }

        // Assign image path if we have a valid one here
        if let Some(img_location) = start_img_location {
            state.is_loaded = false;
            state.current_path = Some(img_location.clone());
            state
                .player
                .load(&img_location, state.message_channel.0.clone());
        }
    }

    if matches.contains_id("stdin") {
        debug!("Trying to read from pipe");
        let mut input = vec![];
        if let Ok(bytes_read) = std::io::stdin().read_to_end(&mut input) {
            if bytes_read > 0 {
                debug!("There was stdin");

                match image::load_from_memory(input.as_ref()) {
                    Ok(i) => {
                        // println!("got image");
                        debug!("Sending image!");
                        let _ = state
                            .texture_channel
                            .0
                            .clone()
                            .send(utils::Frame::new_reset(i.to_rgba8()));
                    }
                    Err(e) => error!("ERR loading from stdin: {e} - for now, oculante only supports data that can be decoded by the image crate."),
                }
            }
        }
    }

    if let Some(port) = matches.value_of("l") {
        match port.parse::<i32>() {
            Ok(p) => {
                state.send_message_info(&format!("Listening on {p}"));
                recv(p, state.texture_channel.0.clone());
                state.current_path = Some(PathBuf::from(&format!("network port {p}")));
                state.network_mode = true;
            }
            Err(_) => error!("Port must be a number"),
        }
    }

    // Set up egui style
    plugins.egui(|ctx| {
        // FIXME: Wait for https://github.com/Nazariglez/notan/issues/315 to close, then remove
        ctx.set_pixels_per_point(app.window().dpi() as f32);
        let mut fonts = FontDefinitions::default();

        fonts
            .font_data
            .insert("my_font".to_owned(), FontData::from_static(FONT));

        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "my_font".to_owned());

        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        match state.persistent_settings.theme {
            ColorTheme::Light => ctx.set_visuals(Visuals::light()),
            ColorTheme::Dark => ctx.set_visuals(Visuals::dark()),
            ColorTheme::System => set_system_theme(ctx),
        }

        let mut style: egui::Style = (*ctx.style()).clone();
        let font_scale = 0.80;

        style.text_styles.get_mut(&TextStyle::Body).unwrap().size = 18. * font_scale;
        style.text_styles.get_mut(&TextStyle::Button).unwrap().size = 18. * font_scale;
        style.text_styles.get_mut(&TextStyle::Small).unwrap().size = 15. * font_scale;
        style.text_styles.get_mut(&TextStyle::Heading).unwrap().size = 22. * font_scale;
        style.visuals.selection.bg_fill = Color32::from_rgb(
            state.persistent_settings.accent_color[0],
            state.persistent_settings.accent_color[1],
            state.persistent_settings.accent_color[2],
        );

        let accent_color = style.visuals.selection.bg_fill.to_array();

        let accent_color_luma = (accent_color[0] as f32 * 0.299
            + accent_color[1] as f32 * 0.587
            + accent_color[2] as f32 * 0.114)
            .max(0.)
            .min(255.) as u8;
        let accent_color_luma = if accent_color_luma < 80 { 220 } else { 80 };
        // Set text on highlighted elements
        style.visuals.selection.stroke = Stroke::new(2.0, Color32::from_gray(accent_color_luma));
        ctx.set_fonts(fonts);

        ctx.set_style(style);
    });

    // load checker texture
    if let Ok(checker_image) = image::load_from_memory(include_bytes!("../res/checker.png")) {
        // state.checker_texture = checker_image.into_rgba8().to_texture(gfx);
        // No mipmaps for the checker pattern!
        let img = checker_image.into_rgba8();
        state.checker_texture = gfx
            .create_texture()
            .from_bytes(&img, img.width(), img.height())
            .with_mipmaps(false)
            .with_format(notan::prelude::TextureFormat::SRgba8)
            .build()
            .ok();
    }

    // force a frame to render so ctx() has a size (important for centering the image)
    gfx.render(&plugins.egui(|_| {}));

    state
}

fn event(app: &mut App, state: &mut OculanteState, evt: Event) {
    match evt {
        Event::KeyUp { .. } => {
            // Fullscreen needs to be on key up on mac (bug)
            if key_pressed(app, state, Fullscreen) {
                toggle_fullscreen(app, state);
            }
        }
        Event::KeyDown { .. } => {
            debug!("key down");

            // return;
            // pan image with keyboard
            let delta = 40.;
            if key_pressed(app, state, PanRight) {
                state.image_geometry.offset.x += delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, PanUp) {
                state.image_geometry.offset.y -= delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, PanLeft) {
                state.image_geometry.offset.x -= delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, PanDown) {
                state.image_geometry.offset.y += delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, CompareNext) {
                compare_next(state);
            }
            if key_pressed(app, state, ResetView) {
                state.reset_image = true
            }
            if key_pressed(app, state, ZenMode) {
                toggle_zen_mode(state, app);
            }
            if key_pressed(app, state, ZoomActualSize) {
                set_zoom(1.0, None, state);
            }
            if key_pressed(app, state, ZoomDouble) {
                set_zoom(2.0, None, state);
            }
            if key_pressed(app, state, ZoomThree) {
                set_zoom(3.0, None, state);
            }
            if key_pressed(app, state, ZoomFour) {
                set_zoom(4.0, None, state);
            }
            if key_pressed(app, state, ZoomFive) {
                set_zoom(5.0, None, state);
            }
            if key_pressed(app, state, Copy) {
                if let Some(img) = &state.current_image {
                    clipboard_copy(img);
                    state.send_message_info("Image copied");
                }
            }

            if key_pressed(app, state, Paste) {
                match clipboard_to_image() {
                    Ok(img) => {
                        state.current_path = None;
                        // Stop in the even that an animation is running
                        state.player.stop();
                        _ = state
                            .player
                            .image_sender
                            .send(crate::utils::Frame::new_still(img));
                        // Since pasted data has no path, make sure it's not set
                        state.send_message_info("Image pasted");
                    }
                    Err(e) => state.send_message_err(&e.to_string()),
                }
            }
            if key_pressed(app, state, Quit) {
                state.persistent_settings.save_blocking();
                app.backend.exit();
            }
            #[cfg(feature = "turbo")]
            if key_pressed(app, state, LosslessRotateRight) {
                debug!("Lossless rotate right");

                if let Some(p) = &state.current_path {
                    if lossless_tx(
                        p,
                        turbojpeg::Transform {
                            op: turbojpeg::TransformOp::Rot90,
                            ..turbojpeg::Transform::default()
                        },
                    )
                    .is_ok()
                    {
                        state.is_loaded = false;
                        // This needs "deep" reload
                        state.player.cache.clear();
                        state.player.load(p, state.message_channel.0.clone());
                    }
                }
            }
            #[cfg(feature = "turbo")]
            if key_pressed(app, state, LosslessRotateLeft) {
                debug!("Lossless rotate left");
                if let Some(p) = &state.current_path {
                    if lossless_tx(
                        p,
                        turbojpeg::Transform {
                            op: turbojpeg::TransformOp::Rot270,
                            ..turbojpeg::Transform::default()
                        },
                    )
                    .is_ok()
                    {
                        state.is_loaded = false;
                        // This needs "deep" reload
                        state.player.cache.clear();
                        state.player.load(p, state.message_channel.0.clone());
                    } else {
                        warn!("rotate left failed")
                    }
                }
            }
            if key_pressed(app, state, Browse) {
                state.redraw = true;
                #[cfg(feature = "file_open")]
                browse_for_image_path(state);
                #[cfg(not(feature = "file_open"))]
                {
                    state.filebrowser_id = Some("OPEN_SHORTCUT".into());
                }
            }

            if key_pressed(app, state, NextImage) {
                if state.is_loaded {
                    next_image(state)
                }
            }
            if key_pressed(app, state, PreviousImage) {
                if state.is_loaded {
                    prev_image(state)
                }
            }
            if key_pressed(app, state, FirstImage) {
                first_image(state)
            }
            if key_pressed(app, state, LastImage) {
                last_image(state)
            }
            if key_pressed(app, state, AlwaysOnTop) {
                state.always_on_top = !state.always_on_top;
                app.window().set_always_on_top(state.always_on_top);
            }
            if key_pressed(app, state, InfoMode) {
                state.persistent_settings.info_enabled = !state.persistent_settings.info_enabled;
                send_extended_info(
                    &state.current_image,
                    &state.current_path,
                    &state.extended_info_channel,
                );
            }
            if key_pressed(app, state, EditMode) {
                state.persistent_settings.edit_enabled = !state.persistent_settings.edit_enabled;
            }
            #[cfg(not(target_os = "netbsd"))]
            if key_pressed(app, state, DeleteFile) {
                if let Some(p) = &state.current_path {
                    _ = trash::delete(p);
                    state.send_message_info("Deleted image");
                }
            }
            if key_pressed(app, state, ZoomIn) {
                let delta = zoomratio(3.5, state.image_geometry.scale);
                let new_scale = state.image_geometry.scale + delta;
                // limit scale
                if new_scale > 0.05 && new_scale < 40. {
                    // We want to zoom towards the center
                    let center: Vector2<f32> = nalgebra::Vector2::new(
                        app.window().width() as f32 / 2.,
                        app.window().height() as f32 / 2.,
                    );
                    state.image_geometry.offset -= scale_pt(
                        state.image_geometry.offset,
                        center,
                        state.image_geometry.scale,
                        delta,
                    );
                    state.image_geometry.scale += delta;
                }
            }
            if key_pressed(app, state, ZoomOut) {
                let delta = zoomratio(-3.5, state.image_geometry.scale);
                let new_scale = state.image_geometry.scale + delta;
                // limit scale
                if new_scale > 0.05 && new_scale < 40. {
                    // We want to zoom towards the center
                    let center: Vector2<f32> = nalgebra::Vector2::new(
                        app.window().width() as f32 / 2.,
                        app.window().height() as f32 / 2.,
                    );
                    state.image_geometry.offset -= scale_pt(
                        state.image_geometry.offset,
                        center,
                        state.image_geometry.scale,
                        delta,
                    );
                    state.image_geometry.scale += delta;
                }
            }
        }
        Event::WindowResize { width, height } => {
            //TODO: remove this if save on exit works
            state.persistent_settings.window_geometry.1 = (width, height);
            state.persistent_settings.window_geometry.0 = (
                app.backend.window().position().0 as u32,
                app.backend.window().position().1 as u32,
            );
            // By resetting the image, we make it fill the window on resize
            if state.persistent_settings.fit_image_on_window_resize {
                state.reset_image = true;
            }
        }
        _ => (),
    }

    match evt {
        Event::Exit => {
            info!("About to exit");
            // save position
            state.persistent_settings.window_geometry = (
                (
                    app.window().position().0 as u32,
                    app.window().position().1 as u32,
                ),
                app.window().size(),
            );
            state.persistent_settings.save_blocking();
        }
        Event::MouseWheel { delta_y, .. } => {
            if !state.pointer_over_ui {
                let wheel_event = MouseWheelEvent::new(delta_y, &app.keyboard);
                let found_action = state.persistent_settings.mouse_wheel_settings.iter().find(
                    |(_action, opt_trigger)| {
                        opt_trigger.is_some_and(|trigger| trigger == wheel_event)
                    },
                );
                if let Some((action, _opt_trigger)) = found_action {
                    action.clone().perform(state, delta_y);
                }
            }
        }

        Event::Drop(file) => {
            if let Some(p) = file.path {
                if let Some(ext) = p.extension() {
                    if SUPPORTED_EXTENSIONS.contains(&ext.to_string_lossy().to_string().as_str()) {
                        state.is_loaded = false;
                        state.current_image = None;
                        state.player.load(&p, state.message_channel.0.clone());
                        state.current_path = Some(p);
                    } else {
                        state.send_message_warn("Unsupported file!");
                    }
                }
            }
        }
        Event::MouseDown { button, .. } => {
            state.drag_enabled = true;
            match button {
                MouseButton::Left => {
                    if !state.mouse_grab {
                        state.drag_enabled = true;
                    }
                }
                MouseButton::Middle => {
                    state.drag_enabled = true;
                }
                _ => {}
            }
        }
        Event::MouseUp { button, .. } => match button {
            MouseButton::Left | MouseButton::Middle => state.drag_enabled = false,
            _ => {}
        },
        _ => {
            // debug!("{:?}", evt);
        }
    }
}

fn update(app: &mut App, state: &mut OculanteState) {
    if state.first_start {
        app.window().set_always_on_top(false);
    }

    // dbg!(format!("upg {}", app.timer.elapsed_f32()));

    if let Some(p) = &state.current_path {
        let t = app.timer.elapsed_f32() % 0.8;
        if t <= 0.05 {
            trace!("chk mod {}", t);
            state
                .player
                .check_modified(p, state.message_channel.0.clone());
        }
    }

    // Save every 1.5 secs
    let t = app.timer.elapsed_f32() % 1.5;
    if t <= 0.01 {
        state.persistent_settings.window_geometry = (
            (
                app.window().position().0 as u32,
                app.window().position().1 as u32,
            ),
            app.window().size(),
        );
        state.persistent_settings.save_blocking();
        trace!("Save {t}");
    }

    let mouse_pos = app.mouse.position();

    state.mouse_delta = Vector2::new(mouse_pos.0, mouse_pos.1) - state.cursor;
    state.cursor = mouse_pos.size_vec();
    if state.drag_enabled {
        if !state.mouse_grab || app.mouse.is_down(MouseButton::Middle) {
            state.image_geometry.offset += state.mouse_delta;
            limit_offset(app, state);
        }
    }

    // Since we can't access the window in the event loop, we store it in the state
    state.window_size = app.window().size().size_vec();

    state.image_geometry.dimensions = state.image_geometry.dimensions;

    if state.persistent_settings.info_enabled || state.edit_state.painting {
        state.cursor_relative = pos_from_coord(
            state.image_geometry.offset,
            state.cursor,
            Vector2::new(
                state.image_geometry.dimensions.0 as f32,
                state.image_geometry.dimensions.1 as f32,
            ),
            state.image_geometry.scale,
        );
    }

    // make sure that in edit mode, RGBA is set.
    // This is a bit lazy. but instead of writing lots of stuff for an ubscure feature,
    // let's disable it here.
    if state.persistent_settings.edit_enabled {
        state.persistent_settings.current_channel = ColorChannel::Rgba;
    }

    // redraw if extended info is missing so we make sure it's promply displayed
    if state.persistent_settings.info_enabled && state.image_info.is_none() {
        app.window().request_frame();
    }

    // check extended info has been sent
    if let Ok(info) = state.extended_info_channel.1.try_recv() {
        debug!("Received extended image info for {}", info.name);
        state.image_info = Some(info);
        app.window().request_frame();
    }

    // check if a new message has been sent
    if let Ok(msg) = state.message_channel.1.try_recv() {
        debug!("Received message: {:?}", msg);
        match msg {
            Message::LoadError(e) => {
                state.toasts.error(e);
                state.current_image = None;
                state.is_loaded = true;
                state.current_texture = None;
            }
            Message::Info(m) => {
                state
                    .toasts
                    .info(m)
                    .set_duration(Some(Duration::from_secs(1)));
            }
            Message::Warning(m) => {
                state.toasts.warning(m);
            }
            Message::Error(m) => {
                state.toasts.error(m);
            }
            Message::Saved(_) => {
                state.toasts.info("Saved");
            }
        }
    }
    state.first_start = false;
}

fn drawe(app: &mut App, gfx: &mut Graphics, plugins: &mut Plugins, state: &mut OculanteState) {
    let mut draw = gfx.create_draw();

    if let Ok(p) = state.load_channel.1.try_recv() {
        state.is_loaded = false;
        state.current_image = None;
        state.player.load(&p, state.message_channel.0.clone());
        if let Some(dir) = p.parent() {
            state.persistent_settings.last_open_directory = dir.to_path_buf();
        }
        state.current_path = Some(p);
    }

    // check if a new texture has been sent
    if let Ok(frame) = state.texture_channel.1.try_recv() {
        let img = frame.buffer;
        debug!("Received image buffer: {:?}", img.dimensions());
        state.image_geometry.dimensions = img.dimensions();
        // state.current_texture = img.to_texture(gfx);

        // debug!("Frame source: {:?}", frame.source);

        set_title(app, state);

        // fill image sequence
        if let Some(p) = &state.current_path {
            state.scrubber = scrubber::Scrubber::new(p);
            state.scrubber.wrap = state.persistent_settings.wrap_folder;

            // debug!("{:#?} from {}", &state.scrubber, p.display());
            if !state.persistent_settings.recent_images.contains(p) {
                state.persistent_settings.recent_images.insert(0, p.clone());
                state.persistent_settings.recent_images.truncate(10);
            }
        }

        match frame.source {
            FrameSource::Still => {
                debug!("Received still");
                state.edit_state.result_image_op = Default::default();
                state.edit_state.result_pixel_op = Default::default();

                if !state.persistent_settings.keep_view {
                    state.reset_image = true;

                    if let Some(p) = state.current_path.clone() {
                        if state.persistent_settings.max_cache != 0 {
                            state.player.cache.insert(&p, img.clone());
                        }
                    }
                }
                // always reset if first image
                if state.current_texture.is_none() {
                    state.reset_image = true;
                }

                if !state.persistent_settings.keep_edits {
                    state.edit_state = Default::default();
                } else {
                    state.edit_state.result_pixel_op = Default::default();
                    state.edit_state.result_image_op = Default::default();
                }

                // Load edit information if any
                if let Some(p) = &state.current_path {
                    if p.with_extension("oculante").is_file() {
                        if let Ok(f) = std::fs::File::open(p.with_extension("oculante")) {
                            if let Ok(edit_state) = serde_json::from_reader::<_, EditState>(f) {
                                state.send_message_info("Edits have been loaded for this image.");
                                state.edit_state = edit_state;
                                state.persistent_settings.edit_enabled = true;
                                state.reset_image = true;
                            }
                        }
                    } else if let Some(parent) = p.parent() {
                        debug!("Looking for {}", parent.join(".oculante").display());
                        if parent.join(".oculante").is_file() {
                            debug!("is file {}", parent.join(".oculante").display());

                            if let Ok(f) = std::fs::File::open(parent.join(".oculante")) {
                                if let Ok(edit_state) = serde_json::from_reader::<_, EditState>(f) {
                                    state.send_message_info(
                                        "Directory edits have been loaded for this image.",
                                    );
                                    state.edit_state = edit_state;
                                    state.persistent_settings.edit_enabled = true;
                                    state.reset_image = true;
                                }
                            }
                        }
                    }
                }
                state.redraw = false;
                state.image_info = None;
            }
            FrameSource::EditResult => {
                // debug!("EditResult");
                // state.edit_state.is_processing = false;
            }
            FrameSource::AnimationStart => {
                state.redraw = true;
                state.reset_image = true
            }
            FrameSource::Animation => {
                state.redraw = true;
            }
            FrameSource::CompareResult => {
                state.redraw = false;
            }
        }

        if let Some(tex) = &mut state.current_texture {
            if tex.width() as u32 == img.width() && tex.height() as u32 == img.height() {
                img.update_texture(gfx, tex);
            } else {
                state.current_texture =
                    img.to_texture(gfx, state.persistent_settings.linear_mag_filter);
            }
        } else {
            debug!("Setting texture");
            state.current_texture =
                img.to_texture(gfx, state.persistent_settings.linear_mag_filter);
        }

        state.is_loaded = true;

        match &state.persistent_settings.current_channel {
            // Unpremultiply the image
            ColorChannel::Rgb => {
                state.current_texture =
                    unpremult(&img).to_texture(gfx, state.persistent_settings.linear_mag_filter)
            }
            // Do nuttin'
            ColorChannel::Rgba => (),
            // Display the channel
            _ => {
                state.current_texture =
                    solo_channel(&img, state.persistent_settings.current_channel as usize)
                        .to_texture(gfx, state.persistent_settings.linear_mag_filter)
            }
        }
        state.current_image = Some(img);
        if state.persistent_settings.info_enabled {
            debug!("Sending extended info");
            send_extended_info(
                &state.current_image,
                &state.current_path,
                &state.extended_info_channel,
            );
        }
    }

    if state.redraw {
        trace!("Force redraw");
        app.window().request_frame();
    }

    // TODO: Do we need/want a "global" checker?
    // if state.persistent_settings.show_checker_background {
    //     if let Some(checker) = &state.checker_texture {
    //         draw.pattern(checker)
    //             .blend_mode(BlendMode::ADD)
    //             .size(app.window().width() as f32, app.window().height() as f32);
    //     }
    // }

    let egui_output = plugins.egui(|ctx| {
        // the top menu bar
        // ctx.request_repaint_after(Duration::from_secs(1));
        state.toasts.show(ctx);

        if let Some(id) = state.filebrowser_id.take() {
            ctx.memory_mut(|w| w.open_popup(Id::new(id)));
        }
        #[cfg(not(feature = "file_open"))]
        {
            if ctx.memory(|w| w.is_popup_open(Id::new("OPEN_SHORTCUT"))) {
                filebrowser::browse_modal(
                    false,
                    SUPPORTED_EXTENSIONS,
                    |p| {
                        let _ = state.load_channel.0.clone().send(p.to_path_buf());
                    },
                    ctx,
                );
            }
        }

        if !state.persistent_settings.zen_mode {
            egui::TopBottomPanel::top("menu")
                .min_height(30.)
                .default_height(30.)
                .show(ctx, |ui| {
                    main_menu(ui, state, app, gfx);
                });
        }
        if state.persistent_settings.zen_mode && state.persistent_settings.borderless {
            egui::TopBottomPanel::top("menu_zen")
                .min_height(40.)
                .default_height(40.)
                .show_separator_line(false)
                .frame(egui::containers::Frame::none())
                .show(ctx, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        drag_area(ui, state, app);
                        ui.add_space(15.);
                        draw_hamburger_menu(ui, state, app);
                    });
                });
        }

        if state.persistent_settings.show_scrub_bar {
            egui::TopBottomPanel::bottom("scrubber")
                .max_height(22.)
                .min_height(22.)
                .show(ctx, |ui| {
                    scrubber_ui(state, ui);
                });
        }

        if state.persistent_settings.info_enabled
            && !state.settings_enabled
            && !state.persistent_settings.zen_mode
        {
            info_ui(ctx, state, gfx);
        }

        if state.persistent_settings.edit_enabled
            && !state.settings_enabled
            && !state.persistent_settings.zen_mode
        {
            edit_ui(app, ctx, state, gfx);
        }

        state.pointer_over_ui = ctx.is_pointer_over_area();
        // ("using pointer {}", ctx.is_using_pointer());

        // if there is interaction on the ui (dragging etc)
        // we don't want zoom & pan to work, so we "grab" the pointer
        if ctx.is_using_pointer()
            || state.edit_state.painting
            || ctx.is_pointer_over_area()
            || state.edit_state.block_panning
        {
            state.mouse_grab = true;
        } else {
            state.mouse_grab = false;
        }

        if ctx.wants_keyboard_input() {
            state.key_grab = true;
        } else {
            state.key_grab = false;
        }

        if state.reset_image {
            if let Some(current_image) = &state.current_image {
                let draw_area = ctx.available_rect();
                let window_size = nalgebra::Vector2::new(
                    draw_area.width().min(app.window().width() as f32),
                    draw_area.height().min(app.window().height() as f32),
                );
                let img_size = current_image.size_vec();
                state.image_geometry.scale = (window_size.x / img_size.x)
                    .min(window_size.y / img_size.y)
                    .min(1.0);
                state.image_geometry.offset =
                    window_size / 2.0 - (img_size * state.image_geometry.scale) / 2.0;
                // offset by left UI elements
                state.image_geometry.offset.x += draw_area.left();
                // offset by top UI elements
                state.image_geometry.offset.y += draw_area.top();
                debug!("Image has been reset.");
                state.reset_image = false;
                app.window().request_frame();
            }
        }

        // Settings come last, as they block keyboard grab (for hotkey assigment)
        settings_ui(app, ctx, state, gfx);
    });

    if let Some(texture) = &state.current_texture {
        if state.persistent_settings.show_checker_background {
            if let Some(checker) = &state.checker_texture {
                draw.pattern(checker)
                    // .size(texture.width() as f32, texture.height() as f32)
                    .size(texture.width() as f32 * state.image_geometry.scale * state.tiling as f32, texture.height() as f32 * state.image_geometry.scale* state.tiling as f32)
                    .blend_mode(BlendMode::ADD)
                    .translate(state.image_geometry.offset.x, state.image_geometry.offset.y)
                    // .scale(state.image_geometry.scale, state.image_geometry.scale)
                    ;
            }
        }
        if state.tiling < 2 {
            draw.image(texture)
                .blend_mode(BlendMode::NORMAL)
                .scale(state.image_geometry.scale, state.image_geometry.scale)
                .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);
        } else {
            draw.pattern(texture)
                .scale(state.image_geometry.scale, state.image_geometry.scale)
                .translate(state.image_geometry.offset.x, state.image_geometry.offset.y)
                .size(
                    texture.width() * state.tiling as f32,
                    texture.height() * state.tiling as f32,
                );
        }

        if state.persistent_settings.show_frame {
            draw.rect((0.0, 0.0), texture.size())
                .stroke(1.0)
                .color(Color {
                    r: 0.5,
                    g: 0.5,
                    b: 0.5,
                    a: 0.5,
                })
                .blend_mode(BlendMode::ADD)
                .scale(state.image_geometry.scale, state.image_geometry.scale)
                .translate(state.image_geometry.offset.x, state.image_geometry.offset.y);
        }

        if state.persistent_settings.show_minimap {
            // let offset_x = app.window().size().0 as f32 - state.dimensions.0 as f32;
            let offset_x = 0.0;

            let scale = 200. / app.window().size().0 as f32;
            let show_minimap = state.image_geometry.dimensions.0 as f32
                * state.image_geometry.scale
                > app.window().size().0 as f32;

            if show_minimap {
                draw.image(texture)
                    .blend_mode(BlendMode::NORMAL)
                    .translate(offset_x, 100.)
                    .scale(scale, scale);
            }
        }

        // Draw a brush preview when paint mode is on
        if state.edit_state.painting {
            if let Some(stroke) = state.edit_state.paint_strokes.last() {
                let dim = texture.width().min(texture.height()) / 50.;
                draw.circle(20.)
                    // .translate(state.cursor_relative.x, state.cursor_relative.y)
                    .alpha(0.5)
                    .stroke(1.5)
                    .scale(state.image_geometry.scale, state.image_geometry.scale)
                    .scale(stroke.width * dim, stroke.width * dim)
                    .translate(state.cursor.x, state.cursor.y);

                // For later: Maybe paint the actual brush? Maybe overkill.

                // if let Some(brush) = state.edit_state.brushes.get(stroke.brush_index) {
                //     if let Some(brush_tex) = brush.to_texture(gfx) {
                //         draw.image(&brush_tex)
                //             .blend_mode(BlendMode::NORMAL)
                //             .translate(state.cursor.x, state.cursor.y)
                //             .scale(state.scale, state.scale)
                //             .scale(stroke.width*dim, stroke.width*dim)
                //             // .translate(state.offset.x as f32, state.offset.y as f32)
                //             // .transform(state.cursor_relative)
                //             ;
                //     }
                // }
            }
        }
    }

    if state.network_mode {
        app.window().request_frame();
    }
    // if state.edit_state.is_processing {
    //     app.window().request_frame();
    // }
    let c = state.persistent_settings.background_color;
    // draw.clear(Color:: from_bytes(c[0], c[1], c[2], 255));
    draw.clear(Color::from_rgb(
        c[0] as f32 / 255.,
        c[1] as f32 / 255.,
        c[2] as f32 / 255.,
    ));
    gfx.render(&draw);
    gfx.render(&egui_output);
}

// Show file browser to select image to load
#[cfg(feature = "file_open")]
fn browse_for_image_path(state: &mut OculanteState) {
    let start_directory = state.persistent_settings.last_open_directory.clone();
    let load_sender = state.load_channel.0.clone();
    state.redraw = true;
    std::thread::spawn(move || {
        let uppercase_lowercase_ext = [
            utils::SUPPORTED_EXTENSIONS
                .into_iter()
                .map(|e| e.to_ascii_lowercase())
                .collect::<Vec<_>>(),
            utils::SUPPORTED_EXTENSIONS
                .into_iter()
                .map(|e| e.to_ascii_uppercase())
                .collect::<Vec<_>>(),
        ]
        .concat();
        let file_dialog_result = rfd::FileDialog::new()
            .add_filter("All Supported Image Types", &uppercase_lowercase_ext)
            .add_filter("All File Types", &["*"])
            .set_directory(start_directory)
            .pick_file();
        if let Some(file_path) = file_dialog_result {
            let _ = load_sender.send(file_path);
        }
    });
}

// Make sure offset is restricted to window size so we don't offset to infinity
fn limit_offset(app: &mut App, state: &mut OculanteState) {
    let window_size = app.window().size();
    let scaled_image_size = (
        state.image_geometry.dimensions.0 as f32 * state.image_geometry.scale,
        state.image_geometry.dimensions.1 as f32 * state.image_geometry.scale,
    );
    state.image_geometry.offset.x = state
        .image_geometry
        .offset
        .x
        .min(window_size.0 as f32)
        .max(-scaled_image_size.0);
    state.image_geometry.offset.y = state
        .image_geometry
        .offset
        .y
        .min(window_size.1 as f32)
        .max(-scaled_image_size.1);
}

fn set_zoom(scale: f32, from_center: Option<Vector2<f32>>, state: &mut OculanteState) {
    let delta = scale - state.image_geometry.scale;
    let zoom_point = from_center.unwrap_or(state.cursor);
    state.image_geometry.offset -= scale_pt(
        state.image_geometry.offset,
        zoom_point,
        state.image_geometry.scale,
        delta,
    );
    state.image_geometry.scale = scale;
}
