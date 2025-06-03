use arboard::Clipboard;

use img_parts::{Bytes, DynImage, ImageEXIF};
use log::{debug, error, info};
use nalgebra::{clamp, Vector2};
use notan::graphics::Texture;
use notan::prelude::{App, Graphics};
use rayon::prelude::ParallelIterator;
use rayon::slice::ParallelSliceMut;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsStr;

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use image::{self, DynamicImage, GenericImageView};
use image::{EncodableLayout, Rgba, RgbaImage};
use std::sync::mpsc::{self};
use std::sync::mpsc::{Receiver, Sender};
use strum::Display;
use strum_macros::EnumIter;

use crate::appstate::{ImageGeometry, Message, OculanteState};
use crate::cache::Cache;
use crate::image_loader::{open_image, rotate_dynimage};
use crate::settings::DecoderSettings;
use crate::shortcuts::{lookup, InputEvent, Shortcuts};

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
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
    "icns",
    "nrw",
    "raw",
    "avif",
    "jxl",
    "ppm",
    "dcm",
    "ima",
    "qoi",
    "ktx2",
    "kra",
    #[cfg(feature = "j2k")]
    "jp2",
    #[cfg(feature = "heif")]
    "heif",
    #[cfg(feature = "heif")]
    "heic",
    #[cfg(feature = "heif")]
    "heifs",
    #[cfg(feature = "heif")]
    "heics",
    #[cfg(feature = "heif")]
    "avci",
    #[cfg(feature = "heif")]
    "avcs",
    #[cfg(feature = "heif")]
    "hif",
];

fn is_pixel_fully_transparent(p: &Rgba<u8>) -> bool {
    p.0 == [0, 0, 0, 0]
}

#[derive(Debug, Clone, Default)]
pub struct DicomData {
    pub physical_size: (f32, f32),
    pub dicom_data: HashMap<String, String>,
}

#[derive(Debug, Clone, Default)]
pub struct ExtendedImageInfo {
    pub num_pixels: usize,
    pub num_transparent_pixels: usize,
    pub num_colors: usize,
    pub red_histogram: Vec<(i32, u64)>,
    pub green_histogram: Vec<(i32, u64)>,
    pub blue_histogram: Vec<(i32, u64)>,
    pub exif: HashMap<String, String>,
    pub dicom: Option<DicomData>,
    pub raw_exif: Option<Bytes>,
    pub name: String,
}

impl ExtendedImageInfo {
    pub fn with_exif(&mut self, image_path: &Path) -> Result<()> {
        self.name = image_path.to_string_lossy().to_string();
        if image_path.extension() == Some(OsStr::new("gif")) {
            return Ok(());
        }

        let input = std::fs::read(image_path)?;

        // Store original EXIF to write in in case of save event
        if let Some(d) = DynImage::from_bytes(input.clone().into())? {
            self.raw_exif = d.exif()
        }

        // User-friendly Exif in key/value form
        let mut c = Cursor::new(input);
        let exifreader = exif::Reader::new();
        let exif = exifreader.read_from_container(&mut c)?;
        // in case exif could not be set, for example for DNG or other "exotic" formats,
        // just bang in raw exif and let the writer deal with it later.
        // The good stuff is that this will be automagically preserved across formats.
        if self.raw_exif.is_none() {
            self.raw_exif = Some(exif.buf().to_vec().into());
        }
        for f in exif.fields() {
            self.exif.insert(
                f.tag.to_string(),
                f.display_value().with_unit(&exif).to_string(),
            );
        }
        Ok(())
    }

