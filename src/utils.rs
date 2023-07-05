use arboard::Clipboard;

// use image::codecs::gif::GifDecoder;

use log::{debug, error, info};
use nalgebra::{clamp, Vector2};
use notan::graphics::Texture;
use notan::prelude::{App, Graphics, TextureFilter};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use rayon::slice::ParallelSliceMut;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;

use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use image::{self};
use image::{EncodableLayout, Rgba, RgbaImage};
use std::sync::mpsc::{self};
use std::sync::mpsc::{Receiver, Sender};
use strum::Display;
use strum_macros::EnumIter;

use crate::appstate::{ImageGeometry, OculanteState, Message};
use crate::cache::Cache;
use crate::image_editing::{self, ImageOperation};
use crate::image_loader::open_image;
use crate::shortcuts::{lookup, InputEvent, Shortcuts};

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

        let num_pixels = img.width() as usize * img.height() as usize;
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
        }

        let mut green_histogram: Vec<(i32, i32)> = green_histogram
            .par_iter()
            .map(|(k, v)| (*k as i32, *v as i32))
            .collect();
        green_histogram.par_sort_by(|a, b| a.0.cmp(&b.0));

        let mut red_histogram: Vec<(i32, i32)> = red_histogram
            .par_iter()
            .map(|(k, v)| (*k as i32, *v as i32))
            .collect();
        red_histogram.par_sort_by(|a, b| a.0.cmp(&b.0));

        let mut blue_histogram: Vec<(i32, i32)> = blue_histogram
            .par_iter()
            .map(|(k, v)| (*k as i32, *v as i32))
            .collect();
        blue_histogram.par_sort_by(|a, b| a.0.cmp(&b.0));

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

    pub fn load(&mut self, img_location: &Path, message_sender: Sender<Message>) {
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
    message_sender: Sender<Message>,
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
                            _ = message_sender.send(Message::warn("This image exceeded the maximum resolution and will be be scaled down."));
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
                                filter: image_editing::ScaleFilter::Box,
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
                _ = message_sender.send(Message::LoadError(e.to_string()));
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
    pub fn new(buffer: RgbaImage, delay: u16, source: FrameSource) -> Frame {
        Frame {
            buffer,
            delay,
            source,
        }
    }

    pub fn new_reset(buffer: RgbaImage) -> Frame {
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

pub fn fit(oldvalue: f32, oldmin: f32, oldmax: f32, newmin: f32, newmax: f32) -> f32 {
    (((oldvalue - oldmin) * (newmax - newmin)) / (oldmax - oldmin)) + newmin
}

pub fn toggle_zen_mode(state: &mut OculanteState, app: &mut App) {
    state.persistent_settings.zen_mode = !state.persistent_settings.zen_mode;
    if state.persistent_settings.zen_mode {
        _ = state.message_channel.0.send(Message::Info(format!(
            "Zen mode on. Press '{}' to toggle.",
            lookup(&state.persistent_settings.shortcuts, &InputEvent::ZenMode)
        )));
    }
    set_title(app, state);
}
