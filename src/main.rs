#![windows_subsystem = "windows"]

use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};

use std::path::{PathBuf};
use ::image as image_crate;
use image_crate::{Pixel, ImageDecoder};

use piston_window::*;
// use Event::Input;
mod utils;
use utils::{scale_pt, pos_from_coord, open_image, is_ext_compatible, solo_channel, unpremult};
use clap;
use clap::{App, Arg};
use nalgebra::Vector2;




fn img_shift(file: &PathBuf, inc: i8) -> PathBuf {
    if let Some(parent) = file.parent() {
        let mut files = std::fs::read_dir(parent)
        .unwrap()
        .map(|x| x.unwrap().path().to_path_buf())
        .filter(|x| is_ext_compatible(x))
        .collect::<Vec<PathBuf>>()
        ;
        files.sort();
        for (i, f) in files.iter().enumerate() {
            if f == file {
                if let Some(next) = files.get( (i as i8 + inc) as usize) {
                    return next.clone();
                }
            }
        }
    }
    file.clone()

}




fn main() {
    
    let font = include_bytes!("IBMPlexSans-Regular.ttf");
    // let loading_img = include_bytes!("loading.png");

    let matches = App::new("Oculante")
        .arg(
            Arg::with_name("INPUT")
                .help("Display this image")
                // .required(true)
                .index(1),
        )
        .get_matches();

    let img_path = matches.value_of("INPUT").unwrap_or_default().to_string();

    let opengl = OpenGL::V3_2;

    let mut window: PistonWindow = WindowSettings::new("Oculante", [1000, 800])
        .exit_on_esc(true)
        .graphics_api(opengl)
        // .fullscreen(true)
        .build()
        .unwrap();

    let (texture_sender, texture_receiver): (
        Sender<image_crate::RgbaImage>,
        Receiver<image_crate::RgbaImage>,
    ) = mpsc::channel();

    let (state_sender, state_receiver): (
        Sender<String>,
        Receiver<String>,
    ) = mpsc::channel();


    let mut tx_settings = TextureSettings::new();
    tx_settings.set_mag(Filter::Nearest);
    // tx_settings.set_min(Filter::Nearest);

    let mut offset = Vector2::new(0.0, 0.0);
    let mut cursor = Vector2::new(0.0, 0.0);
    let mut cursor_in_image = Vector2::new(0.0, 0.0);
    let mut scale = 1.0;
    let mut drag = false;
    let scale_increment = 0.1;
    let mut reset = false;
    let mut message = "Drag image here".to_string();
    let mut dimensions = (0, 0);
    let mut current_image = image_crate::DynamicImage::new_rgba8(1, 1).to_rgba(); //TODO: make this shorter
    let mut current_color = (0, 0, 0, 0);
    let mut texture = Texture::empty(&mut window.create_texture_context());
    let mut glyphs = Glyphs::from_bytes(
        font,
        window.create_texture_context(),
        TextureSettings::new(),
    )
    .unwrap();
    let mut loaded = false;

    let mut img_location = PathBuf::from(&img_path);
    open_image(&img_location, texture_sender.clone(), state_sender.clone());
    window.set_max_fps(45);

    if img_location.is_file() {
        message = "Loading...".to_string();
    }

    while let Some(e) = window.next() {

        // a new texture has been sent
        if let Ok(img) = texture_receiver.try_recv() {
            texture = Texture::from_image(
                &mut window.create_texture_context(),
                &img,
                &tx_settings,
            );
            current_image = img;
            
            let window_size = Vector2::new(window.size().width, window.size().height);
            let img_size = Vector2::new(current_image.width() as f64, current_image.height() as f64);
            offset = window_size/2.0 - img_size/2.0;
            loaded = true;

        }

 


        if let Event::Input(Input::FileDrag(FileDrag::Drop(p)), None) = &e {
            window.set_lazy(false);
            message = "Loading...".to_string();
            loaded = false;
            img_location = p.clone();
            open_image(&img_location, texture_sender.clone(), state_sender.clone());
        }
        
        


        if let Some(Button::Mouse(_)) = e.press_args() {
            drag = true;
            cursor_in_image = pos_from_coord(offset, cursor, Vector2::new(dimensions.0 as f64, dimensions.1 as f64), scale);
            current_color = current_image.get_pixel(cursor_in_image.x as u32, cursor_in_image.y as u32).channels4();            
        }

        if let Some(Button::Mouse(_)) = e.release_args() {
            drag = false;
        }

        if let Some(Button::Keyboard(key)) = e.press_args() {
            if key == Key::V {
                reset = true;
            }

            if key == Key::Q {
                std::process::exit(0);
            }

            if key == Key::F {
                //TODO: Fullscreen
                // window.window.;
                // std::process::exit(0);
            }

            if key == Key::U {
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &unpremult(&current_image),
                    &tx_settings,
                );
            }
            
            if key == Key::R {
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &solo_channel(&current_image, 0),
                    &tx_settings,
                );
            }

            if key == Key::G {
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &solo_channel(&current_image, 1),
                    &tx_settings,
                );
            }

            if key == Key::B {
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &solo_channel(&current_image, 2),
                    &tx_settings,
                );
            }
            if key == Key::A {
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &solo_channel(&current_image, 3),
                    &tx_settings,
                );
            }
            if key == Key::C {
                texture = Texture::from_image(
                    &mut window.create_texture_context(),
                    &current_image,
                    &tx_settings,
                );
            }

            if key == Key::Right {
                window.set_lazy(false);
                loaded = false;
                img_location = img_shift(&img_location, 1);
                open_image(&img_location, texture_sender.clone(), state_sender.clone());
            }

            if key == Key::Left {
                window.set_lazy(false);
                loaded = false;
                img_location = img_shift(&img_location, -1);
                open_image(&img_location, texture_sender.clone(), state_sender.clone());
            }
        };

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

        // e.file_drag();

        e.mouse_cursor(|d| {
            cursor = Vector2::new(d[0], d[1]);
            cursor_in_image = pos_from_coord(offset, cursor, Vector2::new(dimensions.0 as f64, dimensions.1 as f64), scale);
        });

        // e.resize(|args| {
        //     println!("Resized '{}, {}'", args.window_size[0], args.window_size[1])
        // });



        let size = window.size();

        window.draw_2d(&e, |c, gfx, device| {
            clear([0.2; 4], gfx);

            if reset {
                let window_size = Vector2::new(size.width, size.height);
                let img_size = Vector2::new(current_image.width() as f64, current_image.height() as f64);
                let scale_factor = (window_size.x/img_size.x).min(1.0);
                dbg!(scale_factor);
                scale = scale_factor;
                offset = Vector2::new(0.0, 0.0);
                offset += window_size/2.0 - (img_size*scale)/2.0;
                reset = false;
            }

            let transform = c.
                transform
                .trans(offset.x as f64, offset.y as f64)
                .zoom(scale);

                
            // draw the image
            if let Ok(tex) = &texture {
                image(tex, transform, gfx);
                dimensions = tex.get_size();
            }


            let info = format!("{} {}X{} rgba {} {} {} {} / {:.2} {:.2} {:.2} {:.2} {:.2}x{:.2} @{}X",
                &img_location.to_string_lossy(),
                dimensions.0,
                dimensions.1,
                current_color.0,
                current_color.1,
                current_color.2,
                current_color.3,
                current_color.0 as f32 / 255.0,
                current_color.1 as f32 / 255.0,
                current_color.2 as f32 / 255.0,
                current_color.3 as f32 / 255.0,
                cursor_in_image[0].round() as i32,
                cursor_in_image[1].round() as i32,
                (scale * 10.0).round() / 10.0
            );

            // Draw text three times to simulate outline

            // fn draw_txt(pos: (f64, f64), size: u32, text: &String, cache: GlyphCache<TextureContext<Factory, Resources, CommandBuffer>, Texture<Resources>>) {

            //     text::Text::new_color([1.0, 1.0, 1.0, 0.7], 18)
            //     .draw(
            //         &text,
            //         &mut glyphs,
            //         &c.draw_state,
            //         c.transform.trans(10.0, 20.0),
            //         gfx,
            //     )
            //     .unwrap();

            // }


            // fn render_text(x: f64, y: f64,
            //     text: &str, size: u32,
            //     c: Context, g: &mut G2d, 
            //     g: &mut glyph_cache::rusttype::GlyphCache<GfxFactory, G2dTexture>) {
            // text::Text::new(size).draw(
            //     text,
            //     g,
            //     &c.draw_state,
            //     c.transform.trans(x, y),
            //     g
            // ).unwrap();
            // } 


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


                if ! loaded {
                    text::Text::new_color([1.0, 1.0, 1.0, 0.7], 36)
                    .draw(
                        &message,
                        &mut glyphs,
                        &c.draw_state,
                        c.transform.trans(size.width/2.0-120.0, size.height/2.0),
                        gfx,
                    )
                    .unwrap();
                }
            glyphs.factory.encoder.flush(device);
            
        });

        if let Ok(_) = state_receiver.try_recv() {
            // an image has been received
            reset = true;
            // loaded = true;
            window.set_lazy(true);
            
        }


    }
}


