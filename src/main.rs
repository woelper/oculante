#![windows_subsystem = "windows"]
#![feature(test)]
// #![feature(core_intrinsics)]

use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use ::image as image_crate;
use image_crate::Pixel;
use std::path::PathBuf;
use piston_window::*;

mod utils;
use utils::*;
mod net;
use clap::{App, Arg};
use nalgebra::Vector2;
use net::*;
extern crate graphics;


#[cfg(test)]
mod tests;


fn main() {


    let mut state = OculanteState::default();

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
        .get_matches();

    let font = include_bytes!("IBMPlexSans-Regular.ttf");
    let img_path = matches.value_of("INPUT").unwrap_or_default().to_string();
    let mut img_location = PathBuf::from(&img_path);
    let (texture_sender, texture_receiver): (
        Sender<image_crate::RgbaImage>,
        Receiver<image_crate::RgbaImage>,
    ) = mpsc::channel();

    let player = Player::new(texture_sender.clone());


    // let (state_sender, state_receiver): (Sender<String>, Receiver<String>) = mpsc::channel();
    //let mut timer = std::time::Instant::now();
    // send_image_threaded(&img_location, texture_sender.clone(), state_sender.clone());
    if img_location.extension() == Some(&std::ffi::OsString::from("gif")) {
        player.load(&img_location);
        
    } else {
        player.load_blocking(&img_location);

    }

    let opengl = OpenGL::V3_2;

    let ws = WindowSettings::new("Oculante", [1000, 800])
        .graphics_api(opengl)
        .fullscreen(false)
        .vsync(true)
        .exit_on_esc(true);

    // let mut window: PistonWindow<Sdl2Window> = WindowSettings::new("Oculante", [1000, 800])
    // let mut window: PistonWindow<WinitWindow> = ws.build().unwrap();
    // use glfw_window::GlfwWindow;
    // let mut window: PistonWindow<GlfwWindow> = ws.build().unwrap();
    let mut window: PistonWindow = ws.build().unwrap();
    
    // use winit;
    // // winit::
    // use winit::EventsLoop;
    // use winit::{ControlFlow, WindowEvent};
    // let mut events_loop = EventsLoop::new();
    // let mut w = winit::Window::new(&events_loop).unwrap();
    // dbg!(w.get_hidpi_factor());
    // let dim = w.get_current_monitor().get_dimensions();
    // // dbg!(s);
    // // events_loop.
    // events_loop.poll_events(|event| {});
    // ControlFlow::Break;


    // Set inspection-friendly magnification filter
    let mut tx_settings = TextureSettings::new();
    tx_settings.set_mag(Filter::Nearest);

    // These should all be a nice config struct...
    let mut current_image = image_crate::DynamicImage::new_rgba8(8, 8).to_rgba();
    let mut texture = Texture::empty(&mut window.create_texture_context());

    let mut glyphs = Glyphs::from_bytes(
        font,
        window.create_texture_context(),
        TextureSettings::new(),
    )
    .unwrap();
    
// fn print_type_of<T>(_: &T) {
//     println!("{}", unsafe { std::intrinsics::type_name::<T>() });
// }
//     print_type_of(&glyphs);

    if img_location.is_file() {
        state.message = "Loading...".to_string();
    }

    if let Some(port) = matches.value_of("l") {
        match port.parse::<i32>() {
            Ok(p) => {
                state.message = format!("Listening on {}", p);
                recv(p, texture_sender);
            }
            Err(_) => println!("Port must be a number"),
        }
    }

 


    while let Some(e) = window.next() {
        // a new texture has been sent
        if let Ok(img) = texture_receiver.try_recv() {
            window.set_lazy(false);

            // dbg!(timer.elapsed());
            texture = Texture::from_image(&mut window.create_texture_context(), &img, &tx_settings);
            current_image = img;

            let window_size = Vector2::new(window.size().width, window.size().height);
            let img_size =
                Vector2::new(current_image.width() as f64, current_image.height() as f64);
            state.offset = window_size / 2.0 - img_size / 2.0;
            state.is_loaded = true;
        }

    

        // Receive a dragged file
        if let Event::Input(Input::FileDrag(FileDrag::Drop(p)), None) = &e {
            window.set_lazy(false);
            state.message = "Loading...".to_string();
            state.is_loaded = false;
            img_location = p.clone();
            player.load(&img_location);
            // send_image_threaded(&img_location, texture_sender.clone(), state_sender.clone());
        }

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

                // let d = window.
                // dbg!(d);

                // state.reset_image = true;
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &current_image,
                    &tx_settings,
                );
                glyphs = Glyphs::from_bytes(
                    font,
                    window.create_texture_context(),
                    TextureSettings::new(),
                )
                .unwrap();
                state.fullscreen_enabled = !state.fullscreen_enabled;
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

            if key == Key::Right {
                state.reset_image = true;
                window.set_lazy(false);
                state.is_loaded = false;
                img_location = img_shift(&img_location, 1);
                player.load(&img_location);
                // send_image_threaded(&img_location, texture_sender.clone(), state_sender.clone());
            }

            if key == Key::Left {
                state.reset_image = true;
                window.set_lazy(false);
                state.is_loaded = false;
                img_location = img_shift(&img_location, -1);
                player.load(&img_location);
                // send_image_threaded(&img_location, texture_sender.clone(), state_sender.clone());
            }
        };

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
                state.offset += Vector2::new(d[0], d[1]);
            }
        });

        e.mouse_cursor(|d| {
            state.cursor = Vector2::new(d[0], d[1]);
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
                let scale_factor = (window_size.x / img_size.x).min(1.0);
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

            let info = format!(
                "{} {}X{} @{}X",
                &img_location.to_string_lossy(),
                state.image_dimension.0,
                state.image_dimension.1,
                (state.scale * 10.0).round() / 10.0
            );

            // Draw text three times to simulate outline


            

            for i in &[(-2, -2), (-2, -0), (0, -2), (2, 2), (2, 0)] {
                text::Text::new_color([0.0, 0.0, 0.0, 1.0], state.font_size)
                    .draw(
                        &info,
                        &mut glyphs,
                        &c.draw_state,
                        c.transform.trans(10.0 + i.0 as f64, 20.0 + i.1 as f64),
                        gfx,
                    )
                    .unwrap();
            }
            text::Text::new_color([1.0, 1.0, 1.0, 0.7], state.font_size)
                .draw(
                    &info,
                    &mut glyphs,
                    &c.draw_state,
                    c.transform.trans(10.0, 20.0),
                    gfx,
                )
                .unwrap();

            if !state.is_loaded {
                text::Text::new_color([1.0, 1.0, 1.0, 0.7], state.font_size * 2)
                    .draw(
                        &state.message,
                        &mut glyphs,
                        &c.draw_state,
                        c.transform
                            .trans(size.width / 2.0 - 120.0, size.height / 2.0),
                        gfx,
                    )
                    .unwrap();
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
                    let pixel_rect = Rectangle::new(col_inv);
                    let frame = Rectangle::new_border([0.0, 0.0, 0.0, 0.5], 2.);
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



                text::Text::new_color(col_inv, state.font_size)
                    .draw(
                        &format!(
                            "P {},{} / {},{}",
                            state.cursor_relative[0].floor() as i32 + 1,
                            state.image_dimension.1 as i32
                                - (state.cursor_relative[1].floor() as i32),
                            state.cursor_relative[0].floor() as i32 + 1,
                            state.cursor_relative[1].floor() as i32 + 1,
                        ),
                        &mut glyphs,
                        &c.draw_state,
                        c.transform.trans(state.cursor.x, state.cursor.y - 4.),
                        gfx,
                    )
                    .unwrap();

                text::Text::new_color(col_inv, state.font_size)
                    .draw(
                        &format!(
                            "C {} / {}",
                            disp_col(state.sampled_color),
                            disp_col_norm(state.sampled_color, 255.0),
                        ),
                        &mut glyphs,
                        &c.draw_state,
                        c.transform.trans(
                            state.cursor.x,
                            state.cursor.y - state.font_size as f64 * 1.05,
                        ),
                        gfx,
                    )
                    .unwrap();
            }
            glyphs.factory.encoder.flush(device);
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
