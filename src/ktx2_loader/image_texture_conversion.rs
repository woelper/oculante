use std::convert::TryInto;

use crate::ktx2_loader::Image;

use exr::prelude::f16;
use image::{DynamicImage, ImageBuffer, Rgba32FImage};
use log::info;
use thiserror::Error;
use wgpu::TextureFormat;

impl Image {
    /// Converts a [`DynamicImage`] to an [`Image`].

    /// Convert a [`Image`] to a [`DynamicImage`]. Useful for editing image
    /// data. Not all [`TextureFormat`] are covered, therefore it will return an
    /// error if the format is unsupported. Supported formats are:
    /// - `TextureFormat::R8Unorm`
    /// - `TextureFormat::Rg8Unorm`
    /// - `TextureFormat::Rgba8UnormSrgb`
    /// - `TextureFormat::Bgra8UnormSrgb`
    ///
    /// To convert [`Image`] to a different format see: [`Image::convert`].
    pub fn try_into_dynamic(self) -> Result<DynamicImage, IntoDynamicImageError> {
        info!(
            "Attemting to interpret format {:?}",
            self.texture_descriptor.format
        );
        info!("Mip levels: {:?}", self.texture_descriptor.mip_level_count);
        info!(
            "Array Layers {:?}",
            self.texture_descriptor.array_layer_count()
        );
        info!("Size {:?}", self.texture_descriptor.size);
        info!("W x H {}x{}", self.width(), self.height());
        match self.texture_descriptor.format {
            TextureFormat::R8Unorm => ImageBuffer::from_raw(self.width(), self.height(), self.data)
                .map(DynamicImage::ImageLuma8),
            TextureFormat::Rg8Unorm => {
                ImageBuffer::from_raw(self.width(), self.height(), self.data)
                    .map(DynamicImage::ImageLumaA8)
            }
            TextureFormat::Rgba8UnormSrgb => {
                ImageBuffer::from_raw(self.width(), self.height(), self.data)
                    .map(DynamicImage::ImageRgba8)
            }
            // This format is commonly used as the format for the swapchain texture
            // This conversion is added here to support screenshots
            TextureFormat::Bgra8UnormSrgb | TextureFormat::Bgra8Unorm => {
                ImageBuffer::from_raw(self.width(), self.height(), {
                    let mut data = self.data;
                    for bgra in data.chunks_exact_mut(4) {
                        bgra.swap(0, 2);
                    }
                    data
                })
                .map(DynamicImage::ImageRgba8)
            }
            TextureFormat::Rgba16Float => {
                let d = self
                    .data
                    .chunks_exact(2)
                    .map(|c| [f16::from_le_bytes(c[0..=1].try_into().unwrap()).to_f32()])
                    .flatten()
                    .collect::<Vec<_>>();
                Rgba32FImage::from_vec(self.width() as u32, self.height() as u32, d)
                    .map(|i| DynamicImage::ImageRgba8(DynamicImage::ImageRgba32F(i).to_rgba8()))
            }
            TextureFormat::Rgba32Float => {
                let d = self
                    .data
                    .chunks_exact(4)
                    .map(|c| [f32::from_le_bytes(c[0..=3].try_into().unwrap())])
                    .flatten()
                    .map(|p| p.powf(2.2))
                    .map(|p| (p.powf(1.0 / 2.2).max(0.0).min(1.0)))
                    .collect::<Vec<_>>();
                Rgba32FImage::from_vec(self.width() as u32, self.height() as u32, d)
                    .map(|i| DynamicImage::ImageRgba8(DynamicImage::ImageRgba32F(i).to_rgba8()))
            }
            // Throw and error if conversion isn't supported
            texture_format => return Err(IntoDynamicImageError::UnsupportedFormat(texture_format)),
        }
        .ok_or(IntoDynamicImageError::UnknownConversionError(
            self.texture_descriptor.format,
        ))
    }
}

/// Errors that occur while converting an [`Image`] into a [`DynamicImage`]
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum IntoDynamicImageError {
    /// Conversion into dynamic image not supported for source format.
    #[error("KTX2: Unsupported format: {0:?}. Please open a github issue and attach your image.")]
    UnsupportedFormat(TextureFormat),

    /// Encountered an unknown error during conversion.
    #[error("KTX2: Failed interpret buffer: {0:?}.")]
    UnknownConversionError(TextureFormat),
}
