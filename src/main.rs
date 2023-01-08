#![windows_subsystem = "windows"]

use arboard::Clipboard;
use clap::Arg;
use clap::Command;
use log::debug;
use log::error;
use log::info;
use log::warn;
use nalgebra::Vector2;
use notan::app::Event;
use notan::draw::*;
use notan::egui::{self, *};
use notan::prelude::*;
use shortcuts::key_pressed;
use shortcuts::lookup;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::mpsc;
use strum::IntoEnumIterator;

pub mod cache;
pub mod scrubber;
pub mod settings;
pub mod shortcuts;
use crate::scrubber::find_first_image_in_directory;
use crate::shortcuts::InputEvent::*;
mod utils;

use utils::*;
// mod events;
#[cfg(target_os = "macos")]
mod mac;
mod net;
use net::*;
#[cfg(test)]
mod tests;
mod ui;
mod update;
use ui::*;

use crate::image_editing::EditState;

mod image_editing;
pub mod paint;

#[notan_main]
fn main() -> Result<(), String> {
    // hack for wayland
    std::env::set_var("WINIT_UNIX_BACKEND", "x11");
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "warning");
    }
    // on debug builds, override log level
    #[cfg(debug_assertions)]
    {
        std::env::set_var("RUST_LOG", "debug");
        let _ = env_logger::try_init();
    }

    let mut window_config = WindowConfig::new()
        .title(&format!("Oculante | {}", env!("CARGO_PKG_VERSION")))
        .size(1026, 600) // window's size
        .resizable(true) // window can be resized
        .min_size(600, 400); // Set a minimum window size

    #[cfg(target_os = "windows")]
    {
        window_config = window_config.vsync(true);
    }

    #[cfg(target_os = "linux")]
    {
        window_config = window_config.lazy_loop(true).vsync(true);
    }

    #[cfg(target_os = "netbsd")]
    {
        window_config = window_config.lazy_loop(true).vsync(true);
    }

    #[cfg(target_os = "macos")]
    {
        window_config = window_config.lazy_loop(true).vsync(true).high_dpi(true);
    }

    #[cfg(target_os = "macos")]
    {
        // MacOS needs an incredible dance performed just to open a file
        let _ = mac::launch();
    }

    if let Ok(settings) = settings::PersistentSettings::load() {
        window_config.vsync = settings.vsync;
        info!("Loaded vsync.");
    }

    info!("Starting oculante.");
    notan::init_with(init)
        .add_config(window_config)
        .add_config(EguiConfig)
        .add_config(DrawConfig)
        .event(event)
        .update(update)
        .draw(drawe)
        .build()
}

