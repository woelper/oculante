use crate::types::Color;
use dds::DDS;
use exr;
use image::codecs::gif::GifDecoder;
use log::{error, info};
use nalgebra::{clamp, Vector2};
use piston_window::{CharacterCache, Text};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
// use exr::prelude::rgba_image as rgb_exr;

use exr::prelude as exrs;
use exr::prelude::*;

use image::{self, AnimationDecoder};
use image::{ImageBuffer, Rgba};
//use nsvg;
use lazy_static::lazy_static;
use psd::Psd;
use rgb::*;
use std::io::Read;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;
// use libwebp_image;
use anyhow::{anyhow, Error, Result};
use libwebp_sys::{WebPDecodeRGBA, WebPGetInfo};

pub fn ease(v: f64, r1: (f64, f64), r2: (f64, f64)) -> f64 {
    // let rel_0 = a.0/b.0;
    // let rel_0 = a.1/b.1;

    let rel_0 = v / r1.1;

    rel_0 * r2.1
}

lazy_static! {
    pub static ref PLAYER_STOP: Mutex<bool> = Mutex::new(false);
}

pub struct TextInstruction {
    pub text: String,
    pub color: Color,
    pub position: (f64, f64),
    pub size: u32,
}

impl TextInstruction {
    pub fn new(t: &str, position: (f64, f64)) -> TextInstruction {
        TextInstruction {
            text: t.to_string(),
            color: [1.0, 1.0, 1.0, 0.7],
            position,
            size: 14,
        }
    }

    pub fn new_size(t: &str, position: (f64, f64), size: u32) -> TextInstruction {
        TextInstruction {
            text: t.to_string(),
            color: [1.0, 1.0, 1.0, 0.7],
            position,
            size: size,
        }
    }
    pub fn new_color(t: &str, position: (f64, f64), color: Color) -> TextInstruction {
        TextInstruction {
            text: t.to_string(),
            color,
            position,
            size: 14,
        }
    }
}

pub struct Player {
    pub stop: Mutex<bool>,
    pub frame_sender: Sender<FrameCollection>,
    pub image_sender: Sender<image::RgbaImage>,
}

impl Player {
    pub fn new(image_sender: Sender<image::RgbaImage>) -> Player {
        let (frame_sender, frame_receiver): (Sender<FrameCollection>, Receiver<FrameCollection>) =
            mpsc::channel();
        let move_image_sender = image_sender.clone();
        thread::spawn(move || {
            while let Ok(col) = frame_receiver.try_recv() {
                for frame in col.frames {
                    if Player::is_stopped() {
                        break;
                    }
                    let _ = move_image_sender.send(frame.buffer);
                }
            }
        });
        Player {
            stop: Mutex::new(false),
            frame_sender,
            image_sender,
        }
    }

    pub fn load_blocking(&self, img_location: &PathBuf) {
        *self.stop.lock().unwrap() = true;
        send_image_blocking(&img_location, self.image_sender.clone());
    }

    pub fn load(&self, img_location: &PathBuf) {
        *self.stop.lock().unwrap() = true;
        send_image_threaded(&img_location, self.image_sender.clone());
    }

    pub fn stop() {
        // *self.stop.lock().unwrap() = true;
        *PLAYER_STOP.lock().unwrap() = true;
    }

    pub fn is_stopped() -> bool {
        *PLAYER_STOP.lock().unwrap()
    }

    pub fn start() {
        *PLAYER_STOP.lock().unwrap() = false;
    }
}

/// A single frame
#[derive(Debug, Clone)]
pub struct Frame {
    pub buffer: image::RgbaImage,
    /// How long to paunse until the next frame
    pub delay: u16,
}

/// A collection of frames that can loop/repeat
#[derive(Debug, Default, Clone)]
pub struct FrameCollection {
    pub frames: Vec<Frame>,
    pub repeat: bool,
}

