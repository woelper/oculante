#![windows_subsystem = "windows"]

use ::image as image_crate;
use clap::{App, Arg};
use image_crate::Pixel;
use log::info;
use nalgebra::Vector2;
use piston_window::*;
use splines::{Interpolation, Spline};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};

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

const TOAST_TIME: f64 = 2.3;

fn set_title(window: &mut PistonWindow, text: &str) {
    let title = format!("Oculante {} | {}", env!("CARGO_PKG_VERSION"), text);
    window.set_title(title);
}

fn main() {
    //update::update();

    //simple_logging::log_to_file("/Users/king2/oculante.log", LevelFilter::Info);

    info!("Starting oculante.");

    let mut state = OculanteState::default();
    state.font_size = 14;
    let mut toast_time = std::time::Instant::now();

    info!("Now matching arguments {:?}", std::env::args());
    let args: Vec<String> = std::env::args().filter(|a| !a.contains("psn_")).collect();

    let matches = App::new("Oculante")
        .arg(
            Arg::with_name("INPUT")
                .help("Display this image")
                // .required(true)
                .index(1),
        )
        .arg(
            Arg::with_name("l")
                .short("l")
                .help("Listen on port")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("chainload")
                .required(false)
                .takes_value(false)
                .short("c")
                .help("Chainload on Mac"),
        )
        .get_matches_from(args);

    info!("Completed argument parsing.");

    let font_regular = include_bytes!("IBMPlexSans-Regular.ttf");

    // animation
    let k1 = splines::Key::new(0., 0., Interpolation::Cosine);
    let k2 = splines::Key::new(TOAST_TIME * 0.125, 80., Interpolation::default());
    let k3 = splines::Key::new(TOAST_TIME * 0.875, 80., Interpolation::default());
    let k4 = splines::Key::new(TOAST_TIME, 0., Interpolation::default());
    let spline = Spline::from_vec(vec![k1, k2, k3, k4]);

    let mut maybe_img_location = matches.value_of("INPUT").map(|arg| PathBuf::from(arg));

    let (texture_sender, texture_receiver): (
        Sender<image_crate::RgbaImage>,
        Receiver<image_crate::RgbaImage>,
    ) = mpsc::channel();

    let (toast_sender, toast_receiver): (Sender<String>, Receiver<String>) = mpsc::channel();

    let player = Player::new(texture_sender.clone());

    info!("Image is: {:?}", maybe_img_location);

    #[cfg(target_os = "macos")]
    if !matches.is_present("chainload") && maybe_img_location.is_none() {
        info!("Chainload not specified, and no input file present. Invoking mac hack.");
        // MacOS needs an incredible dance performed just to open a file
        let _ = mac::launch();
    }

    if let Some(ref img_location) = maybe_img_location {
        if img_location.extension() == Some(&std::ffi::OsString::from("gif")) {
            player.load(&img_location);
        } else {
            player.load_blocking(&img_location);
        }
    }

    let opengl = OpenGL::V3_2;

    let ws = WindowSettings::new("Oculante", [1000, 800])
        .graphics_api(opengl)
        .fullscreen(false)
        .vsync(true)
        .exit_on_esc(true);

    let mut window: PistonWindow = ws.build().unwrap();
    set_title(&mut window, "No image");

    let scale_factor = 1.0;
    // #[cfg(target_os = "macos")]
    // let scale_factor = window.draw_size().width / window.size().width;

    // Set inspection-friendly magnification filter
    let mut tx_settings = TextureSettings::new();
    tx_settings.set_mag(Filter::Nearest);
    tx_settings.set_min(Filter::Linear);

    // These should all be a nice config struct...
    let mut current_image = image_crate::DynamicImage::new_rgba8(8, 8).to_rgba8();
    let mut texture = Texture::empty(&mut window.create_texture_context());

    let mut glyphs_regular = Glyphs::from_bytes(
        font_regular,
        window.create_texture_context(),
        TextureSettings::new(),
    )
    .unwrap();

    let mut text_draw_list: Vec<TextInstruction> = vec![];

    if let Some(img_location) = maybe_img_location.as_ref() {
        if img_location.is_file() {
            state.message = "Loading...".to_string();
            set_title(&mut window, &img_location.to_string_lossy().to_string());
        }
    }

    if let Some(port) = matches.value_of("l") {
        match port.parse::<i32>() {
            Ok(p) => {
                state.message = format!("Listening on {}", p);
                recv(p, texture_sender);
            }
            Err(_) => eprintln!("Port must be a number"),
        }
    }

    let _ = toast_sender
        .clone()
        .send("Press 'h' to toggle help!".to_string());

    // Event loop
    while let Some(e) = window.next() {
        // check if a new texture has been sent
        if let Ok(img) = texture_receiver.try_recv() {
            window.set_lazy(false);

            texture = Texture::from_image(&mut window.create_texture_context(), &img, &tx_settings);
            current_image = img;

            let draw_size = window.size();

            let window_size = Vector2::new(draw_size.width, draw_size.height);
            let img_size =
                Vector2::new(current_image.width() as f64, current_image.height() as f64);
            state.offset = window_size / 2.0 - img_size / 2.0;
            state.reset_image = true;
            state.is_loaded = true;
        }

        // Receive a dragged file
        if let Event::Input(Input::FileDrag(FileDrag::Drop(p)), None) = &e {
            window.set_lazy(false);
            state.message = "Loading...".to_string();
            state.is_loaded = false;
            player.load(&p);
            set_title(&mut window, &p.to_string_lossy().to_string());
        }

        // Initiate a pan operation
        if let Some(Button::Mouse(_)) = e.press_args() {
            state.drag_enabled = true;
            state.cursor_relative = pos_from_coord(
                state.offset,
                state.cursor,
                Vector2::new(
                    state.image_dimension.0 as f64,
                    state.image_dimension.1 as f64,
                ),
                state.scale,
            );
            // state.sampled_color = current_image.get_pixel(state.cursor_relative.x as u32, state.cursor_relative.y as u32).channels4();
        }

        //handle_events(&mut state, &e);

        if let Some(Button::Mouse(_)) = e.release_args() {
            state.drag_enabled = false;
        }

        if let Some(Button::Keyboard(key)) = e.press_args() {
            if key == Key::V {
                state.reset_image = true;
            }

            // Quit
            if key == Key::Q {
                std::process::exit(0);
            }

            // Set state.fullscreen_enabled
            if key == Key::F {
                if !state.fullscreen_enabled {
                    window.set_size([1920, 1080]);
                    window = ws.clone().fullscreen(true).build().unwrap();
                } else {
                    window = ws.clone().fullscreen(false).build().unwrap();
                }

                // state.reset_image = true;
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &current_image,
                    &tx_settings,
                );

                glyphs_regular = Glyphs::from_bytes(
                    font_regular,
                    window.create_texture_context(),
                    TextureSettings::new(),
                )
                .unwrap();

                state.fullscreen_enabled = !state.fullscreen_enabled;
                // pause so we don't enter a fullscreen loop - otherwise some OSes crash
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            // Display color unpremultiplied (just rgb without multiplying by alpha)
            if key == Key::U {
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &unpremult(&current_image),
                    &tx_settings,
                );
            }

            // Only red
            if key == Key::R {
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &solo_channel(&current_image, 0),
                    &tx_settings,
                );
            }

            // Only green
            if key == Key::G {
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &solo_channel(&current_image, 1),
                    &tx_settings,
                );
            }

            // Only blue
            if key == Key::B {
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &solo_channel(&current_image, 2),
                    &tx_settings,
                );
            }

            // Only alpha
            if key == Key::A {
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &solo_channel(&current_image, 3),
                    &tx_settings,
                );
            }

            // Color channel (RGB)
            if key == Key::C {
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &current_image,
                    &tx_settings,
                );
            }

            // Toggle extended info
            if key == Key::I {
                state.info_enabled = !state.info_enabled;
            }

            // Toggle extended info
            if key == Key::D1 {
                state.scale = window.size().width / window.draw_size().width;
                let window_size = Vector2::new(window.size().width, window.size().height);
                let img_size =
                    Vector2::new(current_image.width() as f64, current_image.height() as f64);
                state.offset = window_size / 2.0 - (img_size * state.scale) / 2.0;
            }

            // Toggle tooltip
            if key == Key::H {
                state.tooltip = !state.tooltip;
            }

            // Next image
            if key == Key::Right {
                if let Some(img_location) = maybe_img_location.as_mut() {
                    let next_img = img_shift(&img_location, 1);
                    // prevent reload if at last or first
                    if &next_img != img_location {
                        state.reset_image = true;
                        window.set_lazy(false);
                        state.is_loaded = false;
                        *img_location = next_img;
                        player.load(&img_location);
                        set_title(&mut window, &img_location.to_string_lossy().to_string());
                    }
                }
            }

            // Prev image
            if key == Key::Left {
                if let Some(img_location) = maybe_img_location.as_mut() {
                    let next_img = img_shift(&img_location, -1);
                    // prevent reload if at last or first
                    if &next_img != img_location {
                        state.reset_image = true;
                        window.set_lazy(false);
                        state.is_loaded = false;
                        *img_location = next_img;
                        player.load(&img_location);
                        set_title(&mut window, &img_location.to_string_lossy().to_string());
                    }
                }
            }

            if key == Key::Comma {
                update::update(toast_sender.clone());
            }
        };

        // TODO: rate-limit zoom (for trackpads, as they fire zoom events really fast)
        // TODO: clamp cursor position to image bounds for zoom
        e.mouse_scroll(|d| {
            // Map zoom nicely so it does not feel awkward whan zoomed out/in
            let delta = zoomratio(d[1], state.scale);
            // prevent negative / small zoom
            if delta + state.scale < 0.1 {
                return;
            }
            // make sure we zoom to the mouse cursor
            state.offset -= scale_pt(state.offset, state.cursor, state.scale, delta);
            state.scale += delta;
        });

        e.mouse_relative(|d| {
            if state.drag_enabled {
                state.offset += Vector2::new(d[0] / scale_factor, d[1] / scale_factor);
            }
        });

        e.mouse_cursor(|d| {
            state.cursor = Vector2::new(d[0] / scale_factor, d[1] / scale_factor);
            state.cursor_relative = pos_from_coord(
                state.offset,
                state.cursor,
                Vector2::new(
                    state.image_dimension.0 as f64,
                    state.image_dimension.1 as f64,
                ),
                state.scale,
            );
            if state.cursor_relative.x as u32 <= current_image.width()
                && state.cursor_relative.y as u32 <= current_image.height()
                && state.info_enabled
            {
                let p = current_image
                    .get_pixel(
                        state.cursor_relative.x as u32,
                        state.cursor_relative.y as u32,
                    )
                    .channels4();
                state.sampled_color = [p.0 as f32, p.1 as f32, p.2 as f32, p.3 as f32];
            }
        });

        // e.resize(|args| {
        //     println!("Resized '{}, {}'", args.window_size[0], args.window_size[1])
        // });

        let size = window.size();

        window.draw_2d(&e, |c, gfx, device| {
            clear([0.2; 4], gfx);
            
            
            if state.reset_image {
                let window_size = Vector2::new(size.width, size.height);
                let img_size =
                    Vector2::new(current_image.width() as f64, current_image.height() as f64);
                let scale_factor = (window_size.x / img_size.x).min(window_size.y / img_size.y).min(1.0);
                state.scale = scale_factor;
                state.offset = Vector2::new(0.0, 0.0);
                state.offset += window_size / 2.0 - (img_size * state.scale) / 2.0;
                state.reset_image = false;
            }

            let transform = c
                .transform
                .trans(state.offset.x as f64, state.offset.y as f64)
                .zoom(state.scale);

            // draw the image
            if let Ok(tex) = &texture {
                image(tex, transform, gfx);
                state.image_dimension = tex.get_size();
            }

            let default_path = PathBuf::from("No image");
            let filename = maybe_img_location.as_ref().unwrap_or(&default_path).file_name().unwrap_or_default().to_string_lossy();
            let info = format!(
                "{} {}X{} @{}X",
                filename,
                state.image_dimension.0,
                state.image_dimension.1,
                (state.scale * 10.0).round() / 10.0
            );

            if state.info_enabled {
                text_draw_list.push(TextInstruction::new(
                    &info,
                    c.transform.trans(10.0 as f64, 20.0 as f64),
                ));

            }

            if !state.is_loaded {
                text_draw_list.push(TextInstruction::new_size(
                    &state.message,
                    c.transform
                        .trans(size.width / 2.0 - 120.0, size.height / 2.0),
                    state.font_size * 2,
                ));
            }
  

            if state.tooltip {
                text_draw_list.push(TextInstruction::new(
                    "Press i to toggle info, r,g,b,a to toggle channels, c for all channels",
                    c.transform
                        .trans(50., size.height - 80.),
                ));

                text_draw_list.push(TextInstruction::new(
                    "Press u for unpremultiplied alpha, v to reset view, 1 for 100% zoom, <- -> to view next/prev image",
                    c.transform
                        .trans(50., size.height - 60.),
                ));

                text_draw_list.push(TextInstruction::new(
                    "Press ',' (comma) to update from latest github release",
                    c.transform
                        .trans(50., size.height - 40.),
                ));
            }


            if state.info_enabled {
                let col_inv = invert_rgb_8bit(state.sampled_color);

                // draw the zoomed image
                if let Ok(tex) = &texture {
                    let rect_size = 128.0;
                    let cur = state.cursor;
                    let mut cur_relative = state.cursor_relative;
                    // Snap relative mouse position so we see the exact pixel position
                    cur_relative.x = cur_relative.x.floor();
                    cur_relative.y = cur_relative.y.floor();

                    let cropped_res = 16.0;
                    let image = Image::new()
                        .src_rect([
                            cur_relative.x - cropped_res / 2.,
                            cur_relative.y - cropped_res / 2.,
                            cropped_res,
                            cropped_res,
                        ])
                        .rect([0.0, 0.0, rect_size, rect_size]);
                    let t_cursor = c.transform.trans(cur.x, cur.y).zoom(1.0);

                    // Draw the picker window
                    image.draw(tex, &draw_state::DrawState::default(), t_cursor, gfx);

                    let t_rect_center = c
                        .transform
                        .trans(cur.x + rect_size / 2., cur.y + rect_size / 2.)
                        .zoom(1.0);

                    // A small rect covering the picked pixel
                    let pixel_rect = Rectangle::new(col_inv);
                    // A frame over the magnified texture
                    let frame = Rectangle::new_border([0.0, 0.0, 0.0, 0.5], 1.);
                    frame.draw(
                        [0.0, 0.0, rect_size, rect_size],
                        &draw_state::DrawState::default(),
                        t_cursor,
                        gfx,
                    );
                    pixel_rect.draw(
                        [0.0, 0.0, cropped_res / 2., cropped_res / 2.],
                        &draw_state::DrawState::default(),
                        t_rect_center,
                        gfx,
                    );
                }

                text_draw_list.push(TextInstruction::new(
                    &format!(
                        "P {},{} / {},{}",
                        state.cursor_relative[0].floor() as i32 + 1,
                        state.image_dimension.1 as i32 - (state.cursor_relative[1].floor() as i32),
                        state.cursor_relative[0].floor() as i32 + 1,
                        state.cursor_relative[1].floor() as i32 + 1,
                    ),
                    c.transform.trans(state.cursor.x, state.cursor.y - 4.),
                ));

                text_draw_list.push(TextInstruction::new(
                    &format!(
                        "C {} / {}",
                        disp_col(state.sampled_color),
                        disp_col_norm(state.sampled_color, 255.0),
                    ),
                    c.transform.trans(
                        state.cursor.x,
                        state.cursor.y - state.font_size as f64 * 1.5,
                    ),
                ));
            }


            // The toast system
            {
                if let Ok(toast) = toast_receiver.try_recv() {
                    toast_time = std::time::Instant::now();
                    state.toast = toast;
                }
                if state.toast != "" {
                    let elapsed = toast_time.elapsed().as_secs_f64();

                    if elapsed < TOAST_TIME {
                        let text_rect = Rectangle::new([0.,0.,0.,0.7]);
                        text_rect.draw(
                            [0., 0., size.width, -100.],
                            &draw_state::DrawState::default(),
                            c.transform.trans(0., spline.clamped_sample(elapsed).unwrap() + 10.),
                            gfx,
                        );
    
                        let text_size = text::Text::new( 14).width(&state.toast,&mut glyphs_regular);

                        text_draw_list.push(TextInstruction::new(
                            &state.toast,
                    c.transform.trans(size.width/2. - text_size.0/2., spline.clamped_sample(elapsed).unwrap()),
                        ));
                    }
                }
            }


            

            // Draw all text
            for t in &text_draw_list {

                let text_rect = Rectangle::new([0.,0.,0., 0.5*t.color[3]]);
                // A frame over the magnified texture
          
                let x = text::Text::new( t.size).width(&t.text,&mut glyphs_regular);
                
                let margin = 5.;
                text_rect.draw(
                    [-margin, margin, x.0 + margin*2., -(x.1 + margin)],
                    &draw_state::DrawState::default(),
                    t.position,
                    gfx,
                );

                text::Text::new_color(t.color, t.size)
                    .draw(&t.text, &mut glyphs_regular, &c.draw_state, t.position, gfx)
                    .unwrap();
                }
            text_draw_list.clear();

            // rectrender(&c.draw_state, gfx);

    


            glyphs_regular.factory.encoder.flush(device);
        });

        // if let Ok(state_msg) = state_receiver.try_recv() {
        //     // an image has been received
        //     // window.set_lazy(false);
        //     state.is_loaded = true;

        //     if state_msg != "ANIM_FRAME" {
        //         state.reset_image = true;
        //         window.set_lazy(true);
        //     } else {
        //     }

        // }
    }
}
