//#![windows_subsystem = "windows"]

use clap;
use clap::{App, Arg};
use nalgebra::Vector2;
use piston_window::*;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
extern crate image;

//https://docs.piston.rs/piston_window/image/trait.GenericImageView.html#tymethod.get_pixel


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

    let mut texture = Texture::empty(&mut window.create_texture_context());

    let mut glyphs = Glyphs::from_bytes(
        font,
        window.create_texture_context(),
        TextureSettings::new(),
    )
    .unwrap();

    fn scale_pt(
        origin: Vector2<f64>,
        pt: Vector2<f64>,
        scale: f64,
        scale_inc: f64,
    ) -> Vector2<f64> {
        ((pt - origin) * scale_inc) / scale
    }

    let i = img_path.clone();
    
    
    thread::spawn(move || {
    println!("started thrread");

        match image::open(i) {
            Ok(img) => texture_sender.send(img).unwrap(),
            Err(e) => println!("ERR {:?}", e)
        }

        // Texture::from_path(
        //     &mut window.create_texture_context(),
        //     &img_path,
        //     Flip::None,
        //     &tx_settings,
        // )

    });

    while let Some(e) = window.next() {
        if let Some(Button::Mouse(_)) = e.press_args() {
            drag = true;
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

        if let Ok(tex) = texture_receiver.try_recv() {
            println!("received image data from loader");

            texture = Texture::from_image(&mut window.create_texture_context(), &tex.to_rgba(), &tx_settings);
            window.next();
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

            text::Text::new_color([0.8, 0.5, 0.8, 0.7], 16)
                .draw(
                    &format!("{} {}X{}", img_path, dimensions.0, dimensions.1),
                    &mut glyphs,
                    &c.draw_state,
                    c.transform.trans(10.0, 20.0),
                    gfx,
                )
                .unwrap();

            text::Text::new_color([0.8, 0.5, 0.8, 0.7], 16)
                .draw(
                    &format!("Scale {}", (scale * 10.0).round() / 10.0),
                    &mut glyphs,
                    &c.draw_state,
                    c.transform.trans(10.0, 50.0),
                    gfx,
                )
                .unwrap();

            glyphs.factory.encoder.flush(device);
        });
    }
}
