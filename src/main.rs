#![windows_subsystem = "windows"]

use clap::Arg;
use clap::Command;
use image::GenericImageView;
use image_editing::LegacyEditState;
use log::debug;
use log::error;
use log::info;
use log::trace;
use log::warn;
use nalgebra::Vector2;
use notan::app::Event;
use notan::draw::*;
use notan::egui;
use notan::egui::Align;
use notan::egui::EguiConfig;
use notan::egui::EguiPluginSugar;
use notan::egui::FontData;
use notan::egui::FontDefinitions;
use notan::egui::FontFamily;
use notan::egui::FontTweak;
use notan::egui::Id;
use notan::prelude::*;
use oculante::comparelist::CompareItem;
use std::io::{stdin, IsTerminal, Read};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

#[cfg(feature = "file_open")]
use filebrowser::browse_for_image_path;
use oculante::appstate::*;
use oculante::utils::*;
use oculante::*;
use shortcuts::key_pressed;
use ui::PANEL_WIDTH;
use ui::*;

#[cfg(feature = "turbo")]
use image_editing::lossless_tx;
use image_editing::EditState;
use scrubber::find_first_image_in_directory;
use shortcuts::InputEvent::*;

#[notan_main]
fn main() -> Result<(), String> {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }
    // on debug builds, override log level
    #[cfg(debug_assertions)]
    {
        println!("Debug");
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

    #[cfg(any(target_os = "netbsd", target_os = "freebsd"))]
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
        let _ = oculante::mac::launch();
    }

    // Unfortunately we need to load the volatile settings here, too - the window settings need
    // to be set before window creation
    match settings::VolatileSettings::load() {
        Ok(volatile_settings) => {
            if volatile_settings.window_geometry != Default::default() {
                window_config.width = volatile_settings.window_geometry.1 .0;
                window_config.height = volatile_settings.window_geometry.1 .1;
            }
        }
        Err(e) => error!("Could not load volatile settings: {e}"),
    }

    // Unfortunately we need to load the persistent settings here, too - the window settings need
    // to be set before window creation
    match settings::PersistentSettings::load() {
        Ok(settings) => {
            window_config.vsync = settings.vsync;
            window_config.lazy_loop = !settings.force_redraw;
            window_config.decorations = !settings.borderless;

            trace!("Loaded settings.");
            if settings.zen_mode {
                let mut title_string = window_config.title.clone();
                title_string.push_str(&format!(
                    "          '{}' to disable zen mode",
                    shortcuts::lookup(&settings.shortcuts, &shortcuts::InputEvent::ZenMode)
                ));
                window_config = window_config.set_title(&title_string);
            }
            window_config.min_size = Some(settings.min_window_size);

            // LIBHEIF_SECURITY_LIMITS needs to be set before a libheif context is created
            #[cfg(feature = "heif")]
            settings.decoders.heif.maybe_limits();
        }
        Err(e) => {
            error!("Could not load persistent settings: {e}");
        }
    }
    window_config.always_on_top = true;
    window_config.max_size = None;

    debug!("Starting oculante.");
    notan::init_with(init)
        .add_config(window_config)
        .add_config(EguiConfig)
        .add_config(DrawConfig)
        .event(process_events)
        .update(update)
        .draw(drawe)
        .build()
}

