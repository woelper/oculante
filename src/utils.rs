use arboard::Clipboard;
use dds::DDS;
use jxl_oxide::{JxlImage, PixelFormat, RenderResult};

// use image::codecs::gif::GifDecoder;
use exr::prelude as exrs;
use exr::prelude::*;
use image::{DynamicImage, EncodableLayout, GrayAlphaImage, RgbImage, RgbaImage};
use log::{debug, error, info};
use nalgebra::{clamp, Vector2};
use notan::graphics::Texture;
use notan::prelude::{App, Graphics, TextureFilter};
use quickraw::{data, DemosaicingMethod, Export, Input, Output, OutputType};
use rayon::prelude::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rayon::slice::ParallelSliceMut;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use tiff::decoder::Limits;
use usvg::{TreeParsing, TreeTextToPath};

use anyhow::{anyhow, bail, Context, Result};
use image::Rgba;
use image::{self};
use libwebp_sys::{WebPDecodeRGBA, WebPGetInfo};
use psd::Psd;
use rgb::*;
use std::io::Read;
use std::sync::mpsc::{self, channel};
use std::sync::mpsc::{Receiver, Sender};
use strum::Display;
use strum_macros::EnumIter;

use crate::appstate::{ImageGeometry, OculanteState};
use crate::cache::Cache;
use crate::image_editing::{self, ImageOperation};
use crate::shortcuts::{lookup, InputEvent, Shortcuts};
use crate::FONT;

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "bmp", "dds", "exr", "ff", "gif", "hdr", "ico", "jpeg", "jpg", "png", "pnm", "psd", "svg",
    "tga", "tif", "tiff", "webp", "nef", "cr2", "dng", "mos", "erf", "raf", "arw", "3fr", "ari",
    "srf", "sr2", "braw", "r3d", "nrw", "raw", "avif", "jxl", "ppm",
];

fn is_pixel_fully_transparent(p: &Rgba<u8>) -> bool {
    p.0 == [0, 0, 0, 0]
}

#[derive(Debug)]
pub struct ExtendedImageInfo {
    pub num_pixels: usize,
    pub num_transparent_pixels: usize,
    pub num_colors: usize,
    pub red_histogram: Vec<(i32, i32)>,
    pub green_histogram: Vec<(i32, i32)>,
    pub blue_histogram: Vec<(i32, i32)>,
    pub exif: HashMap<String, String>,
    pub name: String,
}

impl ExtendedImageInfo {
    pub fn with_exif(&mut self, image_path: &Path) -> Result<()> {
        self.name = image_path.to_string_lossy().to_string();
        if image_path.extension() == Some(OsStr::new("gif")) {
            return Ok(());
        }

        let file = std::fs::File::open(image_path)?;
        let mut bufreader = std::io::BufReader::new(&file);
        let exifreader = exif::Reader::new();
        let exif = exifreader.read_from_container(&mut bufreader)?;
        for f in exif.fields() {
            //     let s = format!("{} {} {}",
            //              f.tag, f.ifd_num, f.display_value().with_unit(&exif));
            self.exif.insert(
                f.tag.to_string(),
                f.display_value().with_unit(&exif).to_string(),
            );
        }
        Ok(())
    }

    pub fn from_image(img: &RgbaImage) -> Self {
        let mut colors: HashSet<Rgba<u8>> = Default::default();
        let mut red_histogram: HashMap<u8, usize> = Default::default();
        let mut green_histogram: HashMap<u8, usize> = Default::default();
        let mut blue_histogram: HashMap<u8, usize> = Default::default();

        let mut num_pixels = 0;
        let mut num_transparent_pixels = 0;
        for p in img.pixels() {
            if is_pixel_fully_transparent(p) {
                num_transparent_pixels += 1;
            }

            *red_histogram.entry(p.0[0]).or_default() += 1;
            *green_histogram.entry(p.0[1]).or_default() += 1;
            *blue_histogram.entry(p.0[2]).or_default() += 1;

            let mut p = *p;
            p.0[3] = 255;
            colors.insert(p);
            num_pixels += 1;
        }

        let mut green_histogram: Vec<(i32, i32)> = green_histogram
            .par_iter()
            .map(|(k, v)| (*k as i32, *v as i32))
            .collect();
        green_histogram.sort_by(|a, b| a.0.cmp(&b.0));

        let mut red_histogram: Vec<(i32, i32)> = red_histogram
            .par_iter()
            .map(|(k, v)| (*k as i32, *v as i32))
            .collect();
        red_histogram.sort_by(|a, b| a.0.cmp(&b.0));

        let mut blue_histogram: Vec<(i32, i32)> = blue_histogram
            .par_iter()
            .map(|(k, v)| (*k as i32, *v as i32))
            .collect();
        blue_histogram.sort_by(|a, b| a.0.cmp(&b.0));

        Self {
            num_pixels,
            num_transparent_pixels,
            num_colors: colors.len(),
            blue_histogram,
            green_histogram,
            red_histogram,
            name: Default::default(),
            exif: Default::default(),
        }
    }
}