impl FrameCollection {
    fn add(&mut self, buffer: image::RgbaImage, delay: u16) {
        self.frames.push(Frame::new(buffer, delay))
    }
    fn add_default(&mut self, buffer: image::RgbaImage) {
        self.frames.push(Frame::new(buffer, 0))
    }
}

impl Frame {
    fn new(buffer: image::RgbaImage, delay: u16) -> Frame {
        Frame { buffer, delay }
    }
}

pub trait TextExt {
    #[allow(unused_variables)]
    fn width<C>(&self, text: &str, cache: &mut C) -> (f64, f64)
    where
        C: CharacterCache,
    {
        unimplemented!()
    }
}

impl TextExt for Text {
    /// Draws text with a character cache
    fn width<C>(&self, text: &str, cache: &mut C) -> (f64, f64)
    where
        C: CharacterCache,
    {
        let mut x = 0.0;
        let mut y = 0.0;
        for ch in text.chars() {
            //let character = cache.character(self.font_size, ch)?;
            let c2 = cache.character(self.font_size, ch);
            if let Ok(character) = c2 {
                x += character.advance_width();
                y += character.advance_height();

                y = y.max(character.top());
            }
        }

        (x, y)
    }
}

/// The state of the application
#[derive(Debug, Clone)]
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
    pub tooltip: bool,
    pub toast: String,
    //pub toast: Option<String>
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
            offset: Vector2::default(),
            cursor: Vector2::default(),
            cursor_relative: Vector2::default(),
            image_dimension: (0, 0),
            info_enabled: false,
            sampled_color: [0., 0., 0., 0.],
            font_size: 18,
            tooltip: false,
            toast: "".to_string(),
        }
    }
}

// Unsafe webp decoding using webp-sys
fn decode_webp(buf: &[u8]) -> Option<image::RgbaImage> {
    let mut width = 0;
    let mut height = 0;
    let len = buf.len();
    let mut webp_buffer: Vec<u8> = vec![];
    unsafe {
        WebPGetInfo(buf.as_ptr(), len, &mut width, &mut height);
        let out_buf = WebPDecodeRGBA(buf.as_ptr(), len, &mut width, &mut height);
        let len = width * height * 4;
        webp_buffer = Vec::from_raw_parts(out_buf, len as usize, len as usize);
    }
    image::ImageBuffer::from_raw(width as u32, height as u32, webp_buffer)
}

pub fn zoomratio(i: f64, s: f64) -> f64 {
    // i * i * i.signum()
    i * s * 0.1
}

pub fn invert_rgb_8bit(c: [f32; 4]) -> [f32; 4] {
    [
        (255. - c[0]) / 255.,
        (255. - c[1]) / 255.,
        (255. - c[2]) / 255.,
        1.0,
    ]
}

pub fn disp_col(col: [f32; 4]) -> String {
    format!("{:.0} {:.0} {:.0} {:.0}", col[0], col[1], col[2], col[3])
}

pub fn disp_col_norm(col: [f32; 4], divisor: f32) -> String {
    format!(
        "{:.2} {:.2} {:.2} {:.2}",
        col[0] / divisor,
        col[1] / divisor,
        col[2] / divisor,
        col[3] / divisor
    )
}

