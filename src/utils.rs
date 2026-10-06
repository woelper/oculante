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
    pub name: String,
}

impl ExtendedImageInfo {
    pub fn with_exif(&mut self, image_path: &Path) -> Result<()> {
        self.name = image_path.to_string_lossy().to_string();
        if image_path.extension() == Some(OsStr::new("gif")) {
            return Ok(());
        }

        // User-friendly Exif in key/value form. The reader only reads as much of
        // the file as it needs, and gives up early on formats without EXIF.
        let mut reader = std::io::BufReader::new(std::fs::File::open(image_path)?);
        let exif = exif::Reader::new().read_from_container(&mut reader)?;
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
        if is_dicom(image_path) {
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
        Self::from_bands(img.height(), |y, rows, counts| {
            let row = img.width() as usize * 4;
            let band = &img.as_raw()[y as usize * row..(y + rows) as usize * row];
            counts.add(band.chunks_exact(4).map(|p| [p[0], p[1], p[2], p[3]]));
        })
    }

    /// Like `from_image`, but reads 8 bit images in the layout they have, and
    /// converts others band by band, instead of converting the whole image to
    /// RGBA first.
    pub fn from_dynamic_image(img: &DynamicImage) -> Self {
        let width = img.width() as usize;
        // the samples of the rows from y on
        let band = |channels: usize, y: u32, rows: u32| {
            y as usize * width * channels..(y + rows) as usize * width * channels
        };
        Self::from_bands(img.height(), |y, rows, counts| match img {
            DynamicImage::ImageRgba8(i) => counts.add(
                i.as_raw()[band(4, y, rows)]
                    .chunks_exact(4)
                    .map(|p| [p[0], p[1], p[2], p[3]]),
            ),
            DynamicImage::ImageRgb8(i) => counts.add(
                i.as_raw()[band(3, y, rows)]
                    .chunks_exact(3)
                    .map(|p| [p[0], p[1], p[2], u8::MAX]),
            ),
            DynamicImage::ImageLuma8(i) => counts.add(
                i.as_raw()[band(1, y, rows)]
                    .iter()
                    .map(|l| [*l, *l, *l, u8::MAX]),
            ),
            DynamicImage::ImageLumaA8(i) => counts.add(
                i.as_raw()[band(2, y, rows)]
                    .chunks_exact(2)
                    .map(|p| [p[0], p[0], p[0], p[1]]),
            ),
            // the conversion of the image crate, on one band at a time
            _ => {
                let rgba = img.crop_imm(0, y, img.width(), rows).to_rgba8();
                counts.add(
                    rgba.as_raw()
                        .chunks_exact(4)
                        .map(|p| [p[0], p[1], p[2], p[3]]),
                );
            }
        })
    }

    /// Counts the image in bands of rows, in parallel. `band` adds the pixels of
    /// the rows from y on to the counts.
    fn from_bands(height: u32, band: impl Fn(u32, u32, &mut PixelCounts) + Sync) -> Self {
        use rayon::prelude::*;
        const BAND_ROWS: u32 = 64;
        // Every color that occurs sets a bit, shared by all bands
        let color_map: Vec<std::sync::atomic::AtomicU32> = (0..PixelCounts::COLOR_WORDS)
            .map(|_| std::sync::atomic::AtomicU32::new(0))
            .collect();
        let counts = (0..height.div_ceil(BAND_ROWS))
            .into_par_iter()
            .map(|index| {
                let y = index * BAND_ROWS;
                let mut counts = PixelCounts::new(&color_map);
                band(y, BAND_ROWS.min(height - y), &mut counts);
                counts
            })
            .reduce(
                || PixelCounts::new(&color_map),
                |mut a, b| {
                    a.merge(&b);
                    a
                },
            );

        let num_colors = color_map
            .iter()
            .map(|word| word.load(std::sync::atomic::Ordering::Relaxed).count_ones() as usize)
            .sum();
        let histogram = |channel: &[u64; 256]| -> Vec<(i32, u64)> {
            channel
                .iter()
                .enumerate()
                .map(|(k, v)| (k as i32, *v))
                .collect()
        };

        Self {
            num_pixels: counts.pixels,
            num_transparent_pixels: counts.transparent,
            num_colors,
            blue_histogram: histogram(&counts.histograms[2]),
            green_histogram: histogram(&counts.histograms[1]),
            red_histogram: histogram(&counts.histograms[0]),
            name: Default::default(),
            exif: Default::default(),
            dicom: Default::default(),
        }
    }
}

/// Histograms and pixel counts of a part of an image
struct PixelCounts<'a> {
    histograms: [[u64; 256]; 3],
    pixels: usize,
    transparent: usize,
    color_map: &'a [std::sync::atomic::AtomicU32],
}

