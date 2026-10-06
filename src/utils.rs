use arboard::Clipboard;

use img_parts::{Bytes, DynImage, ImageEXIF};
use log::{debug, error, info};
use nalgebra::{Vector2, clamp};
use rayon::prelude::ParallelIterator;
use rayon::slice::ParallelSliceMut;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ffi::OsStr;

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::SystemTime;

use anyhow::{Context, Result};
use image::{self, DynamicImage, GenericImageView};
use image::{EncodableLayout, Rgba, RgbaImage};
use std::sync::mpsc::{self};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, OnceLock};
use strum::Display;
use strum::EnumIter;

use crate::appstate::{ImageGeometry, Message, OculanteState};
use crate::cache::Cache;
use crate::image_loader::{open_image, rotate_dynimage};
use crate::scrubber::find_first_image_in_directory;
use crate::settings::DecoderSettings;
use crate::shortcuts::{InputEvent, Shortcuts, lookup};

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
    "jfif",
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
    "ora",
    "otb",
    "pcx",
    "sgi",
    "wbmp",
    "xbm",
    "xpm",
    #[cfg(feature = "j2k")]
    "jp2",
    #[cfg(any(feature = "heif", feature = "heif_native"))]
    "heif",
    #[cfg(any(feature = "heif", feature = "heif_native"))]
    "heic",
    #[cfg(feature = "heif")]
    "heifs",
    #[cfg(feature = "heif")]
    "heics",
    #[cfg(feature = "heif")]
    "avci",
    #[cfg(feature = "heif")]
    "avcs",
    #[cfg(any(feature = "heif", feature = "heif_native"))]
    "hif",
];

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
                if let Ok(e) = obj.element_by_name(name)
                    && let Ok(s) = e.to_str()
                {
                    info!("{name}: {s}");
                    dicom_data.insert(name.to_string(), s.to_string());
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
        Self::from_pixels(
            img.as_raw()
                .chunks_exact(4)
                .map(|p| [p[0], p[1], p[2], p[3]]),
        )
    }

    /// Like `from_image`, but reads 8 bit images in the layout they have instead of
    /// converting the whole image to RGBA first.
    pub fn from_dynamic_image(img: &DynamicImage) -> Self {
        match img {
            DynamicImage::ImageRgba8(i) => Self::from_image(i),
            DynamicImage::ImageRgb8(i) => Self::from_pixels(
                i.as_raw()
                    .chunks_exact(3)
                    .map(|p| [p[0], p[1], p[2], u8::MAX]),
            ),
            DynamicImage::ImageLuma8(i) => {
                Self::from_pixels(i.as_raw().iter().map(|l| [*l, *l, *l, u8::MAX]))
            }
            DynamicImage::ImageLumaA8(i) => {
                Self::from_pixels(i.as_raw().chunks_exact(2).map(|p| [p[0], p[0], p[0], p[1]]))
            }
            _ => Self::from_image(&img.to_rgba8()),
        }
    }

    fn from_pixels(pixels: impl Iterator<Item = [u8; 4]>) -> Self {
        let mut hist_r: [u64; 256] = [0; 256];
        let mut hist_g: [u64; 256] = [0; 256];
        let mut hist_b: [u64; 256] = [0; 256];

        let mut num_pixels = 0;
        let mut num_transparent_pixels = 0;

        //Colors counting
        const FIXED_RGB_SIZE: usize = 24;
        const SUB_INDEX_SIZE: usize = 5;
        const MAIN_INDEX_SIZE: usize = 1 << (FIXED_RGB_SIZE - SUB_INDEX_SIZE);
        let mut color_map = vec![0u32; MAIN_INDEX_SIZE];

        for p in pixels {
            num_pixels += 1;
            if p == [0, 0, 0, 0] {
                num_transparent_pixels += 1;
            }

            hist_r[p[0] as usize] += 1;
            hist_g[p[1] as usize] += 1;
            hist_b[p[2] as usize] += 1;

            //Store every existing color combination in a bit
            //Therefore we use a 24 bit index, splitted into a main and a sub index.
            let pos = u32::from_le_bytes([p[0], p[1], p[2], 0]);
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

    // Updates decoder settings for subsequently loaded images. To apply to images that are already loaded, clear cache and reload.
    pub fn set_decoder_opts(&mut self, decoder_opts: DecoderSettings) {
        self.decoder_opts = decoder_opts;
    }

    pub fn check_modified(&mut self, path: &Path) {
        if let Some(watched_mod) = self.watcher.get(path)
            && let Ok(meta) = std::fs::metadata(path)
            && let Ok(modified) = meta.modified()
            && watched_mod != &modified
        {
            debug!(
                "Modified! read from meta {:?} stored: {:?}",
                modified, watched_mod
            );

            self.cache.data.remove(path);
            self.load(path);
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

        if let Ok(meta) = std::fs::metadata(img_location)
            && let Ok(modified) = meta.modified()
        {
            self.watcher.insert(img_location.into(), modified);
        }
    }

    pub fn load(&mut self, img_location: &Path) {
        self.load_advanced(img_location, None);
    }

    pub fn stop(&self) {
        _ = self.stop_sender.send(());
    }
}

/// The egui context, for threads that have something new to show.
static REPAINT_CONTEXT: OnceLock<egui::Context> = OnceLock::new();

/// Remember the context, so background threads can ask for a repaint.
pub fn set_repaint_context(ctx: &egui::Context) {
    _ = REPAINT_CONTEXT.set(ctx.clone());
}

/// Ask the UI to draw a frame. The UI only draws on input, so without this the
/// result of background work would not show up before the next input event.
pub fn request_repaint() {
    if let Some(ctx) = REPAINT_CONTEXT.get() {
        ctx.request_repaint();
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
        let timer = std::time::Instant::now();

        match open_image(&loc, Some(message_sender.clone()), Some(decoder_opts)) {
            Ok(frame_receiver) => {
                debug!("Got a frame receiver from opening image");

                // The frames of an animation are handed over as they are decoded.
                // The app keeps them and keeps the time, so this thread never
                // waits and ends with the decoding.
                let mut animation_frames = 0;
                for mut f in frame_receiver.iter() {
                    if stop_receiver.try_recv().is_ok() {
                        debug!("Stopped from receiver.");
                        return;
                    }

                    match f {
                        Frame::Animation(ref buffer, _) => {
                            if animation_frames == 0 {
                                _ = texture_sender.send(Frame::new_reset(buffer.clone()));
                            }
                            animation_frames += 1;
                            _ = texture_sender.send(f);
                            request_repaint();
                        }
                        Frame::Still(ref mut buffer) => {
                            debug!("Received image in {:?}", timer.elapsed());
                            // nobody else holds the image yet, so this does not copy it
                            _ = rotate_dynimage(Arc::make_mut(buffer), &path);

                            // TODO force frame sournce
                            if let Some(new_frame) = forced_frame_source {
                                debug!("Converting from {f} to {new_frame}");
                                let _ = texture_sender.send(f.transmute(new_frame));
                            } else {
                                let _ = texture_sender.send(f);
                            }
                            request_repaint();
                            return;
                        }
                        _ => (),
                    }
                }

                if animation_frames > 0 {
                    debug!("Animation decoded, {animation_frames} frames");
                    _ = texture_sender.send(Frame::AnimationEnd);
                    request_repaint();
                }
            }
            Err(e) => {
                error!("{e}");
                _ = message_sender.send(Message::LoadError(format!("{e}")));
                _ = message_sender.send(Message::LoadError(format!(
                    "Failed to load {}",
                    path.display()
                )));
                request_repaint();
            }
        }
    });
}

/// A single frame
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Display)]
pub enum Frame {
    /// A regular still frame (most common)
    Still(Arc<DynamicImage>),
    /// Part of an animation. Delay in ms
    Animation(Arc<DynamicImage>, u32),
    /// First frame of animation. This is necessary to reset the image and stop the player.
    AnimationStart(Arc<DynamicImage>),
    /// Result of an edit operation with image
    EditResult(Arc<DynamicImage>),
    /// Only update the current texture.
    UpdateTexture,
    /// All frames of the animation were sent, it loops from here on.
    AnimationEnd,
    /// TODO: Replace with edit result. A result of a compare operation. Image keeps transform.
    CompareResult(Arc<DynamicImage>, ImageGeometry),
    /// A member of a custom image collection, for example when dropping many files or opening the app with more than one file as argument
    ImageCollectionMember(Arc<DynamicImage>),
}

