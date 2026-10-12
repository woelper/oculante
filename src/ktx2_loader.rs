//! KTX2 textures. Basis Universal and ASTC are transcoded by basisu, the other
//! block formats decoded by texture2ddecoder, uncompressed formats converted here.
//! Only the full size images are shown, not the smaller mip levels.

use std::borrow::Cow;
use std::io::Read;

use anyhow::{Context, Result, anyhow, bail};
use exr::prelude::f16;
use image::{DynamicImage, ImageBuffer, Rgb32FImage, Rgba32FImage, RgbaImage};
use ktx2::{Format, Header, Reader, SupercompressionScheme};

/// How many images a texture holds: array layers, cube faces and depth slices.
/// The 80 bytes of the header are enough.
pub fn image_count(bytes: &[u8]) -> Result<usize> {
    let header = bytes
        .get(..Header::LENGTH)
        .and_then(|h| h.try_into().ok())
        .context("Not a valid KTX2 file: too short")?;
    let header = Header::from_bytes(header).map_err(|e| anyhow!("Not a valid KTX2 file: {e:?}"))?;
    Ok(layers(&header) * faces(&header) * slices(&header))
}

/// One image of a texture. `index` goes through the slices of a face, the faces
/// of a layer, then the layers, the order KTX2 stores them in.
pub fn decode(bytes: &[u8], index: usize) -> Result<DynamicImage> {
    let reader = reader(bytes)?;
    let header = reader.header();
    if index >= layers(&header) * faces(&header) * slices(&header) {
        bail!("KTX2 has no image {index}");
    }
    let (width, height) = (header.pixel_width, header.pixel_height.max(1));
    let layer = index / (faces(&header) * slices(&header));
    let face = index / slices(&header) % faces(&header);

    let format = match header.format {
        // Basis Universal has no Vulkan format
        None => return transcode(bytes, layer, face, width, height),
        Some(f) if is_astc(f) => return transcode(bytes, layer, face, width, height),
        Some(f) => f,
    };
    let (block_size, block_bytes) =
        texel(format).with_context(|| format!("KTX2 format {format:?} is not supported"))?;

    let level = reader.levels().next().context("KTX2 has no image data")?;
    let data: Cow<[u8]> = match header.supercompression_scheme {
        None => Cow::Borrowed(level.data),
        Some(SupercompressionScheme::Zstandard) => {
            let mut out = vec![];
            ruzstd::StreamingDecoder::new(level.data)
                .map_err(|e| anyhow!("KTX2 zstd: {e}"))?
                .read_to_end(&mut out)?;
            Cow::Owned(out)
        }
        Some(SupercompressionScheme::ZLIB) => {
            let mut out = vec![];
            flate2::read::ZlibDecoder::new(level.data).read_to_end(&mut out)?;
            Cow::Owned(out)
        }
        Some(other) => bail!("KTX2 supercompression {other:?} is not supported"),
    };
    let image_bytes =
        (width.div_ceil(block_size) * height.div_ceil(block_size)) as usize * block_bytes;
    let data = data
        .get(index * image_bytes..(index + 1) * image_bytes)
        .context("KTX2 image data is too short")?;
    convert(format, data, width, height)
}

fn reader(bytes: &[u8]) -> Result<Reader<&[u8]>> {
    Reader::new(bytes).map_err(|e| anyhow!("Not a valid KTX2 file: {e:?}"))
}

fn layers(header: &Header) -> usize {
    header.layer_count.max(1) as usize
}

fn faces(header: &Header) -> usize {
    header.face_count.max(1) as usize
}

fn slices(header: &Header) -> usize {
    header.pixel_depth.max(1) as usize
}

fn is_astc(format: Format) -> bool {
    // the ASTC formats are numbered 157 to 184, the HDR ones 1000066000 to 1000066013
    matches!(format.value(), 157..=184 | 1000066000..=1000066013)
}

