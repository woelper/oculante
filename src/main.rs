#![windows_subsystem = "windows"]

use clap::Arg;
use clap::Command;
use log::debug;
use log::error;
use log::info;
use nalgebra::Vector2;
use notan::app::Event;
// use piston_window::types::{Color, Matrix2d};
// use piston_window::*;
use notan::draw::*;
use notan::egui::{self, *};
use notan::prelude::keyboard::KeyCode;
use notan::prelude::mouse::MouseButton;
use notan::prelude::*;
use std::ffi::OsStr;
// use splines::{Interpolation, Spline};
use std::path::PathBuf;
use std::sync::mpsc;

mod utils;
use utils::*;
// mod events;
#[cfg(target_os = "macos")]
mod mac;
mod net;
use net::*;
#[cfg(test)]
mod tests;
mod update;


#[notan_main]
fn main() -> Result<(), String> {
    // hack for wayland
    std::env::set_var("WINIT_UNIX_BACKEND", "x11");
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "warning");
    }
    // on debug builds, override log level
    #[cfg(debug_assertions)]
    std::env::set_var("RUST_LOG", "info");
    let _ = env_logger::try_init();

    let window_config = WindowConfig::new()
        .title(&format!("Oculante | {}", env!("CARGO_PKG_VERSION")))
        .size(1026, 600) // window's size
        // .vsync() // enable vsync
        .lazy_loop()
        .resizable() // window can be resized
        .min_size(600, 400); // Set a minimum window size

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

    info!("Completed argument parsing.");

    let maybe_img_location = matches.value_of("INPUT").map(|arg| PathBuf::from(arg));

    let mut state = OculanteState {
        texture_channel: mpsc::channel(),
        // current_path: maybe_img_location.cloned(/),
        ..Default::default()
    };

    state.player = Player::new(state.texture_channel.0.clone());

    info!("Image is: {:?}", maybe_img_location);

    #[cfg(target_os = "macos")]
    if !matches.is_present("chainload") && maybe_img_location.is_none() {
        info!("Chainload not specified, and no input file present. Invoking mac hack.");
        // MacOS needs an incredible dance performed just to open a file
        let _ = mac::launch();
    }

    if let Some(ref img_location) = maybe_img_location {
        state.current_path = Some(img_location.clone());
        if img_location.extension() == Some(&std::ffi::OsString::from("gif")) {
            state.player.load(&img_location);
        } else {
            state.player.load_blocking(&img_location);
        }
    }

    if let Some(port) = matches.value_of("l") {
        match port.parse::<i32>() {
            Ok(p) => {
                state.message = format!("Listening on {}", p);
                recv(p, state.texture_channel.0.clone());
                state.current_path = Some(PathBuf::from(&format!("network port {p}")));
            }
            Err(_) => error!("Port must be a number"),
        }
    }

    // Set up egui style
    plugins.egui(|ctx| {
        let mut fonts = FontDefinitions::default();

        fonts.font_data.insert(
            "customfont".to_owned(),
            FontData::from_static(include_bytes!("NotoSans-Regular.ttf")),
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
        ctx.set_style(style);
        ctx.set_fonts(fonts);
    });

    state
}

fn event(state: &mut OculanteState, evt: Event) {
    match evt {
        Event::MouseWheel { delta_y, .. } => {
            let delta = zoomratio(delta_y, state.scale);
            state.offset -= scale_pt(state.offset, state.cursor, state.scale, delta);
            state.scale += delta;
        }
        Event::KeyDown { key: KeyCode::V } => state.reset_image = true,
        Event::KeyDown { key: KeyCode::Q } => std::process::exit(0),
        Event::KeyDown { key: KeyCode::I } => state.info_enabled = !state.info_enabled,
        Event::WindowResize { width, height } => {
            let window_size = (width, height).size_vec();
            if let Some(current_image) = &state.current_image {
                let img_size = current_image.size_vec();
                state.offset = window_size / 2.0 - (img_size * state.scale) / 2.0;
            }
        }
        Event::Drop(file) => {
            if let Some(p) = file.path {
                state.is_loaded = false;
                state.current_image = None;
                state.player.load(&p);
                state.current_path = Some(p);
            }
        }

        _ => {}
    }
}

