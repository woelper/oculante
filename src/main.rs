#![windows_subsystem = "windows"]


use ::image as image_crate;
use image_crate::{Pixel, ImageDecoder};
use std::time::{Duration, Instant};
mod utils;
use clap;
use clap::{App, Arg};
use nalgebra::Vector2;
use gif::*;
use gif_dispose;

extern crate exr;
use exr::prelude::*;

use piston_window::*;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
// use resvg::prelude::*;
use nsvg;

use std::io::BufReader;
use std::fs::File;
use std::path::{PathBuf};
// use std::cmp::Ordering;
use dds::DDS;
use rgb::*;
use psd::Psd;
use std::io::Read;
// use exr::prelude::*;
// use exr;
// use exr::image::full::*;
// use exr::math::Vec2;
use utils::{scale_pt, pos_from_coord};


fn is_ext_compatible(fname: &PathBuf) -> bool {
    match fname
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
        .to_lowercase()
        .as_str() {
        "png" => true,
        "exr" => true,
        "jpg" => true,
        "jpeg" => true,
        "psd" => true,
        "dds" => true,
        "gif" => true,
        "hdr" => true,
        "bmp" => true,
        "ico" => true,
        "tga" => true,
        "tiff" => true,
        "tif" => true,
        "webp" => true,
        "pnm" => true,
        "svg" => true,
        _ => false
    }
}


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


fn open_image(img_location: &PathBuf, texture_sender: Sender<image_crate::RgbaImage>) {
    let img_location = img_location.clone();
    thread::spawn(move || 
        {
    
            match img_location.extension().unwrap_or_default().to_str() {
                Some("dds") => {
                    let file = File::open(img_location).unwrap();
                    let mut reader = BufReader::new(file);
                    let dds = DDS::decode(&mut reader).unwrap();
                    if let Some(main_layer) = dds.layers.get(0) {
                        let buf = main_layer.as_bytes();
                        let buffer: image_crate::RgbaImage = image_crate::ImageBuffer::from_raw(dds.header.width, dds.header.height, buf.into()).unwrap();
                        let _ = texture_sender.send(buffer.clone());
                    }
                },
                Some("svg") => {


                    // Load and parse the svg
                    let svg = nsvg::parse_file(&img_location, nsvg::Units::Pixel, 96.0).unwrap();
                  
                    // Create a scaled raster
                    let scale = 3.0;
                    let image = svg.rasterize(scale).unwrap();
                    let dimensions = image.dimensions();
                    // This is just to convert between different crate versions of "image". TODO: remove if crates catch up
                    let raw = image.into_raw();
                    let buffer: image_crate::RgbaImage = image_crate::ImageBuffer::from_raw(dimensions.0, dimensions.1, raw).unwrap();

                    let _ = texture_sender.send(buffer);

 

                },
                Some("exr") => {

                    // let mut image = rgba::Image::read_from_file(&img_location, read_options::high()).unwrap();
                    // let buffer: image_crate::RgbaImage = image_crate::ImageBuffer::from_raw(image.resolution.0 as u32, image.resolution.1 as u32, image).unwrap();

                    // println!("loaded image {:#?}", image);
    
    
                },
                Some("psd") => {
                    let mut file = File::open(img_location).unwrap();
                    let mut contents = vec![];
                    if let Ok(_) = file.read_to_end(&mut contents){
                        let psd = Psd::from_bytes(&contents).unwrap();
                        let buffer: Option<image_crate::RgbaImage> = image_crate::ImageBuffer::from_raw(psd.width(), psd.height(), psd.rgba());
                        if let Some(b) = buffer {
                            let _ = texture_sender.send(b.clone());
                        }
                    }
                },
                Some("gif") => {
                    
                    let file = File::open(&img_location).unwrap();
                    let mut decoder = gif::Decoder::new(file);
                    decoder.set(gif::ColorOutput::Indexed);
                    let mut reader = decoder.read_info().unwrap();
                    
                    
                    let mut screen = gif_dispose::Screen::new_reader(&reader);
                    let dim = (screen.pixels.width() as u32, screen.pixels.height() as u32);
                    
                        
                    println!("GIF:");
                    while let Some(frame) = reader.read_next_frame().unwrap() {
                        screen.blit_frame(&frame).unwrap();
                        let buffer: Option<image_crate::RgbaImage> = image_crate::ImageBuffer::from_raw(dim.0, dim.1, screen.pixels.buf().as_bytes().to_vec());
                        texture_sender.send(buffer.unwrap()).unwrap();
                        std::thread::sleep_ms(150);

                    }
                    

                }
                _ => {
                    // println!("opening...");
                    match image_crate::open(img_location) {
                        Ok(img) => {
                            // println!("open. sending");
                            texture_sender.send(img.to_rgba()).unwrap();
                            },
                        Err(e) => println!("ERR {:?}", e),
                    }
                }
            }
        }
        );
}



