use arboard::Clipboard;
use dds::DDS;
use exr;
use image::codecs::gif::GifDecoder;
use image::{EncodableLayout, Pixel, RgbaImage};

use log::{debug, error};
use nalgebra::{clamp, Vector2};
use notan::egui::{Color32, Pos2};
use notan::graphics::Texture;
use notan::prelude::Graphics;
use notan::AppState;
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use rayon::prelude::ParallelIterator;
use rayon::slice::ParallelSliceMut;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use exr::prelude as exrs;
use exr::prelude::*;

use anyhow::{anyhow, Result};
use image::Rgba;
use image::{self, AnimationDecoder};
use lazy_static::lazy_static;
use libwebp_sys::{WebPDecodeRGBA, WebPGetInfo};
use psd::Psd;
use rgb::*;
use std::io::Read;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;
use strum::Display;
use strum_macros::EnumIter;

use crate::image_editing::ImageOperation;

lazy_static! {
    pub static ref PLAYER_STOP: Mutex<bool> = Mutex::new(false);
}

fn is_pixel_fully_transparent(p: &Rgba<u8>) -> bool {
    // dbg!(p.0.iter());
    p.0 == [0, 0, 0, 0]
    // p.0[3] == 0 &&
}

#[derive(Debug)]
pub struct ExtendedImageInfo {
    pub num_pixels: usize,
    pub num_transparent_pixels: usize,
    pub num_colors: usize,
    pub grey_histogram: Vec<(i32, i32)>,
    pub red_histogram: Vec<(i32, i32)>,
    pub green_histogram: Vec<(i32, i32)>,
    pub blue_histogram: Vec<(i32, i32)>,
}

impl ExtendedImageInfo {
    pub fn from_image(img: &RgbaImage) -> Self {
        let mut colors: HashSet<Rgba<u8>> = Default::default();
        // let mut histogram: HashMap<u8, usize> = Default::default();
        let mut grey_histogram: HashMap<u8, usize> = Default::default();
        let mut red_histogram: HashMap<u8, usize> = Default::default();
        let mut green_histogram: HashMap<u8, usize> = Default::default();
        let mut blue_histogram: HashMap<u8, usize> = Default::default();

        let mut num_pixels = 0;
        let mut num_transparent_pixels = 0;
        for p in img.pixels() {
            if is_pixel_fully_transparent(p) {
                num_transparent_pixels += 1;
            }

            let luma_p = ((p.0[0] as i32 + p.0[1] as i32 + p.0[2] as i32) / 3).min(255);
            *grey_histogram.entry(luma_p as u8).or_default() += 1;
            *red_histogram.entry(p.0[0]).or_default() += 1;
            *green_histogram.entry(p.0[1]).or_default() += 1;
            *blue_histogram.entry(p.0[2]).or_default() += 1;

            let mut p = p.clone();
            p.0[3] = 255;
            colors.insert(p.clone());
            num_pixels += 1;
        }

        let mut grey_histogram: Vec<(i32, i32)> = grey_histogram
            .iter()
            .map(|(k, v)| (*k as i32, *v as i32))
            .collect();
        grey_histogram.sort_by(|a, b| a.0.cmp(&b.0));

        let mut green_histogram: Vec<(i32, i32)> = green_histogram
            .iter()
            .map(|(k, v)| (*k as i32, *v as i32))
            .collect();
        green_histogram.sort_by(|a, b| a.0.cmp(&b.0));

        let mut red_histogram: Vec<(i32, i32)> = red_histogram
            .iter()
            .map(|(k, v)| (*k as i32, *v as i32))
            .collect();
        red_histogram.sort_by(|a, b| a.0.cmp(&b.0));

        let mut blue_histogram: Vec<(i32, i32)> = blue_histogram
            .iter()
            .map(|(k, v)| (*k as i32, *v as i32))
            .collect();
        blue_histogram.sort_by(|a, b| a.0.cmp(&b.0));

        Self {
            num_pixels,
            num_transparent_pixels,
            num_colors: colors.len(),
            grey_histogram,
            blue_histogram,
            green_histogram,
            red_histogram,
        }
    }
}

