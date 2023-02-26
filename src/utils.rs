use arboard::Clipboard;
use dds::DDS;
use exr;
// use image::codecs::gif::GifDecoder;
use exr::prelude as exrs;
use exr::prelude::*;
use image::{DynamicImage, EncodableLayout, RgbImage, RgbaImage};
use log::{debug, error, info};
use nalgebra::{clamp, Vector2};
use notan::graphics::Texture;
use notan::prelude::{App, Graphics, TextureFilter};
use notan::AppState;
use quickraw::{data, DemosaicingMethod, Export, Input, Output, OutputType};
use rayon::prelude::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rayon::slice::ParallelSliceMut;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use tonemap::filmic::*;

use anyhow::{anyhow, bail, Context, Result};
use image::Rgba;
use image::{self};
use libwebp_sys::{WebPDecodeRGBA, WebPGetInfo};
use psd::Psd;
use rgb::*;
use std::io::Read;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, Sender};
use strum::Display;
use strum_macros::EnumIter;

use crate::cache::Cache;
use crate::image_editing::EditState;
use crate::scrubber::Scrubber;
use crate::settings::PersistentSettings;
use crate::shortcuts::{lookup, InputEvent, Shortcuts};

pub const SUPPORTED_EXTENSIONS: &'static [&'static str] = &[
    "bmp",
    "dds",
    "exr",
    "ff",
    "gif",
    "hdr",
    "ico",
    "jpeg",
    "jpg",
    "png",
    "pnm",
    "psd",
    "svg",
    "tga",
    "tif",
    "tiff",
    "webp",
    "nef",
    "cr2",
    "dng",
    "mos",
    "erf",
    "raf",
    "arw",
    "3fr",
    "ari",
    "srf",
    "sr2",
    "braw",
    "r3d",
    "nrw",
    "raw",
    "avif",
    #[cfg(feature = "jpgxl")]
    "jxl",
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

            let mut p = p.clone();
            p.0[3] = 255;
            colors.insert(p.clone());
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
    pub frame_sender: Sender<FrameCollection>,
    pub image_sender: Sender<Frame>,
    pub stop_sender: Sender<()>,
    pub cache: Cache,
}

impl Player {
    pub fn new(image_sender: Sender<Frame>, cache_size: usize) -> Player {
        let (frame_sender, _): (Sender<FrameCollection>, Receiver<FrameCollection>) =
            mpsc::channel();
        let (stop_sender, _): (Sender<()>, Receiver<()>) = mpsc::channel();
        Player {
            frame_sender,
            image_sender,
            stop_sender,
            cache: Cache {
                data: Default::default(),
                cache_size,
            },
        }
    }