#[derive(Debug)]
pub struct Player {
    pub image_sender: Sender<Frame>,
    pub stop_sender: Sender<()>,
    pub cache: Cache,
    pub max_texture_size: u32,
}

impl Player {
    pub fn new(image_sender: Sender<Frame>, cache_size: usize, max_texture_size: u32) -> Player {
        let (stop_sender, _): (Sender<()>, Receiver<()>) = mpsc::channel();
        Player {
            image_sender,
            stop_sender,
            cache: Cache {
                data: Default::default(),
                cache_size,
            },
            max_texture_size,
        }
    }

    pub fn load(&mut self, img_location: &Path, message_sender: Sender<String>) {
        debug!("Stopping player on load");
        self.stop();
        let (stop_sender, stop_receiver): (Sender<()>, Receiver<()>) = mpsc::channel();
        self.stop_sender = stop_sender;

        if let Some(cached_image) = self.cache.get(img_location) {
            _ = self.image_sender.send(Frame::new_still(cached_image));
            info!("Cache hit for {}", img_location.display());
            return;
        }

        send_image_threaded(
            img_location,
            self.image_sender.clone(),
            message_sender,
            stop_receiver,
            self.max_texture_size,
        );
    }

    pub fn stop(&self) {
        _ = self.stop_sender.send(());
    }
}

pub fn send_image_threaded(
    img_location: &Path,
    texture_sender: Sender<Frame>,
    message_sender: Sender<String>,
    stop_receiver: Receiver<()>,
    max_texture_size: u32,
) {
    let loc = img_location.to_owned();

    thread::spawn(move || {
        let mut framecache = vec![];
        let mut timer = std::time::Instant::now();

        match open_image(&loc) {
            Ok(frame_receiver) => {
                // _ = texture_sender
                // .clone()
                // .send(Frame::new_reset(f.buffer.clone()));

                let mut first = true;
                for f in frame_receiver.iter() {
                    if stop_receiver.try_recv().is_ok() {
                        info!("Stopped from receiver.");
                        return;
                    }
                    // a "normal image (no animation)"
                    if f.source == FrameSource::Still {
                        let largest_side = f.buffer.dimensions().0.max(f.buffer.dimensions().1);

                        // Check if texture is too large to fit on the texture
                        if largest_side > max_texture_size {
                            _ = message_sender.send("This image exceeded the maximum resolution and will be be scaled down.".to_string());
                            let scale_factor = max_texture_size as f32 / largest_side as f32;
                            let new_dimensions = (
                                (f.buffer.dimensions().0 as f32 * scale_factor)
                                    .min(max_texture_size as f32)
                                    as u32,
                                (f.buffer.dimensions().1 as f32 * scale_factor)
                                    .min(max_texture_size as f32)
                                    as u32,
                            );

                            let mut frame = f;
                            let op = ImageOperation::Resize {
                                dimensions: new_dimensions,
                                aspect: true,
                                filter: image_editing::ScaleFilter::Bilinear,
                            };
                            _ = op.process_image(&mut frame.buffer);
                            let _ = texture_sender.send(frame);
                        } else {
                            let _ = texture_sender.send(f);
                        }

                        return;
                    }
                    if f.source == FrameSource::Animation {
                        framecache.push(f.clone());
                        if first {
                            _ = texture_sender
                                .clone()
                                .send(Frame::new_reset(f.buffer.clone()));
                        } else {
                            let _ = texture_sender.send(f.clone());
                        }
                        let elapsed = timer.elapsed().as_millis();
                        let wait_time_after_loading = f.delay.saturating_sub(elapsed as u16);
                        debug!("elapsed {elapsed}, wait {wait_time_after_loading}");
                        std::thread::sleep(Duration::from_millis(wait_time_after_loading as u64));
                        timer = std::time::Instant::now();
                    }

                    first = false;
                }

                // loop over the image. For sanity, stop at a limit of iterations.
                for _ in 0..500 {
                    // let frames = col.frames.clone();
                    for frame in &framecache {
                        if stop_receiver.try_recv().is_ok() {
                            info!("Stopped from receiver.");
                            return;
                        }
                        let _ = texture_sender.send(frame.clone());
                        if frame.delay > 0 {
                            //                                                  cap at 60fps
                            thread::sleep(Duration::from_millis(frame.delay.max(17) as u64));
                        } else {
                            thread::sleep(Duration::from_millis(40_u64));
                        }
                    }
                }
            }
            Err(e) => {
                error!("{e}");
                _ = message_sender.send(e.to_string());
            }
        }
    });
}

