//! Images with an ICC profile are converted to sRGB when they are loaded, in one
//! place for all formats. Photos from phones are in Display P3, shown as they are
//! stored they look dull. The screen is taken to be sRGB, as most are and as
//! everything else in the window is drawn.

use crate::utils::Frame;
use anyhow::{Result, anyhow};
use image::{DynamicImage, ImageDecoder, ImageReader};
use log::{debug, warn};
use moxcms::{
    ColorProfile, DataColorSpace, Layout, ProfileText, TransformExecutor, TransformOptions,
};
use rayon::prelude::*;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

/// The ICC profile of a file, `extension` is the format it is loaded as
pub fn icc_profile(path: &Path, extension: &str) -> Option<Vec<u8>> {
    let read = || -> Result<Option<Vec<u8>>> {
        Ok(match extension.to_lowercase().as_str() {
            // RAW files are developed to sRGB, JPEG XL is converted by its decoder
            "jxl" => None,
            raw if crate::image_loader::is_raw(raw) => None,
            "avif" | "heif" | "heic" | "hif" => isobmff_icc(&std::fs::read(path)?),
            _ => {
                let mut reader = ImageReader::open(path)?.with_guessed_format()?;
                reader.no_limits();
                reader.into_decoder()?.icc_profile()?
            }
        })
    };
    read().unwrap_or_else(|e| {
        debug!("No color profile read from {}: {e}", path.display());
        None
    })
}

/// The boxes of an ISO base media file, such as AVIF and HEIF: their types and
/// what is in them
fn boxes(mut data: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    std::iter::from_fn(move || {
        let size = u32::from_be_bytes(data.get(..4)?.try_into().ok()?) as usize;
        let kind: [u8; 4] = data.get(4..8)?.try_into().ok()?;
        let (header, size) = match size {
            // the last box, it runs to the end
            0 => (8, data.len()),
            1 => (
                16,
                u64::from_be_bytes(data.get(8..16)?.try_into().ok()?) as usize,
            ),
            size => (8, size),
        };
        let content = data.get(header..size)?;
        data = &data[size..];
        Some((kind, content))
    })
}

fn find_box<'a>(data: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data)
        .find(|(k, _)| k == kind)
        .map(|(_, content)| content)
}

/// The ICC profile in the colr property of an AVIF or HEIF file
fn isobmff_icc(data: &[u8]) -> Option<Vec<u8>> {
    // meta is a full box, it starts with its version and flags
    let meta = find_box(data, b"meta")?.get(4..)?;
    let properties = find_box(find_box(meta, b"iprp")?, b"ipco")?;
    boxes(properties).find_map(|(kind, content)| {
        let profile = content
            .strip_prefix(b"prof")
            .or(content.strip_prefix(b"rICC"))?;
        (&kind == b"colr").then(|| profile.to_vec())
    })
}

/// What a profile is called
pub fn name(icc: &[u8]) -> String {
    ColorProfile::new_from_slice(icc)
        .ok()
        .and_then(|profile| profile.description)
        .map(|description| match description {
            ProfileText::PlainString(text) => text,
            ProfileText::Localizable(texts) => texts
                .iter()
                .find(|text| text.language == "en")
                .or(texts.first())
                .map(|text| text.value.clone())
                .unwrap_or_default(),
            ProfileText::Description(text) => text.ascii_string,
        })
        .map(|name| {
            name.trim_matches(|c: char| c == '\0' || c.is_whitespace())
                .to_string()
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "ICC profile".into())
}

/// Converts images in the colors of a profile to sRGB
pub struct ToSrgb {
    profile: ColorProfile,
}

impl ToSrgb {
    /// None when there is nothing to convert: the profile is sRGB, or one that
    /// changes nothing, as many small sRGB profiles do, or not one for RGB.
    pub fn new(icc: &[u8]) -> Option<Self> {
        let profile = ColorProfile::new_from_slice(icc)
            .inspect_err(|e| warn!("Unreadable color profile: {e}"))
            .ok()?;
        if profile.color_space != DataColorSpace::Rgb {
            return None;
        }
        let to_srgb = Self { profile };
        // 9 steps of each channel
        let steps: Vec<u8> = (0..=8).map(|step| (step * 32).min(255) as u8).collect();
        let mut colors = vec![];
        for &r in &steps {
            for &g in &steps {
                for &b in &steps {
                    colors.extend([r, g, b]);
                }
            }
        }
        let mut image = DynamicImage::ImageRgb8(image::RgbImage::from_raw(
            colors.len() as u32 / 3,
            1,
            colors.clone(),
        )?);
        to_srgb
            .apply(&mut image)
            .inspect_err(|e| warn!("Color profile can't be converted to sRGB: {e}"))
            .ok()?;
        let changes = colors
            .iter()
            .zip(image.as_bytes())
            .any(|(a, b)| a.abs_diff(*b) > 1);
        changes.then_some(to_srgb)
    }

    /// Converts the RGB images of 8 and 16 bits, gray and floating point images
    /// stay as they are
    pub fn apply(&self, image: &mut DynamicImage) -> Result<()> {
        let options = TransformOptions::default();
        let to_8bit = |layout| {
            self.profile
                .create_transform_8bit(layout, &ColorProfile::new_srgb(), layout, options)
        };
        let to_16bit = |layout| {
            self.profile
                .create_transform_16bit(layout, &ColorProfile::new_srgb(), layout, options)
        };
        match image {
            DynamicImage::ImageRgb8(buffer) => convert(buffer, 3, &*to_8bit(Layout::Rgb)?),
            DynamicImage::ImageRgba8(buffer) => convert(buffer, 4, &*to_8bit(Layout::Rgba)?),
            DynamicImage::ImageRgb16(buffer) => convert(buffer, 3, &*to_16bit(Layout::Rgb)?),
            DynamicImage::ImageRgba16(buffer) => convert(buffer, 4, &*to_16bit(Layout::Rgba)?),
            _ => Ok(()),
        }
    }

    /// The frames of `receiver`, converted on a thread of their own as they come
    pub fn frames(self, receiver: Receiver<Frame>) -> Receiver<Frame> {
        let (sender, converted) = channel();
        std::thread::spawn(move || {
            for mut frame in receiver {
                if let Frame::Still(image)
                | Frame::Animation(image, _)
                | Frame::AnimationStart(image) = &mut frame
                    && let Err(e) = self.apply(Arc::make_mut(image))
                {
                    warn!("Colors not converted to sRGB: {e:#}");
                }
                if sender.send(frame).is_err() {
                    break;
                }
            }
        });
        converted
    }
}

/// Converts the samples in parts, at the same time
fn convert<T: Copy + Default + Send + Sync>(
    samples: &mut [T],
    channels: usize,
    transform: &(dyn TransformExecutor<T> + Send + Sync),
) -> Result<()> {
    const PIXELS: usize = 1 << 16;
    samples
        .par_chunks_mut(PIXELS * channels)
        .try_for_each(|chunk| {
            let source = chunk.to_vec();
            transform.transform(&source, chunk)
        })
        .map_err(|e| anyhow!("{e}"))
}