    pub fn load(&mut self, img_location: &Path, message_sender: Sender<String>) {
        self.stop();
        let (stop_sender, stop_receiver): (Sender<()>, Receiver<()>) = mpsc::channel();
        self.stop_sender = stop_sender;

        if let Some(cached_image) = self.cache.get(img_location) {
            _ = self.image_sender.send(Frame::new_still(cached_image));
            info!("Cache hit for {}", img_location.display());
            return;
        }

        send_image_threaded(
            &img_location,
            self.image_sender.clone(),
            message_sender,
            stop_receiver,
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
) {
    let loc = img_location.to_owned();

    thread::spawn(move || {
        match open_image(&loc) {
            Ok(col) => {
                let cycles = if col.repeat { 200 } else { 1 };

                if col.repeat && col.frames.len() > 1 {
                    let mut i = 0;

                    // Send reset frame
                    if let Some(f) = col.frames.first() {
                        _ = texture_sender
                            .clone()
                            .send(Frame::new_reset(f.buffer.clone()));
                    }

                    while i < cycles {
                        // let frames = col.frames.clone();
                        for frame in &col.frames {
                            if stop_receiver.try_recv().is_ok() {
                                info!("Stopped from receiver.");
                                return;
                            }
                            let _ = texture_sender.send(frame.clone());
                            if frame.delay > 0 {
                                thread::sleep(Duration::from_millis(frame.delay as u64));
                            } else {
                                thread::sleep(Duration::from_millis(40 as u64));
                            }
                        }
                        i += 1;
                    }
                } else {
                    // single frame. This saves one clone().
                    for frame in col.frames {
                        let _ = texture_sender.send(frame);
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
    Animation,
    Still,
    EditResult,
    Reset,
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
            source: FrameSource::Reset,
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
/// A collection of frames that can loop/repeat
#[derive(Debug, Default, Clone)]
pub struct FrameCollection {
    pub frames: Vec<Frame>,
    pub repeat: bool,
}

impl FrameCollection {
    fn add_anim_frame(&mut self, buffer: RgbaImage, delay: u16) {
        self.frames
            .push(Frame::new(buffer, delay, FrameSource::Animation))
    }
    fn add_still(&mut self, buffer: RgbaImage) {
        self.frames.push(Frame::new(buffer, 0, FrameSource::Still))
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
    pub fn hotkey(&self, shortcuts: &Shortcuts) -> String {
        match self {
            Self::Red => lookup(shortcuts, &InputEvent::RedChannel),
            Self::Green => lookup(shortcuts, &InputEvent::GreenChannel),
            Self::Blue => lookup(shortcuts, &InputEvent::BlueChannel),
            Self::Alpha => lookup(shortcuts, &InputEvent::AlphaChannel),
            Self::RGB => lookup(shortcuts, &InputEvent::RGBChannel),
            Self::RGBA => lookup(shortcuts, &InputEvent::RGBAChannel),
        }
    }
}

/// The state of the application
#[derive(Debug, AppState)]
pub struct OculanteState {
    /// The scale of the displayed image
    pub scale: f32,
    pub scale_increment: f32,
    /// Image offset on canvas
    pub offset: Vector2<f32>,
    pub drag_enabled: bool,
    pub reset_image: bool,
    pub message: Option<String>,
    /// Is the image fully loaded?
    pub is_loaded: bool,
    pub window_size: Vector2<f32>,
    pub cursor: Vector2<f32>,
    pub cursor_relative: Vector2<f32>,
    pub image_dimension: (u32, u32),
    pub sampled_color: [f32; 4],
    /// Show the image info panal
    pub info_enabled: bool,
    pub mouse_delta: Vector2<f32>,
    pub texture_channel: (Sender<Frame>, Receiver<Frame>),
    pub message_channel: (Sender<String>, Receiver<String>),
    pub extended_info_channel: (Sender<ExtendedImageInfo>, Receiver<ExtendedImageInfo>),
    pub extended_info_loading: bool,
    /// The Player, responsible for loading and sending Frames
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
    pub key_grab: bool,
    pub edit_state: EditState,
    pub pointer_over_ui: bool,
    /// Things that perisist between launches
    pub persistent_settings: PersistentSettings,
    pub always_on_top: bool,
    pub network_mode: bool,
    /// how long the toast message appears
    pub toast_cooldown: f32,
    pub fullscreen_offset: Option<(i32, i32)>,
    /// List of images to cycle through. Usually the current dir or dropped files
    pub scrubber: Scrubber,
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
            player: Player::new(tx_channel.0.clone(), 20),
            texture_channel: tx_channel,
            message_channel: mpsc::channel(),
            extended_info_channel: mpsc::channel(),
            extended_info_loading: Default::default(),
            mouse_delta: Default::default(),
            current_texture: Default::default(),
            current_image: Default::default(),
            current_path: Default::default(),
            current_channel: Channel::RGBA,
            settings_enabled: Default::default(),
            edit_enabled: Default::default(),
            image_info: Default::default(),
            tiling: 1,
            mouse_grab: Default::default(),
            key_grab: Default::default(),
            edit_state: Default::default(),
            pointer_over_ui: Default::default(),
            persistent_settings: Default::default(),
            always_on_top: Default::default(),
            network_mode: Default::default(),
            window_size: Default::default(),
            toast_cooldown: Default::default(),
            fullscreen_offset: Default::default(),
            scrubber: Default::default(),
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

// TODO:move to utils
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
        state.offset.x += window_pos.0 as f32;
        state.offset.y += window_pos.1 as f32;

        // save old window pos
        state.fullscreen_offset = Some(window_pos);
    } else {
        // info!("Is fullscreen {:?}", window_pos);

        if let Some(sf) = state.fullscreen_offset {
            state.offset.x -= sf.0 as f32;
            state.offset.y -= sf.1 as f32;
        }
    }
    app.window().set_fullscreen(!fullscreen);
}

/// Determine if an enxtension is compatible with oculante
pub fn is_ext_compatible(fname: &PathBuf) -> bool {
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
        tonemap_f32(px[0]),
        tonemap_f32(px[1]),
        tonemap_f32(px[2]),
        tonemap_f32(px[3]),
    ]
}

fn tonemap_f32(px: f32) -> u8 {
    // (px.powf(1.0 / 2.2).max(0.0).min(1.0) * 255.0) as u8
    (px.filmic() * 255.) as u8
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
pub fn open_image(img_location: &Path) -> Result<FrameCollection> {
    let img_location = img_location.clone();
    let mut col = FrameCollection::default();

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
                col.add_still(buf);
            }
        }
        #[cfg(feature = "dav1d")]
        "avif" => {
            let mut file = File::open(img_location)?;
            let mut buf = vec![];
            file.read_to_end(&mut buf)?;
            let i = libavif_image::read(buf.as_slice())?;
            col.add_still(i.to_rgba8());
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
                    col.add_still(buf);
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
                    col.add_still(buf);
                }
                _ => {
                    bail!("This avif is not yet supported.")
                }
            }
        }
        "svg" => {
            // TODO: Should the svg be scaled? if so by what number?
            // This should be specified in a smarter way, maybe resolution * x?
            let opt = usvg::Options::default();
            let svg_data = std::fs::read(img_location)?;
            if let Ok(rtree) = usvg::Tree::from_data(&svg_data, &opt) {
                let pixmap_size = rtree.size.to_screen_size();

                if let Some(mut pixmap) =
                    tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height())
                {
                    resvg::render(
                        &rtree,
                        usvg::FitTo::Original,
                        tiny_skia::Transform::identity(),
                        pixmap.as_mut(),
                    )
                    .context("Can't render SVG")?;
                    let buf: Option<RgbaImage> = image::ImageBuffer::from_raw(
                        pixmap_size.width(),
                        pixmap_size.height(),
                        pixmap.data().to_vec(),
                    );
                    if let Some(valid_buf) = buf {
                        col.add_still(valid_buf);
                    }
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
            > = reader.from_file(img_location);

            match maybe_image {
                Ok(image) => {
                    let png_buffer = image.layer_data.channel_data.pixels;
                    col.add_still(png_buffer);
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
            let d = DynamicImage::ImageRgb8(x);
            col.add_still(d.to_rgba8());
        }
        #[cfg(feature = "jpgxl")]
        "jxl" => {
            use jpegxl_rs::decoder_builder;
            use jpegxl_rs::image::ToDynamic;
            let sample = std::fs::read(&img_location)?;
            let decoder = decoder_builder().build()?;
            let img = decoder
                .decode_to_image(&sample)?
                .context("Can't decode image from jpgxl")?;
            col.add_still(img.into_rgba8());
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

            let tonemapped_buffer = RgbaImage::from_raw(meta.width, meta.height, s)
                .context("Failed to create RgbaImage with given dimensions")?;
            col.add_still(tonemapped_buffer);
        }
        "psd" => {
            let mut file = File::open(img_location)?;
            let mut contents = vec![];
            if file.read_to_end(&mut contents).is_ok() {
                let psd = Psd::from_bytes(&contents).map_err(|e| anyhow!("{:?}", e))?;
                if let Some(buf) =
                    image::ImageBuffer::from_raw(psd.width(), psd.height(), psd.rgba())
                {
                    col.add_still(buf);
                }
            }
        }
        "webp" => {
            let mut file = File::open(img_location)?;
            let mut contents = vec![];
            if file.read_to_end(&mut contents).is_ok() {
                match decode_webp(&contents) {
                    Some(webp_buf) => col.add_still(webp_buf),
                    None => println!("Error decoding data from {img_location:?}"),
                }
            }
        }
        "png" => {
            let file = File::open(img_location)?;
            let bufread = BufReader::new(file);
            let mut reader = image::io::Reader::new(bufread).with_guessed_format()?;
            reader.no_limits();
            col.add_still(reader.decode()?.into_rgba8());
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
                        col.add_anim_frame(buf.context("Can't read gif frame")?, frame.delay * 10);
                        col.repeat = true;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            // TODO: Re-enable if https://github.com/image-rs/image/issues/1818 is resolved

            // let gif_decoder = GifDecoder::new(file)?;
            // let frames = gif_decoder.into_frames().collect_frames()?;
            // for f in frames {
            //     let delay = f.delay().numer_denom_ms().0 as u16;
            //     col.add_anim_frame(f.into_buffer(), delay);
            //     col.repeat = true;
            // }
            debug!("Done decoding Gif!");
        }
        #[cfg(feature = "turbo")]
        "jpg" | "jpeg" => {
            let jpeg_data = std::fs::read(img_location)?;
            let img: RgbaImage = turbojpeg::decompress_image(&jpeg_data)?;
            col.add_still(img);
        }
        _ => {
            let img = image::open(img_location)?;
            col.add_still(img.to_rgba8());
        }
    }

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
                .load(&img_location, state.message_channel.0.clone());
        }
    }
}

/// Set the window title
pub fn set_title(app: &mut App, state: &mut OculanteState) {
    if let Some(p) = &state.current_path {
        app.window().set_title(
            &state
                .persistent_settings
                .title_format
                .replacen("{APP}", env!("CARGO_PKG_NAME"),10)
                .replacen("{VERSION}", env!("CARGO_PKG_VERSION"),10)
                .replacen("{FULLPATH}", &format!("{}", p.display()),10)
                .replacen(
                    "{FILENAME}",
                    &format!(
                        "{}",
                        p.file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_default()
                    ),
                   10,
                )
                .replacen(
                    "{RES}",
                    &format!("{}x{}", state.image_dimension.0, state.image_dimension.1),
                   10,
                ),
        );

    }
}