    pub fn with_dicom(&mut self, image_path: &Path) -> Result<()> {
        self.name = image_path.to_string_lossy().to_string();
        if image_path.extension() != Some(OsStr::new("dcm"))
            || image_path.extension() != Some(OsStr::new("ima"))
        {
            let obj = dicom_object::open_file(image_path)?;
            let mut dicom_data = HashMap::new();

            // WIP: Find out interesting items to display
            for name in &[
                "StudyDate",
                "ModalitiesInStudy",
                "Modality",
                "SourceType",
                "ImageType",
                "Manufacturer",
                "InstitutionName",
                "PrivateDataElement",
                "PrivateDataElementName",
                "OperatorsName",
                "ManufacturerModelName",
                "PatientName",
                "PatientBirthDate",
                "PatientAge",
                "PixelSpacing",
            ] {
                if let Ok(e) = obj.element_by_name(name) {
                    if let Ok(s) = e.to_str() {
                        info!("{name}: {s}");
                        dicom_data.insert(name.to_string(), s.to_string());
                    }
                }
            }
            self.dicom = Some(DicomData {
                physical_size: (0.0, 0.0),
                dicom_data,
            })
        }

        Ok(())
    }

    pub fn from_image(img: &RgbaImage) -> Self {
        let mut hist_r: [u64; 256] = [0; 256];
        let mut hist_g: [u64; 256] = [0; 256];
        let mut hist_b: [u64; 256] = [0; 256];

        let num_pixels = img.width() as usize * img.height() as usize;
        let mut num_transparent_pixels = 0;

        //Colors counting
        const FIXED_RGB_SIZE: usize = 24;
        const SUB_INDEX_SIZE: usize = 5;
        const MAIN_INDEX_SIZE: usize = 1 << (FIXED_RGB_SIZE - SUB_INDEX_SIZE);
        let mut color_map = vec![0u32; MAIN_INDEX_SIZE];

        for p in img.pixels() {
            if is_pixel_fully_transparent(p) {
                num_transparent_pixels += 1;
            }

            hist_r[p.0[0] as usize] += 1;
            hist_g[p.0[1] as usize] += 1;
            hist_b[p.0[2] as usize] += 1;

            //Store every existing color combination in a bit
            //Therefore we use a 24 bit index, splitted into a main and a sub index.
            let pos = u32::from_le_bytes([p.0[0], p.0[1], p.0[2], 0]);
            let pos_main = pos >> SUB_INDEX_SIZE;
            let pos_sub = pos - (pos_main << SUB_INDEX_SIZE);
            color_map[pos_main as usize] |= 1 << pos_sub;
        }

        let mut full_colors = 0u32;
        for &intensity in color_map.iter() {
            full_colors += intensity.count_ones();
        }

        let green_histogram: Vec<(i32, u64)> = hist_g
            .iter()
            .enumerate()
            .map(|(k, v)| (k as i32, *v))
            .collect();

        let red_histogram: Vec<(i32, u64)> = hist_r
            .iter()
            .enumerate()
            .map(|(k, v)| (k as i32, *v))
            .collect();

        let blue_histogram: Vec<(i32, u64)> = hist_b
            .iter()
            .enumerate()
            .map(|(k, v)| (k as i32, *v))
            .collect();

        Self {
            num_pixels,
            num_transparent_pixels,
            num_colors: full_colors as usize,
            blue_histogram,
            green_histogram,
            red_histogram,
            raw_exif: Default::default(),
            name: Default::default(),
            exif: Default::default(),
            dicom: Default::default(),
        }
    }
}

#[derive(Debug)]
pub struct Player {
    pub image_sender: Sender<Frame>,
    pub stop_sender: Sender<()>,
    pub message_sender: Sender<Message>,
    pub cache: Cache,
    watcher: HashMap<PathBuf, SystemTime>,
    decoder_opts: DecoderSettings,
}

impl Player {
    /// Create a new Player
    pub fn new(
        image_sender: Sender<Frame>,
        cache_size: usize,
        message_sender: Sender<Message>,
        decoder_opts: DecoderSettings,
    ) -> Player {
        let (stop_sender, _): (Sender<()>, Receiver<()>) = mpsc::channel();
        Player {
            image_sender,
            stop_sender,
            message_sender,
            cache: Cache {
                data: Default::default(),
                cache_size,
            },
            watcher: Default::default(),
            decoder_opts,
        }
    }