/// A single frame
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub enum FrameSource {
    ///Part of an animation
    Animation,
    ///First frame of animation. This is necessary to reset the image and stop the player.
    AnimationStart,
    Still,
    EditResult,
    // AnimationEnd,
}

/// A single frame
#[derive(Debug, Clone)]
pub struct Frame {
    pub buffer: RgbaImage,
    /// How long to pause until the next frame
    pub delay: u16,
    pub source: FrameSource,
}

impl Frame {
    fn new(buffer: RgbaImage, delay: u16, source: FrameSource) -> Frame {
        Frame {
            buffer,
            delay,
            source,
        }
    }

    fn new_reset(buffer: RgbaImage) -> Frame {
        Frame {
            buffer,
            delay: 0,
            source: FrameSource::AnimationStart,
        }
    }

    #[allow(dead_code)]
    pub fn new_edit(buffer: RgbaImage) -> Frame {
        Frame {
            buffer,
            delay: 0,
            source: FrameSource::EditResult,
        }
    }

    pub fn new_still(buffer: RgbaImage) -> Frame {
        Frame {
            buffer,
            delay: 0,
            source: FrameSource::Still,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, EnumIter, Display, Clone, Copy)]
pub enum ColorChannel {
    Red,
    Green,
    Blue,
    Alpha,
    Rgb,
    Rgba,
}

impl ColorChannel {
    pub fn hotkey(&self, shortcuts: &Shortcuts) -> String {
        match self {
            Self::Red => lookup(shortcuts, &InputEvent::RedChannel),
            Self::Green => lookup(shortcuts, &InputEvent::GreenChannel),
            Self::Blue => lookup(shortcuts, &InputEvent::BlueChannel),
            Self::Alpha => lookup(shortcuts, &InputEvent::AlphaChannel),
            Self::Rgb => lookup(shortcuts, &InputEvent::RGBChannel),
            Self::Rgba => lookup(shortcuts, &InputEvent::RGBAChannel),
        }
    }
}

// Unsafe webp decoding using webp-sys
fn decode_webp(buf: &[u8]) -> Option<RgbaImage> {
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

pub fn toggle_fullscreen(app: &mut App, state: &mut OculanteState) {
    let fullscreen = app.window().is_fullscreen();

    if !fullscreen {
        let mut window_pos = app.window().position();
        window_pos.1 += 40;

        debug!("Not fullscreen. Storing offset: {:?}", window_pos);

        let dpi = app.window().dpi();
        debug!("{:?}", dpi);
        window_pos.0 = (window_pos.0 as f64 / dpi) as i32;
        window_pos.1 = (window_pos.1 as f64 / dpi) as i32;
        #[cfg(target_os = "macos")]
        {
            // tweak for osx titlebars
            window_pos.1 += 8;
        }

        // if going from window to fullscreen, offset by window pos
        state.image_geometry.offset.x += window_pos.0 as f32;
        state.image_geometry.offset.y += window_pos.1 as f32;

        // save old window pos
        state.fullscreen_offset = Some(window_pos);
    } else if let Some(sf) = state.fullscreen_offset {
        state.image_geometry.offset.x -= sf.0 as f32;
        state.image_geometry.offset.y -= sf.1 as f32;
    }
    app.window().set_fullscreen(!fullscreen);
}

/// Determine if an enxtension is compatible with oculante
pub fn is_ext_compatible(fname: &Path) -> bool {
    SUPPORTED_EXTENSIONS.contains(
        &fname
            .extension()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default()
            .to_lowercase()
            .as_str(),
    )
}

pub fn solo_channel(img: &RgbaImage, channel: usize) -> RgbaImage {
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
        if pixel[3] == 0 && (pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0) {
            pixel[1] = pixel[1].saturating_add(100);
            pixel[3] = 255;
        }
    });
    updated_img
}

/// Mark pixels with transparency
pub fn highlight_semitrans(img: &RgbaImage) -> RgbaImage {
    let mut updated_img = img.clone();
    updated_img.par_chunks_mut(4).for_each(|pixel| {
        if pixel[3] != 0 && pixel[3] != 255 {
            pixel[1] = pixel[1].saturating_add(100);
            pixel[3] = pixel[1].saturating_add(100);
        }
    });
    updated_img
}

fn tonemap_rgba(px: [f32; 4]) -> [u8; 4] {
    [
        tonemap_f32(px[0]),
        tonemap_f32(px[1]),
        tonemap_f32(px[2]),
        tonemap_f32(px[3]),
    ]
}

