//! File encoders - this defines save options.
//!
//! To add more formats, add a variant to the `[FileEncoder]` struct.

use std::fs::File;
use std::path::Path;
use anyhow::Result;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, PngEncoder};
use image::DynamicImage;
use notan::egui::Ui;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};
use crate::ui::EguiExt;

#[derive(Default, Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Display, EnumIter)]

pub enum CompressionLevel {
    Best,
    #[default]
    Default,
    Fast,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Display, EnumIter)]
pub enum FileEncoder {
    Jpg { quality: u32 },
    Png { compressionlevel: CompressionLevel },
    Bmp,
    WebP,
}

impl Default for FileEncoder {
    fn default() -> Self {
        Self::Png {
            compressionlevel: CompressionLevel::Default,
        }
    }
}

impl FileEncoder {
    pub fn matching_variant(path: &Path, variants: &Vec<Self>) -> Self {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default()
            .to_lowercase()
            .replace("jpeg", "jpg");

        for v in variants {
            if v.ext() == ext {
                return v.clone();
            }
        }

        Self::Png {
            compressionlevel: CompressionLevel::Default,
        }
    }

    pub fn ext(&self) -> String {
        self.to_string().to_lowercase()
    }

    pub fn save(&self, image: &DynamicImage, path: &Path) -> Result<()> {
        match self {
            FileEncoder::Jpg { quality } => {
                let w = File::create(path)?;
                JpegEncoder::new_with_quality(w, *quality as u8).encode_image(image)?;
            }
            FileEncoder::Png { compressionlevel } => {
                let w = File::create(path)?;

                PngEncoder::new_with_quality(
                    w,
                    match compressionlevel {
                        CompressionLevel::Best => CompressionType::Best,
                        CompressionLevel::Default => CompressionType::Default,
                        CompressionLevel::Fast => CompressionType::Fast,
                    },
                    image::codecs::png::FilterType::Adaptive,
                );
                image.save_with_format(path, image::ImageFormat::Png)?;
            }
            FileEncoder::Bmp => {
                image.save_with_format(path, image::ImageFormat::Bmp)?;
            }
            FileEncoder::WebP => {
                image.save_with_format(path, image::ImageFormat::WebP)?;
            }
        }

        Ok(())
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        ui.label(self.ext());

        match self {
            FileEncoder::Jpg { quality } => {
                ui.styled_slider(quality, 0..=100);
            }
            FileEncoder::Png { compressionlevel } => {
                ui.label(self.to_string());
            }
            FileEncoder::Bmp => {
                ui.label(self.to_string());
            }
            FileEncoder::WebP => {
                ui.label(self.to_string());
            }
        }
    }
}
