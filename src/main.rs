#![windows_subsystem = "windows"]


use ::image as image_crate;
use image_crate::{Pixel};

mod utils;
use clap;
use clap::{App, Arg};
use nalgebra::Vector2;

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


                    // let path = Path::new("examples/example.svg");

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

                    // let file = File::open(img_location).unwrap();
                    // let mut reader = BufReader::new(file);

                    // let backend: Box<dyn Render> = Box::new(resvg::backend_qt::Backend);

                    // "cairo" => Box::new(resvg::backend_cairo::Backend),
                    // "qt" => Box::new(resvg::backend_qt::Backend),
                    // "skia" => Box::new(resvg::backend_skia::Backend),
                    // "raqote" => Box::new(resvg::backend_raqote::Backend),

                    // let opts = usvg::Options::default();
                    // let tree = usvg::Tree::from_file(&img_location, &opts).map_err(|e| e.to_string());

                    // backend.render_node_to_image(&tree, &opts);

                },
                Some("exr") => {


                    // let img = FullImage::read_from_file(img_location, ReadOptions::default()).unwrap();
                    // // println!("file meta data: {:#?}", img); // does not print actual pixel values
                    // // TODO: Add EXR support
                
    
                    // fn save_f32_image_as_png(data: &[f32], size: Vec2<usize>, name: String) {
                    //     let mut png_buffer = image::GrayImage::new(size.0 as u32, size.1 as u32);
                    //     let mut sorted = Vec::from(data);
                    //     sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Less));
                
                    //     // sixth percentile normalization
                    //     let max = sorted[7 * sorted.len() / 8];
                    //     let min = sorted[1 * sorted.len() / 8];
                
                    //     let tone = |v: f32| (v - 0.5).tanh() * 0.5 + 0.5;
                    //     let max_toned = tone(*sorted.last().unwrap());
                    //     let min_toned = tone(*sorted.first().unwrap());
                
                    //     for (x, y, pixel) in png_buffer.enumerate_pixels_mut() {
                    //         let v = data[(y * size.0 as u32 + x) as usize];
                    //         let v = (v - min) / (max - min);
                    //         let v = tone(v);
                
                    //         let v = (v - min_toned) / (max_toned - min_toned);
                    //         *pixel = image::Luma([(v.max(0.0).min(1.0) * 255.0) as u8]);
                    //     }
                
                    //     println!("Saving to {}", name);
                    //     // png_buffer.save(&name).unwrap();
                    // }
    
    
    
                    // for (part_index, part) in img.parts.iter().enumerate() {
    
                    //     for channel in &part.channels {
                    //         match &channel.content {
                    //             ChannelData::F16(levels) => {
                    //                 let levels = levels.as_flat_samples().unwrap();
                    //                 for sample_block in levels.as_slice() {
                    //                     let data : Vec<f32> = sample_block.samples.iter().map(|f16| f16.to_f32()).collect();
    
                    //                     dbg!(&channel.name);
                    //                     save_f32_image_as_png(&data, sample_block.resolution, format!(
                    //                         "{}_f16_{}x{}.png",
                    //                         channel.name,
                    //                         sample_block.resolution.0,
                    //                         sample_block.resolution.1,
                    //                     ))
                    //                 }
                    //             },
                    //             ChannelData::F32(levels) => {
                    //                 let levels = levels.as_flat_samples().unwrap();
                    //                 for sample_block in levels.as_slice() {
                    //                     dbg!(&channel.name);
    
                    //                     save_f32_image_as_png(&sample_block.samples, sample_block.resolution, format!(
                    //                         "{}_f16_{}x{}.png",
                    //                         channel.name,
                    //                         sample_block.resolution.0,
                    //                         sample_block.resolution.1,
                    //                     ))
                    //                 }
                    //             },
                    //             _ => panic!()
                    //         }
                    //     }
                    // }
    
    
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
                _ => {
                    match image_crate::open(img_location) {
                        Ok(img) => {
                            texture_sender.send(img.to_rgba()).unwrap();
                            },
                        Err(e) => println!("ERR {:?}", e),
                    }
                }
            }
        }
        );
}



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

    // window.set_lazy(true);
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

    open_image(&img_location, texture_sender.clone());



    while let Some(e) = window.next() {

        // a new texture has been sent
        if let Ok(img) = texture_receiver.try_recv() {
            // println!("received image data from loader");
            window.set_lazy(false);

            // let dimensions = img.dimensions();
            // This is just to convert between different crate versions of "image". TODO: remove if crates catch up
            // let raw = img.into_raw();
            // let buffer: piston::image::RgbaImage = image_crate::ImageBuffer::from_raw(dimensions.0, dimensions.1, raw).unwrap();

            
            texture = Texture::from_image(
                &mut window.create_texture_context(),
                &img,
                &tx_settings,
            );
            current_image = img;
            let window_size = Vector2::new(window.size().width, window.size().height);
            let img_size = Vector2::new(current_image.width() as f64, current_image.height() as f64);
            offset = window_size/2.0 - img_size/2.0;
            window.set_lazy(true);

        }

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

            if key == Key::Q {
                std::process::exit(0);
            }

            if key == Key::F {
                // window.window.;
                // std::process::exit(0);
            }

            if key == Key::Right {
                img_location = img_shift(&img_location, 1);
                window.set_lazy(false);
                reset = true;
                open_image(&img_location, texture_sender.clone());
            }

            if key == Key::Left {
                img_location = img_shift(&img_location, -1);
                window.set_lazy(false);
                reset = true;
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
    }
}