/// Basis Universal and ASTC, to 8 bit RGBA, or to floats for HDR
fn transcode(
    bytes: &[u8],
    layer: usize,
    face: usize,
    width: u32,
    height: u32,
) -> Result<DynamicImage> {
    use basisu::{DecodeFlags, TargetFormat, Transcoder};
    let transcoder = Transcoder::new(bytes).map_err(|e| anyhow!("KTX2: {e:?}"))?;
    let (layer, face) = (layer as u32, face as u32);
    if transcoder.supports(TargetFormat::Rgba32) {
        let pixels = transcoder
            .transcode_image(0, layer, face, TargetFormat::Rgba32, DecodeFlags::NONE)
            .map_err(|e| anyhow!("KTX2: {e:?}"))?;
        let image =
            RgbaImage::from_raw(width, height, pixels).context("KTX2 image is too small")?;
        return Ok(DynamicImage::ImageRgba8(image));
    }
    let pixels = transcoder
        .transcode_image(0, layer, face, TargetFormat::RgbaHalf, DecodeFlags::NONE)
        .map_err(|e| anyhow!("KTX2: {e:?}"))?;
    let floats = pixels
        .as_chunks::<2>()
        .0
        .iter()
        .map(|h| f16::from_le_bytes(*h).to_f32())
        .collect();
    let image = Rgba32FImage::from_raw(width, height, floats).context("KTX2 image is too small")?;
    Ok(DynamicImage::ImageRgba8(
        DynamicImage::ImageRgba32F(image).to_rgba8(),
    ))
}

/// The width and height of a block in pixels and its size in bytes. Uncompressed
/// formats are blocks of one pixel.
fn texel(format: Format) -> Option<(u32, usize)> {
    use Format as F;
    Some(match format {
        F::R8_UNORM | F::R8_SRGB => (1, 1),
        F::R8G8_UNORM | F::R8G8_SRGB => (1, 2),
        F::R8G8B8_UNORM | F::R8G8B8_SRGB | F::B8G8R8_UNORM | F::B8G8R8_SRGB => (1, 3),
        F::R8G8B8A8_UNORM | F::R8G8B8A8_SRGB | F::B8G8R8A8_UNORM | F::B8G8R8A8_SRGB => (1, 4),
        F::R16_UNORM | F::R16_SFLOAT => (1, 2),
        F::R16G16B16_UNORM | F::R16G16B16_SFLOAT => (1, 6),
        F::R16G16B16A16_UNORM | F::R16G16B16A16_SFLOAT => (1, 8),
        F::R32_SFLOAT | F::B10G11R11_UFLOAT_PACK32 | F::E5B9G9R9_UFLOAT_PACK32 => (1, 4),
        F::R32G32B32_SFLOAT => (1, 12),
        F::R32G32B32A32_SFLOAT => (1, 16),
        F::BC1_RGB_UNORM_BLOCK
        | F::BC1_RGB_SRGB_BLOCK
        | F::BC1_RGBA_UNORM_BLOCK
        | F::BC1_RGBA_SRGB_BLOCK
        | F::BC4_UNORM_BLOCK
        | F::ETC2_R8G8B8_UNORM_BLOCK
        | F::ETC2_R8G8B8_SRGB_BLOCK
        | F::ETC2_R8G8B8A1_UNORM_BLOCK
        | F::ETC2_R8G8B8A1_SRGB_BLOCK
        | F::EAC_R11_UNORM_BLOCK => (4, 8),
        F::BC2_UNORM_BLOCK
        | F::BC2_SRGB_BLOCK
        | F::BC3_UNORM_BLOCK
        | F::BC3_SRGB_BLOCK
        | F::BC5_UNORM_BLOCK
        | F::BC6H_UFLOAT_BLOCK
        | F::BC6H_SFLOAT_BLOCK
        | F::BC7_UNORM_BLOCK
        | F::BC7_SRGB_BLOCK
        | F::ETC2_R8G8B8A8_UNORM_BLOCK
        | F::ETC2_R8G8B8A8_SRGB_BLOCK
        | F::EAC_R11G11_UNORM_BLOCK => (4, 16),
        _ => return None,
    })
}

/// A texture2ddecoder function: blocks, width, height, pixels to fill
type BlockDecoder = fn(&[u8], usize, usize, &mut [u32]) -> Result<(), &'static str>;