    pub fn check_modified(&mut self, path: &Path) {
        if let Some(watched_mod) = self.watcher.get(path) {
            if let Ok(meta) = std::fs::metadata(path) {
                if let Ok(modified) = meta.modified() {
                    if watched_mod != &modified {
                        debug!(
                            "Modified! read from meta {:?} stored: {:?}",
                            modified, watched_mod
                        );

                        self.cache.data.remove(path);
                        self.load(path);
                    }
                }
            }
        }
    }

    /// The main loading function of the player
    pub fn load_advanced(&mut self, img_location: &Path, forced_frame_source: Option<Frame>) {
        debug!("Stopping player on load");
        self.stop();
        let (stop_sender, stop_receiver): (Sender<()>, Receiver<()>) = mpsc::channel();
        self.stop_sender = stop_sender;

        if let Some(cached_image) = self.cache.get(img_location) {
            debug!("Cache hit for {}", img_location.display());

            let frame = Frame::new_still(cached_image);
            if let Some(fs) = forced_frame_source {
                debug!("Frame source set to {fs}");
                _ = self.image_sender.send(frame.transmute(fs));
            } else {
                _ = self.image_sender.send(frame);
            }
            return;
        }

        debug!("Image not in cache.");

        send_image_threaded(
            img_location,
            self.image_sender.clone(),
            self.message_sender.clone(),
            stop_receiver,
            forced_frame_source,
            self.decoder_opts,
        );

        if let Ok(meta) = std::fs::metadata(img_location) {
            if let Ok(modified) = meta.modified() {
                self.watcher.insert(img_location.into(), modified);
            }
        }
    }