fn tonemap_f32(px: f32) -> u8 {
    (px.powf(1.0 / 2.2).max(0.0).min(1.0) * 255.0) as u8
    // (px.filmic() * 255.) as u8
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

pub fn send_extended_info(
    current_image: &Option<RgbaImage>,
    current_path: &Option<PathBuf>,
    channel: &(Sender<ExtendedImageInfo>, Receiver<ExtendedImageInfo>),
) {
    if let Some(img) = current_image {
        let copied_img = img.clone();
        let sender = channel.0.clone();
        let current_path = current_path.clone();
        thread::spawn(move || {
            let mut e_info = ExtendedImageInfo::from_image(&copied_img);
            if let Some(p) = current_path {
                _ = e_info.with_exif(&p);
            }
            _ = sender.send(e_info);
        });
    }
}

/// Open an image from disk and send it somewhere
pub fn open_image(img_location: &Path) -> Result<Receiver<Frame>> {
    let (sender, receiver): (Sender<Frame>, Receiver<Frame>) = channel();

    let img_location = (*img_location).to_owned();
    // let mut col = FrameCollection::default();

    match img_location
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "dds" => {
            let file = File::open(img_location)?;
            let mut reader = BufReader::new(file);
            let dds = DDS::decode(&mut reader).map_err(|e| anyhow!("{:?}", e))?;
            if let Some(main_layer) = dds.layers.get(0) {
                let buf = main_layer.as_bytes();
                let buf =
                    image::ImageBuffer::from_raw(dds.header.width, dds.header.height, buf.into())
                        .context("Can't create DDS ImageBuffer with given res")?;
                _ = sender.send(Frame::new_still(buf));
                return Ok(receiver);
            }
        }
        #[cfg(feature = "dav1d")]
        "avif" => {
            let mut file = File::open(img_location)?;
            let mut buf = vec![];
            file.read_to_end(&mut buf)?;
            let i = libavif_image::read(buf.as_slice())?;
            _ = sender.send(Frame::new_still(i.to_rgba8()));
            return Ok(receiver);

            // col.add_still(i.to_rgba8());
        }
        #[cfg(feature = "avif_native")]
        #[cfg(not(feature = "dav1d"))]
        "avif" => {
            let mut file = File::open(img_location)?;
            let avif = avif_decode::Decoder::from_reader(&mut file)?.to_image()?;
            match avif {
                avif_decode::Image::Rgb8(img) => {
                    let mut img_buffer = vec![];
                    let (buf, width, height) = img.into_contiguous_buf();
                    for b in buf {
                        img_buffer.push(b.r);
                        img_buffer.push(b.g);
                        img_buffer.push(b.b);
                        img_buffer.push(255);
                    }

                    let buf = image::ImageBuffer::from_vec(width as u32, height as u32, img_buffer)
                        .context("Can't create avif ImageBuffer with given res")?;
                    _ = sender.send(Frame::new_still(buf));
                    return Ok(receiver);

                    // col.add_still(buf);
                }
                avif_decode::Image::Rgba8(img) => {
                    let mut img_buffer = vec![];
                    let (buf, width, height) = img.into_contiguous_buf();
                    for b in buf {
                        img_buffer.push(b.r);
                        img_buffer.push(b.g);
                        img_buffer.push(b.b);
                        img_buffer.push(b.a);
                    }

                    let buf = image::ImageBuffer::from_vec(width as u32, height as u32, img_buffer)
                        .context("Can't create avif ImageBuffer with given res")?;
                    _ = sender.send(Frame::new_still(buf));
                    return Ok(receiver);

                    // col.add_still(buf);
                }
                _ => {
                    anyhow::bail!("This avif is not yet supported.")
                }
            }
        }
        "svg" => {
            // TODO: Should the svg be scaled? if so by what number?
            // This should be specified in a smarter way, maybe resolution * x?

            let render_scale = 2.;
            let mut opt = usvg::Options::default();
            opt.font_family = "Inter".into();
            opt.font_size = 6.;

            let svg_data = std::fs::read(img_location)?;
            if let Ok(mut tree) = usvg::Tree::from_data(&svg_data, &opt) {
                let pixmap_size = resvg::IntSize::from_usvg(tree.size);

                let scaled_size = (
                    (pixmap_size.width() as f32 * render_scale) as u32,
                    (pixmap_size.height() as f32 * render_scale) as u32,
                );

                if let Some(mut pixmap) = tiny_skia::Pixmap::new(scaled_size.0, scaled_size.1) {
                    let mut fontdb = usvg::fontdb::Database::new();
                    fontdb.load_system_fonts();
                    fontdb.load_font_data(FONT.to_vec());
                    // for f in fontdb.faces() {
                    //     info!("{:?}",f.post_script_name);
                    // }
                    fontdb.set_cursive_family("Inter");
                    fontdb.set_sans_serif_family("Inter");
                    fontdb.set_serif_family("Inter");
                    tree.convert_text(&fontdb);

                    let rtree = resvg::Tree::from_usvg(&tree);

                    rtree.render(
                        tiny_skia::Transform::from_scale(render_scale, render_scale),
                        &mut pixmap.as_mut(),
                    );
                    let buf: RgbaImage = image::ImageBuffer::from_raw(
                        scaled_size.0,
                        scaled_size.1,
                        pixmap.data().to_vec(),
                    )
                    .context("Can't create image buffer from SVG render")?;

                    _ = sender.send(Frame::new_still(buf));
                    return Ok(receiver);
                }
            }
        }
        "exr" => {
            let reader = exrs::read()
                .no_deep_data()
                .largest_resolution_level()
                .rgba_channels(
                    |resolution, _channels: &RgbaChannels| -> RgbaImage {
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
                Image<Layer<SpecificChannels<RgbaImage, RgbaChannels>>>,
                exrs::Error,
            > = reader.from_file(&img_location);

            match maybe_image {
                Ok(image) => {
                    let buf = image.layer_data.channel_data.pixels;
                    _ = sender.send(Frame::new_still(buf));
                    return Ok(receiver);
                    // return Ok(OpenResult::still(buf));

                    // col.add_still(png_buffer);
                }
                Err(e) => error!("{} from {:?}", e, img_location),
            }
        }
        "nef" | "cr2" | "dng" | "mos" | "erf" | "raf" | "arw" | "3fr" | "ari" | "srf" | "sr2"
        | "braw" | "r3d" | "nrw" | "raw" => {
            debug!("Loading RAW");

            let export_job = Export::new(
                Input::ByFile(&img_location.to_string_lossy()),
                Output::new(
                    DemosaicingMethod::SuperPixel,
                    data::XYZ2SRGB,
                    data::GAMMA_SRGB,
                    OutputType::Raw16,
                    true,
                    true,
                ),
            )?;

            let (image, width, height) = export_job.export_16bit_image();
            let image = image
                .into_par_iter()
                .map(|x| tonemap_f32(x as f32 / 65536.))
                .collect::<Vec<_>>();

            // Construct rgb image
            let x = RgbImage::from_raw(width as u32, height as u32, image)
                .context("can't decode raw output as image")?;
            // make it a Dynamic image
            let buf = DynamicImage::ImageRgb8(x).to_rgba8();
            // return Ok(OpenResult::still(d.to_rgba8()));
            _ = sender.send(Frame::new_still(buf));
            return Ok(receiver);

            // col.add_still(d.to_rgba8());
        }
        "jxl" => {
            //TODO this needs to be a thread

            fn foo(img_location: &Path, frame_sender: Sender<Frame>) -> Result<()> {
                let mut image = JxlImage::open(img_location).map_err(|e| anyhow!("{e}"))?;
                let mut renderer = image.renderer();

                debug!("{:#?}", renderer.image_header().metadata);
                let is_jxl_anim = renderer.image_header().metadata.animation.is_some();
                let ticks_ms = renderer
                    .image_header()
                    .metadata
                    .animation
                    .as_ref()
                    .map(|hdr| hdr.tps_numerator as f32 / hdr.tps_denominator as f32)
                    // map this into milliseconds
                    .map(|x| 1000. / x)
                    .map(|x| x as u16)
                    .unwrap_or(40);
                debug!("TPS: {ticks_ms}");
                loop {
                    // create a mutable image to hold potential decoding results. We can then use this only once at the end of the loop/
                    let image_result: DynamicImage;
                    let result = renderer
                        .render_next_frame()
                        .map_err(|e| anyhow!("{e}"))
                        .context("Can't render JXL")?;
                    match result {
                        RenderResult::Done(render) => {
                            let frame_duration = render.duration() as u16 * ticks_ms;
                            debug!("duration {frame_duration} ms");
                            let framebuffer = render.image();
                            debug!("{:?}", renderer.pixel_format());
                            match renderer.pixel_format() {
                                PixelFormat::Graya => {
                                    let float_image = GrayAlphaImage::from_raw(
                                        framebuffer.width() as u32,
                                        framebuffer.height() as u32,
                                        framebuffer
                                            .buf()
                                            .par_iter()
                                            .map(|x| x * 255. + 0.5)
                                            .map(|x| x as u8)
                                            .collect::<Vec<_>>(),
                                    )
                                    .context("Can't decode gray alpha buffer")?;
                                    image_result = DynamicImage::ImageLumaA8(float_image);
                                }
                                PixelFormat::Gray => {
                                    let float_image = image::GrayImage::from_raw(
                                        framebuffer.width() as u32,
                                        framebuffer.height() as u32,
                                        framebuffer
                                            .buf()
                                            .par_iter()
                                            .map(|x| x * 255. + 0.5)
                                            .map(|x| x as u8)
                                            .collect::<Vec<_>>(),
                                    )
                                    .context("Can't decode gray buffer")?;
                                    image_result = DynamicImage::ImageLuma8(float_image);
                                }
                                PixelFormat::Rgba => {
                                    let float_image = RgbaImage::from_raw(
                                        framebuffer.width() as u32,
                                        framebuffer.height() as u32,
                                        framebuffer
                                            .buf()
                                            .par_iter()
                                            .map(|x| x * 255. + 0.5)
                                            .map(|x| x as u8)
                                            .collect::<Vec<_>>(),
                                    )
                                    .context("Can't decode rgba buffer")?;
                                    image_result = DynamicImage::ImageRgba8(float_image);
                                }
                                PixelFormat::Rgb => {
                                    let float_image = RgbImage::from_raw(
                                        framebuffer.width() as u32,
                                        framebuffer.height() as u32,
                                        framebuffer
                                            .buf()
                                            .par_iter()
                                            .map(|x| x * 255. + 0.5)
                                            .map(|x| x as u8)
                                            .collect::<Vec<_>>(),
                                    )
                                    .context("Can't decode rgb buffer")?;
                                    image_result = DynamicImage::ImageRgb8(float_image);
                                }
                                _ => {
                                    bail!("JXL: Pixel format: {:?}", renderer.pixel_format())
                                }
                            }

                            // Dispatch to still or animation
                            if is_jxl_anim {
                                // col.add_anim_frame(image_result.to_rgba8(), frame_duration);
                                _ = frame_sender.send(Frame::new(
                                    image_result.to_rgba8(),
                                    frame_duration,
                                    FrameSource::Animation,
                                ));
                            } else {
                                // col.add_still(image_result.to_rgba8());
                                _ = frame_sender.send(Frame::new_still(image_result.to_rgba8()));
                            }
                        }
                        RenderResult::NeedMoreData => {
                            info!("Need more data in JXL");
                        }
                        RenderResult::NoMoreFrames => break,
                    }
                }
                debug!("Done decoding JXL");

                Ok(())
            }

            std::thread::spawn(move || {
                if let Err(e) = foo(&img_location, sender) {
                    error!("{e}");
                }
            });
            return Ok(receiver);
        }
        "hdr" => {
            let f = File::open(img_location)?;
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

            let buf = RgbaImage::from_raw(meta.width, meta.height, s)
                .context("Failed to create RgbaImage with given dimensions")?;
            // col.add_still(buf);
            _ = sender.send(Frame::new_still(buf));
            return Ok(receiver);
        }
        "psd" => {
            let mut file = File::open(img_location)?;
            let mut contents = vec![];
            file.read_to_end(&mut contents)?;

            let psd = Psd::from_bytes(&contents).map_err(|e| anyhow!("{:?}", e))?;
            let buf = image::ImageBuffer::from_raw(psd.width(), psd.height(), psd.rgba())
                .context("Can't create imagebuffer from PSD")?;

            _ = sender.send(Frame::new_still(buf));
            return Ok(receiver);
        }
        "webp" => {
            let mut file = File::open(img_location)?;
            let mut contents = vec![];
            file.read_to_end(&mut contents)?;
            let buf = decode_webp(&contents).context("Can't decode webp")?;
            _ = sender.send(Frame::new_still(buf));
            return Ok(receiver);
        }
        "png" => {
            let file = File::open(img_location)?;
            let bufread = BufReader::new(file);
            let mut reader = image::io::Reader::new(bufread).with_guessed_format()?;
            reader.no_limits();
            _ = sender.send(Frame::new_still(reader.decode()?.into_rgba8()));
            return Ok(receiver);
            // col.add_still(reader.decode()?.into_rgba8());
        }
        "gif" => {
            let file = File::open(img_location)?;

            // Below is a workaround for partially corrupt gifs.
            let mut gif_opts = gif::DecodeOptions::new();
            gif_opts.set_color_output(gif::ColorOutput::Indexed);
            let mut decoder = gif_opts.read_info(file)?;
            let dim = (decoder.width() as u32, decoder.height() as u32);
            let mut screen = gif_dispose::Screen::new_decoder(&decoder);
            loop {
                if let Ok(i) = decoder.read_next_frame() {
                    debug!("decoded frame");
                    if let Some(frame) = i {
                        screen.blit_frame(&frame)?;
                        let buf: Option<image::RgbaImage> = image::ImageBuffer::from_raw(
                            dim.0,
                            dim.1,
                            screen.pixels.buf().as_bytes().to_vec(),
                        );
                        _ = sender.send(Frame::new(
                            buf.context("Can't read gif frame")?,
                            frame.delay * 10,
                            FrameSource::Animation,
                        ));
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            debug!("Done decoding Gif!");

            return Ok(receiver);

            // TODO: Re-enable if https://github.com/image-rs/image/issues/1818 is resolved

            // let gif_decoder = GifDecoder::new(file)?;
            // let frames = gif_decoder.into_frames().collect_frames()?;
            // for f in frames {
            //     let delay = f.delay().numer_denom_ms().0 as u16;
            //     col.add_anim_frame(f.into_buffer(), delay);
            //     col.repeat = true;
            // }
        }
        #[cfg(feature = "turbo")]
        "jpg" | "jpeg" => {
            let jpeg_data = std::fs::read(img_location)?;
            let buf: RgbaImage = turbojpeg::decompress_image(&jpeg_data)?;
            _ = sender.send(Frame::new_still(buf));
            return Ok(receiver);
            // col.add_still(img);
        }
        "tif" | "tiff" => {
            debug!("TIFF");
            let data = File::open(img_location)?;

            let mut decoder = tiff::decoder::Decoder::new(&data)?.with_limits(Limits::unlimited());
            let dim = decoder.dimensions()?;
            debug!("Color type: {:?}", decoder.colortype());
            let result = decoder.read_image()?;
            // A container for the low dynamic range image
            let ldr_img: Vec<u8>;

            match result {
                tiff::decoder::DecodingResult::U8(contents) => {
                    debug!("TIFF U8");
                    ldr_img = contents;
                }
                tiff::decoder::DecodingResult::U16(contents) => {
                    debug!("TIFF U16");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, u16::MIN as f32, u16::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::U32(contents) => {
                    debug!("TIFF U32");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, u32::MIN as f32, u32::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::U64(contents) => {
                    debug!("TIFF U64");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, u64::MIN as f32, u64::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::F32(contents) => {
                    debug!("TIFF F32");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p, 0.0, 1.0, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::F64(contents) => {
                    debug!("TIFF F64");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, 0.0, 1.0, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::I8(contents) => {
                    debug!("TIFF I8");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, i8::MIN as f32, i8::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::I16(contents) => {
                    debug!("TIFF I16");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, i16::MIN as f32, i16::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::I32(contents) => {
                    debug!("TIFF I32");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, i32::MIN as f32, i32::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::I64(contents) => {
                    debug!("TIFF I64");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, i64::MIN as f32, i64::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
            }

            match decoder.colortype()? {
                tiff::ColorType::Gray(_) => {
                    debug!("Loading gray color");
                    let i = image::GrayImage::from_raw(dim.0, dim.1, ldr_img)
                        .context("Can't load gray img")?;
                    // col.add_still(DynamicImage::ImageLuma8(i).into_rgba8());
                    _ = sender.send(Frame::new_still(DynamicImage::ImageLuma8(i).into_rgba8()));
                    return Ok(receiver);
                }
                tiff::ColorType::RGB(_) => {
                    debug!("Loading rgb color");
                    let i = image::RgbImage::from_raw(dim.0, dim.1, ldr_img)
                        .context("Can't load RGB img")?;
                    // col.add_still(DynamicImage::ImageRgb8(i).into_rgba8());
                    _ = sender.send(Frame::new_still(DynamicImage::ImageRgb8(i).into_rgba8()));
                    return Ok(receiver);
                }
                tiff::ColorType::RGBA(_) => {
                    debug!("Loading rgba color");
                    let i = image::RgbaImage::from_raw(dim.0, dim.1, ldr_img)
                        .context("Can't load RGBA img")?;
                    // col.add_still(i);
                    _ = sender.send(Frame::new_still(i));
                    return Ok(receiver);
                }
                tiff::ColorType::GrayA(_) => {
                    debug!("Loading gray color with alpha");
                    let i = image::GrayAlphaImage::from_raw(dim.0, dim.1, ldr_img)
                        .context("Can't load gray alpha img")?;
                    // col.add_still(image::DynamicImage::ImageLumaA8(i).into_rgba8());
                    _ = sender.send(Frame::new_still(
                        image::DynamicImage::ImageLumaA8(i).into_rgba8(),
                    ));
                    return Ok(receiver);
                }
                _ => {
                    bail!(
                        "Error: This TIFF image type is unsupported, please open a ticket! {:?}",
                        decoder.colortype()
                    )
                }
            }
        }
        _ => {
            // All other supported image files are handled by using `image`
            let img = image::open(img_location)?;
            // col.add_still(img.to_rgba8());
            _ = sender.send(Frame::new_still(img.to_rgba8()));
            return Ok(receiver);
        }
    }

    Ok(receiver)
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
            .with_mipmaps(true)
            .with_format(notan::prelude::TextureFormat::SRgba8)
            // .with_premultiplied_alpha()
            .with_filter(TextureFilter::Linear, TextureFilter::Nearest)
            // .with_wrap(TextureWrap::Clamp, TextureWrap::Clamp)
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
        if let Err(e) = gfx.update_texture(texture).with_data(self).update() {
            error!("{e}");
        }
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

pub fn prev_image(state: &mut OculanteState) {
    if let Some(img_location) = state.current_path.as_mut() {
        let next_img = state.scrubber.prev();
        // prevent reload if at last or first
        if &next_img != img_location {
            state.is_loaded = false;
            *img_location = next_img;
            state
                .player
                .load(img_location, state.message_channel.0.clone());
        }
    }
}

pub fn load_image_from_path(p: &Path, state: &mut OculanteState) {
    state.is_loaded = false;
    state.player.load(p, state.message_channel.0.clone());
    state.current_path = Some(p.to_owned());
}

pub fn last_image(state: &mut OculanteState) {
    if let Some(img_location) = state.current_path.as_mut() {
        let last = state.scrubber.len().saturating_sub(1);
        let next_img = state.scrubber.set(last);
        // prevent reload if at last or first
        if &next_img != img_location {
            state.is_loaded = false;
            *img_location = next_img;
            state
                .player
                .load(img_location, state.message_channel.0.clone());
        }
    }
}

pub fn first_image(state: &mut OculanteState) {
    if let Some(img_location) = state.current_path.as_mut() {
        let next_img = state.scrubber.set(0);
        // prevent reload if at last or first
        if &next_img != img_location {
            state.is_loaded = false;
            *img_location = next_img;
            state
                .player
                .load(img_location, state.message_channel.0.clone());
        }
    }
}

pub fn next_image(state: &mut OculanteState) {
    if let Some(img_location) = state.current_path.as_mut() {
        let next_img = state.scrubber.next();
        // prevent reload if at last or first
        if &next_img != img_location {
            state.is_loaded = false;
            *img_location = next_img;
            state
                .player
                .load(img_location, state.message_channel.0.clone());
        }
    }
}

/// Set the window title
pub fn set_title(app: &mut App, state: &mut OculanteState) {
    let p = state.current_path.clone().unwrap_or_default();

    let mut title_string = state
        .persistent_settings
        .title_format
        .replacen("{APP}", env!("CARGO_PKG_NAME"), 10)
        .replacen("{VERSION}", env!("CARGO_PKG_VERSION"), 10)
        .replacen("{FULLPATH}", &format!("{}", p.display()), 10)
        .replacen(
            "{FILENAME}",
            &p.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default(),
            10,
        )
        .replacen(
            "{RES}",
            &format!("{}x{}", state.image_dimension.0, state.image_dimension.1),
            10,
        );

    if state.persistent_settings.zen_mode {
        title_string.push_str(&format!(
            "          '{}' to disable zen mode",
            lookup(&state.persistent_settings.shortcuts, &InputEvent::ZenMode)
        ));
    }

    app.window().set_title(&title_string);
}

pub fn compare_next(state: &mut OculanteState) {
    if let Some(p) = &(state.current_path).clone() {
        let mut compare_list: Vec<(PathBuf, ImageGeometry)> =
            state.compare_list.clone().into_iter().collect();
        compare_list.sort_by(|a, b| a.0.cmp(&b.0));

        let index = compare_list.iter().position(|x| &x.0 == p).unwrap_or(0);
        let index = if index + 1 < compare_list.len() {
            index + 1
        } else {
            0
        };

        if let Some(c) = compare_list.get(index) {
            let path = &c.0;
            let geo = &c.1;
            state.image_geometry = geo.clone();
            state.is_loaded = false;
            state.current_image = None;
            state.player.load(path, state.message_channel.0.clone());
            state.current_path = Some(path.clone());
            state.persistent_settings.keep_view = true;
        }
    }
}

fn fit(oldvalue: f32, oldmin: f32, oldmax: f32, newmin: f32, newmax: f32) -> f32 {
    (((oldvalue - oldmin) * (newmax - newmin)) / (oldmax - oldmin)) + newmin
}

pub fn toggle_zen_mode(state: &mut OculanteState, app: &mut App) {
    state.persistent_settings.zen_mode = !state.persistent_settings.zen_mode;
    if state.persistent_settings.zen_mode {
        _ = state.message_channel.0.send(format!(
            "Zen mode on. Press '{}' to toggle.",
            lookup(&state.persistent_settings.shortcuts, &InputEvent::ZenMode)
        ));
    }
    set_title(app, state);
}