fn init(_app: &mut App, gfx: &mut Graphics, plugins: &mut Plugins) -> OculanteState {
    debug!("Now matching arguments {:?}", std::env::args());
    // Filter out strange mac args
    let args: Vec<String> = std::env::args().filter(|a| !a.contains("psn_")).collect();

    let mut matches = Command::new("Oculante")
        .arg(
            Arg::new("INPUT")
                .help("Display this image")
                .multiple_values(true), // .index(1)
                                        // )
        )
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

    let mut state = OculanteState {
        texture_channel: mpsc::channel(),
        // current_path: maybe_img_location.cloned(/),
        ..Default::default()
    };

    state.player = Player::new(
        state.texture_channel.0.clone(),
        state.persistent_settings.max_cache,
        state.message_channel.0.clone(),
        state.persistent_settings.decoders,
    );

    debug!("matches {:?}", matches);

    let paths_to_open = piped_paths(&matches)
        .map(|iter| iter.collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(
            matches
                .remove_many::<String>("INPUT")
                .unwrap_or_default()
                .map(PathBuf::from),
        )
        .collect::<Vec<_>>();

    debug!("Image is: {:?}", paths_to_open);

    if paths_to_open.len() == 1 {
        let location = paths_to_open
            .into_iter()
            .next()
            .expect("It should be tested already that exactly one argument was passed.");
        if location.is_dir() {
            // Folder - Pick first image from the folder...
            if let Ok(first_img_location) = find_first_image_in_directory(&location) {
                state.is_loaded = false;
                state.player.load(&first_img_location);
                state.current_path = Some(first_img_location);
            }
        } else {
            state.is_loaded = false;
            state.player.load(&location);
            state.current_path = Some(location);
        };
    } else if paths_to_open.len() > 1 {
        let location = paths_to_open
            .first()
            .expect("It should be verified already that exactly one argument was passed.");
        if location.is_dir() {
            // Folder - Pick first image from the folder...
            if let Ok(first_img_location) = find_first_image_in_directory(location) {
                state.is_loaded = false;
                state.current_path = Some(first_img_location.clone());
                state.player.load_advanced(
                    &first_img_location,
                    Some(Frame::ImageCollectionMember(Default::default())),
                );
            }
        } else {
            state.is_loaded = false;
            state.current_path = Some(location.clone());
            state.player.load_advanced(
                location,
                Some(Frame::ImageCollectionMember(Default::default())),
            );
        };

        // If launched with more than one path and none of those paths are directories, it's likely
        // that the user wants to view a fixed set of images rather than traverse into directories.
        // This handles the case where the app is launched with files from different dirs as well e.g.
        // a/1.png b/2.png c/3.png
        state.scrubber.fixed_paths = paths_to_open.iter().all(|path| path.is_file());
        state.scrubber.entries = paths_to_open;
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
                            .send(utils::Frame::new_reset(i));
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
                net::recv(p, state.texture_channel.0.clone());
                state.current_path = Some(PathBuf::from(&format!("network port {p}")));
                state.network_mode = true;
            }
            Err(_) => error!("Port must be a number"),
        }
    }

    // Set up egui style / theme
    plugins.egui(|ctx| {
        // FIXME: Wait for https://github.com/Nazariglez/notan/issues/315 to close, then remove

        let mut fonts = FontDefinitions::default();
        egui_extras::install_image_loaders(ctx);

        ctx.options_mut(|o| o.zoom_with_keyboard = false);

        info!("This Display has DPI {:?}", gfx.dpi());
        let offset = if gfx.dpi() > 1.0 { 0.0 } else { -1.4 };

        fonts.font_data.insert(
            "inter".to_owned(),
            Arc::new(FontData::from_static(FONT).tweak(FontTweak {
                scale: 1.0,
                y_offset_factor: 0.0,
                y_offset: offset,
                baseline_offset_factor: 0.0,
            })),
        );

        fonts.font_data.insert(
            "inter_bold".to_owned(),
            Arc::new(FontData::from_static(BOLD_FONT).tweak(FontTweak {
                scale: 1.0,
                y_offset_factor: 0.0,
                y_offset: offset,
                baseline_offset_factor: 0.0,
            })),
        );
        fonts.families.insert(
            FontFamily::Name("bold".to_owned().into()),
            vec!["inter_bold".into()],
        );

        fonts.font_data.insert(
            "icons".to_owned(),
            Arc::new(
                FontData::from_static(include_bytes!("../res/fonts/icons.ttf")).tweak(FontTweak {
                    scale: 1.0,
                    y_offset_factor: 0.0,
                    y_offset: 1.0,
                    baseline_offset_factor: 0.0,
                }),
            ),
        );

        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "icons".to_owned());

        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "inter".to_owned());

        let fonts = load_system_fonts(fonts);

        debug!("Theme {:?}", state.persistent_settings.theme);
        apply_theme(&mut state, ctx);
        ctx.set_fonts(fonts);
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