impl<'a> PixelCounts<'a> {
    /// One bit for every 24 bit color
    const COLOR_WORDS: usize = 1 << (24 - 5);

    fn new(color_map: &'a [std::sync::atomic::AtomicU32]) -> Self {
        Self {
            histograms: [[0; 256]; 3],
            pixels: 0,
            transparent: 0,
            color_map,
        }
    }

    fn add(&mut self, pixels: impl Iterator<Item = [u8; 4]>) {
        use std::sync::atomic::Ordering::Relaxed;
        for p in pixels {
            self.pixels += 1;
            if p == [0, 0, 0, 0] {
                self.transparent += 1;
            }
            self.histograms[0][p[0] as usize] += 1;
            self.histograms[1][p[1] as usize] += 1;
            self.histograms[2][p[2] as usize] += 1;
            let color = u32::from_le_bytes([p[0], p[1], p[2], 0]);
            let (word, bit) = ((color >> 5) as usize, 1 << (color & 31));
            // most colors are seen again and again, reading first saves writes
            if self.color_map[word].load(Relaxed) & bit == 0 {
                self.color_map[word].fetch_or(bit, Relaxed);
            }
        }
    }

    fn merge(&mut self, other: &Self) {
        for (mine, theirs) in self.histograms.iter_mut().zip(&other.histograms) {
            for (a, b) in mine.iter_mut().zip(theirs) {
                *a += b;
            }
        }
        self.pixels += other.pixels;
        self.transparent += other.transparent;
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
                let mut end_sent = false;
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
                        Frame::AnimationEnd(plays) => {
                            debug!("Animation decoded, {animation_frames} frames, plays {plays:?}");
                            _ = texture_sender.send(f);
                            request_repaint();
                            end_sent = true;
                        }
                        _ => (),
                    }
                }

                // a loader that does not know the play count, the animation loops
                if animation_frames > 0 && !end_sent {
                    debug!("Animation decoded, {animation_frames} frames");
                    _ = texture_sender.send(Frame::AnimationEnd(None));
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
    /// All frames of the animation were sent. Holds how often the file asks to
    /// play it, `None` for forever.
    AnimationEnd(Option<u32>),
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
                Frame::UpdateTexture | Frame::AnimationEnd(_) => (),
            },
            Frame::UpdateTexture | Frame::AnimationEnd(_) => (),
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
    if let Some(p) = state.current_path.clone() {
        let name = p
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        #[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
        let deleted = trash::delete(&p).map_err(|e| e.to_string());
        #[cfg(any(target_os = "netbsd", target_os = "freebsd"))]
        let deleted = std::fs::remove_file(&p).map_err(|e| e.to_string());

        // The image stays when the file is still there
        if let Err(e) = deleted {
            state.send_message_err(&format!("Could not delete {name}: {e}"));
            return;
        }
        state.send_message_info(&format!("Deleted {name}"));
        // remove from cache so we don't suceed to load it agaim
        state.player.cache.data.remove(&p);
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

/// Computes the numbers of the info panel for an image in the background. They
/// come back with the version of the image they belong to.
pub fn send_extended_info(
    current_image: &Option<Arc<DynamicImage>>,
    current_path: &Option<PathBuf>,
    version: u64,
    channel: &ExtendedInfoChannel,
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
            _ = sender.send((version, e_info));
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
/// The channel for the numbers of the info panel, with the version of the image
pub type ExtendedInfoChannel = (
    Sender<(u64, ExtendedImageInfo)>,
    Receiver<(u64, ExtendedImageInfo)>,
);

/// The EXIF data of a file, to write into a copy of the image that is saved
pub fn raw_exif(path: &Path) -> Option<Bytes> {
    let input: Bytes = std::fs::read(path).ok()?.into();
    if let Ok(Some(image)) = DynImage::from_bytes(input.clone())
        && let Some(exif) = image.exif()
    {
        return Some(exif);
    }
    // Other formats, DNG for example: the EXIF block as the reader finds it.
    // It is kept across formats when it is written again.
    exif::Reader::new()
        .read_from_container(&mut Cursor::new(&input[..]))
        .ok()
        .map(|exif| exif.buf().to_vec().into())
}

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

/// DICOM files carry "DICM" after a preamble of 128 bytes. Files are told
/// apart by that, whatever their name.
fn is_dicom(path: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 132];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut head))
        .is_ok()
        && &head[128..] == b"DICM"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file that can not be deleted is reported as such and stays on screen.
    /// Before, the app said it was deleted and moved on.
    #[test]
    fn failed_delete_is_reported() {
        let mut state = OculanteState::default();
        let missing = std::env::temp_dir().join("oculante_test_not_there/missing.png");
        state.current_path = Some(missing.clone());
        delete_file(&mut state);
        let messages: Vec<Message> = state.message_channel.1.try_iter().collect();
        assert!(
            matches!(messages.as_slice(), [Message::Error(e)] if e.contains("missing.png")),
            "expected one error, got {messages:?}"
        );
        assert_eq!(state.current_path, Some(missing));
    }

    /// Only DICOM files are read as DICOM. Every image was tried before.
    #[test]
    fn dicom_is_recognised_by_content() {
        let dir = std::env::temp_dir();
        let dicom = dir.join("oculante_test_is_dicom.png");
        let mut bytes = vec![0u8; 128];
        bytes.extend_from_slice(b"DICM");
        std::fs::write(&dicom, &bytes).unwrap();
        let short = dir.join("oculante_test_is_dicom_short.dcm");
        std::fs::write(&short, b"DICM").unwrap();
        assert!(is_dicom(&dicom), "a DICOM file with the extension of a PNG");
        assert!(!is_dicom(&short), "a file too short to be DICOM");
        assert!(!is_dicom(Path::new("res/tests/test.png")));
        _ = std::fs::remove_file(dicom);
        _ = std::fs::remove_file(short);
    }

    #[test]
    fn image_info_does_not_depend_on_the_layout() {
        // high enough for several bands of rows, counted in parallel
        let (w, h) = (37, 150);
        let rgba = RgbaImage::from_fn(w, h, |x, y| {
            let v = (x * 7 + y * 13) as u8;
            image::Rgba([
                v,
                v.wrapping_mul(3),
                (x * y) as u8,
                if x % 5 == 0 { 0 } else { 255 },
            ])
        });
        let base = DynamicImage::ImageRgba8(rgba);
        let images = [
            DynamicImage::ImageLuma8(base.to_luma8()),
            DynamicImage::ImageLumaA8(base.to_luma_alpha8()),
            DynamicImage::ImageRgb8(base.to_rgb8()),
            base.clone(),
            DynamicImage::ImageLuma16(base.to_luma16()),
            DynamicImage::ImageRgb16(base.to_rgb16()),
            DynamicImage::ImageRgba16(base.to_rgba16()),
            DynamicImage::ImageRgb32F(base.to_rgb32f()),
            DynamicImage::ImageRgba32F(base.to_rgba32f()),
        ];
        for img in images {
            // counted the simple way, from the image converted to RGBA
            let pixels: Vec<[u8; 4]> = img.to_rgba8().pixels().map(|p| p.0).collect();
            let colors: std::collections::HashSet<[u8; 3]> =
                pixels.iter().map(|p| [p[0], p[1], p[2]]).collect();
            let histogram = |c: usize| -> Vec<(i32, u64)> {
                (0..256)
                    .map(|v| {
                        (
                            v as i32,
                            pixels.iter().filter(|p| p[c] as usize == v).count() as u64,
                        )
                    })
                    .collect()
            };
            let info = ExtendedImageInfo::from_dynamic_image(&img);
            let layout = img.color();
            assert_eq!(info.num_pixels, (w * h) as usize, "{layout:?}");
            assert_eq!(info.num_colors, colors.len(), "{layout:?}");
            assert_eq!(
                info.num_transparent_pixels,
                pixels.iter().filter(|p| **p == [0, 0, 0, 0]).count(),
                "{layout:?}"
            );
            assert_eq!(info.red_histogram, histogram(0), "{layout:?}");
            assert_eq!(info.green_histogram, histogram(1), "{layout:?}");
            assert_eq!(info.blue_histogram, histogram(2), "{layout:?}");
        }
    }
}
