// use bevy_math::{AspectRatio, UVec2, Vec2};
use std::hash::Hash;
// use thiserror::Error;
use wgpu_types::{Extent3d, TextureDimension, TextureFormat};

pub struct Image {
    pub data: Vec<u8>,
    // TODO: this nesting makes accessing Image metadata verbose. Either flatten out descriptor or add accessors
    pub texture_descriptor:
        wgpu_types::TextureDescriptor<Option<&'static str>, &'static [TextureFormat]>,
}

impl Default for Image {
    /// default is a 1x1x1 all '1.0' texture
    fn default() -> Self {
        let format = TextureFormat::Rgba8UnormSrgb;
        let data = vec![255; format.pixel_size()];
        Image {
            data,
            texture_descriptor: wgpu_types::TextureDescriptor {
                size: Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                format,
                dimension: TextureDimension::D2,
                label: None,
                mip_level_count: 1,
                sample_count: 1,
                usage: wgpu_types::TextureUsages::TEXTURE_BINDING
                    | wgpu_types::TextureUsages::COPY_DST,
                view_formats: &[],
            },
        }
    }
}

impl Image {
    /// Returns the width of a 2D image.
    #[inline]
    pub fn width(&self) -> u32 {
        self.texture_descriptor.size.width
    }

    /// Returns the height of a 2D image.
    #[inline]
    pub fn height(&self) -> u32 {
        self.texture_descriptor.size.height
    }

    // /// Returns the aspect ratio (width / height) of a 2D image.
    // #[inline]
    // pub fn aspect_ratio(&self) -> AspectRatio {
    //     AspectRatio::from_pixels(self.width(), self.height())
    // }

    // /// Returns the size of a 2D image as f32.
    // #[inline]
    // pub fn size_f32(&self) -> Vec2 {
    //     Vec2::new(self.width() as f32, self.height() as f32)
    // }

    // /// Returns the size of a 2D image.
    // #[inline]
    // pub fn size(&self) -> UVec2 {
    //     UVec2::new(self.width(), self.height())
    // }

    // /// Convert a texture from a format to another. Only a few formats are
    // /// supported as input and output:
    // /// - `TextureFormat::R8Unorm`
    // /// - `TextureFormat::Rg8Unorm`
    // /// - `TextureFormat::Rgba8UnormSrgb`
    // ///
    // /// To get [`Image`] as a [`image::DynamicImage`] see:
    // /// [`Image::try_into_dynamic`].
    // pub fn convert(&self, new_format: TextureFormat) -> Option<Self> {
    //     self.clone()
    //         .try_into_dynamic()
    //         .ok()
    //         .and_then(|img| match new_format {
    //             TextureFormat::R8Unorm => {
    //                 Some((image::DynamicImage::ImageLuma8(img.into_luma8()), false))
    //             }
    //             TextureFormat::Rg8Unorm => Some((
    //                 image::DynamicImage::ImageLumaA8(img.into_luma_alpha8()),
    //                 false,
    //             )),
    //             TextureFormat::Rgba8UnormSrgb => {
    //                 Some((image::DynamicImage::ImageRgba8(img.into_rgba8()), true))
    //             }
    //             _ => None,
    //         })
    //         .map(|(dyn_img, is_srgb)| Self::from_dynamic(dyn_img, is_srgb))
    // }

    /// Whether the texture format is compressed or uncompressed
    pub fn is_compressed(&self) -> bool {
        let format_description = self.texture_descriptor.format;
        format_description
            .required_features()
            .contains(wgpu_types::Features::TEXTURE_COMPRESSION_ASTC)
            || format_description
                .required_features()
                .contains(wgpu_types::Features::TEXTURE_COMPRESSION_BC)
            || format_description
                .required_features()
                .contains(wgpu_types::Features::TEXTURE_COMPRESSION_ETC2)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum DataFormat {
    Rgb,
    Rgba,
    Rrr,
    Rrrg,
    Rg,
}

#[derive(Clone, Copy, Debug)]
pub enum TranscodeFormat {
    Etc1s,
    Uastc(DataFormat),
    // Has to be transcoded to R8Unorm for use with `wgpu`
    R8UnormSrgb,
    // Has to be transcoded to R8G8Unorm for use with `wgpu`
    Rg8UnormSrgb,
    // Has to be transcoded to Rgba8 for use with `wgpu`
    Rgb8,
}

/// An error that occurs when loading a texture
#[derive(Debug)]
// The texts are shown to the user through Debug, which dead code analysis ignores
#[allow(dead_code)]
pub enum TextureError {
    UnsupportedTextureFormat(String),
    SuperDecompressionError(String),
    InvalidData(String),
    FormatRequiresTranscodingError(TranscodeFormat),
}

/// Extends the wgpu [`TextureFormat`] with information about the pixel.
pub trait TextureFormatPixelInfo {
    /// Returns the size of a pixel in bytes of the format.
    fn pixel_size(&self) -> usize;
}

impl TextureFormatPixelInfo for TextureFormat {
    fn pixel_size(&self) -> usize {
        let info = self;
        match info.block_dimensions() {
            (1, 1) => info.block_copy_size(None).unwrap() as usize,
            _ => panic!("Using pixel_size for compressed textures is invalid"),
        }
    }
}

bitflags::bitflags! {
    #[derive(Default, Clone, Copy, Eq, PartialEq, Debug)]
    #[repr(transparent)]
    pub struct CompressedImageFormats: u32 {
        const NONE     = 0;
        const ASTC_LDR = (1 << 0);
        const BC       = (1 << 1);
        const ETC2     = (1 << 2);
    }
}

impl CompressedImageFormats {
    pub fn supports(&self, format: TextureFormat) -> bool {
        match format {
            TextureFormat::Bc1RgbaUnorm
            | TextureFormat::Bc1RgbaUnormSrgb
            | TextureFormat::Bc2RgbaUnorm
            | TextureFormat::Bc2RgbaUnormSrgb
            | TextureFormat::Bc3RgbaUnorm
            | TextureFormat::Bc3RgbaUnormSrgb
            | TextureFormat::Bc4RUnorm
            | TextureFormat::Bc4RSnorm
            | TextureFormat::Bc5RgUnorm
            | TextureFormat::Bc5RgSnorm
            | TextureFormat::Bc6hRgbUfloat
            | TextureFormat::Bc6hRgbFloat
            | TextureFormat::Bc7RgbaUnorm
            | TextureFormat::Bc7RgbaUnormSrgb => self.contains(CompressedImageFormats::BC),
            TextureFormat::Etc2Rgb8Unorm
            | TextureFormat::Etc2Rgb8UnormSrgb
            | TextureFormat::Etc2Rgb8A1Unorm
            | TextureFormat::Etc2Rgb8A1UnormSrgb
            | TextureFormat::Etc2Rgba8Unorm
            | TextureFormat::Etc2Rgba8UnormSrgb
            | TextureFormat::EacR11Unorm
            | TextureFormat::EacR11Snorm
            | TextureFormat::EacRg11Unorm
            | TextureFormat::EacRg11Snorm => self.contains(CompressedImageFormats::ETC2),
            TextureFormat::Astc { .. } => self.contains(CompressedImageFormats::ASTC_LDR),
            _ => true,
        }
    }
}