pub fn img_shift(file: &PathBuf, inc: isize) -> PathBuf {
    if let Some(parent) = file.parent() {
        let mut files = std::fs::read_dir(parent)
            .unwrap()
            .map(|x| x.unwrap().path())
            .filter(|x| is_ext_compatible(x))
            .collect::<Vec<PathBuf>>();
        files.sort();
        for (i, f) in files.iter().enumerate() {
            if f == file {
                // dbg!(&f, i, i + inc);
                if let Some(next) = files.get((i as isize + inc) as usize) {
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
    // TODO make this FP
    let mut updated_img = img.clone();
    for pixel in updated_img.pixels_mut() {
        pixel.0[0] = pixel.0[channel];
        pixel.0[1] = pixel.0[channel];
        pixel.0[2] = pixel.0[channel];
        pixel.0[3] = 255;
    }
    updated_img
}

pub fn unpremult(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    // TODO make this FP
    let mut updated_img = img.clone();
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

pub fn send_image_threaded(img_location: &PathBuf, texture_sender: Sender<image::RgbaImage>) {
    let loc = img_location.clone();

    thread::spawn(move || {
        let col = open_image(&loc).expect("Opening failed");

        if col.repeat {
            let mut i = 0;
            while !Player::is_stopped() && i < 200 {
                let frames = col.frames.clone();
                for frame in frames {
                    if Player::is_stopped() {
                        break;
                    }
                    let _ = texture_sender.send(frame.buffer);
                    if frame.delay > 0 {
                        thread::sleep(Duration::from_millis(frame.delay as u64));
                    } else {
                        thread::sleep(Duration::from_millis(40 as u64));
                    }
                }
                i += 1;
            }
        } else {
            for frame in col.frames {
                if Player::is_stopped() {
                    break;
                }
                let _ = texture_sender.send(frame.buffer);

                if frame.delay > 0 {
                    thread::sleep(Duration::from_millis(frame.delay as u64));
                }
            }
        }
    });
}

pub fn send_image_blocking(img_location: &PathBuf, texture_sender: Sender<image::RgbaImage>) {
    match open_image(&img_location) {
        Ok(col) => {
            for frame in col.frames {
                if Player::is_stopped() {
                    break;
                }

                let _ = texture_sender.send(frame.buffer);
                // dbg!(&frame.delay);
                if frame.delay > 0 {
                    thread::sleep(Duration::from_millis(frame.delay as u64));
                }
            }
            // let _ = state_sender.send("".into());
        }
        Err(e) => error!("Error {:?} from {:?}", e, img_location),
    }
}

/// Open an image from disk and send it somewhere
pub fn open_image(img_location: &PathBuf) -> Result<FrameCollection> {
    let img_location = img_location.clone();
    let mut col = FrameCollection::default();

    // Stop all current images being sent
    Player::stop();

    match img_location.extension().unwrap_or_default().to_str() {
        Some("dds") => {
            let file = File::open(img_location)?;
            let mut reader = BufReader::new(file);
            let dds = DDS::decode(&mut reader).map_err(|e| anyhow!("{:?}", e))?;
            if let Some(main_layer) = dds.layers.get(0) {
                let buf = main_layer.as_bytes();
                let buf =
                    image::ImageBuffer::from_raw(dds.header.width, dds.header.height, buf.into())
                        .ok_or(anyhow!("Can't create DDS ImageBuffer with given res"))?;
                col.add_default(buf);
            }
        }
        Some("svg") => {
            // TODO: Should the svg be scaled? if so by what number?
            // This should be specified in a smarter way, maybe resolution * x?
            //let (width, height) = (3000, 3000);
            let opt = usvg::Options::default();
            let svg_data = std::fs::read(&img_location)?;
            if let Ok(rtree) = usvg::Tree::from_data(&svg_data, &opt.to_ref()) {
                let pixmap_size = rtree.svg_node().size.to_screen_size()
                // .scale_to(ScreenSize::new(width, height)?)
                ;

                if let Some(mut pixmap) =
                    tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height())
                {
                    resvg::render(
                        &rtree,
                        usvg::FitTo::Original,
                        tiny_skia::Transform::identity(),
                        pixmap.as_mut(),
                    )
                    .ok_or(anyhow!("Can't render SVG"))?;
                    // resvg::render(&rtree, usvg::FitTo::Height(height), pixmap.as_mut())?;
                    let buf: Option<ImageBuffer<Rgba<u8>, Vec<u8>>> = image::ImageBuffer::from_raw(
                        pixmap_size.width(),
                        pixmap_size.height(),
                        pixmap.data().to_vec(),
                    );
                    if let Some(valid_buf) = buf {
                        col.add_default(valid_buf);
                    }
                }
            }
        }
        Some("exr") => {
            let reader = exrs::read()
                .no_deep_data()
                .largest_resolution_level()
                .rgba_channels(
                    |resolution, _channels: &RgbaChannels| -> image::RgbaImage {
                        image::ImageBuffer::new(
                            resolution.width() as u32,
                            resolution.height() as u32,
                        )
                    },
                    // set each pixel in the png buffer from the exr file
                    |png_pixels, position, (r, g, b, a): (f32, f32, f32, f32)| {
                        png_pixels.put_pixel(
                            position.x() as u32,
                            position.y() as u32,
                            // exr's tonemap:
                            // image::Rgba([tone_map(r), tone_map(g), tone_map(b), (a * 255.0) as u8]),
                            image::Rgba(tonemap_rgba([r, g, b, a])),
                        );
                    },
                )
                .first_valid_layer()
                .all_attributes();

            // an image that contains a single layer containing an png rgba buffer
            let maybe_image: Result<
                Image<Layer<SpecificChannels<image::RgbaImage, RgbaChannels>>>,
                exrs::Error,
            > = reader.from_file(&img_location);

            match maybe_image {
                Ok(image) => {
                    let png_buffer = image.layer_data.channel_data.pixels;
                    col.add_default(png_buffer);
                }
                Err(e) => error!("{} from {:?}", e, img_location),
            }
        }

        Some("hdr") => {
            let f = File::open(&img_location)?;
            let reader = BufReader::new(f);
            let hdr_decoder = image::codecs::hdr::HdrDecoder::new(reader)?;
            let meta = hdr_decoder.metadata();
            let mut ldr_img: Vec<image::Rgba<u8>> = vec![];

            let hdr_img = hdr_decoder.read_image_hdr()?;
            for pixel in hdr_img {
                let tp = image::Rgba(tonemap_rgb(pixel.0));
                ldr_img.push(tp);
            }
            let mut s: Vec<u8> = vec![];
            let l = ldr_img.clone();
            for p in l {
                let mut x = vec![p.0[0], p.0[1], p.0[2], 255];
                s.append(&mut x);
            }

            let tonemapped_buffer = image::RgbaImage::from_raw(meta.width, meta.height, s)
                .ok_or(anyhow!("Failed to create RgbaImage with given dimensions"))?;
            col.add_default(tonemapped_buffer);
        }
        Some("psd") => {
            let mut file = File::open(img_location)?;
            let mut contents = vec![];
            if file.read_to_end(&mut contents).is_ok() {
                let psd = Psd::from_bytes(&contents).map_err(|e| anyhow!("{:?}", e))?;
                if let Some(buf) =
                    image::ImageBuffer::from_raw(psd.width(), psd.height(), psd.rgba())
                {
                    col.add_default(buf);
                }
            }
        }
        Some("webp") => {
            let mut file = File::open(&img_location)?;
            let mut contents = vec![];
            if let Ok(_) = file.read_to_end(&mut contents) {
                match decode_webp(&contents) {
                    Some(webp_buf) => col.add_default(webp_buf),
                    None => println!("Error decoding data from {:?}", img_location),
                }
            }
        }
        Some("gif") => {

            // of course this is shit. Don't reload the image all the time.
            let file = File::open(&img_location)?;
            let gif_decoder = GifDecoder::new(file)?;
            let frames = gif_decoder.into_frames().collect_frames()?;
            for f in frames {
                let delay = f.delay().numer_denom_ms().0 as u16;
                // let delay= 10;
                info!("{delay}");
                col.add(f.into_buffer(), delay);
                col.repeat = true;
            }
        }
        _ => match image::open(&img_location) {
            Ok(img) => {
                col.add_default(img.to_rgba8());
            }
            Err(e) => println!("Can't open image {:?} from {:?}", e, img_location),
        },
    }

    Player::start();
    Ok(col)
}