/// One image of `texel(format)` layout to pixels. Floats are shown clamped to
/// 0..1, as Oculante always showed float KTX2.
fn convert(format: Format, data: &[u8], width: u32, height: u32) -> Result<DynamicImage> {
    use Format as F;
    use texture2ddecoder as t2d;
    let short = || anyhow!("KTX2 image data is too short");
    let halfs = || {
        data.as_chunks::<2>()
            .0
            .iter()
            .map(|h| f16::from_le_bytes(*h).to_f32())
    };
    let floats = || {
        data.as_chunks::<4>()
            .0
            .iter()
            .map(|f| f32::from_le_bytes(*f))
    };
    let words = || {
        data.as_chunks::<2>()
            .0
            .iter()
            .map(|w| u16::from_le_bytes(*w))
    };
    let rgb_floats = |values: Vec<f32>| -> Result<DynamicImage> {
        let image = Rgb32FImage::from_raw(width, height, values).ok_or_else(short)?;
        Ok(DynamicImage::ImageRgba8(
            DynamicImage::ImageRgb32F(image).to_rgba8(),
        ))
    };
    let blocks = |decode: BlockDecoder| -> Result<DynamicImage> {
        let mut pixels = vec![0u32; width as usize * height as usize];
        decode(data, width as usize, height as usize, &mut pixels)
            .map_err(|e| anyhow!("KTX2: {e}"))?;
        // texture2ddecoder packs a pixel as 0xAARRGGBB
        let rgba = pixels
            .iter()
            .flat_map(|p| {
                let [b, g, r, a] = p.to_le_bytes();
                [r, g, b, a]
            })
            .collect();
        Ok(DynamicImage::ImageRgba8(
            RgbaImage::from_raw(width, height, rgba).ok_or_else(short)?,
        ))
    };
    let swapped = |channels: usize| {
        let mut pixels = data.to_vec();
        pixels.chunks_exact_mut(channels).for_each(|p| p.swap(0, 2));
        pixels
    };
    Ok(match format {
        F::R8_UNORM | F::R8_SRGB => DynamicImage::ImageLuma8(
            ImageBuffer::from_raw(width, height, data.to_vec()).ok_or_else(short)?,
        ),
        F::R8G8_UNORM | F::R8G8_SRGB => DynamicImage::ImageLumaA8(
            ImageBuffer::from_raw(width, height, data.to_vec()).ok_or_else(short)?,
        ),
        F::R8G8B8_UNORM | F::R8G8B8_SRGB => DynamicImage::ImageRgb8(
            ImageBuffer::from_raw(width, height, data.to_vec()).ok_or_else(short)?,
        ),
        F::B8G8R8_UNORM | F::B8G8R8_SRGB => DynamicImage::ImageRgb8(
            ImageBuffer::from_raw(width, height, swapped(3)).ok_or_else(short)?,
        ),
        F::R8G8B8A8_UNORM | F::R8G8B8A8_SRGB => DynamicImage::ImageRgba8(
            ImageBuffer::from_raw(width, height, data.to_vec()).ok_or_else(short)?,
        ),
        F::B8G8R8A8_UNORM | F::B8G8R8A8_SRGB => DynamicImage::ImageRgba8(
            ImageBuffer::from_raw(width, height, swapped(4)).ok_or_else(short)?,
        ),
        F::R16_UNORM => DynamicImage::ImageLuma16(
            ImageBuffer::from_raw(width, height, words().collect()).ok_or_else(short)?,
        ),
        F::R16G16B16_UNORM => DynamicImage::ImageRgb16(
            ImageBuffer::from_raw(width, height, words().collect()).ok_or_else(short)?,
        ),
        F::R16G16B16A16_UNORM => DynamicImage::ImageRgba16(
            ImageBuffer::from_raw(width, height, words().collect()).ok_or_else(short)?,
        ),
        F::R16_SFLOAT => rgb_floats(halfs().flat_map(|v| [v; 3]).collect())?,
        F::R32_SFLOAT => rgb_floats(floats().flat_map(|v| [v; 3]).collect())?,
        F::R16G16B16_SFLOAT => rgb_floats(halfs().collect())?,
        F::R32G32B32_SFLOAT => rgb_floats(floats().collect())?,
        F::R16G16B16A16_SFLOAT => {
            let image =
                Rgba32FImage::from_raw(width, height, halfs().collect()).ok_or_else(short)?;
            DynamicImage::ImageRgba8(DynamicImage::ImageRgba32F(image).to_rgba8())
        }
        F::R32G32B32A32_SFLOAT => {
            let image =
                Rgba32FImage::from_raw(width, height, floats().collect()).ok_or_else(short)?;
            DynamicImage::ImageRgba8(DynamicImage::ImageRgba32F(image).to_rgba8())
        }
        F::B10G11R11_UFLOAT_PACK32 => rgb_floats(
            data.as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| unpack_b10g11r11(u32::from_le_bytes(*p)))
                .collect(),
        )?,
        F::E5B9G9R9_UFLOAT_PACK32 => rgb_floats(
            data.as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| unpack_e5b9g9r9(u32::from_le_bytes(*p)))
                .collect(),
        )?,
        F::BC1_RGB_UNORM_BLOCK | F::BC1_RGB_SRGB_BLOCK => blocks(t2d::decode_bc1)?,
        F::BC1_RGBA_UNORM_BLOCK | F::BC1_RGBA_SRGB_BLOCK => blocks(t2d::decode_bc1a)?,
        F::BC2_UNORM_BLOCK | F::BC2_SRGB_BLOCK => blocks(t2d::decode_bc2)?,
        F::BC3_UNORM_BLOCK | F::BC3_SRGB_BLOCK => blocks(t2d::decode_bc3)?,
        F::BC4_UNORM_BLOCK => blocks(t2d::decode_bc4)?,
        F::BC5_UNORM_BLOCK => blocks(t2d::decode_bc5)?,
        F::BC6H_UFLOAT_BLOCK => blocks(t2d::decode_bc6_unsigned)?,
        F::BC6H_SFLOAT_BLOCK => blocks(t2d::decode_bc6_signed)?,
        F::BC7_UNORM_BLOCK | F::BC7_SRGB_BLOCK => blocks(t2d::decode_bc7)?,
        F::ETC2_R8G8B8_UNORM_BLOCK | F::ETC2_R8G8B8_SRGB_BLOCK => blocks(t2d::decode_etc2_rgb)?,
        F::ETC2_R8G8B8A1_UNORM_BLOCK | F::ETC2_R8G8B8A1_SRGB_BLOCK => {
            blocks(t2d::decode_etc2_rgba1)?
        }
        F::ETC2_R8G8B8A8_UNORM_BLOCK | F::ETC2_R8G8B8A8_SRGB_BLOCK => {
            blocks(t2d::decode_etc2_rgba8)?
        }
        F::EAC_R11_UNORM_BLOCK => blocks(t2d::decode_eacr)?,
        F::EAC_R11G11_UNORM_BLOCK => blocks(t2d::decode_eacrg)?,
        other => bail!("KTX2 format {other:?} is not supported"),
    })
}