#[derive(Debug)]
pub struct Player {
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
            frame_sender,
            image_sender,
        }
    }

    pub fn load_blocking(&self, img_location: &PathBuf) {
        Self::stop();
        send_image_blocking(&img_location, self.image_sender.clone());
    }

    pub fn load(&self, img_location: &PathBuf) {
        Self::stop();
        send_image_threaded(&img_location, self.image_sender.clone());
    }

    pub fn stop() {
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

#[derive(Debug, PartialEq, EnumIter, Display, Clone, Copy)]
pub enum Channel {
    Red,
    Green,
    Blue,
    Alpha,
    RGB,
    RGBA,
}

impl Channel {
    pub fn hotkey(&self) -> &str {
        match self {
            Self::Red => "r",
            Self::Green => "g",
            Self::Blue => "b",
            Self::Alpha => "a",
            Self::RGB => "c",
            Self::RGBA => "u",
        }
    }
}

#[derive(Debug)]
pub struct EditState {
    pub result_pixel_op: RgbaImage,
    pub result_image_op: RgbaImage,
    pub painting: bool,
    pub non_destructive_painting: bool,
    pub paint_strokes: Vec<PaintStroke>,
    pub paint_fade: bool,
    pub brushes: Vec<RgbaImage>,
    pub pixel_op_stack: Vec<ImageOperation>,
    pub image_op_stack: Vec<ImageOperation>,
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            result_pixel_op: RgbaImage::default(),
            result_image_op: RgbaImage::default(),
            painting: Default::default(),
            non_destructive_painting: Default::default(),
            paint_strokes: Default::default(),
            paint_fade: false,
            brushes: vec![
                image::load_from_memory(include_bytes!("brush1.png"))
                    .unwrap()
                    .into_rgba8(),
                image::load_from_memory(include_bytes!("brush2.png"))
                    .unwrap()
                    .into_rgba8(),
                image::load_from_memory(include_bytes!("brush3.png"))
                    .unwrap()
                    .into_rgba8(),
                image::load_from_memory(include_bytes!("brush4.png"))
                    .unwrap()
                    .into_rgba8(),
                image::load_from_memory(include_bytes!("brush5.png"))
                    .unwrap()
                    .into_rgba8(),
            ],
            pixel_op_stack: vec![],
            image_op_stack: vec![],
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PaintStroke {
    pub points: Vec<Pos2>,
    pub fade: bool,
    pub color: [f32; 4],
    /// brush width from 0-1. 1 is equal to 1/10th of the smallest image dimension.
    pub width: f32,
    pub brush_index: usize,
    /// For ui preview: if highlit, paint brush stroke differently
    pub highlight: bool,
    pub committed: bool,
    pub flip_random: bool,
}

impl PaintStroke {
    pub fn without_points(&self) -> Self {
        Self {
            points: vec![],
            ..self.clone()
        }
    }

    pub fn new() -> Self {
        Self {
            color: [1., 1., 1., 1.],
            width: 0.05,
            ..Default::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    // render brush stroke
    pub fn render(&self, img: &mut RgbaImage, brushes: &Vec<RgbaImage>) {
        // Calculate the brush: use a fraction of the smallest image size
        let max_brush_size = img.width().min(img.height());

        let mut brush = image::imageops::resize(
            &brushes[self.brush_index],
            (self.width * max_brush_size as f32) as u32,
            (self.width * max_brush_size as f32) as u32,
            image::imageops::Triangle,
        );

        // transform points from UV into image space
        let abs_points = self
            .points
            .iter()
            .map(|p| Pos2::new(img.width() as f32 * p.x, img.height() as f32 * p.y))
            .collect::<Vec<_>>();

        let points = notan::egui::Shape::dotted_line(
            &abs_points,
            Color32::DARK_RED,
            (brush.width() as f32 / 4.0).max(1.5), // .min(60.)
            0.,
        );

        for (i, p) in points.iter().enumerate() {
            let pos_on_line = p.visual_bounding_rect().center();

            if self.flip_random {
                // seed by brush position so randomness only changes per brush instance
                let mut rng =
                    ChaCha8Rng::seed_from_u64(pos_on_line.x as u64 + pos_on_line.y as u64);

                let flip_x: bool = rng.gen();
                let flip_y: bool = rng.gen();

                if flip_x {
                    image::imageops::flip_horizontal_in_place(&mut brush);
                }
                if flip_y {
                    image::imageops::flip_vertical_in_place(&mut brush);
                }
            }

            let mut stroke_color = self.color;

            if self.fade {
                let fraction = 1.0 - i as f32 / points.len() as f32;
                stroke_color[3] = stroke_color[3] * fraction;
            }

            if self.highlight {
                stroke_color[0] *= 2.5;
                stroke_color[1] *= 2.5;
                stroke_color[2] *= 2.5;
                stroke_color[3] *= 2.5;
            }
            paint_at(img, &brush, &pos_on_line, stroke_color);
        }
    }
}

/// The state of the application
#[derive(Debug, AppState)]
pub struct OculanteState {
    pub scale: f32,
    pub scale_increment: f32,
    pub drag_enabled: bool,
    pub reset_image: bool,
    pub message: Option<String>,
    pub is_loaded: bool,
    pub offset: Vector2<f32>,
    pub cursor: Vector2<f32>,
    pub cursor_relative: Vector2<f32>,
    pub image_dimension: (u32, u32),
    pub sampled_color: [f32; 4],
    pub info_enabled: bool,
    pub mouse_delta: Vector2<f32>,
    pub texture_channel: (Sender<RgbaImage>, Receiver<RgbaImage>),
    pub message_channel: (Sender<String>, Receiver<String>),
    pub extended_info_channel: (Sender<ExtendedImageInfo>, Receiver<ExtendedImageInfo>),
    pub extended_info_loading: bool,
    pub player: Player,
    pub current_texture: Option<Texture>,
    pub current_path: Option<PathBuf>,
    pub current_image: Option<RgbaImage>,
    pub current_channel: Channel,
    pub settings_enabled: bool,
    pub edit_enabled: bool,
    pub image_info: Option<ExtendedImageInfo>,
    pub tiling: usize,
    pub mouse_grab: bool,
    pub edit_state: EditState,
    pub pointer_over_ui: bool,
}

impl Default for OculanteState {
    fn default() -> OculanteState {
        let tx_channel = mpsc::channel();
        OculanteState {
            scale: 1.0,
            scale_increment: 0.1,
            drag_enabled: Default::default(),
            reset_image: Default::default(),
            message: Default::default(),
            is_loaded: Default::default(),
            offset: Default::default(),
            cursor: Default::default(),
            cursor_relative: Default::default(),
            image_dimension: (0, 0),
            info_enabled: Default::default(),
            sampled_color: [0., 0., 0., 0.],
            player: Player::new(tx_channel.0.clone()),
            texture_channel: tx_channel,
            message_channel: mpsc::channel(),
            extended_info_channel: mpsc::channel(),
            extended_info_loading: false,
            mouse_delta: Default::default(),
            current_texture: Default::default(),
            current_image: Default::default(),
            current_path: Default::default(),
            current_channel: Channel::RGBA,
            settings_enabled: false,
            edit_enabled: false,
            image_info: None,
            tiling: 1,
            mouse_grab: false,
            edit_state: Default::default(),
            pointer_over_ui: Default::default(),
        }
    }
}

// Unsafe webp decoding using webp-sys
fn decode_webp(buf: &[u8]) -> Option<image::RgbaImage> {
    let mut width = 0;
    let mut height = 0;
    let len = buf.len();
    let webp_buffer: Vec<u8>;
    unsafe {
        WebPGetInfo(buf.as_ptr(), len, &mut width, &mut height);
        let out_buf = WebPDecodeRGBA(buf.as_ptr(), len, &mut width, &mut height);
        let len = width * height * 4;
        webp_buffer = Vec::from_raw_parts(out_buf, len as usize, len as usize);
    }
    image::ImageBuffer::from_raw(width as u32, height as u32, webp_buffer)
}

pub fn zoomratio(i: f32, s: f32) -> f32 {
    // i * i * i.signum()
    i * s * 0.1
}

/// Display RGBA values nicely
pub fn disp_col(col: [f32; 4]) -> String {
    format!("{:.0},{:.0},{:.0},{:.0}", col[0], col[1], col[2], col[3])
}

/// Normalized RGB values (0-1)
pub fn disp_col_norm(col: [f32; 4], divisor: f32) -> String {
    format!(
        "{:.2},{:.2},{:.2},{:.2}",
        col[0] / divisor,
        col[1] / divisor,
        col[2] / divisor,
        col[3] / divisor
    )
}

/// Advance to the prev/next image
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
                if let Some(next) = files.get((i as isize + inc) as usize) {
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

pub fn solo_channel(img: &RgbaImage, channel: usize) -> RgbaImage {
    // TODO make this FP
    let mut updated_img = img.clone();
    updated_img.par_chunks_mut(4).for_each(|pixel| {
        pixel[0] = pixel[channel];
        pixel[1] = pixel[channel];
        pixel[2] = pixel[channel];
        pixel[3] = 255;
    });
    updated_img
}

pub fn unpremult(img: &RgbaImage) -> RgbaImage {
    let mut updated_img = img.clone();
    updated_img.par_chunks_mut(4).for_each(|pixel| {
        pixel[3] = 255;
    });
    updated_img
}

/// Mark pixels with no alpha but color info
pub fn highlight_bleed(img: &RgbaImage) -> RgbaImage {
    let mut updated_img = img.clone();
    updated_img.par_chunks_mut(4).for_each(|pixel| {
        if pixel[3] == 0 {
            if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
                pixel[1] = pixel[1].checked_add(100).unwrap_or(255);
                pixel[3] = 255;
            }
        }
    });
    updated_img
}

/// Mark pixels with transparency
pub fn highlight_semitrans(img: &RgbaImage) -> RgbaImage {
    let mut updated_img = img.clone();
    updated_img.par_chunks_mut(4).for_each(|pixel| {
        if pixel[3] != 0 && pixel[3] != 255 {
            pixel[1] = pixel[1].checked_add(100).unwrap_or(255);
            pixel[3] = pixel[1].checked_add(100).unwrap_or(255);
        }
    });
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
    origin: Vector2<f32>,
    pt: Vector2<f32>,
    scale: f32,
    scale_inc: f32,
) -> Vector2<f32> {
    ((pt - origin) * scale_inc) / scale
}

pub fn pos_from_coord(
    origin: Vector2<f32>,
    pt: Vector2<f32>,
    bounds: Vector2<f32>,
    scale: f32,
) -> Vector2<f32> {
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
                        i = 200;
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

pub fn send_extended_info(
    current_image: &Option<RgbaImage>,
    channel: &(Sender<ExtendedImageInfo>, Receiver<ExtendedImageInfo>),
) {
    if let Some(img) = current_image {
        let copied_img = img.clone();
        let sender = channel.0.clone();
        thread::spawn(move || {
            let e_info = ExtendedImageInfo::from_image(&copied_img);
            let _ = sender.send(e_info);
        });
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
                    let buf: Option<RgbaImage> = image::ImageBuffer::from_raw(
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
                debug!(" Frame delay {delay}");
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

pub trait ImageExt {
    fn size_vec(&self) -> Vector2<f32> {
        unimplemented!()
    }

    fn to_texture(&self, _: &mut Graphics) -> Option<Texture> {
        unimplemented!()
    }

    fn to_texture_premult(&self, _: &mut Graphics) -> Option<Texture> {
        unimplemented!()
    }

    fn update_texture(&self, _: &mut Graphics, _: &mut Texture) {
        unimplemented!()
    }

    fn to_image(&self, _: &mut Graphics) -> Option<RgbaImage> {
        unimplemented!()
    }
}

impl ImageExt for RgbaImage {
    fn size_vec(&self) -> Vector2<f32> {
        Vector2::new(self.width() as f32, self.height() as f32)
    }

    fn to_texture(&self, gfx: &mut Graphics) -> Option<Texture> {
        gfx.create_texture()
            .from_bytes(self, self.width() as i32, self.height() as i32)
            // .with_premultiplied_alpha()
            // .with_filter(TextureFilter::Linear, TextureFilter::Nearest)
            // .with_wrap(TextureWrap::Repeat, TextureWrap::Repeat)
            .build()
            .ok()
    }

    fn to_texture_premult(&self, gfx: &mut Graphics) -> Option<Texture> {
        gfx.create_texture()
            .from_bytes(self, self.width() as i32, self.height() as i32)
            .with_premultiplied_alpha()
            // .with_filter(TextureFilter::Linear, TextureFilter::Nearest)
            // .with_wrap(TextureWrap::Repeat, TextureWrap::Repeat)
            .build()
            .ok()
    }

    fn update_texture(&self, gfx: &mut Graphics, texture: &mut Texture) {
        gfx.update_texture(texture)
            .with_data(self)
            .update()
            .unwrap();
    }
}

impl ImageExt for (i32, i32) {
    fn size_vec(&self) -> Vector2<f32> {
        Vector2::new(self.0 as f32, self.1 as f32)
    }
}

impl ImageExt for (f32, f32) {
    fn size_vec(&self) -> Vector2<f32> {
        Vector2::new(self.0, self.1)
    }
}

impl ImageExt for (u32, u32) {
    fn size_vec(&self) -> Vector2<f32> {
        Vector2::new(self.0 as f32, self.1 as f32)
    }
}

pub fn clipboard_copy(img: &RgbaImage) {
    if let Ok(clipboard) = &mut Clipboard::new() {
        let _ = clipboard.set_image(arboard::ImageData {
            width: img.width() as usize,
            height: img.height() as usize,
            bytes: std::borrow::Cow::Borrowed(img.clone().as_bytes()),
        });
    }
}

pub fn paint_at(img: &mut RgbaImage, brush: &RgbaImage, pos: &Pos2, color: [f32; 4]) {
    // To test
    // img.put_pixel(pos.x as u32, pos.y as u32, color_to_pixel(color));
    // return;

    let brush_offset = Pos2::new(brush.width() as f32 / 2., brush.height() as f32 / 2.);

    for (b_x, b_y, b_pixel) in brush.enumerate_pixels() {
        if let Some(p) = img.get_pixel_mut_checked(
            (*pos - brush_offset).x as u32 + b_x,
            (*pos - brush_offset).y as u32 + b_y,
        ) {
            // multiply brush with user color os it's tinted
            let colored_pixel = Rgba([
                (color[0] * b_pixel[0] as f32) as u8,
                (color[1] * b_pixel[1] as f32) as u8,
                (color[2] * b_pixel[2] as f32) as u8,
                (color[3] * b_pixel[3] as f32) as u8,
            ]);
            // colored_pixel.blend(&color_to_pixel(color));
            p.blend(&colored_pixel);
        }
    }
}