fn update(app: &mut App, state: &mut OculanteState) {
    let mouse_pos = app.mouse.position();

    state.mouse_delta = Vector2::new(mouse_pos.0, mouse_pos.1) - state.cursor;
    state.cursor = mouse_pos.size_vec();

    if app.mouse.is_down(MouseButton::Left) {
        state.drag_enabled = true;

        state.offset += state.mouse_delta;
    }

    if state.info_enabled {
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

    if app.mouse.was_released(MouseButton::Left) {
        state.drag_enabled = false;
    }
}

fn drawe(app: &mut App, gfx: &mut Graphics, plugins: &mut Plugins, state: &mut OculanteState) {
    if state.reset_image {
        let window_size = app.window().size().size_vec();
        // let window_size = app.backend.

        if let Some(current_image) = &state.current_image {
            let img_size = current_image.size_vec();
            let scale_factor = (window_size.x / img_size.x)
                .min(window_size.y / img_size.y)
                .min(1.0);
            state.scale = scale_factor;
            state.offset = window_size / 2.0 - (img_size * state.scale) / 2.0;
            state.reset_image = false;
            debug!("Done reset");
        }
    }

    let mut draw = gfx.create_draw();

    if let Some(p) = &state.current_path {
        if p.extension() == Some(OsStr::new("gif")) {
            app.window().request_frame();
        }
    }

    // check if a new texture has been sent
    if let Ok(img) = state.texture_channel.1.try_recv() {
        info!("Received image buffer");
        state.image_dimension = (img.width(), img.height());
        state.current_texture = img.to_texture(gfx);

        //center the image
        state.offset = gfx.size().size_vec() / 2.0 - img.size_vec() / 2.0;

        state.reset_image = true;
        state.is_loaded = true;
        state.current_image = Some(img);
    }

    if let Some(texture) = &state.current_texture {
        draw.image(texture)
            .blend_mode(BlendMode::NORMAL)
            .translate(state.offset.x as f32, state.offset.y as f32)
            .scale(state.scale, state.scale);
    }

    let egui_output = plugins.egui(|ctx| {
        egui::TopBottomPanel::top("menu").show(&ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Channels");

                if ui.button("R").clicked() || app.keyboard.was_pressed(KeyCode::R) {
                    if let Some(img) = &state.current_image {
                        state.current_texture = solo_channel(img, 0).to_texture(gfx);
                    }
                }
                if ui.button("G").clicked() || app.keyboard.was_pressed(KeyCode::G) {
                    if let Some(img) = &state.current_image {
                        state.current_texture = solo_channel(img, 1).to_texture(gfx);
                    }
                }
                if ui.button("B").clicked() || app.keyboard.was_pressed(KeyCode::B) {
                    if let Some(img) = &state.current_image {
                        state.current_texture = solo_channel(img, 2).to_texture(gfx);
                    }
                }
                if ui.button("A").clicked() || app.keyboard.was_pressed(KeyCode::A) {
                    if let Some(img) = &state.current_image {
                        state.current_texture = solo_channel(img, 3).to_texture(gfx);
                    }
                }

                if ui.button("Unpremultiplied").clicked() || app.keyboard.was_pressed(KeyCode::U) {
                    if let Some(img) = &state.current_image {
                        state.current_texture = unpremult(img).to_texture(gfx);
                    }
                }
                if ui.button("RGBA").clicked() || app.keyboard.was_pressed(KeyCode::C) {
                    if let Some(img) = &state.current_image {
                        state.current_texture = img.to_texture(gfx);
                    }
                }

                ui.add(egui::Separator::default().vertical());

                if ui.button("⛶").clicked() || app.keyboard.was_pressed(KeyCode::F) {
                    let fullscreen = app.window().is_fullscreen();
                    app.window().set_fullscreen(!fullscreen);
                    // state.reset_image = true;
                }

                ui.checkbox(&mut state.info_enabled, "Show extended info");

                if ui.button("◀").clicked() || app.keyboard.was_pressed(KeyCode::Left) {
                    if let Some(img_location) = state.current_path.as_mut() {
                        let next_img = img_shift(&img_location, -1);
                        // prevent reload if at last or first
                        if &next_img != img_location {
                            state.is_loaded = false;
                            *img_location = next_img;
                            state.player.load(&img_location);
                            // set_title(&mut window, &img_location.to_string_lossy().to_string());
                        }
                    }
                }
                if ui.button("▶").clicked() || app.keyboard.was_pressed(KeyCode::Right) {
                    if let Some(img_location) = state.current_path.as_mut() {
                        let next_img = img_shift(&img_location, 1);
                        // prevent reload if at last or first
                        if &next_img != img_location {
                            state.is_loaded = false;
                            *img_location = next_img;
                            state.player.load(&img_location);
                            // set_title(&mut window, &img_location.to_string_lossy().to_string());
                        }
                    }
                }
            });
        });

        if state.info_enabled {
            egui::SidePanel::left("side_panel").show(&ctx, |ui| {
                ui.label(format!(
                    "Size: {}x{}",
                    state.image_dimension.0, state.image_dimension.1
                ));

                if let Some(path) = &state.current_path {
                    ui.label(format!("Path: {}", path.display()));
                }

                if let Some(img) = &state.current_image {
                    if let Some(p) = img.get_pixel_checked(
                        state.cursor_relative.x as u32,
                        state.cursor_relative.y as u32,
                    ) {
                        state.sampled_color = [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32];
                    }
                }

                if let Some(texture) = &state.current_texture {
                    let desired_width = 200.;
                    let img_size: egui::Vec2 = texture.size().into();
                    let scale = desired_width / img_size.x;
                    let img_size = img_size * scale;

                    let tex_id = gfx.egui_register_texture(&texture);

                    let uv = (
                        state.cursor_relative.x / state.image_dimension.0 as f32,
                        state.cursor_relative.y / state.image_dimension.1 as f32,
                    );

                    ui.label(format!("UV: {:.3},{:.3}", uv.0, uv.1));
                    ui.label(format!(
                        "PX: {:.0},{:.0}",
                        state.cursor_relative.x, state.cursor_relative.y
                    ));
                    ui.label(format!("CLR: {:?}", state.sampled_color));
                    ui.add(
                        egui::Image::new(tex_id, img_size)
                            .uv(egui::Rect::from_x_y_ranges(
                                uv.0 - 0.1..=uv.0 + 0.1,
                                uv.1 - 0.1..=uv.1 + 0.1,
                            ))
                            .bg_fill(egui::Color32::GRAY),
                    );
                    // ui.image(tex_id, img_size);
                }
            });
        }

        if !state.is_loaded {
            egui::Window::new("")
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .collapsible(false)
                .resizable(false)
                .default_width(400.)
                .title_bar(false)
                .show(&ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::default());

                        ui.label(format!(
                            "Loading {}",
                            state.current_path.clone().unwrap_or_default().display()
                        ));
                    });
                });
        }
    });

    // output.clear_color(Color::BLACK);

    draw.clear(Color::from_rgb(0.2, 0.2, 0.2));
    gfx.render(&draw);
    gfx.render(&egui_output);
    if egui_output.needs_repaint() {
        app.window().request_frame();
    }

    // redraw constantly until the image is fully loaded or it's reset on canvas
    if !state.is_loaded || state.reset_image {
        app.window().request_frame();
    }
}

// fn set_title(window: &mut PistonWindow, text: &str) {
//     let title = format!("Oculante {} | {}", env!("CARGO_PKG_VERSION"), text);
//     window.set_title(title);
// }