    pub fn load(&mut self, img_location: &Path) {
        self.load_advanced(img_location, None);
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
    forced_frame_source: Option<Frame>,
    decoder_opts: DecoderSettings,
) {
    let loc = img_location.to_owned();

    let path = img_location.to_path_buf();
    thread::spawn(move || {
        let mut framecache = vec![];
        let mut timer = std::time::Instant::now();

        match open_image(&loc, Some(message_sender.clone()), Some(decoder_opts)) {
            Ok(frame_receiver) => {
                debug!("Got a frame receiver from opening image");

                let mut first = true;
                for mut f in frame_receiver.iter() {
                    if stop_receiver.try_recv().is_ok() {
                        debug!("Stopped from receiver.");
                        return;
                    }

                    match f {
                        Frame::Animation(ref buffer, delay) => {
                            framecache.push(f.clone());
                            if first {
                                _ = texture_sender
                                    .clone()
                                    .send(Frame::new_reset(buffer.clone()));
                            } else {
                                let _ = texture_sender.send(f.clone());
                            }
                            let elapsed = timer.elapsed().as_millis();
                            let wait_time_after_loading = delay.saturating_sub(elapsed as u16);
                            debug!("elapsed {elapsed}, wait {wait_time_after_loading}");
                            std::thread::sleep(Duration::from_millis(
                                wait_time_after_loading as u64,
                            ));
                            timer = std::time::Instant::now();
                        }
                        Frame::Still(ref mut buffer) => {
                            debug!("Received image in {:?}", timer.elapsed());
                            _ = rotate_dynimage(buffer, &path);

                            // TODO force frame sournce
                            if let Some(new_frame) = forced_frame_source {
                                debug!("Converting from {f} to {new_frame}");
                                let _ = texture_sender.send(f.transmute(new_frame));
                            } else {
                                let _ = texture_sender.send(f);
                            }
                            return;
                        }
                        _ => (),
                    }

                    first = false;
                }

                // loop over the image. For sanity, stop at a limit of iterations.
                for _ in 0..500 {
                    for frame in &framecache {
                        if stop_receiver.try_recv().is_ok() {
                            debug!("Stopped from receiver.");
                            return;
                        }

                        if let Frame::Animation(_, delay) = frame {
                            let _ = texture_sender.send(frame.clone());
                            if *delay > 0 {
                                //                                      cap at 60fps
                                thread::sleep(Duration::from_millis(*delay.max(&17) as u64));
                            } else {
                                thread::sleep(Duration::from_millis(40_u64));
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("{e}");
                _ = message_sender.send(Message::LoadError(format!("{e}")));
                _ = message_sender.send(Message::LoadError(format!(
                    "Failed to load {}",
                    path.display()
                )));
            }
        }
    });
}

/// A single frame
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Display)]
pub enum Frame {
    /// A regular still frame (most common)
    Still(DynamicImage),
    /// Part of an animation. Delay in ms
    Animation(DynamicImage, u16),
    /// First frame of animation. This is necessary to reset the image and stop the player.
    AnimationStart(DynamicImage),
    /// Result of an edit operation with image
    EditResult(DynamicImage),
    /// Only update the current texture.
    UpdateTexture,
    /// TODO: Replace with edit result. A result of a compare operation. Image keeps transform.
    CompareResult(DynamicImage, ImageGeometry),
    /// A member of a custom image collection, for example when dropping many files or opening the app with more than one file as argument
    ImageCollectionMember(DynamicImage),
}

impl Frame {
    pub fn new(source: Frame) -> Frame {
        source
    }

    pub fn new_reset(buffer: DynamicImage) -> Frame {
        Frame::AnimationStart(buffer)
    }

    pub fn new_animation(buffer: DynamicImage, delay_ms: u16) -> Frame {
        Frame::Animation(buffer, delay_ms)
    }

    #[allow(dead_code)]
    pub fn new_edit(buffer: DynamicImage) -> Frame {
        Frame::EditResult(buffer)
    }

    #[allow(dead_code)]
    pub fn new_empty_edit() -> Frame {
        Frame::UpdateTexture
    }

    pub fn new_still(buffer: DynamicImage) -> Frame {
        Frame::Still(buffer)
    }

    // Convert one `Frame` variant to something else, replacing its buffer.
    // This is useful to force a certain frame type.
    pub fn transmute(self, forced_variant: Self) -> Frame {
        let mut forced_variant = forced_variant;
        match &self {
            Frame::Still(img)
            | Frame::Animation(img, _)
            | Frame::AnimationStart(img)
            | Frame::EditResult(img)
            | Frame::CompareResult(img, _)
            | Frame::ImageCollectionMember(img) => match forced_variant {
                Frame::Still(ref mut image_buffer)
                | Frame::Animation(ref mut image_buffer, _)
                | Frame::AnimationStart(ref mut image_buffer)
                | Frame::EditResult(ref mut image_buffer)
                | Frame::CompareResult(ref mut image_buffer, _)
                | Frame::ImageCollectionMember(ref mut image_buffer) => *image_buffer = img.clone(),
                Frame::UpdateTexture => (),
            },
            Frame::UpdateTexture => (),
        }
        forced_variant
    }

    /// Return the image buffor of a `Frame`.
    pub fn get_image(&self) -> Option<DynamicImage> {
        match self {
            Frame::AnimationStart(img)
            | Frame::Still(img)
            | Frame::EditResult(img)
            | Frame::CompareResult(img, _)
            | Frame::Animation(img, _)
            | Frame::ImageCollectionMember(img) => Some(img.clone()),
            _ => None,
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

pub fn delete_file(state: &mut OculanteState) {
    if let Some(p) = &state.current_path {
        #[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
        {
            _ = trash::delete(p);
        }
        #[cfg(any(target_os = "netbsd", target_os = "freebsd"))]
        {
            _ = std::fs::remove_file(p)
        }

        state.send_message_info(&format!(
            "Deleted {}",
            p.file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default()
        ));
        // remove from cache so we don't suceed to load it agaim
        state.player.cache.data.remove(p);
    }
    clear_image(state);
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

pub fn solo_channel(img: &DynamicImage, channel: usize) -> DynamicImage {
    let mut updated_img = img.to_rgba8();
    updated_img.par_chunks_mut(4).for_each(|pixel| {
        pixel[0] = pixel[channel];
        pixel[1] = pixel[channel];
        pixel[2] = pixel[channel];
        pixel[3] = 255;
    });
    DynamicImage::ImageRgba8(updated_img)
}

pub fn unpremult(img: &DynamicImage) -> DynamicImage {
    // FIXME: Respect previous image format
    let mut updated_img = img.to_rgba8();
    updated_img.par_chunks_mut(4).for_each(|pixel| {
        pixel[3] = 255;
    });
    DynamicImage::ImageRgba8(updated_img)
}

/// Mark pixels with no alpha but color info
pub fn highlight_bleed(img: &DynamicImage) -> DynamicImage {
    let mut updated_img = img.to_rgba8();
    updated_img.par_chunks_mut(4).for_each(|pixel| {
        if pixel[3] == 0 && (pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0) {
            pixel[1] = pixel[1].saturating_add(100);
            pixel[3] = 255;
        }
    });
    DynamicImage::ImageRgba8(updated_img)
}

/// Mark pixels with transparency
pub fn highlight_semitrans(img: &DynamicImage) -> DynamicImage {
    let mut updated_img = img.to_rgba8();
    updated_img.par_chunks_mut(4).for_each(|pixel| {
        if pixel[3] != 0 && pixel[3] != 255 {
            pixel[1] = pixel[1].saturating_add(100);
            pixel[3] = pixel[1].saturating_add(100);
        }
    });
    DynamicImage::ImageRgba8(updated_img)
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
    current_image: &Option<DynamicImage>,
    current_path: &Option<PathBuf>,
    channel: &(Sender<ExtendedImageInfo>, Receiver<ExtendedImageInfo>),
) {
    if let Some(img) = current_image {
        let copied_img = img.to_rgba8();
        let sender = channel.0.clone();
        let current_path = current_path.clone();
        thread::spawn(move || {
            let mut e_info = ExtendedImageInfo::from_image(&copied_img);
            if let Some(p) = current_path {
                _ = e_info.with_exif(&p);
                _ = e_info.with_dicom(&p);
            }
            debug!("Sending extended info");
            _ = sender.send(e_info);
        });
    }
}

pub trait ImageExt {
    fn size_vec(&self) -> Vector2<f32> {
        unimplemented!()
    }

    fn to_texture_premult(&self, _: &mut Graphics) -> Option<Texture> {
        unimplemented!()
    }

    #[allow(unused)]
    fn update_texture(&self, _: &mut Graphics, _: &mut Texture) {
        unimplemented!()
    }

    #[allow(unused)]
    fn to_image(&self, _: &mut Graphics) -> Option<RgbaImage> {
        unimplemented!()
    }
}

impl ImageExt for RgbaImage {
    fn size_vec(&self) -> Vector2<f32> {
        Vector2::new(self.width() as f32, self.height() as f32)
    }

    fn to_texture_premult(&self, gfx: &mut Graphics) -> Option<Texture> {
        gfx.clean();

        gfx.create_texture()
            .from_bytes(self, self.width(), self.height())
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

impl ImageExt for DynamicImage {
    fn size_vec(&self) -> Vector2<f32> {
        Vector2::new(self.width() as f32, self.height() as f32)
    }

    fn to_texture_premult(&self, gfx: &mut Graphics) -> Option<Texture> {
        gfx.clean();

        gfx.create_texture()
            .from_bytes(&self.to_rgba8(), self.width(), self.height())
            .with_premultiplied_alpha()
            // .with_filter(TextureFilter::Linear, TextureFilter::Nearest)
            // .with_wrap(TextureWrap::Repeat, TextureWrap::Repeat)
            .build()
            .ok()
    }

    fn update_texture(&self, gfx: &mut Graphics, texture: &mut Texture) {
        if let Err(e) = gfx
            .update_texture(texture)
            .with_data(&self.as_bytes())
            .update()
        {
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

pub fn clipboard_copy(img: &DynamicImage) {
    if let Ok(clipboard) = &mut Clipboard::new() {
        let _ = clipboard.set_image(arboard::ImageData {
            width: img.width() as usize,
            height: img.height() as usize,
            bytes: std::borrow::Cow::Borrowed(img.to_rgba8().as_bytes()),
        });
    }
}

pub fn load_image_from_path(p: &Path, state: &mut OculanteState) {
    state.is_loaded = false;
    state.player.load(p);
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
            state.player.load(img_location);
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
            state.player.load(img_location);
        }
    }
}

/// clear the current image
pub fn clear_image(state: &mut OculanteState) {
    let next_img = state.scrubber.remove_current();
    debug!("Clearing image. Next is {}", next_img.display());
    if state.scrubber.entries.len() == 0 {
        state.current_image = None;
        state.current_texture.clear();
        state.current_path = None;
        state.image_metadata = None;
        return;
    }
    // prevent reload if at last or first
    if Some(&next_img) != state.current_path.as_ref() {
        state.is_loaded = false;
        state.current_path = Some(next_img.clone());
        state.player.load(&next_img);
    }
}

pub fn next_image(state: &mut OculanteState) {
    let next_img = state.scrubber.next();
    // prevent reload if at last or first
    if Some(&next_img) != state.current_path.as_ref() {
        state.is_loaded = false;
        state.current_path = Some(next_img.clone());
        state.player.load(&next_img);
    }
}

pub fn prev_image(state: &mut OculanteState) {
    let prev_img = state.scrubber.prev();
    // prevent reload if at last or first
    if Some(&prev_img) != state.current_path.as_ref() {
        state.is_loaded = false;
        state.current_path = Some(prev_img.clone());
        state.player.load(&prev_img);
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
            &format!(
                "{}x{}",
                state.image_geometry.dimensions.0, state.image_geometry.dimensions.1
            ),
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

/// Fix missing exif by re-applying exif to saved files
pub fn fix_exif(p: &Path, exif: Option<Bytes>) -> Result<()> {
    use std::fs::{self, File};
    let input = fs::read(p)?;
    let mut dynimage = DynImage::from_bytes(input.into())?.context("Unsupported EXIF format")?;
    dynimage.set_exif(exif);
    let output = File::create(p)?;
    dynimage.encoder().write_to(output)?;
    Ok(())
}

pub fn clipboard_to_image() -> Result<DynamicImage> {
    let clipboard = &mut Clipboard::new()?;

    let imagedata = clipboard.get_image()?;
    let image = image::RgbaImage::from_raw(
        imagedata.width as u32,
        imagedata.height as u32,
        (imagedata.bytes).to_vec(),
    )
    .context("Can't decode RgbaImage")?;

    Ok(DynamicImage::ImageRgba8(image))
}

pub fn set_zoom(scale: f32, from_center: Option<Vector2<f32>>, state: &mut OculanteState) {
    let delta = scale - state.image_geometry.scale;
    let zoom_point = from_center.unwrap_or(state.cursor);
    state.image_geometry.offset -= scale_pt(
        state.image_geometry.offset,
        zoom_point,
        state.image_geometry.scale,
        delta,
    );
    state.image_geometry.scale = scale;
}

pub fn get_pixel_checked(img: &DynamicImage, x: u32, y: u32) -> Option<Rgba<u8>> {
    if img.in_bounds(x, y) {
        return Some(img.get_pixel(x, y));
    }
    None
}