fn process_events(app: &mut App, state: &mut OculanteState, evt: Event) {
    if state.key_grab {
        return;
    }
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
                state.image_geometry.offset.x -= delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, PanUp) {
                state.image_geometry.offset.y += delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, PanLeft) {
                state.image_geometry.offset.x += delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, PanDown) {
                state.image_geometry.offset.y -= delta;
                limit_offset(app, state);
            }
            if key_pressed(app, state, CompareNext) {
                compare_next(app, state);
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
                _ = state.persistent_settings.save_blocking();
                _ = state.volatile_settings.save_blocking();
                app.backend.exit();
            }
            #[cfg(feature = "turbo")]
            if key_pressed(app, state, LosslessRotateRight) {
                debug!("Lossless rotate right");

                if let Some(p) = &state.current_path {
                    if lossless_tx(p, turbojpeg::Transform::op(turbojpeg::TransformOp::Rot90))
                        .is_ok()
                    {
                        state.is_loaded = false;
                        // This needs "deep" reload
                        state.player.cache.clear();
                        state.player.load(p);
                    }
                }
            }
            #[cfg(feature = "turbo")]
            if key_pressed(app, state, LosslessRotateLeft) {
                debug!("Lossless rotate left");
                if let Some(p) = &state.current_path {
                    if lossless_tx(p, turbojpeg::Transform::op(turbojpeg::TransformOp::Rot270))
                        .is_ok()
                    {
                        state.is_loaded = false;
                        // This needs "deep" reload
                        state.player.cache.clear();
                        state.player.load(p);
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
                    state.filebrowser_id = Some("OPEN".into());
                }
            }

            if key_pressed(app, state, NextImage) {
                next_image(state)
            }
            if key_pressed(app, state, PreviousImage) {
                prev_image(state)
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
            }
            if key_pressed(app, state, EditMode) {
                state.persistent_settings.edit_enabled = !state.persistent_settings.edit_enabled;
            }
            if key_pressed(app, state, DeleteFile) {
                // TODO: needs confirmation
                delete_file(state);
            }
            if key_pressed(app, state, ClearImage) {
                clear_image(state);
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
            state.volatile_settings.window_geometry.1 = (width, height);
            state.volatile_settings.window_geometry.0 = (
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
            state.volatile_settings.window_geometry = (
                (
                    app.window().position().0 as u32,
                    app.window().position().1 as u32,
                ),
                app.window().size(),
            );
            _ = state.persistent_settings.save_blocking();
            _ = state.volatile_settings.save_blocking();
        }
        Event::MouseWheel { delta_y, .. } => {
            trace!("Mouse wheel event");
            if !state.pointer_over_ui {
                if app.keyboard.ctrl() {
                    // Change image to next/prev
                    // - map scroll-down == next, as that's the natural scrolling direction
                    if delta_y > 0.0 {
                        prev_image(state)
                    } else {
                        next_image(state)
                    }
                } else {
                    let divisor = if cfg!(target_os = "macos") { 0.1 } else { 10. };
                    // Normal scaling
                    let delta = zoomratio(
                        ((delta_y / divisor) * state.persistent_settings.zoom_multiplier)
                            .clamp(-5.0, 5.0),
                        state.image_geometry.scale,
                    );
                    trace!("Delta {delta}, raw {delta_y}");
                    let new_scale = state.image_geometry.scale + delta;
                    // limit scale
                    if new_scale > 0.01 && new_scale < 40. {
                        state.image_geometry.offset -= scale_pt(
                            state.image_geometry.offset,
                            state.cursor,
                            state.image_geometry.scale,
                            delta,
                        );
                        state.image_geometry.scale += delta;
                    }
                }
            }
        }

        Event::Drop(file) => {
            trace!("File drop event");
            if let Some(p) = file.path {
                if let Some(ext) = p.extension() {
                    if SUPPORTED_EXTENSIONS
                        .contains(&ext.to_string_lossy().to_string().to_lowercase().as_str())
                    {
                        state.is_loaded = false;
                        state.current_image = None;
                        state.player.load(&p);
                        state.current_path = Some(p);
                    } else {
                        state.send_message_warn("Unsupported file!");
                    }
                }
            }
        }
        Event::MouseDown { button, .. } => match button {
            MouseButton::Left => {
                if !state.mouse_grab {
                    state.drag_enabled = true;
                }
            }
            MouseButton::Middle => {
                state.drag_enabled = true;
            }
            _ => {}
        },
        Event::MouseUp { button, .. } => match button {
            MouseButton::Left | MouseButton::Middle => state.drag_enabled = false,
            _ => {}
        },
        _ => {
            trace!("Event: {:?}", evt);
        }
    }
}