impl Frame {
    pub fn new(source: Frame) -> Frame {
        source
    }

    pub fn new_reset(buffer: impl Into<Arc<DynamicImage>>) -> Frame {
        Frame::AnimationStart(buffer.into())
    }

    pub fn new_animation(buffer: impl Into<Arc<DynamicImage>>, delay_ms: u32) -> Frame {
        Frame::Animation(buffer.into(), delay_ms)
    }

    #[allow(dead_code)]
    pub fn new_edit(buffer: impl Into<Arc<DynamicImage>>) -> Frame {
        Frame::EditResult(buffer.into())
    }

    #[allow(dead_code)]
    pub fn new_empty_edit() -> Frame {
        Frame::UpdateTexture
    }

    pub fn new_still(buffer: impl Into<Arc<DynamicImage>>) -> Frame {
        Frame::Still(buffer.into())
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
                Frame::UpdateTexture | Frame::AnimationEnd => (),
            },
            Frame::UpdateTexture | Frame::AnimationEnd => (),
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
            | Frame::ImageCollectionMember(img) => Some(DynamicImage::clone(img)),
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

pub fn toggle_fullscreen(ctx: &egui::Context, state: &mut OculanteState) {
    let fullscreen = ctx.input(|i| i.viewport().fullscreen).unwrap_or(false);

    if !fullscreen {
        // Entering fullscreen: offset image by window position so the pixel
        // under the cursor stays in the same screen location.
        let window_pos = ctx
            .input(|i| i.viewport().outer_rect)
            .map(|r| (r.left(), r.top()))
            .unwrap_or((0.0, 0.0));

        // The menu bar offset: in fullscreen the top panel disappears in zen mode,
        // but the available_rect shift covers that. We just need the window origin.
        let offset = (window_pos.0 as i32, window_pos.1 as i32);

        debug!("Entering fullscreen. Window pos: {:?}", offset);

        state.image_geometry.offset.x += offset.0 as f32;
        state.image_geometry.offset.y += offset.1 as f32;
        state.fullscreen_offset = Some(offset);
    } else if let Some(sf) = state.fullscreen_offset {
        // Exiting fullscreen: reverse the offset
        state.image_geometry.offset.x -= sf.0 as f32;
        state.image_geometry.offset.y -= sf.1 as f32;
        state.fullscreen_offset = None;
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
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
    current_image: &Option<Arc<DynamicImage>>,
    current_path: &Option<PathBuf>,
    channel: &(Sender<ExtendedImageInfo>, Receiver<ExtendedImageInfo>),
) {
    if let Some(img) = current_image {
        // The image is shared with the thread, not copied
        let img = img.clone();
        let sender = channel.0.clone();
        let current_path = current_path.clone();
        thread::spawn(move || {
            let mut e_info = ExtendedImageInfo::from_dynamic_image(&img);
            if let Some(p) = current_path {
                _ = e_info.with_exif(&p);
                _ = e_info.with_dicom(&p);
            }
            debug!("Sending extended info");
            _ = sender.send(e_info);
            request_repaint();
        });
    }
}

pub trait ImageExt {
    fn size_vec(&self) -> Vector2<f32> {
        unimplemented!()
    }
}

impl ImageExt for RgbaImage {
    fn size_vec(&self) -> Vector2<f32> {
        Vector2::new(self.width() as f32, self.height() as f32)
    }
}

impl ImageExt for DynamicImage {
    fn size_vec(&self) -> Vector2<f32> {
        Vector2::new(self.width() as f32, self.height() as f32)
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

// Have user facing copy functions use this, effective_image includes image edits
pub fn effective_image(state: &OculanteState) -> Option<&DynamicImage> {
    if state.edit_state.result_pixel_op.width() > 0 {
        Some(&state.edit_state.result_pixel_op)
    } else {
        state.current_image.as_deref()
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

pub fn clipboard_copy_path(path: &Path) {
    if let Ok(clipboard) = &mut Clipboard::new() {
        let _ = clipboard.set_text(path.display().to_string());
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
    if state.scrubber.entries.is_empty() {
        state.current_image = None;
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

/// Show the next image of the compare list at the position stored for it.
pub fn compare_next(state: &mut OculanteState) {
    if let Some(item) = state.compare_list.next() {
        let (path, geometry) = (item.path.clone(), item.geometry);
        state.is_loaded = false;
        state.player.load_advanced(
            &path,
            Some(Frame::CompareResult(Default::default(), geometry)),
        );
        state.current_path = Some(path);
    }
}

/// Open the images or folders the app was started with.
pub fn open_paths(state: &mut OculanteState, paths_to_open: Vec<PathBuf>) {
    debug!("Image is: {:?}", paths_to_open);

    if paths_to_open.len() == 1 {
        let location = paths_to_open.into_iter().next().unwrap();
        if location.is_dir() {
            if let Ok(first) = find_first_image_in_directory(&location) {
                state.is_loaded = false;
                state.player.load(&first);
                state.current_path = Some(first);
            }
        } else {
            state.is_loaded = false;
            state.player.load(&location);
            state.current_path = Some(location);
        }
    } else if paths_to_open.len() > 1 {
        let location = paths_to_open.first().unwrap();
        if location.is_dir() {
            if let Ok(first) = find_first_image_in_directory(location) {
                state.is_loaded = false;
                state.current_path = Some(first.clone());
                state.player.load_advanced(
                    &first,
                    Some(Frame::ImageCollectionMember(Default::default())),
                );
            }
        } else {
            state.is_loaded = false;
            state.current_path = Some(location.clone());
            state.player.load_advanced(
                location,
                Some(Frame::ImageCollectionMember(Default::default())),
            );
        }
        state.scrubber.fixed_paths = paths_to_open.iter().all(|p| p.is_file());
        state.scrubber.entries = paths_to_open;
        state.scrubber.wrap = state.persistent_settings.wrap_folder;
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

// Oculante's version, in release builds it shows as a version number, in debug builds it shows dev hash
pub fn app_version() -> String {
    if cfg!(debug_assertions) {
        format!("dev ({})", env!("GIT_HASH"))
    } else {
        env!("CARGO_PKG_VERSION").into()
    }
}

// For debug section in preferences.
pub fn detailed_version() -> String {
    if cfg!(debug_assertions) {
        app_version()
    } else {
        format!("{} ({})", env!("CARGO_PKG_VERSION"), env!("GIT_HASH"))
    }
}

/// Set the window title
pub fn set_title(ctx: &egui::Context, state: &mut OculanteState) {
    let p = state.current_path.clone().unwrap_or_default();

    let mut title_string = state
        .persistent_settings
        .title_format
        .replacen("{APP}", env!("CARGO_PKG_NAME"), 10)
        .replacen("{VERSION}", &app_version(), 10)
        .replacen("{FULLPATH}", &format!("{}", p.display()), 10)
        .replacen(
            "{NUM}",
            &format!(
                "{}/{}",
                state.scrubber.index + 1,
                state.scrubber.entries.len()
            ),
            10,
        )
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

    ctx.send_viewport_cmd(egui::ViewportCommand::Title(title_string));
}

pub fn fit(oldvalue: f32, oldmin: f32, oldmax: f32, newmin: f32, newmax: f32) -> f32 {
    (((oldvalue - oldmin) * (newmax - newmin)) / (oldmax - oldmin)) + newmin
}

pub fn toggle_zen_mode(state: &mut OculanteState, ctx: &egui::Context) {
    state.persistent_settings.zen_mode = !state.persistent_settings.zen_mode;
    if state.persistent_settings.zen_mode && state.persistent_settings.show_zen_mode_notification {
        _ = state.message_channel.0.send(Message::Info(format!(
            "Zen mode on. Press '{}' to toggle.",
            lookup(&state.persistent_settings.shortcuts, &InputEvent::ZenMode)
        )));
    }
    set_title(ctx, state);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Something with many different values, in every channel
    fn pattern(bytes_per_pixel: usize) -> Vec<u8> {
        (0..37 * 23 * bytes_per_pixel)
            .map(|i| (i * 7 % 256) as u8)
            .collect()
    }

    #[test]
    fn image_info_does_not_depend_on_the_layout() {
        let images = [
            DynamicImage::ImageLuma8(image::GrayImage::from_raw(37, 23, pattern(1)).unwrap()),
            DynamicImage::ImageLumaA8(image::GrayAlphaImage::from_raw(37, 23, pattern(2)).unwrap()),
            DynamicImage::ImageRgb8(image::RgbImage::from_raw(37, 23, pattern(3)).unwrap()),
            DynamicImage::ImageRgba8(RgbaImage::from_raw(37, 23, pattern(4)).unwrap()),
            DynamicImage::ImageRgb16(
                image::ImageBuffer::from_raw(37, 23, vec![40000u16; 37 * 23 * 3]).unwrap(),
            ),
        ];
        for img in images {
            // what the info was computed from before: the image converted to RGBA
            let expected = ExtendedImageInfo::from_image(&img.to_rgba8());
            let info = ExtendedImageInfo::from_dynamic_image(&img);
            assert_eq!(info.num_pixels, 37 * 23, "{:?}", img.color());
            assert_eq!(info.num_pixels, expected.num_pixels);
            assert_eq!(info.num_colors, expected.num_colors, "{:?}", img.color());
            assert_eq!(info.num_transparent_pixels, expected.num_transparent_pixels);
            assert_eq!(info.red_histogram, expected.red_histogram);
            assert_eq!(info.green_histogram, expected.green_histogram);
            assert_eq!(info.blue_histogram, expected.blue_histogram);
        }
    }
}
