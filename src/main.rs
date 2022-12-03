#![windows_subsystem = "windows"]

use arboard::Clipboard;
use clap::Arg;
use clap::Command;
use log::debug;
use log::error;
use log::info;
use nalgebra::Vector2;
use notan::app::Event;
use notan::draw::*;
use notan::egui::{self, *};
use notan::prelude::*;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::mpsc;
use strum::IntoEnumIterator;

pub mod settings;

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
        window_config = window_config.lazy_loop(true).vsync(true);
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
    }

    state.player = Player::new(state.texture_channel.0.clone());

    debug!("Image is: {:?}", maybe_img_location);

    if let Some(ref img_location) = maybe_img_location {
        state.current_path = Some(img_location.clone());
        if img_location.extension() == Some(&std::ffi::OsString::from("gif")) {
            state
                .player
                .load(&img_location, state.message_channel.0.clone());
        } else {
            state
                .player
                .load_blocking(&img_location, state.message_channel.0.clone());
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
    // pan image with keyboard
    if !state.mouse_grab {
        if app.keyboard.shift() {
            if app.keyboard.is_down(KeyCode::Right) {
                state.offset.x += 10.;
            }
            if app.keyboard.is_down(KeyCode::Left) {
                state.offset.x -= 10.;
            }

            if app.keyboard.is_down(KeyCode::Up) {
                state.offset.y -= 10.;
            }
            if app.keyboard.is_down(KeyCode::Down) {
                state.offset.y += 10.;
            }
        }
    }

    match evt {
        Event::MouseWheel { delta_y, .. } => {
            if !state.pointer_over_ui {
                let delta = zoomratio(delta_y, state.scale);
                let new_scale = state.scale + delta;
                // limit scale
                if new_scale > 0.05 && new_scale < 40. {
                    state.offset -= scale_pt(state.offset, state.cursor, state.scale, delta);
                    state.scale += delta;
                }
            }
        }
        Event::KeyDown { key: KeyCode::V } => {
            if !state.mouse_grab {
                state.reset_image = true
            }
        }
        Event::KeyDown { key: KeyCode::Q } => {
            if !state.mouse_grab {
                std::process::exit(0)
            }
        }
        Event::KeyDown { key: KeyCode::I } => {
            if !state.mouse_grab {
                state.info_enabled = !state.info_enabled
            }
        }

        Event::KeyDown { key: KeyCode::E } => {
            if !state.mouse_grab {
                state.edit_enabled = !state.edit_enabled
            }
        }
        // zoom in
        Event::KeyDown { key: KeyCode::Plus } => {
            let delta = zoomratio(1.5, state.scale);
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
        Event::KeyDown {
            key: KeyCode::Minus,
        } => {
            let delta = zoomratio(-1.5, state.scale);
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
        Event::KeyDown {
            key: KeyCode::Paste,
        } => {}
        Event::WindowResize { width, height } => {
            if !state.edit_enabled {
                let delta = state.window_size - (width, height).size_vec();
                state.offset -= delta / 2.;
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
        _ => {}
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

    // reload constantly if gif so we keep receiving
    if let Some(p) = &state.current_path {
        if p.extension() == Some(OsStr::new("gif")) {
            app.window().request_frame();
        }
    }

    // check extended info has been sent
    if let Ok(info) = state.extended_info_channel.1.try_recv() {
        debug!("Finished calculating extended image info for {}", info.name);

        state.image_info = Some(info);
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
        debug!("Received image buffer:");
        state.image_dimension = img.dimensions();
        // state.current_texture = img.to_texture(gfx);

        if let Some(tex) = &mut state.current_texture {
            if tex.width() as u32 == img.width() && img.height() as u32 == img.height() {
                img.update_texture(gfx, tex);
            } else {
                state.current_texture = img.to_texture(gfx);
            }
        } else {
            state.current_texture = img.to_texture(gfx);
        }

        //center the image
        if frame.source != FrameSource::Animation {}

        debug!("Frame source: {:?}", frame.source);

        match frame.source {
            FrameSource::Still => {
                state.offset = Default::default();
                state.scale = Default::default();
                state.reset_image = true;
                state.image_info = None;
            }
            FrameSource::EditResult => {
                // debug!("EditResult");
                // state.edit_state.is_processing = false;
            }
            FrameSource::Reset => state.reset_image = true,
            _ => (),
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
            send_extended_info(
                &state.current_image,
                &state.current_path,
                &state.extended_info_channel,
            );
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
        egui::TopBottomPanel::top("menu")
            .min_height(30.)
            .default_height(30.)
            .show(&ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label("Channels");

                    let mut changed_channels = false;

                    if app.keyboard.was_pressed(KeyCode::R) && !state.mouse_grab {
                        state.current_channel = Channel::Red;
                        changed_channels = true;
                    }
                    if app.keyboard.was_pressed(KeyCode::G) && !state.mouse_grab {
                        state.current_channel = Channel::Green;
                        changed_channels = true;
                    }
                    if app.keyboard.was_pressed(KeyCode::B) && !state.mouse_grab {
                        state.current_channel = Channel::Blue;
                        changed_channels = true;
                    }
                    if app.keyboard.was_pressed(KeyCode::A) && !state.mouse_grab {
                        state.current_channel = Channel::Alpha;
                        changed_channels = true;
                    }

                    if app.keyboard.was_pressed(KeyCode::U) && !state.mouse_grab {
                        state.current_channel = Channel::RGB;
                        changed_channels = true;
                    }
                    if app.keyboard.was_pressed(KeyCode::C) && !state.mouse_grab {
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

                                    if tooltip(r, &channel.to_string(), channel.hotkey(), ui)
                                        .clicked()
                                    {
                                        changed_channels = true;
                                    }
                                }
                            });
                    });

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

                    // ui.add(egui::Separator::default().vertical());

                    if state.current_image.is_some() {
                        if state.current_path.is_some() {
                            if tooltip(unframed_button("◀", ui), "Previous image", "Left Arrow", ui)
                                .clicked()
                                || (!app.keyboard.shift()
                                    && app.keyboard.was_pressed(KeyCode::Left)
                                    && !state.mouse_grab)
                            {
                                if let Some(img_location) = state.current_path.as_mut() {
                                    let next_img = img_shift(&img_location, -1);
                                    // prevent reload if at last or first
                                    if &next_img != img_location {
                                        state.is_loaded = false;
                                        *img_location = next_img;
                                        state
                                            .player
                                            .load(&img_location, state.message_channel.0.clone());
                                    }
                                }
                            }
                            if tooltip(unframed_button("▶", ui), "Next image", "Right Arrow", ui)
                                .clicked()
                                || (!app.keyboard.shift()
                                    && app.keyboard.was_pressed(KeyCode::Right)
                                    && !state.mouse_grab)
                            {
                                if let Some(img_location) = state.current_path.as_mut() {
                                    let next_img = img_shift(&img_location, 1);
                                    // prevent reload if at last or first
                                    if &next_img != img_location {
                                        state.is_loaded = false;
                                        *img_location = next_img;
                                        state
                                            .player
                                            .load(&img_location, state.message_channel.0.clone());
                                    }
                                }
                            }
                        }

                        if tooltip(
                            ui.checkbox(&mut state.info_enabled, "ℹ Info"),
                            "Show image info",
                            "i",
                            ui,
                        )
                        .changed()
                            || app.keyboard.was_pressed(KeyCode::I)
                        {
                            send_extended_info(
                                &state.current_image,
                                &state.current_path,
                                &state.extended_info_channel,
                            );
                        }

                        tooltip(
                            ui.checkbox(&mut state.edit_enabled, "✏ Edit"),
                            "Edit the image",
                            "e",
                            ui,
                        );
                    }

                    if tooltip(unframed_button("⛶", ui), "Full Screen", "f", ui).clicked()
                        || app.keyboard.was_pressed(KeyCode::F)
                    {
                        let fullscreen = app.window().is_fullscreen();
                        app.window().set_fullscreen(!fullscreen);
                    }

                    if tooltip(
                        unframed_button_colored("📌", state.always_on_top, ui),
                        "Always on top",
                        "t",
                        ui,
                    )
                    .clicked()
                        || app.keyboard.was_pressed(KeyCode::T)
                    {
                        state.always_on_top = !state.always_on_top;
                        app.window().set_always_on_top(state.always_on_top);
                    }

                    if let Some(img) = &state.current_image {
                        if unframed_button("🗐 Copy", ui)
                            .on_hover_text("Copy image to clipboard")
                            .clicked()
                            || (app.keyboard.ctrl() && app.keyboard.was_pressed(KeyCode::C))
                        {
                            clipboard_copy(img);
                        }
                    }

                    if unframed_button("📋 Paste", ui)
                        .on_hover_text("Paste image from clipboard")
                        .clicked()
                        || (app.keyboard.ctrl() && app.keyboard.was_pressed(KeyCode::V))
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

        if let Some(message) = &state.message.clone() {
            let max_anim_len = 1.8;

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

        settings_ui(ctx, state);

        state.pointer_over_ui = ctx.is_pointer_over_area();
        // info!("using pointer {}", ctx.is_using_pointer());

        // if there is interaction on the ui (dragging etc)
        // we don't want zoom & pan to work, so we "grab" the pointer
        if ctx.is_using_pointer() || state.edit_state.painting || ctx.is_pointer_over_area() {
            state.mouse_grab = true;
        } else {
            state.mouse_grab = false;
        }
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

// fn set_title(window: &mut PistonWindow, text: &str) {
//     let title = format!("Oculante {} | {}", env!("CARGO_PKG_VERSION"), text);
//     window.set_title(title);
// }
