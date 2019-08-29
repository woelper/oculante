//#![windows_subsystem = "windows"]

mod utils;
use clap;
use clap::{App, Arg};
use nalgebra::Vector2;

use piston_window::*;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
extern crate image;
use crate::image::GenericImageView;
use crate::image::Pixel;
use utils::{scale_pt, pos_from_coord};

fn main() {
    let font = include_bytes!("FiraSans-Regular.ttf");
    let matches = App::new("Oculante")
        .arg(
            Arg::with_name("INPUT")
                .help("Display this image")
                .required(true)
                .index(1),
        )
        .get_matches();

    let img_path = matches.value_of("INPUT").unwrap().to_string();

    let opengl = OpenGL::V3_2;
    let mut window: PistonWindow = WindowSettings::new("Oculante", [1000, 800])
        .exit_on_esc(true)
        .graphics_api(opengl)
        // .samples(4)
        .build()
        .unwrap();

    let (texture_sender, texture_receiver): (
        Sender<image::DynamicImage>,
        Receiver<image::DynamicImage>,
    ) = mpsc::channel();

    let mut tx_settings = TextureSettings::new();
    tx_settings.set_mag(Filter::Nearest);
    // tx_settings.set_min(Filter::Nearest);

    // window.set_lazy(true);
    let mut offset = Vector2::new(0.0, 0.0);
    let mut cursor = Vector2::new(0.0, 0.0);
    let mut scale = 1.0;
    let mut drag = false;
    let scale_increment = 0.2;
    let mut reset = false;
    let mut dimensions = (0, 0);
    let mut current_image = image::DynamicImage::new_rgba8(1, 1);
    let mut current_color = (0, 0, 0, 0);

    let mut texture = Texture::empty(&mut window.create_texture_context());

    let mut glyphs = Glyphs::from_bytes(
        font,
        window.create_texture_context(),
        TextureSettings::new(),
    )
    .unwrap();

 

    let i = img_path.clone();

    thread::spawn(move || match image::open(i) {
        Ok(img) => texture_sender.send(img).unwrap(),
        Err(e) => println!("ERR {:?}", e),
    });

    while let Some(e) = window.next() {
        if let Some(Button::Mouse(_)) = e.press_args() {
            drag = true;
            let pos = pos_from_coord(offset, cursor, Vector2::new(dimensions.0 as f64, dimensions.1 as f64), scale);
            // dbg!(pos);

            current_color = current_image.get_pixel(pos.x as u32, pos.y as u32).channels4();


            
            // println!("Cursor {:?} OFFSET {:?}", cursor, scale_pt(offset, cursor, scale, scale_increment));
        }
        if let Some(Button::Mouse(_)) = e.release_args() {
            drag = false;
        }

        if let Some(Button::Keyboard(key)) = e.press_args() {
            if key == Key::R {
                reset = true;
            }
        };
        // if let Some(Button::Keyboard(key)) = e.press_args() {
        //     if key == Key::P {
        //         offset -= scale_pt(offset, cursor, scale, scale_increment);
        //         scale += scale_increment;
        //         }
        // };

        e.mouse_scroll(|d| {
            if d[1] > 0.0 {
                offset -= scale_pt(offset, cursor, scale, scale_increment);
                scale += scale_increment;
            } else {
                if scale > scale_increment + 0.01 {
                    offset += scale_pt(offset, cursor, scale, scale_increment);
                    scale -= scale_increment;
                }
            }
        });
        e.mouse_relative(|d| {
            if drag {
                offset += Vector2::new(d[0], d[1]);
            }
        });

        e.mouse_cursor(|d| {
            cursor = Vector2::new(d[0], d[1]);
        });

        // e.resize(|args| {
        //     println!("Resized '{}, {}'", args.window_size[0], args.window_size[1])
        // });

        if let Ok(img) = texture_receiver.try_recv() {
            println!("received image data from loader");

            texture = Texture::from_image(
                &mut window.create_texture_context(),
                &img.to_rgba(),
                &tx_settings,
            );
            current_image = img;
        }

        window.draw_2d(&e, |c, gfx, device| {
            clear([0.2; 4], gfx);
            if reset {
                offset = Vector2::new(0.0, 0.0);
                scale = 1.0;
                reset = false;
            }
            let transform = c
                .transform
                .trans(offset.x as f64, offset.y as f64)
                .zoom(scale);

            if let Ok(tex) = &texture {
                image(tex, transform, gfx);
                dimensions = tex.get_size();
            }


            let info = format!("{} {}X{} R{} G{} B{} A{} @{}X", img_path, dimensions.0, dimensions.1, current_color.0, current_color.1, current_color.2, current_color.3, (scale * 10.0).round() / 10.0);

            // Draw text three times to simulate outline

            for i in vec![(-2,-2), (-2,-0), (0,-2), (2,2), (2,0)] {

                text::Text::new_color([0.0, 0.0, 0.0, 1.0], 18)
                    .draw(
                        &info,
                        &mut glyphs,
                        &c.draw_state,
                        c.transform.trans(10.0 + i.0 as f64, 20.0 + i.1 as f64),
                        gfx,
                    )
                    .unwrap();

            }
            text::Text::new_color([1.0, 1.0, 1.0, 0.7], 18)
                .draw(
                    &info,
                    &mut glyphs,
                    &c.draw_state,
                    c.transform.trans(10.0, 20.0),
                    gfx,
                )
                .unwrap();

            glyphs.factory.encoder.flush(device);

        });
    }
}