/// Three small floats without a sign: 5 bits of exponent each, with 6, 6 and 5 bits of mantissa
fn unpack_b10g11r11(p: u32) -> [f32; 3] {
    let small_float = |bits: u32, mantissa_bits: u32| {
        let (exponent, mantissa) = (bits >> mantissa_bits, bits & ((1 << mantissa_bits) - 1));
        let mantissa = mantissa as f32 / (1 << mantissa_bits) as f32;
        match exponent {
            0 => mantissa * 2f32.powi(-14),
            31 => f32::INFINITY,
            e => (1.0 + mantissa) * 2f32.powi(e as i32 - 15),
        }
    };
    [
        small_float(p & 0x7ff, 6),
        small_float((p >> 11) & 0x7ff, 6),
        small_float(p >> 22, 5),
    ]
}

/// Three 9 bit mantissas sharing one 5 bit exponent
fn unpack_e5b9g9r9(p: u32) -> [f32; 3] {
    let scale = 2f32.powi((p >> 27) as i32 - 15 - 9);
    [
        (p & 0x1ff) as f32 * scale,
        ((p >> 9) & 0x1ff) as f32 * scale,
        ((p >> 18) & 0x1ff) as f32 * scale,
    ]
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{decode, image_count};
    use image::GenericImageView;
    use ktx2::Format;

    /// A KTX2 file with one level and a data format descriptor that is only its size
    pub(crate) fn ktx2_file(
        format: Format,
        size: (u32, u32, u32),
        layers: u32,
        faces: u32,
        zstd: bool,
        level: &[u8],
    ) -> Vec<u8> {
        let mut file = vec![
            0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
        ];
        let header = [
            format.value(),
            1,
            size.0,
            size.1,
            size.2,
            layers,
            faces,
            1,
            if zstd { 2 } else { 0 },
        ];
        header.iter().for_each(|v| file.extend(v.to_le_bytes()));
        // the descriptor, then the key/values and global data, which there are none of
        let dfd_offset = 80u32 + 24;
        [dfd_offset, 4, 0, 0]
            .iter()
            .for_each(|v| file.extend(v.to_le_bytes()));
        file.extend([0; 16]);
        let data = if zstd {
            stored_zstd(level)
        } else {
            level.to_vec()
        };
        [dfd_offset as u64 + 4, data.len() as u64, level.len() as u64]
            .iter()
            .for_each(|v| file.extend(v.to_le_bytes()));
        file.extend(4u32.to_le_bytes());
        file.extend(data);
        file
    }

    /// A zstd frame with the bytes in one block, not compressed
    fn stored_zstd(bytes: &[u8]) -> Vec<u8> {
        assert!(bytes.len() < 256);
        let mut frame = vec![0x28, 0xB5, 0x2F, 0xFD, 0x20, bytes.len() as u8];
        frame.extend((((bytes.len() as u32) << 3) | 1).to_le_bytes()[..3].iter());
        frame.extend(bytes);
        frame
    }

    fn colors(image: &image::DynamicImage) -> Vec<[u8; 4]> {
        image.pixels().map(|(_, _, p)| p.0).collect()
    }

    #[test]
    fn ci_ktx2_layers_faces_and_slices() {
        // two layers of a cube, every face of 2x2 pixels in its own red
        let red = |layer: u8, face: u8| layer * 100 + face * 10;
        let level: Vec<u8> = (0..2)
            .flat_map(|layer| (0..6).flat_map(move |face| [red(layer, face), 0, 0, 255].repeat(4)))
            .collect();
        let file = ktx2_file(Format::R8G8B8A8_UNORM, (2, 2, 0), 2, 6, false, &level);
        assert_eq!(image_count(&file).unwrap(), 12);
        assert_eq!(image_count(&file[..80]).unwrap(), 12);
        for index in 0..12 {
            let image = decode(&file, index).unwrap();
            assert_eq!(
                colors(&image),
                vec![[red(index as u8 / 6, index as u8 % 6), 0, 0, 255]; 4]
            );
        }
        assert!(decode(&file, 12).is_err());

        // a 3D texture of three slices
        let file = ktx2_file(
            Format::R8_UNORM,
            (2, 2, 3),
            0,
            1,
            false,
            &[[10; 4], [20; 4], [30; 4]].concat(),
        );
        assert_eq!(image_count(&file).unwrap(), 3);
        assert_eq!(
            colors(&decode(&file, 2).unwrap()),
            vec![[30, 30, 30, 255]; 4]
        );
    }

    #[test]
    fn ci_ktx2_formats() {
        let one_pixel = |format: Format, bytes: &[u8]| {
            decode(&ktx2_file(format, (1, 1, 0), 0, 1, false, bytes), 0).unwrap()
        };
        // 1.0, 0.5 and 0.25 as 11, 11 and 10 bit floats
        let packed: u32 = (15 << 6) | ((14 << 6) << 11) | ((13 << 5) << 22);
        assert_eq!(
            colors(&one_pixel(
                Format::B10G11R11_UFLOAT_PACK32,
                &packed.to_le_bytes()
            )),
            [[255, 128, 64, 255]]
        );
        // the same with a shared exponent
        let packed: u32 = 256 | (128 << 9) | (64 << 18) | (16 << 27);
        assert_eq!(
            colors(&one_pixel(
                Format::E5B9G9R9_UFLOAT_PACK32,
                &packed.to_le_bytes()
            )),
            [[255, 128, 64, 255]]
        );
        assert_eq!(
            colors(&one_pixel(Format::B8G8R8A8_UNORM, &[1, 2, 3, 4])),
            [[3, 2, 1, 4]]
        );
        assert_eq!(
            colors(&one_pixel(
                Format::R16G16B16A16_UNORM,
                &[0, 0, 0, 128, 255, 255, 255, 255]
            )),
            [[0, 128, 255, 255]]
        );

        // a BC1 block of red, with zstd
        let block = [0x00, 0xF8, 0x00, 0x00, 0, 0, 0, 0];
        let file = ktx2_file(Format::BC1_RGB_UNORM_BLOCK, (4, 4, 0), 0, 1, true, &block);
        assert_eq!(
            colors(&decode(&file, 0).unwrap()),
            vec![[255, 0, 0, 255]; 16]
        );
    }

    #[test]
    fn ci_ktx2_basis_etc2_and_astc() {
        // all red, give or take the compression
        for name in [
            "r8g8b8a8_srgb_mip_blze",
            "r8g8b8a8_srgb_mip_etc2",
            "r8g8b8a8_srgb_mip_astc",
        ] {
            let file = std::fs::read(format!("res/tests/khronos/{name}.ktx2")).unwrap();
            let image = decode(&file, 0).unwrap();
            assert_eq!(image.dimensions(), (64, 64), "{name}");
            assert!(
                colors(&image)
                    .iter()
                    .all(|p| p[0] > 250 && p[1] < 5 && p[2] < 5),
                "{name}"
            );
        }
        // UASTC with zstd: the top left cell is red, the top right magenta
        let file = std::fs::read("res/tests/khronos/color_grid_uastc_zstd_5.ktx2").unwrap();
        let image = decode(&file, 0).unwrap();
        let [r, g, b, _] = image.get_pixel(64, 64).0;
        assert!(r > 200 && g < 130 && b < 130, "{r} {g} {b}");
        let [r, g, b, _] = image.get_pixel(960, 64).0;
        assert!(r > 200 && g < 150 && b > 180, "{r} {g} {b}");
    }
}
