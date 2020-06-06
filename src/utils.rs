use nalgebra::{Vector2, clamp};
use std::thread;
use std::time::Duration;
use std::io::BufReader;
use std::fs::File;
use std::path::{PathBuf};
use exr;

use dds::DDS;
use rgb::*;
use psd::Psd;
use std::io::Read;
use gif_dispose;
use gif::{SetParameter, ColorOutput};
use exr::prelude::rgba_image as rgb_exr;
use nsvg;
// use ::image as image;
use image;
use std::sync::mpsc::Sender;


pub fn is_ext_compatible(fname: &PathBuf) -> bool {
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
        "ff" => true,
        _ => false
    }
}

fn tonemap_rgba(px: [f32; 4]) -> [u8; 4] {
        [
            (px[0].powf(1.0/2.2).max(0.0).min(1.0) * 255.0) as u8,
            (px[1].powf(1.0/2.2).max(0.0).min(1.0) * 255.0) as u8,
            (px[2].powf(1.0/2.2).max(0.0).min(1.0) * 255.0) as u8,
            (px[3].powf(1.0/2.2).max(0.0).min(1.0) * 255.0) as u8,
        ]
    }

    fn tonemap_rgb(px: [f32; 3]) -> [u8; 4] {
        let mut tm = tonemap_rgba([px[0], px[1], px[2], 1.0]);
        tm[3] = 255;
        tm
    }


pub fn scale_pt(
        origin: Vector2<f64>,
        pt: Vector2<f64>,
        scale: f64,
        scale_inc: f64,
    ) -> Vector2<f64> {
        ((pt - origin) * scale_inc) / scale
    }

pub fn pos_from_coord(origin: Vector2<f64>, pt: Vector2<f64>, bounds: Vector2<f64>, scale: f64) -> Vector2<f64> {
        let mut size = (pt - origin) / scale;
        size.x = clamp(size.x, 0.0, bounds.x-1.0);
        size.y = clamp(size.y, 0.0, bounds.y-1.0);
        size

    }


/// Open an image from disk and send it somewhere
pub fn open_image(img_location: &PathBuf, texture_sender: Sender<image::RgbaImage>, state_sender: Sender<String>) {
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
                            let buffer: image::RgbaImage = image::ImageBuffer::from_raw(dds.header.width, dds.header.height, buf.into()).unwrap();
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
                        let buffer: image::RgbaImage = image::ImageBuffer::from_raw(dimensions.0, dimensions.1, raw).unwrap();
                        let _ = texture_sender.send(buffer);
    
                    },
                    Some("exr") => {
    
    
                        // read the image from a file and keep only the png buffer
                        let (_info, png_buffer) = rgb_exr::ImageInfo::read_pixels_from_file(
                            &img_location,
                            rgb_exr::read_options::high(),
    
                            // how to create an empty png buffer from exr image meta data (used for loading the exr image)
                            |info: &rgb_exr::ImageInfo| -> image::RgbaImage {
                                image::ImageBuffer::new(
                                    info.resolution.width() as u32,
                                    info.resolution.height() as u32
                                )
                            },
    
                            // set each pixel in the png buffer from the exr file
                            |png_pixels: &mut image::RgbaImage, position: rgb_exr::Vec2<usize>, pixel: rgb_exr::Pixel| {
                                png_pixels.put_pixel(
                                    position.x() as u32, position.y() as u32,
    
                                    image::Rgba(tonemap_rgb([pixel.red.to_f32(), pixel.green.to_f32(), pixel.blue.to_f32()])
                                )
                                );
                            },
                        ).unwrap();
    
                        let _ = texture_sender.send(png_buffer);
                    },
                    Some("psd") => {
                        let mut file = File::open(img_location).unwrap();
                        let mut contents = vec![];
                        if let Ok(_) = file.read_to_end(&mut contents){
                            let psd = Psd::from_bytes(&contents).unwrap();
                            if let Some(buffer) = image::ImageBuffer::from_raw(psd.width(), psd.height(), psd.rgba()) {
                                let _ = texture_sender.send(buffer.clone());
                            }
                        }
                    },
                    Some("gif") => {
    
                        // loop {
                            // of course this is shit. Don't reload the image all the time.
                            let file = File::open(&img_location).unwrap();
                            let mut decoder = gif::Decoder::new(file);
                            // let mut decoder = gif::Decoder::new(r.by_ref());
                            decoder.set(ColorOutput::Indexed);
                            let mut reader = decoder.read_info().unwrap();
                            let mut screen = gif_dispose::Screen::new_reader(&reader);
                            let dim = (screen.pixels.width() as u32, screen.pixels.height() as u32);
              
    
                            while let Some(frame) = reader.read_next_frame().unwrap() {
                                screen.blit_frame(&frame).unwrap();
                                let buffer: Option<image::RgbaImage> = image::ImageBuffer::from_raw(dim.0, dim.1, screen.pixels.buf().as_bytes().to_vec());
                                texture_sender.send(buffer.unwrap()).unwrap();
                                std::thread::sleep(Duration::from_millis((frame.delay*10) as u64));
                            }
                        // }
                    }
                    Some("hdr") => {
    
                        match  File::open(&img_location) {
                            Ok(f) => {
                                let reader = BufReader::new(f);
                                match image::hdr::HdrDecoder::new(reader) {
                                    Ok(hdr_decoder) => {
                                        let meta = hdr_decoder.metadata();
                                        let mut ldr_img: Vec<image::Rgba<u8>> = vec![];
                                        let hdr_img = hdr_decoder.read_image_hdr().unwrap();
                                        for pixel in hdr_img {
                                            let tp = image::Rgba(tonemap_rgb(pixel.0) );
                                            ldr_img.push(tp);
                                        }
                                        let tonemapped_buffer: image::RgbaImage = image::ImageBuffer::from_raw(meta.width, meta.height, ldr_img.as_rgba().as_bytes().to_vec()).unwrap();
                                        texture_sender.send(tonemapped_buffer).unwrap();
                                    },
                                    Err(e) => println!("{:?}", e)
                                }
                            },
                            Err(e) => println!("{:?}", e)
                        }
                    }
    
                    _ => {
                        match image::open(img_location) {
                            Ok(img) => {
                                texture_sender.send(img.to_rgba()).unwrap();
                            },
                            Err(e) => println!("ERR {:?}", e),
                        }
                    }
                }
                state_sender.send(String::new()).unwrap();
            }
            );
    }