fn init(_gfx: &mut Graphics, plugins: &mut Plugins) -> OculanteState {
    info!("Now matching arguments {:?}", std::env::args());
    // Filter out strange mac args
    let args: Vec<String> = std::env::args().filter(|a| !a.contains("psn_")).collect();

    let matches = Command::new("Oculante")
        .arg(
            Arg::new("INPUT")
                .help("Display this image")
                // .required(true)
                .index(1),
        )
        .arg(
            Arg::new("l")
                .short('l')
                .help("Listen on port")
                .takes_value(true),
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

    let maybe_img_location = matches.value_of("INPUT").map(|arg| PathBuf::from(arg));

    let mut state = OculanteState {
        texture_channel: mpsc::channel(),
        // current_path: maybe_img_location.cloned(/),
        ..Default::default()
    };

    if let Ok(settings) = settings::PersistentSettings::load() {
        state.persistent_settings = settings;
    } else {
        warn!("Settings failed to load. This may happen after application updates. Generating a fresh file.");
        state.persistent_settings = Default::default();
        _ = state.persistent_settings.save();
    }

    state.player = Player::new(
        state.texture_channel.0.clone(),
        state.persistent_settings.max_cache,
    );

    debug!("Image is: {:?}", maybe_img_location);

    if let Some(ref location) = maybe_img_location {
        // Check if path is a directory or a file (and that it even exists)
        let mut start_img_location: Option<PathBuf> = None;

        if let Ok(maybe_location_metadata) = location.metadata() {
            if maybe_location_metadata.is_dir() {
                // Folder - Pick first image from the folder...
                if let Ok(first_img_location) = find_first_image_in_directory(location) {
                    start_img_location = Some(first_img_location.clone());
                }
            } else if is_ext_compatible(location) {
                // Image File with a usable extension
                start_img_location = Some(location.clone());
            } else {
                // Unsupported extension
                state.message = Some(format!("ERROR: Unsupported file: {} - Open Github issue if you think this should not happen.", location.display()));
            }
        } else {
            // Not a valid path, or user doesn't have permission to access?
            state.message = Some(format!("ERROR: Can't open file: {}", location.display()));
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

    if let Some(port) = matches.value_of("l") {
        match port.parse::<i32>() {
            Ok(p) => {
                state.message = Some(format!("Listening on {}", p));
                recv(p, state.texture_channel.0.clone());
                state.current_path = Some(PathBuf::from(&format!("network port {p}")));
                state.network_mode = true;
            }
            Err(_) => error!("Port must be a number"),
        }
    }

    // Set up egui style
    plugins.egui(|ctx| {
        let mut fonts = FontDefinitions::default();

        fonts.font_data.insert(
            "customfont".to_owned(),
            FontData::from_static(include_bytes!("../res/fonts/NotoSans-Regular.ttf")),
        );

        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, "customfont".into());

        let mut style: egui::Style = (*ctx.style()).clone();

        style.text_styles.get_mut(&TextStyle::Body).unwrap().size = 18.;
        style.text_styles.get_mut(&TextStyle::Button).unwrap().size = 18.;
        style.text_styles.get_mut(&TextStyle::Small).unwrap().size = 15.;
        style.text_styles.get_mut(&TextStyle::Heading).unwrap().size = 22.;
        style.visuals.selection.bg_fill = Color32::from_rgb(
            state.persistent_settings.accent_color[0],
            state.persistent_settings.accent_color[1],
            state.persistent_settings.accent_color[2],
        );
        // style.visuals.selection.bg_fill = Color32::from_rgb(200, 240, 200);
        ctx.set_style(style);
        ctx.set_fonts(fonts);
    });

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
                state.offset.x += delta;
            }
            if key_pressed(app, state, PanUp) {
                state.offset.y -= delta;
            }
            if key_pressed(app, state, PanLeft) {
                state.offset.x -= delta;
            }
            if key_pressed(app, state, PanDown) {
                state.offset.y += delta;
            }

            if key_pressed(app, state, ResetView) {
                state.reset_image = true
            }

            if key_pressed(app, state, Quit) {
                std::process::exit(0)
            }

            #[cfg(feature = "file_open")]
            if key_pressed(app, state, Browse) {
                browse_for_image_path(state)
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
                state.info_enabled = !state.info_enabled;
                send_extended_info(
                    &state.current_image,
                    &state.current_path,
                    &state.extended_info_channel,
                );
            }

            if key_pressed(app, state, EditMode) {
                state.edit_enabled = !state.edit_enabled
            }

            // let zoomratio = 3.5;
            if key_pressed(app, state, ZoomIn) {
                let delta = zoomratio(3.5, state.scale);
                let new_scale = state.scale + delta;
                // limit scale
                if new_scale > 0.05 && new_scale < 40. {
                    // We want to zoom towards the center
                    let center: Vector2<f32> = nalgebra::Vector2::new(
                        app.window().width() as f32 / 2.,
                        app.window().height() as f32 / 2.,
                    );
                    state.offset -= scale_pt(state.offset, center, state.scale, delta);
                    state.scale += delta;
                }
            }

            if key_pressed(app, state, ZoomOut) {
                let delta = zoomratio(-3.5, state.scale);
                let new_scale = state.scale + delta;
                // limit scale
                if new_scale > 0.05 && new_scale < 40. {
                    // We want to zoom towards the center
                    let center: Vector2<f32> = nalgebra::Vector2::new(
                        app.window().width() as f32 / 2.,
                        app.window().height() as f32 / 2.,
                    );
                    state.offset -= scale_pt(state.offset, center, state.scale, delta);
                    state.scale += delta;
                }
            }
        }
        _ => (),
    }

    match evt {
        Event::MouseWheel { delta_y, .. } => {
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
                    // Normal scaling
                    let delta = zoomratio(delta_y, state.scale);
                    let new_scale = state.scale + delta;
                    // limit scale
                    if new_scale > 0.01 && new_scale < 40. {
                        state.offset -= scale_pt(state.offset, state.cursor, state.scale, delta);
                        state.scale += delta;
                    }
                }
            }
        }

        Event::Drop(file) => {
            if let Some(p) = file.path {
                state.is_loaded = false;
                state.current_image = None;
                state.player.load(&p, state.message_channel.0.clone());
                state.current_path = Some(p);
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
    let mouse_pos = app.mouse.position();

    state.mouse_delta = Vector2::new(mouse_pos.0, mouse_pos.1) - state.cursor;
    state.cursor = mouse_pos.size_vec();
    if state.drag_enabled {
        if !state.mouse_grab || app.mouse.is_down(MouseButton::Middle) {
            state.offset += state.mouse_delta;
        }
    }

    // Since we can't access the window in the event loop, we store it in the state
    state.window_size = app.window().size().size_vec();

    if state.info_enabled || state.edit_state.painting {
        state.cursor_relative = pos_from_coord(
            state.offset,
            state.cursor,
            Vector2::new(
                state.image_dimension.0 as f32,
                state.image_dimension.1 as f32,
            ),
            state.scale,
        );
    }

    // redraw constantly until the image is fully loaded or it is reset on canvas
    if !state.is_loaded || state.reset_image {
        app.window().request_frame();
    }

    // make sure that in edit mode, RGBA is set.
    // This is a bit lazy. but instead of writing lots of stuff for an ubscure feature,
    // let's disable it here.
    if state.edit_enabled {
        state.current_channel = Channel::RGBA;
    }

    // redraw if extended info is missing so we make sure it's promply displayed
    if state.info_enabled && state.image_info.is_none() {
        app.window().request_frame();
    }

    // reload constantly if gif so we keep receiving
    if let Some(p) = &state.current_path {
        if p.extension() == Some(OsStr::new("gif")) {
            app.window().request_frame();
        }
    }

    // check extended info has been sent
    if let Ok(info) = state.extended_info_channel.1.try_recv() {
        debug!("Received extended image info for {}", info.name);

        state.image_info = Some(info);
        app.window().request_frame();
    }

    // check if a new texture has been sent
    if let Ok(msg) = state.message_channel.1.try_recv() {
        debug!("Received message");
        state.message = Some(msg);
    }
}

fn drawe(app: &mut App, gfx: &mut Graphics, plugins: &mut Plugins, state: &mut OculanteState) {
    let mut draw = gfx.create_draw();

    // check if a new texture has been sent
    if let Ok(frame) = state.texture_channel.1.try_recv() {
        let img = frame.buffer;
        debug!("Received image buffer: {:?}", img.dimensions());
        state.image_dimension = img.dimensions();
        // state.current_texture = img.to_texture(gfx);

        debug!("Frame source: {:?}", frame.source);

        // fill image sequence
        if let Some(p) = &state.current_path {
            state.scrubber = scrubber::Scrubber::new(p);
            state.scrubber.wrap = state.persistent_settings.wrap_folder;

            debug!("{:#?} from {}", &state.scrubber, p.display());
        }

        match frame.source {
            FrameSource::Still => {
                if !state.persistent_settings.keep_view {
                    state.reset_image = true;

                    if let Some(p) = state.current_path.clone() {
                        state.player.cache.insert(&p, img.clone());
                    }
                }
                // always reset if first image
                if state.current_texture.is_none() {
                    state.reset_image = true;
                }

                state.image_info = None;
            }
            FrameSource::EditResult => {
                // debug!("EditResult");
                // state.edit_state.is_processing = false;
            }
            FrameSource::Reset => state.reset_image = true,
            _ => (),
        }

        if let Some(tex) = &mut state.current_texture {
            if tex.width() as u32 == img.width() && img.height() as u32 == img.height() {
                img.update_texture(gfx, tex);
            } else {
                state.current_texture = img.to_texture(gfx);
            }
        } else {
            state.current_texture = img.to_texture(gfx);
        }

        state.is_loaded = true;

        match &state.current_channel {
            // Unpremultiply the image
            Channel::RGB => state.current_texture = unpremult(&img).to_texture(gfx),
            // Do nuttin'
            Channel::RGBA => (),
            // Display the channel
            _ => {
                state.current_texture =
                    solo_channel(&img, *&state.current_channel as usize).to_texture(gfx)
            }
        }
        state.current_image = Some(img);
        if state.info_enabled {
            debug!("Sending extended info");
            send_extended_info(
                &state.current_image,
                &state.current_path,
                &state.extended_info_channel,
            );
        }
    }

    if state.reset_image {
        let window_size = app.window().size().size_vec();
        if let Some(current_image) = &state.current_image {
            let img_size = current_image.size_vec();
            let scale_factor = (window_size.x / img_size.x)
                .min(window_size.y / img_size.y)
                .min(1.0);
            state.scale = scale_factor;
            state.offset = window_size / 2.0 - (img_size * state.scale) / 2.0;

            state.edit_state = Default::default();

            // Load edit information if any
            if let Some(p) = &state.current_path {
                if let Ok(f) = std::fs::File::open(p.with_extension("oculante")) {
                    if let Ok(edit_state) = serde_json::from_reader::<_, EditState>(f) {
                        state.message = Some("Edits have been loaded for this image.".into());
                        state.edit_state = edit_state;
                        state.edit_enabled = true;
                    }
                }
            }

            debug!("Image has been reset.");
            state.reset_image = false;
        }
    }

    if let Some(texture) = &state.current_texture {
        if state.tiling < 2 {
            draw.image(texture)
                .blend_mode(BlendMode::NORMAL)
                .translate(state.offset.x as f32, state.offset.y as f32)
                .scale(state.scale, state.scale);
        } else {
            draw.pattern(texture)
                .translate(state.offset.x as f32, state.offset.y as f32)
                .scale(state.scale, state.scale)
                .size(
                    texture.width() * state.tiling as f32,
                    texture.height() * state.tiling as f32,
                );
        }

        // Draw a brush preview when paint mode is on
        if state.edit_state.painting {
            if let Some(stroke) = state.edit_state.paint_strokes.last() {
                let dim = texture.width().min(texture.height()) / 50.;
                draw.circle(20.)
                    // .translate(state.cursor_relative.x, state.cursor_relative.y)
                    .translate(state.cursor.x, state.cursor.y)
                    .alpha(0.5)
                    .stroke(1.5)
                    .scale(state.scale, state.scale)
                    .scale(stroke.width * dim, stroke.width * dim);

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

    let egui_output = plugins.egui(|ctx| {
        // the top menu bar
        egui::TopBottomPanel::top("menu")
            .min_height(30.)
            .default_height(30.)
            .show(&ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label("Channels");

                    let mut changed_channels = false;

                    if key_pressed(app, state, RedChannel) {
                        state.current_channel = Channel::Red;
                        changed_channels = true;
                    }
                    if key_pressed(app, state, GreenChannel) {
                        state.current_channel = Channel::Green;
                        changed_channels = true;
                    }
                    if key_pressed(app, state, BlueChannel) {
                        state.current_channel = Channel::Blue;
                        changed_channels = true;
                    }
                    if key_pressed(app, state, AlphaChannel) {
                        state.current_channel = Channel::Alpha;
                        changed_channels = true;
                    }

                    if key_pressed(app, state, RGBChannel) {
                        state.current_channel = Channel::RGB;
                        changed_channels = true;
                    }
                    if key_pressed(app, state, RGBAChannel) {
                        state.current_channel = Channel::RGBA;
                        changed_channels = true;
                    }

                    ui.add_enabled_ui(!state.edit_enabled, |ui| {
                        // hack to center combo box in Y

                        ui.spacing_mut().button_padding = Vec2::new(10., 0.);
                        egui::ComboBox::from_id_source("channels")
                            .selected_text(format!("{:?}", state.current_channel))
                            .show_ui(ui, |ui| {
                                for channel in Channel::iter() {
                                    let r = ui.selectable_value(
                                        &mut state.current_channel,
                                        channel.clone(),
                                        channel.to_string(),
                                    );

                                    if tooltip(
                                        r,
                                        &channel.to_string(),
                                        &channel.hotkey(&state.persistent_settings.shortcuts),
                                        ui,
                                    )
                                    .clicked()
                                    {
                                        changed_channels = true;
                                    }
                                }
                            });
                    });

                    // TODO: remove redundancy
                    if changed_channels {
                        if let Some(img) = &state.current_image {
                            match &state.current_channel {
                                Channel::RGB => {
                                    state.current_texture = unpremult(img).to_texture(gfx)
                                }
                                Channel::RGBA => state.current_texture = img.to_texture(gfx),
                                _ => {
                                    state.current_texture =
                                        solo_channel(img, *&state.current_channel as usize)
                                            .to_texture(gfx)
                                }
                            }
                        }
                    }

                    if state.current_image.is_some() {
                        if state.current_path.is_some() {
                            if tooltip(
                                unframed_button("◀", ui),
                                "Previous image",
                                &lookup(&state.persistent_settings.shortcuts, &PreviousImage),
                                ui,
                            )
                            .clicked()
                            {
                                prev_image(state)
                            }
                            if tooltip(
                                unframed_button("▶", ui),
                                "Next image",
                                &lookup(&state.persistent_settings.shortcuts, &NextImage),
                                ui,
                            )
                            .clicked()
                            {
                                next_image(state)
                            }
                        }

                        if tooltip(
                            // ui.checkbox(&mut state.info_enabled, "ℹ Info"),
                            ui.selectable_label(state.info_enabled, "ℹ Info"),
                            "Show image info",
                            &lookup(&state.persistent_settings.shortcuts, &InfoMode),
                            ui,
                        )
                        .clicked()
                        {
                            state.info_enabled = !state.info_enabled;
                            send_extended_info(
                                &state.current_image,
                                &state.current_path,
                                &state.extended_info_channel,
                            );
                        }

                        if tooltip(
                            ui.selectable_label(state.edit_enabled, "✏ Edit"),
                            "Edit the image",
                            &lookup(&state.persistent_settings.shortcuts, &EditMode),
                            ui,
                        )
                        .clicked()
                        {
                            state.edit_enabled = !state.edit_enabled;
                        }
                    }

                    if tooltip(
                        unframed_button("⛶", ui),
                        "Full Screen",
                        &lookup(&state.persistent_settings.shortcuts, &Fullscreen),
                        ui,
                    )
                    .clicked()
                    {
                        toggle_fullscreen(app, state);
                    }

                    if tooltip(
                        unframed_button_colored("📌", state.always_on_top, ui),
                        "Always on top",
                        &lookup(&state.persistent_settings.shortcuts, &AlwaysOnTop),
                        ui,
                    )
                    .clicked()
                    {
                        state.always_on_top = !state.always_on_top;
                        app.window().set_always_on_top(state.always_on_top);
                    }

                    let copy_pressed = key_pressed(app, state, Copy);
                    if let Some(img) = &state.current_image {
                        if unframed_button("🗐 Copy", ui)
                            .on_hover_text("Copy image to clipboard")
                            .clicked()
                            || copy_pressed
                        {
                            clipboard_copy(img);
                        }
                    }

                    if unframed_button("📋 Paste", ui)
                        .on_hover_text("Paste image from clipboard")
                        .clicked()
                        || key_pressed(app, state, Paste)
                    {
                        if let Ok(clipboard) = &mut Clipboard::new() {
                            if let Ok(imagedata) = clipboard.get_image() {
                                if let Some(image) = image::RgbaImage::from_raw(
                                    imagedata.width as u32,
                                    imagedata.height as u32,
                                    (&imagedata.bytes).to_vec(),
                                ) {
                                    // Stop in the even that an animation is running
                                    state.player.stop();
                                    _ = state
                                        .player
                                        .image_sender
                                        .send(crate::utils::Frame::new_still(image));
                                    // Since pasted data has no path, make sure it's not set
                                    state.current_path = None;
                                }
                            }
                        }
                    }

                    if unframed_button_colored("⛭", state.settings_enabled, ui)
                        .on_hover_text("Open settings")
                        .clicked()
                    {
                        state.settings_enabled = !state.settings_enabled;
                    }
                    if let Some(file) = state.current_path.as_ref().map(|p| p.file_name()).flatten()
                    {
                        ui.label(format!("{}", file.to_string_lossy()));
                        ui.label(format!(
                            "{}x{}",
                            state.image_dimension.0, state.image_dimension.1
                        ));
                    }
                });
            });

        if state.persistent_settings.show_scrub_bar {
            egui::TopBottomPanel::bottom("scrubber")
                .max_height(22.)
                .min_height(22.)
                .show(&ctx, |ui| {
                    scrubber_ui(state, ui);
                });
        }

        if let Some(message) = &state.message.clone() {
            let max_anim_len = 2.8;

            state.toast_cooldown += app.timer.delta_f32();
            if state.toast_cooldown > max_anim_len {
                state.toast_cooldown = 0.;
                state.message = None;
            } else {
                let toast_height = ((max_anim_len - state.toast_cooldown) * 30.).min(25.);

                egui::TopBottomPanel::bottom("toast")
                    .max_height(toast_height)
                    .min_height(toast_height)
                    .show(&ctx, |ui| {
                        ui.ctx().request_repaint();
                        ui.horizontal(|ui| {
                            ui.label(message);
                            ui.spacing();

                            if unframed_button("❌", ui).clicked() {
                                state.message = None;
                                state.toast_cooldown = 0.;
                            }
                        });
                    });
            }
        }

        if state.info_enabled {
            info_ui(ctx, state, gfx);
        }

        if state.edit_enabled {
            edit_ui(ctx, state, gfx);
        }

        if !state.is_loaded {
            egui::Window::new("")
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .collapsible(false)
                .resizable(false)
                .default_width(400.)
                .title_bar(false)
                .show(&ctx, |ui| {
                    if state.current_path.is_some() {
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::default());
                            ui.label(format!(
                                "Loading {}",
                                state.current_path.clone().unwrap_or_default().display()
                            ));
                        });
                    } else {
                        ui.heading("🖼 Please drag an image here!");
                    }
                });
            app.window().request_frame();
        }

        state.pointer_over_ui = ctx.is_pointer_over_area();
        // info!("using pointer {}", ctx.is_using_pointer());

        // if there is interaction on the ui (dragging etc)
        // we don't want zoom & pan to work, so we "grab" the pointer
        if ctx.is_using_pointer() || state.edit_state.painting || ctx.is_pointer_over_area() {
            state.mouse_grab = true;
        } else {
            state.mouse_grab = false;
        }

        if ctx.wants_keyboard_input() {
            state.key_grab = true;
        } else {
            state.key_grab = false;
        }
        // Settings come last, as they block keyboard grab (for hotkey assigment)
        settings_ui(app, ctx, state);
    });

    if state.network_mode {
        app.window().request_frame();
    }
    // if state.edit_state.is_processing {
    //     app.window().request_frame();
    // }
    draw.clear(Color::from_rgb(0.2, 0.2, 0.2));
    gfx.render(&draw);
    gfx.render(&egui_output);
    if egui_output.needs_repaint() {
        app.window().request_frame();
    }
}

// Show file browser to select image to load
#[cfg(feature = "file_open")]
fn browse_for_image_path(state: &mut OculanteState) {
    let start_directory = if let Some(img_path) = &state.current_path {
        img_path.clone()
    } else {
        std::env::current_dir().unwrap_or_default()
    };

    let file_dialog_result = rfd::FileDialog::new()
        .add_filter("All Supported Image Types", &utils::SUPPORTED_EXTENSIONS)
        .add_filter("All File Types", &["*"])
        .set_directory(&start_directory)
        .pick_file();

    if let Some(file_path) = file_dialog_result {
        debug!("Selected File Path = {:?}", file_path);
        state.is_loaded = false;
        state.current_image = None;
        state
            .player
            .load(&file_path, state.message_channel.0.clone());
        state.current_path = Some(file_path);
    }
}
