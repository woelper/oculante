use exr;
use nalgebra::{clamp, Vector2};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use dds::DDS;
use exr::prelude::rgba_image as rgb_exr;
use gif::{ColorOutput, SetParameter};
use gif_dispose;
use nsvg;
use psd::Psd;
use rgb::*;
use std::io::Read;
use image;
use image::{ImageBuffer, Rgba};
use std::sync::mpsc::Sender;
// use std::fmt::Display;


#[derive (Debug)]
pub struct OculanteState {
    pub scale: f64,
    pub scale_increment: f64,
    pub drag_enabled: bool,
    pub reset_image: bool,
    pub message: String,
    pub fullscreen_enabled: bool,
    pub is_loaded: bool,
    pub offset: Vector2<f64>,
    pub cursor: Vector2<f64>,
    pub cursor_relative: Vector2<f64>,
    pub image_dimension: (u32, u32),
    pub sampled_color: [f32; 4],
    pub info_enabled: bool,
    pub font_size: u32,
}


impl Default for OculanteState {
fn default() -> OculanteState {
    OculanteState {
        scale: 1.0,
        scale_increment: 0.1,
        drag_enabled: false,
        reset_image: false,
        message: "Drag image here".into(),
        fullscreen_enabled: false,
        is_loaded: false,
        offset: Vector2::new(0.0, 0.0),
        cursor: Vector2::new(0.0, 0.0),
        cursor_relative: Vector2::new(0.0, 0.0),
        image_dimension: (0,0),
        info_enabled: false,
        sampled_color: [0.,0.,0.,0.],
        font_size: 18,

    }
}
}

pub fn zoomratio(i: f64, s: f64) -> f64 {
    // i * i * i.signum()
    i * s * 0.1
}


pub fn disp_col(col: [f32;4]) -> String {
    format!(
        "{:.0} {:.0} {:.0} {:.0}",
        col[0], col[1], col[2], col[3])
}

pub fn disp_col_norm(col: [f32;4], divisor: f32) -> String {
    format!(
        "{:.2} {:.2} {:.2} {:.2}",
        col[0]/divisor, col[1]/divisor, col[2]/divisor, col[3]/divisor)
} 

pub fn img_shift(file: &PathBuf, inc: isize) -> PathBuf {
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
                // dbg!(&f, i, i + inc);
                if let Some(next) = files.get( (i as isize + inc) as usize ) {
                    // dbg!(&next, i + inc);

                    return next.clone();
                }
            }
        }
    }
    file.clone()

}

pub fn is_ext_compatible(fname: &PathBuf) -> bool {
    match fname
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
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
        _ => false,
    }
}

pub fn solo_channel(
    img: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    channel: usize,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let mut updated_img = img.clone();
    // let updated_img = img.pixels();

    for pixel in updated_img.pixels_mut() {
        pixel.0[0] = pixel.0[channel];
        pixel.0[1] = pixel.0[channel];
        pixel.0[2] = pixel.0[channel];
        pixel.0[3] = 255;
    }
    updated_img
}

pub fn unpremult(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let mut updated_img = img.clone();
    // let updated_img = img.pixels();

    for pixel in updated_img.pixels_mut() {
        pixel.0[3] = 255;
    }
    updated_img
}