fn draw_status (img: Vec<u8>, texture_sender: Sender<image_crate::RgbaImage>) {

    // match image_crate::png::PngReader::read_to_end(img)
    // let b =
    // let mut reader = BufReader::new(img);
    
    let png = image_crate::png::PngDecoder::new(&*img).unwrap();
    let mut b = vec![0; png.total_bytes() as usize];
    png.read_image(&mut b);

    let buffer: image_crate::RgbaImage = image_crate::ImageBuffer::from_raw(256, 256, b).unwrap();
    // dbg!(&buffer);
    let _ = texture_sender.send(buffer.clone());
    // if let Some(buffer) = png {
    //     let _ = texture_sender.send(b.clone());
    // }
}


fn main() {
    
    let font = include_bytes!("IBMPlexSans-Regular.ttf");
    let loading_img = include_bytes!("loading.png");

    let mut now = Instant::now();

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
        // .fullscreen(true)
        .build()
        .unwrap();

    let (texture_sender, texture_receiver): (
        Sender<image_crate::RgbaImage>,
        Receiver<image_crate::RgbaImage>,
    ) = mpsc::channel();

    let mut tx_settings = TextureSettings::new();
    tx_settings.set_mag(Filter::Nearest);
    // tx_settings.set_min(Filter::Nearest);

    let mut offset = Vector2::new(0.0, 0.0);
    let mut cursor = Vector2::new(0.0, 0.0);
    let mut scale = 1.0;
    let mut drag = false;
    let scale_increment = 0.2;
    let mut reset = false;
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


    let mut img_location = PathBuf::from(&img_path);

    draw_status(loading_img.to_vec(), texture_sender.clone());


    open_image(&img_location, texture_sender.clone());

    window.set_max_fps(30);
    while let Some(e) = window.next() {

        // dbg!(now.elapsed().as_secs());
        // if now.elapsed().as_secs() > 5 && now.elapsed().as_secs() < 7 {
        //     println!("old!");
        //     window.set_lazy(true);
        // }

        // a new texture has been sent
        if let Ok(img) = texture_receiver.try_recv() {
            // println!("received image data from loader");
            // window.set_lazy(false);
            
            texture = Texture::from_image(
                &mut window.create_texture_context(),
                &img,
                &tx_settings,
            );
            current_image = img;
            let window_size = Vector2::new(window.size().width, window.size().height);
            let img_size = Vector2::new(current_image.width() as f64, current_image.height() as f64);
            offset = window_size/2.0 - img_size/2.0;
            now = Instant::now();

        } 

        if let Some(Button::Mouse(_)) = e.press_args() {
            drag = true;
            let pos = pos_from_coord(offset, cursor, Vector2::new(dimensions.0 as f64, dimensions.1 as f64), scale);
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

            if key == Key::Q {
                std::process::exit(0);
            }

            if key == Key::F {
                //TODO: Fullscreen
                // window.window.;
                // std::process::exit(0);
            }

            if key == Key::Right {
                img_location = img_shift(&img_location, 1);
                reset = true;
                draw_status(loading_img.to_vec(), texture_sender.clone());
                open_image(&img_location, texture_sender.clone());
            }

            if key == Key::Left {
                img_location = img_shift(&img_location, -1);
                reset = true;
                draw_status(loading_img.to_vec(), texture_sender.clone());
                open_image(&img_location, texture_sender.clone());
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

        e.mouse_cursor(|d| {
            cursor = Vector2::new(d[0], d[1]);
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
                offset = Vector2::new(0.0, 0.0);
                offset += window_size/2.0 - img_size/2.0;
                scale = 1.0;
                reset = false;
            }

            let transform = c.
                transform
                .trans(offset.x as f64, offset.y as f64)
                .zoom(scale);

                
            if let Ok(tex) = &texture {
                image(tex, transform, gfx);
                dimensions = tex.get_size();
            }


            let info = format!("{} {}X{} rgba {} {} {} {} / {:.2} {:.2} {:.2} {:.2} @{}X", &img_location.to_string_lossy(),
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
                (scale * 10.0).round() / 10.0);

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

        // dbg!(&dirty);


    }
}