fn update(app: &mut App, state: &mut OculanteState) {
    if state.first_start {
        app.window().set_always_on_top(false);
    }

    if let Some(p) = &state.current_path {
        let t = app.timer.elapsed_f32() % 0.8;
        if t <= 0.05 {
            trace!("chk mod {}", t);
            state.player.check_modified(p);
        }
    }

    // Save every 5 secs
    let t = app.timer.elapsed_f32() % 5.0;
    if t <= 0.01 {
        state.volatile_settings.window_geometry = (
            (
                app.window().position().0 as u32,
                app.window().position().1 as u32,
            ),
            app.window().size(),
        );
        _ = state.persistent_settings.save_blocking();
        _ = state.volatile_settings.save_blocking();
        trace!("Save {t}");
    }

    let mouse_pos = app.mouse.position();

    state.mouse_delta = Vector2::new(mouse_pos.0, mouse_pos.1) - state.cursor;
    state.cursor = mouse_pos.size_vec();
    if state.drag_enabled && !state.mouse_grab || app.mouse.is_down(MouseButton::Middle) {
        state.image_geometry.offset += state.mouse_delta;
        limit_offset(app, state);
    }

    // Since we can't access the window in the event loop, we store it in the state
    state.window_size = app.window().size().size_vec();

    if let Some(dimensions) = state.current_image.as_ref().map(|image| image.dimensions()) {
        state.image_geometry.dimensions = dimensions;
    }

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

    // redraw if extended info is missing so we make sure it's promply displayed
    if state.persistent_settings.info_enabled && state.image_metadata.is_none() {
        app.window().request_frame();
    }

    // check extended info has been sent
    if let Ok(info) = state.extended_info_channel.1.try_recv() {
        debug!("Received extended image info for {}", info.name);
        state.image_metadata = Some(info);
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
                state.current_texture.clear();
            }
            Message::Info(m) => {
                state.toasts.info(m).duration(Some(Duration::from_secs(1)));
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
    let mut zoom_image = gfx.create_draw();
    if let Ok(p) = state.load_channel.1.try_recv() {
        state.is_loaded = false;
        state.current_image = None;
        state.player.load(&p);
        if let Some(dir) = p.parent() {
            state.volatile_settings.last_open_directory = dir.to_path_buf();
        }
        state.current_path = Some(p);
        state.scrubber.fixed_paths = false;
    }

    // check if a new loaded image has been sent
    if let Ok(frame) = state.texture_channel.1.try_recv() {
        state.is_loaded = true;

        debug!("Got frame: {}", frame);

        if matches!(
            &frame,
            Frame::AnimationStart(_) | Frame::Still(_) | Frame::ImageCollectionMember(_)
        ) {
            // Something new came in, update scrubber (index slider) and path
            if let Some(path) = &state.current_path {
                if state.scrubber.has_folder_changed(path) && !state.scrubber.fixed_paths {
                    debug!("Folder has changed, creating new scrubber");
                    state.scrubber = scrubber::Scrubber::new(path);
                    state.scrubber.wrap = state.persistent_settings.wrap_folder;
                } else {
                    let index = state
                        .scrubber
                        .entries
                        .iter()
                        .position(|p| p == path)
                        .unwrap_or_default();
                    if index < state.scrubber.entries.len() {
                        state.scrubber.index = index;
                    }
                }
            }

            if let Some(path) = &state.current_path {
                if state.persistent_settings.max_recents > 0
                    && !state.volatile_settings.recent_images.contains(path)
                {
                    state
                        .volatile_settings
                        .recent_images
                        .push_front(path.clone());
                    state
                        .volatile_settings
                        .recent_images
                        .truncate(state.persistent_settings.max_recents as usize);
                }
            }
        }

        match &frame {
            Frame::Still(ref img) | Frame::ImageCollectionMember(ref img) => {
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
                if state.current_texture.get().is_none() {
                    state.reset_image = true;
                }

                if !state.persistent_settings.keep_edits {
                    state.edit_state = Default::default();
                    state.edit_state = Default::default();
                }

                // Load edit information if any
                if let Some(p) = &state.current_path {
                    if p.with_extension("oculante").is_file() {
                        if let Ok(f) = std::fs::File::open(p.with_extension("oculante")) {
                            match serde_json::from_reader::<_, EditState>(f) {
                                Ok(edit_state) => {
                                    state.send_message_info(
                                        "Edits have been loaded for this image.",
                                    );
                                    state.edit_state = edit_state;
                                    state.persistent_settings.edit_enabled = true;
                                    state.reset_image = true;
                                }
                                Err(e) => {
                                    // state.send_message_info("Edits have been loaded for this image.");
                                    warn!("{e}");

                                    if let Ok(f) = std::fs::File::open(p.with_extension("oculante"))
                                    {
                                        if let Ok(legacy_edit_state) =
                                            serde_json::from_reader::<_, LegacyEditState>(f)
                                        {
                                            warn!("Legacy edits found");
                                            state.send_message_info(
                                                "Edits have been loaded for this image.",
                                            );
                                            state.edit_state = legacy_edit_state.upgrade();
                                            state.persistent_settings.edit_enabled = true;
                                            state.reset_image = true;
                                            // Migrate config
                                            if let Ok(f) =
                                                std::fs::File::create(p.with_extension("oculante"))
                                            {
                                                _ = serde_json::to_writer_pretty(
                                                    &f,
                                                    &state.edit_state,
                                                );
                                            }
                                        }
                                    } else {
                                        state.send_message_err("Edits could not be loaded.");
                                    }
                                }
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
                // state.image_info = None;
            }
            Frame::EditResult(_) => {
                state.redraw = false;
            }
            Frame::AnimationStart(_) => {
                state.redraw = true;
                state.reset_image = true
            }
            Frame::Animation(_, _) => {
                state.redraw = true;
            }
            Frame::CompareResult(_, geo) => {
                debug!("Received compare result");
                state.image_geometry = *geo;
                // always reset if first image
                if state.current_texture.get().is_none() {
                    state.reset_image = true;
                }

                state.redraw = false;
            }
            Frame::UpdateTexture => {}
        }

        if !matches!(frame, Frame::Animation(_, _)) {
            state.image_metadata = None;
        }

        // Deal with everything that sends an image
        match frame {
            Frame::AnimationStart(img)
            | Frame::Still(img)
            | Frame::EditResult(img)
            | Frame::CompareResult(img, _)
            | Frame::Animation(img, _)
            | Frame::ImageCollectionMember(img) => {
                debug!("Received image buffer: {:?}", img.dimensions(),);
                state.image_geometry.dimensions = img.dimensions();

                if let Err(error) =
                    state
                        .current_texture
                        .set_image(&img, gfx, &state.persistent_settings)
                {
                    state.send_message_warn(&format!("Error while displaying image: {error}"));
                }
                state.current_image = Some(img);
            }
            Frame::UpdateTexture => {
                // Only update the texture.

                // Prefer the edit result, if present
                if state.edit_state.result_pixel_op != Default::default() {
                    if let Err(error) = state.current_texture.set_image(
                        &state.edit_state.result_pixel_op,
                        gfx,
                        &state.persistent_settings,
                    ) {
                        state.send_message_warn(&format!("Error while displaying image: {error}"));
                    }
                } else {
                    // update from image
                    if let Some(img) = &state.current_image {
                        if let Err(error) =
                            state
                                .current_texture
                                .set_image(img, gfx, &state.persistent_settings)
                        {
                            state.send_message_warn(&format!(
                                "Error while displaying image: {error}"
                            ));
                        }
                    }
                }
            }
        }

        set_title(app, state);

        // Update the image buffer in all cases except incoming edits.
        // In those cases, we want the image to stay as it is.
        // TODO: PERF: This copies the image buffer. This should also maybe not run for animation frames
        // although it looks cool.
        send_extended_info(
            &state.current_image,
            &state.current_path,
            &state.extended_info_channel,
        );
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
    let mut bbox_tl: egui::Pos2 = Default::default();
    let mut bbox_br: egui::Pos2 = Default::default();
    let mut info_panel_color = egui::Color32::from_gray(200);
    let egui_output = plugins.egui(|ctx| {
        state.toasts.show(ctx);
        if let Some(id) = state.filebrowser_id.take() {
            ctx.memory_mut(|w| w.open_popup(Id::new(&id)));
        }

        if !state.pointer_over_ui
            && !state.mouse_grab
            && ctx.input(|r| {
                r.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
            })
        {
            toggle_fullscreen(app, state);
        }

        // set info panel color dynamically
        info_panel_color = ctx.style().visuals.panel_fill;

        // open a file browser if requested
        #[cfg(not(feature = "file_open"))]
        {
            if ctx.memory(|w| w.is_popup_open(Id::new("OPEN"))) {
                filebrowser::browse_modal(
                    false,
                    SUPPORTED_EXTENSIONS,
                    &mut state.volatile_settings,
                    |p| {
                        let _ = state.load_channel.0.clone().send(p.to_path_buf());
                        ctx.memory_mut(|w| w.close_popup());
                    },
                    ctx,
                );
            }
        }

        // the top menu bar
        if !state.persistent_settings.zen_mode {
            let menu_height = 36.0;
            egui::TopBottomPanel::top("menu")
                .exact_height(menu_height)
                .show_separator_line(false)
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

        if state.persistent_settings.edit_enabled
            && !state.settings_enabled
            && !state.persistent_settings.zen_mode
            && state.current_image.is_some()
        {
            edit_ui(app, ctx, state, gfx);
        }

        if state.persistent_settings.info_enabled
            && !state.settings_enabled
            && !state.persistent_settings.zen_mode
            && state.current_image.is_some()
        {
            (bbox_tl, bbox_br) = info_ui(ctx, state, gfx);
        }

        state.pointer_over_ui = ctx.is_pointer_over_area();

        // if there is interaction on the ui (dragging etc)
        // we don't want zoom & pan to work, so we "grab" the pointer
        state.mouse_grab = ctx.is_using_pointer()
            || state.edit_state.painting
            || ctx.is_pointer_over_area()
            || state.edit_state.block_panning;

        state.key_grab = ctx.wants_keyboard_input();

        if state.reset_image {
            if let Some(current_image) = &state.current_image {
                let draw_area = ctx.available_rect();
                let window_size = nalgebra::Vector2::new(
                    draw_area.width().min(app.window().width() as f32),
                    draw_area.height().min(app.window().height() as f32),
                );
                let img_size = current_image.size_vec();
                let scaled_to_fit = window_size.component_div(&img_size).amin();
                state.image_geometry.scale = if state.persistent_settings.auto_scale {
                    scaled_to_fit
                } else {
                    scaled_to_fit.min(1.0)
                };
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

    if let Some(texture) = &state.current_texture.get() {
        // align to pixel to prevent distortion
        let aligned_offset_x = state.image_geometry.offset.x.trunc();
        let aligned_offset_y = state.image_geometry.offset.y.trunc();

        if state.persistent_settings.show_checker_background {
            if let Some(checker) = &state.checker_texture {
                draw.pattern(checker)
                    .size(
                        texture.width() * state.image_geometry.scale * state.tiling as f32,
                        texture.height() * state.image_geometry.scale * state.tiling as f32,
                    )
                    .blend_mode(BlendMode::ADD)
                    .translate(aligned_offset_x, aligned_offset_y);
            }
        }
        if state.tiling < 2 {
            texture.draw_textures(
                &mut draw,
                aligned_offset_x,
                aligned_offset_y,
                state.image_geometry.scale,
            );
        } else {
            for yi in 0..state.tiling {
                for xi in 0..state.tiling {
                    //The "old" version used only a static offset, is this correct?
                    let translate_x = (xi as f32 * texture.width() * state.image_geometry.scale
                        + state.image_geometry.offset.x)
                        .trunc();
                    let translate_y = (yi as f32 * texture.height() * state.image_geometry.scale
                        + state.image_geometry.offset.y)
                        .trunc();

                    texture.draw_textures(
                        &mut draw,
                        translate_x,
                        translate_y,
                        state.image_geometry.scale,
                    );
                }
            }
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
                .translate(aligned_offset_x, aligned_offset_y);
        }

        if state.persistent_settings.info_enabled
            && !state.settings_enabled
            && !state.persistent_settings.zen_mode
        {
            draw.rect((0., 0.), (PANEL_WIDTH + 4., state.window_size.y))
                .color(Color::from_rgb(
                    info_panel_color.r() as f32 / 255.,
                    info_panel_color.g() as f32 / 255.,
                    info_panel_color.b() as f32 / 255.,
                ));

            texture.draw_zoomed(
                &mut zoom_image,
                bbox_tl.x,
                bbox_tl.y,
                bbox_br.x - bbox_tl.x,
                (state.cursor_relative.x, state.cursor_relative.y),
                8.0,
            );
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
    gfx.render(&zoom_image);
    gfx.render(&egui_output);
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

// Handle [`CompareNext`] events
fn compare_next(_app: &mut App, state: &mut OculanteState) {
    if let Some(CompareItem { path, geometry }) = state.compare_list.next() {
        state.is_loaded = false;
        state.current_image = None;
        state.player.load_advanced(
            path,
            Some(Frame::CompareResult(Default::default(), *geometry)),
        );
        state.current_path = Some(path.to_owned());
    }
}

// Parse piped file names from stdin.
fn piped_paths(args: &clap::ArgMatches) -> Option<impl Iterator<Item = PathBuf>> {
    // Don't yield paths if user is piping in raw image data
    (!args.contains_id("stdin") && !stdin().is_terminal()).then(|| {
        stdin().lines().flat_map(|line| {
            line.unwrap_or_default()
                .split_whitespace()
                .map(PathBuf::from)
                .collect::<Vec<_>>()
        })
    })
}