fn tonemap_rgba(px: [f32; 4]) -> [u8; 4] {
    [
        (px[0].powf(1.0 / 2.2).max(0.0).min(1.0) * 255.0) as u8,
        (px[1].powf(1.0 / 2.2).max(0.0).min(1.0) * 255.0) as u8,
        (px[2].powf(1.0 / 2.2).max(0.0).min(1.0) * 255.0) as u8,
        (px[3].powf(1.0 / 2.2).max(0.0).min(1.0) * 255.0) as u8,
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

pub fn pos_from_coord(
    origin: Vector2<f64>,
    pt: Vector2<f64>,
    bounds: Vector2<f64>,
    scale: f64,
) -> Vector2<f64> {
    let mut size = (pt - origin) / scale;
    size.x = clamp(size.x, 0.0, bounds.x - 1.0);
    size.y = clamp(size.y, 0.0, bounds.y - 1.0);
    size
}

/// Open an image from disk and send it somewhere
pub fn open_image(
    img_location: &PathBuf,
    texture_sender: Sender<image::RgbaImage>,
    state_sender: Sender<String>,
) {
    let img_location = img_location.clone();
    thread::spawn(move || {
        match img_location.extension().unwrap_or_default().to_str() {
            Some("dds") => {
                let file = File::open(img_location).unwrap();
                let mut reader = BufReader::new(file);
                let dds = DDS::decode(&mut reader).unwrap();
                if let Some(main_layer) = dds.layers.get(0) {
                    let buf = main_layer.as_bytes();
                    let buffer: image::RgbaImage = image::ImageBuffer::from_raw(
                        dds.header.width,
                        dds.header.height,
                        buf.into(),
                    )
                    .unwrap();
                    let _ = texture_sender.send(buffer.clone());
                    let _ = state_sender.send(String::new()).unwrap();
                }
            }
            Some("svg") => {
                // Load and parse the svg
                let svg = nsvg::parse_file(&img_location, nsvg::Units::Pixel, 96.0).unwrap();

                // Create a scaled raster
                let scale = 3.0;
                let image = svg.rasterize(scale).unwrap();
                let dimensions = image.dimensions();
                // This is just to convert between different crate versions of "image". TODO: remove if crates catch up
                let raw = image.into_raw();
                let buffer: image::RgbaImage =
                    image::ImageBuffer::from_raw(dimensions.0, dimensions.1, raw).unwrap();
                let _ = texture_sender.send(buffer);
                let _ = state_sender.send(String::new()).unwrap();
            }
            Some("exr") => {
                // read the image from a file and keep only the png buffer
                let (_info, png_buffer) = rgb_exr::ImageInfo::read_pixels_from_file(
                    &img_location,
                    rgb_exr::read_options::high(),
                    // how to create an empty png buffer from exr image meta data (used for loading the exr image)
                    |info: &rgb_exr::ImageInfo| -> image::RgbaImage {
                        image::ImageBuffer::new(
                            info.resolution.width() as u32,
                            info.resolution.height() as u32,
                        )
                    },
                    // set each pixel in the png buffer from the exr file
                    |png_pixels: &mut image::RgbaImage,
                     position: rgb_exr::Vec2<usize>,
                     pixel: rgb_exr::Pixel| {
                        png_pixels.put_pixel(
                            position.x() as u32,
                            position.y() as u32,
                            image::Rgba(tonemap_rgb([
                                pixel.red.to_f32(),
                                pixel.green.to_f32(),
                                pixel.blue.to_f32(),
                            ])),
                        );
                    },
                )
                .unwrap();

                let _ = texture_sender.send(png_buffer);
                let _ = state_sender.send(String::new()).unwrap();
            }
            Some("psd") => {
                let mut file = File::open(img_location).unwrap();
                let mut contents = vec![];
                if let Ok(_) = file.read_to_end(&mut contents) {
                    let psd = Psd::from_bytes(&contents).unwrap();
                    if let Some(buffer) =
                        image::ImageBuffer::from_raw(psd.width(), psd.height(), psd.rgba())
                    {
                        let _ = texture_sender.send(buffer.clone());
                        let _ = state_sender.send(String::new()).unwrap();
                    }
                }
            }
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
                    let buffer: Option<image::RgbaImage> = image::ImageBuffer::from_raw(
                        dim.0,
                        dim.1,
                        screen.pixels.buf().as_bytes().to_vec(),
                    );
                    texture_sender.send(buffer.unwrap()).unwrap();
                    std::thread::sleep(Duration::from_millis((frame.delay * 10) as u64));
                    let _ = state_sender.send(String::from("ANIM_FRAME")).unwrap();

                }
                let _ = state_sender.send(String::new()).unwrap();

                // }
            }
            Some("hdr") => match File::open(&img_location) {
                Ok(f) => {
                    let reader = BufReader::new(f);
                    match image::hdr::HdrDecoder::new(reader) {
                        Ok(hdr_decoder) => {
                            let meta = hdr_decoder.metadata();
                            let mut ldr_img: Vec<image::Rgba<u8>> = vec![];
                            let hdr_img = hdr_decoder.read_image_hdr().unwrap();
                            for pixel in hdr_img {
                                let tp = image::Rgba(tonemap_rgb(pixel.0));
                                ldr_img.push(tp);
                            }
                            let tonemapped_buffer: image::RgbaImage = image::ImageBuffer::from_raw(
                                meta.width,
                                meta.height,
                                ldr_img.as_rgba().as_bytes().to_vec(),
                            )
                            .unwrap();
                            texture_sender.send(tonemapped_buffer).unwrap();
                            let _ = state_sender.send(String::new()).unwrap();
                        }
                        Err(e) => println!("{:?}", e),
                    }
                }
                Err(e) => println!("{:?}", e),
            },

            _ => match image::open(img_location) {
                Ok(img) => {
                    let _ = texture_sender.send(img.to_rgba()).unwrap();
                    let _ = state_sender.send(String::new()).unwrap();

                }
                Err(e) => println!("ERR {:?}", e),
            },
        }
    });
}
