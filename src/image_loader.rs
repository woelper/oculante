use crate::settings::DecoderSettings;
use crate::utils::{Frame, fit};
use crate::{FONT, appstate::Message, ktx2_loader};
use log::{debug, error, info, warn};
use psd::Psd;

use anyhow::{Context, Result, anyhow, bail};
use dds::DDS;
use exr::prelude as exrs;
use exr::prelude::*;
use image::{
    AnimationDecoder, DynamicImage, GrayAlphaImage, GrayImage, ImageDecoder, ImageReader, RgbImage,
    RgbaImage,
};
use jxl_oxide::{JxlImage, PixelFormat};
use quickraw::Export;
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use rgb::*;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, channel};
use tiff::decoder::Limits;

/// Open an image from disk and send it somewhere
pub fn open_image(
    img_location: &Path,
    message_sender: Option<Sender<Message>>,
    decoder_opts: Option<DecoderSettings>,
) -> Result<Receiver<Frame>> {
    open_image_item(img_location, 0, message_sender, decoder_opts)
}

/// How many images a file holds that are shown one at a time: the pages of a
/// TIFF, the images of a HEIF file, the frames of a DICOM file, the layers, cube
/// faces and slices of a KTX2 texture. 1 for all other files.
pub fn image_count(path: &Path) -> usize {
    let extension = canonical_extension(
        path.extension()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default(),
    );
    let count = || -> Result<usize> {
        Ok(match extension.as_str() {
            "tif" | "tiff" => {
                let mut decoder = tiff_decoder(path, 0)?;
                let mut pages = 1;
                while decoder.more_images() {
                    decoder.next_image()?;
                    pages += 1;
                }
                pages
            }
            "ktx2" => {
                let mut header = [0; ktx2::Header::LENGTH];
                File::open(path)?.read_exact(&mut header)?;
                ktx2_loader::image_count(&header)?
            }
            "dcm" | "ima" => {
                // the pixel data is not needed to count the frames
                let header = dicom_object::OpenFileOptions::new()
                    .read_until(dicom_object::Tag(0x7FE0, 0x0010))
                    .open_file(path)?;
                // Number of Frames, only there when there is more than one
                header
                    .element(dicom_object::Tag(0x0028, 0x0008))
                    .ok()
                    .and_then(|frames| frames.to_int::<usize>().ok())
                    .unwrap_or(1)
            }
            #[cfg(feature = "heif_native")]
            "heif" | "heic" => heic_rs::images(&std::fs::read(path)?)?.len(),
            _ => 1,
        })
    };
    count().unwrap_or(1).max(1)
}

/// Like `open_image`, for one of the images of a file, see `image_count`
pub fn open_image_item(
    img_location: &Path,
    item: usize,
    message_sender: Option<Sender<Message>>,
    decoder_opts: Option<DecoderSettings>,
) -> Result<Receiver<Frame>> {
    let (sender, receiver): (Sender<Frame>, Receiver<Frame>) = channel();
    let img_location = (*img_location).to_owned();

    use file_format::FileFormat;

    let mut extension = canonical_extension(
        img_location
            .extension()
            .unwrap_or_default()
            .to_str()
            .unwrap_or_default(),
    );

    // These are detected incorrectly, for example svg is xml etc
    let unchecked_extensions = ["svg", "kra", "ora", "tga", "dng"];

    if let Ok(fmt) = FileFormat::from_file(&img_location) {
        debug!("Detected as {:?} {}", fmt.name(), fmt.extension());
        if canonical_extension(fmt.extension()) != extension {
            if unchecked_extensions.contains(&extension.as_str()) {
                info!("Extension {extension} skipped check.")
            } else {
                message_sender.as_ref().map(|s| {
                    s.send(Message::Warning(format!(
                        "Extension mismatch. This image is loaded as {}",
                        fmt.extension()
                    )))
                });
                extension = fmt.extension().into()
            }
        }
    } else {
        error!("Can't determine image type")
    }

    debug!("matching '{extension}'");

    match extension.as_str() {
        "dds" => {
            let file = File::open(img_location)?;
            let mut reader = BufReader::new(file);
            let dds = DDS::decode(&mut reader).map_err(|e| anyhow!("{:?}", e))?;
            if let Some(main_layer) = dds.layers.first() {
                let buf = main_layer.as_bytes();
                let buf =
                    image::ImageBuffer::from_raw(dds.header.width, dds.header.height, buf.into())
                        .context("Can't create DDS ImageBuffer with given res")?;
                let d = DynamicImage::ImageRgba8(buf);
                _ = sender.send(Frame::new_still(d));
                return Ok(receiver);
            }
        }
        "dcm" | "ima" => {
            use dicom_pixeldata::PixelDecoder;
            let obj = dicom_object::open_file(img_location)?;
            let image = obj.decode_pixel_data_frame(item as u32)?;
            let dynamic_image = image.to_dynamic_image(0)?;
            _ = sender.send(Frame::new_still(dynamic_image));
        }
        "ktx2" => {
            let image = ktx2_loader::decode(&std::fs::read(img_location)?, item)?;
            _ = sender.send(Frame::new_still(image));
            return Ok(receiver);
        }
        #[cfg(feature = "j2k")]
        "jp2" => {
            let jp2_image = jpeg2k::Image::from_file(img_location)?;

            match jp2_image.get_pixels(Some(255))?.data {
                jpeg2k::ImagePixelData::L8(_) => bail!("jpeg2k L8 unsupported"),
                jpeg2k::ImagePixelData::La8(_) => bail!("jpeg2k La8 unsupported"),
                jpeg2k::ImagePixelData::Rgb8(data) => {
                    let image_buffer =
                        RgbImage::from_raw(jp2_image.width(), jp2_image.height(), data)
                            .context("Can't decode jp2k buffer")?;
                    _ = sender.send(Frame::new_still(DynamicImage::ImageRgb8(image_buffer)));
                    return Ok(receiver);
                }
                jpeg2k::ImagePixelData::Rgba8(data) => {
                    let image_buffer =
                        RgbaImage::from_raw(jp2_image.width(), jp2_image.height(), data)
                            .context("Can't decode jp2k buffer")?;
                    let i = DynamicImage::ImageRgba8(image_buffer);
                    _ = sender.send(Frame::new_still(i));
                    return Ok(receiver);
                }
                jpeg2k::ImagePixelData::L16(_) => bail!("jpeg2k L16 unsupported"),
                jpeg2k::ImagePixelData::La16(_) => bail!("jpeg2k La16 unsupported"),
                jpeg2k::ImagePixelData::Rgb16(_) => bail!("jpeg2k Rgb16 unsupported"),
                jpeg2k::ImagePixelData::Rgba16(_) => bail!("jpeg2k Rgba16 unsupported"),
            }
        }
        #[cfg(feature = "heif_native")]
        "heif" | "heic" => {
            let i = load_heif_native(&img_location, item)?;
            _ = sender.send(Frame::new_still(i));
            return Ok(receiver);
        }
        #[cfg(feature = "avif_native")]
        "avif" => {
            let data = std::fs::read(&img_location)?;
            // 10 and 12 bit images stay 16 bit, as for PNG
            macro_rules! image {
                ($pixels:expr, $layout:ident) => {{
                    let (pixels, width, height) = $pixels.into_contiguous_buf();
                    let samples = bytemuck::cast_slice(&pixels).to_vec();
                    DynamicImage::$layout(
                        image::ImageBuffer::from_raw(width as u32, height as u32, samples)
                            .context("Can't create avif ImageBuffer with given res")?,
                    )
                }};
            }
            let image = match avif_decode::Decoder::from_avif(&data)?.to_image()? {
                avif_decode::Image::Rgb8(pixels) => image!(pixels, ImageRgb8),
                avif_decode::Image::Rgba8(pixels) => image!(pixels, ImageRgba8),
                avif_decode::Image::Rgb16(pixels) => image!(pixels, ImageRgb16),
                avif_decode::Image::Rgba16(pixels) => image!(pixels, ImageRgba16),
                avif_decode::Image::Gray8(pixels) => image!(pixels, ImageLuma8),
                avif_decode::Image::Gray16(pixels) => image!(pixels, ImageLuma16),
            };
            _ = sender.send(Frame::new_still(image));
            return Ok(receiver);
        }
        "svg" => {
            let svg_scale = decoder_opts
                .map(|d| d.svg_scale)
                .filter(|s| *s > 0.0)
                .unwrap_or(crate::settings::DEFAULT_SVG_SCALE);

            let render_scale = svg_scale.clamp(0.01, 100.0);

            let mut opt = usvg::Options::default();

            let fontdb = opt.fontdb_mut();
            fontdb.load_system_fonts();
            fontdb.load_font_data(FONT.to_vec());
            fontdb.set_cursive_family("Inter");
            fontdb.set_sans_serif_family("Inter");
            fontdb.set_serif_family("Inter");

            opt.font_family = "Inter".into();
            opt.font_size = 6.;

            let svg_data = std::fs::read(img_location)?;
            if let Ok(tree) = usvg::Tree::from_data(&svg_data, &opt) {
                let pixmap_size = tree
                    .size()
                    .to_int_size()
                    .scale_by(render_scale)
                    .context("Can't get SVG size at the requested SVG scale")?;

                let mut pixmap =
                    tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height()).context("Can't allocate a canvas for this SVG at the requested SVG scale. Try lowering the SVG scale.")?;

                let mut fontdb = usvg::fontdb::Database::new();
                fontdb.load_system_fonts();
                fontdb.load_font_data(FONT.to_vec());
                fontdb.set_cursive_family("Inter");
                fontdb.set_sans_serif_family("Inter");
                fontdb.set_serif_family("Inter");

                let render_ts = tiny_skia::Transform::from_scale(render_scale, render_scale);
                resvg::render(&tree, render_ts, &mut pixmap.as_mut());
                let buf: RgbaImage = image::ImageBuffer::from_raw(
                    pixmap_size.width(),
                    pixmap_size.height(),
                    pixmap.data().to_vec(),
                )
                .context("Can't create image buffer from SVG render")?;
                let i = DynamicImage::ImageRgba8(buf);

                _ = sender.send(Frame::new_still(i));
                return Ok(receiver);
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
            > = reader.from_file(&img_location);
            match maybe_image {
                Ok(image) => {
                    let buf = image.layer_data.channel_data.pixels;
                    let i = DynamicImage::ImageRgba8(buf);

                    _ = sender.send(Frame::new_still(i));
                    return Ok(receiver);
                }
                Err(e) => {
                    let layer = exrs::read_first_flat_layer_from_file(&img_location)?;

                    let size = layer.layer_data.size;
                    for i in layer.layer_data.channel_data.list {
                        let d = i.sample_data;
                        match d {
                            FlatSamples::F16(_) => bail!("F16 color mode not supported"),
                            FlatSamples::F32(f) => {
                                let Some(gray_image) = GrayImage::from_raw(
                                    size.width() as u32,
                                    size.height() as u32,
                                    f.par_iter().map(|x| tonemap_f32(*x)).collect::<Vec<_>>(),
                                ) else {
                                    error!("Can't decode gray alpha buffer");
                                    continue;
                                };

                                let d = DynamicImage::ImageLuma8(gray_image);
                                _ = sender.send(Frame::new_still(d));
                                return Ok(receiver);
                            }
                            FlatSamples::U32(_) => bail!("U32 color mode not supported"),
                        }
                    }

                    bail!("{} from {:?}", e, img_location)
                }
            }
        }
        "nef" | "cr2" | "dng" | "mos" | "erf" | "raf" | "arw" | "3fr" | "ari" | "srf" | "sr2"
        | "braw" | "r3d" | "nrw" | "raw" => {
            debug!("Loading RAW");
            let buf = load_raw(&img_location)?;
            let i = DynamicImage::ImageRgba8(buf);

            _ = sender.send(Frame::new_still(i));
            return Ok(receiver);
        }
        "jxl" => {
            std::thread::spawn(move || {
                if let Err(e) = load_jxl(&img_location, sender) {
                    error!("{e}");
                }
            });
            return Ok(receiver);
        }
        "hdr" => {
            let f = File::open(img_location)?;
            let reader = BufReader::new(f);
            let hdr_decoder = image::codecs::hdr::HdrDecoder::new(reader)?;

            #[cfg(feature = "hdr")]
            {
                let d = DynamicImage::from_decoder(hdr_decoder)?;
                _ = sender.send(Frame::new_still(d));
                return Ok(receiver);
            }

            #[cfg(not(feature = "hdr"))]
            {
                let meta = hdr_decoder.metadata();
                let hdr_img: image::Rgb32FImage = match DynamicImage::from_decoder(hdr_decoder)? {
                    DynamicImage::ImageRgb32F(image) => image,
                    _ => bail!("expected rgb32f image"),
                };
                let buf = RgbaImage::from_fn(meta.width, meta.height, |x, y| {
                    let pixel = hdr_img.get_pixel(x, y);
                    image::Rgba(tonemap_rgb(pixel.0))
                });
                let i = DynamicImage::ImageRgba8(buf);
                _ = sender.send(Frame::new_still(i));
                return Ok(receiver);
            }
        }
        "psd" => {
            let contents = std::fs::read(img_location)?;
            let psd = Psd::from_bytes(&contents).map_err(|e| anyhow!("{:?}", e))?;
            let buf = image::ImageBuffer::from_raw(psd.width(), psd.height(), psd.rgba())
                .context("Can't create imagebuffer from PSD")?;
            let i = DynamicImage::ImageRgba8(buf);

            _ = sender.send(Frame::new_still(i));
            return Ok(receiver);
        }
        "webp" => {
            debug!("Loading WebP");
            let contents = std::fs::read(&img_location)?;
            let decoder = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(&contents))?;
            if !decoder.has_animation() {
                //force this to webp
                let img = decode_without_limits(ImageReader::with_format(
                    std::io::Cursor::new(contents),
                    image::ImageFormat::WebP,
                ))?;
                _ = sender.send(Frame::new_still(img));
                return Ok(receiver);
            }

            let plays = plays(decoder.loop_count());
            for frame in decoder.into_frames() {
                let frame = frame.context("Can't decode animated webp frame")?;
                let (delay_numer, delay_denom) = frame.delay().numer_denom_ms();
                let delay_ms = delay_numer.checked_div(delay_denom).unwrap_or(0);
                debug!("webp frame delay {delay_ms}ms");
                let i = DynamicImage::ImageRgba8(frame.into_buffer());
                let frame = Frame::new_animation(i, delay_ms);
                _ = sender.send(frame);
            }
            _ = sender.send(Frame::AnimationEnd(plays));

            // TODO: Use thread for animation and return receiver immediately, but this needs error handling
            return Ok(receiver);
        }
        "png" | "apng" => {
            let contents = std::fs::read(&img_location)?;
            let open = || image::codecs::png::PngDecoder::new(std::io::Cursor::new(&contents[..]));

            let decoder = open()?;
            let decoder = if decoder.is_apng()? {
                info!("Image is animated (APNG)");
                match decode_apng(decoder) {
                    Ok(Apng { frames, plays }) => {
                        for (image, delay_ms) in frames {
                            _ = sender.send(Frame::new_animation(image, delay_ms));
                        }
                        _ = sender.send(Frame::AnimationEnd(plays));
                        return Ok(receiver);
                    }
                    // A broken animation, or one with 16 bit colors that can not be
                    // composited, shows its default image instead, as the APNG
                    // specification recommends.
                    Err(e) => {
                        warn!("{e:#}");
                        if let Some(s) = &message_sender {
                            _ = s.send(Message::warn(&format!(
                                "This animation can't be played, showing its still image instead ({e:#})"
                            )));
                        }
                        open()?
                    }
                }
            } else {
                decoder
            };

            debug!("Showing a still image");
            let img = DynamicImage::from_decoder(decoder)?;
            _ = sender.send(Frame::new_still(img));
            return Ok(receiver);
        }
        "gif" => {
            let file = File::open(img_location)?;

            // Below is a workaround for partially corrupt gifs.
            let mut gif_opts = gif::DecodeOptions::new();
            gif_opts.set_color_output(gif::ColorOutput::Indexed);
            let mut decoder = gif_opts.read_info(file)?;
            let dim = (decoder.width() as u32, decoder.height() as u32);
            let mut screen = gif_dispose::Screen::new_decoder(&decoder);
            while let Ok(i) = decoder.read_next_frame() {
                debug!("decoded frame");
                if let Some(frame) = i {
                    screen.blit_frame(frame)?;
                    let buf: Option<image::RgbaImage> = image::ImageBuffer::from_raw(
                        dim.0,
                        dim.1,
                        screen.pixels_rgba().buf().as_bytes().to_vec(),
                    );
                    let buf = buf.context("Can't read gif frame")?;
                    let i = DynamicImage::ImageRgba8(buf);
                    // the delay is in hundredths of a second, up to more than ten minutes
                    _ = sender.send(Frame::new_animation(i, u32::from(frame.delay) * 10));
                } else {
                    break;
                }
            }
            debug!("Done decoding Gif!");
            // A GIF without a loop block plays once. A count of n repeats it n more
            // times after the first play, as browsers read it, and 0 is forever.
            let plays = match decoder.repeat() {
                gif::Repeat::Infinite => None,
                gif::Repeat::Finite(repeats) => Some(u32::from(repeats) + 1),
            };
            _ = sender.send(Frame::AnimationEnd(plays));

            return Ok(receiver);

            // TODO: Re-enable if https://github.com/image-rs/image/issues/1818 is resolved

            // let gif_decoder = GifDecoder::new(file)?;
            // let frames = gif_decoder.into_frames().collect_frames()?;
            // for f in frames {
            //     let delay = f.delay().numer_denom_ms().0 as u16;
            //     col.add_anim_frame(f.into_buffer(), delay);
            //     col.repeat = true;
            // }
        }
        #[cfg(feature = "turbo")]
        "jpg" | "jpeg" => {
            match load_jpeg_turbojpeg(&img_location) {
                Ok(i) => {
                    _ = sender.send(Frame::new_still(i));
                }
                Err(turbo_error) => {
                    // libjpeg-turbo fails on what libjpeg only warns about, such as a
                    // missing end of the file. The image library shows those.
                    error!(
                        "Could not load using turbojpeg: {turbo_error}. Trying to load with image library."
                    );
                    let img = decode_without_limits(ImageReader::open(&img_location)?)
                        .map_err(|e| anyhow!("{e} (libjpeg-turbo: {turbo_error})"))?;
                    _ = sender.send(Frame::new_still(img));
                }
            }
            return Ok(receiver);
        }
        "kra" | "ora" => {
            let i = load_merged_image(&img_location)?;
            _ = sender.send(Frame::new_still(i));
            return Ok(receiver);
        }
        "icns" => {
            let file = BufReader::new(File::open(&img_location)?);
            let icon_family = icns::IconFamily::read(file)?;

            // loop over the largest icons, take the largest one and return
            for icon_type in [
                icns::IconType::RGBA32_512x512_2x,
                icns::IconType::RGBA32_512x512,
                icns::IconType::RGBA32_256x256,
                icns::IconType::RGBA32_128x128,
            ] {
                let mut target = vec![];
                let Ok(image) = icon_family.get_icon_with_type(icon_type) else {
                    continue;
                };
                image.write_png(&mut target)?;
                let d = image::load_from_memory(&target).context("Load icns mem")?;
                _ = sender.send(Frame::new_still(d));
                return Ok(receiver);
            }

            bail!("No valid icons in {}", img_location.display());
        }
        "tif" | "tiff" => match load_tiff(&img_location, item) {
            Ok(buf) => {
                _ = sender.send(Frame::new_still(buf));
                return Ok(receiver);
            }
            Err(tiff_error) => match load_raw(&img_location) {
                Ok(buf) => {
                    let i = DynamicImage::ImageRgba8(buf);

                    info!("This image is a raw image with tiff format.");
                    _ = sender.send(Frame::new_still(i));
                    return Ok(receiver);
                }
                Err(raw_error) => {
                    bail!(
                        "Could not load tiff: {tiff_error}, tried as raw and still got error: {raw_error}"
                    )
                }
            },
        },
        _ => {
            // All other supported image files are handled by using `image` and `image_extras`
            image_extras::register();
            debug!("Loading using generic image library");
            let img = decode_without_limits(ImageReader::open(&img_location)?)?;
            // col.add_still(img.to_rgba8());
            _ = sender.send(Frame::new_still(img));
            return Ok(receiver);
        }
    }

    Ok(receiver)
}

fn tonemap_rgba(px: [f32; 4]) -> [u8; 4] {
    [
        tonemap_f32(px[0]),
        tonemap_f32(px[1]),
        tonemap_f32(px[2]),
        tonemap_f32(px[3]),
    ]
}

/// Gamma 2.2 of a linear value as 8 bit, from a table instead of a `powf` per channel,
/// which took 560 ms of a 24 MP EXR. Gives the same byte as `tonemap_f32_powf` for every
/// float but negative infinity, which is black here like the other negative values.
pub fn tonemap_f32(px: f32) -> u8 {
    if px >= 1.0 {
        return 255;
    }
    // zero, negative and NaN
    if px <= 0.0 || px.is_nan() {
        return 0;
    }
    let table = gamma_table();
    let mut value = table.start[(px.to_bits() >> 16) as usize];
    while value < 255 && px >= table.steps[value as usize] {
        value += 1;
    }
    value
}

fn tonemap_f32_powf(px: f32) -> u8 {
    (px.powf(1.0 / 2.2).clamp(0.0, 1.0) * 255.0) as u8
}

struct GammaTable {
    /// The value at the start of each run of floats that share their upper 16 bits
    start: Vec<u8>,
    /// `steps[k]` is the smallest float that gives k + 1
    steps: [f32; 255],
}

fn gamma_table() -> &'static GammaTable {
    static TABLE: OnceLock<GammaTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        // positive floats sort like their bits, so each step is a binary search over them
        let one = 1f32.to_bits();
        let mut steps = [0f32; 255];
        for (k, step) in steps.iter_mut().enumerate() {
            let (mut low, mut high) = (0, one);
            while low < high {
                let middle = low + (high - low) / 2;
                if tonemap_f32_powf(f32::from_bits(middle)) as usize > k {
                    high = middle;
                } else {
                    low = middle + 1;
                }
            }
            *step = f32::from_bits(low);
        }
        let start = (0..=one >> 16)
            .map(|i| tonemap_f32_powf(f32::from_bits(i << 16)))
            .collect();
        GammaTable { start, steps }
    })
}

#[cfg(not(feature = "hdr"))]
fn tonemap_rgb(px: [f32; 3]) -> [u8; 4] {
    let mut tm = tonemap_rgba([px[0], px[1], px[2], 1.0]);
    tm[3] = 255;
    tm
}

#[allow(unused)]
fn u16_to_u8(p: u16) -> u8 {
    ((p as f32 / u16::MAX as f32) * u8::MAX as f32) as u8
}

fn load_raw(img_location: &Path) -> Result<RgbaImage> {
    let raw_data = std::fs::read(img_location)?;
    let (thumbnail_data, _orientation) = Export::export_thumbnail_data(&raw_data)?;

    let i = image::load_from_memory(thumbnail_data)?;
    Ok(i.to_rgba8())

    // let export_job = Export::new(
    //     Input::ByFile(&img_location.to_string_lossy()),
    //     Output::new(
    //         DemosaicingMethod::SuperPixel,
    //         data::XYZ2SRGB,
    //         data::GAMMA_SRGB,
    //         OutputType::Raw16,
    //         true,
    //         true,
    //     ),
    // )?;

    // let mut buf = vec![];
    // let x = quickraw::Export::export_thumbnail_data(&buf)?;
    // image::load_from_memory(&buf)?;

    // let (image, width, height) = export_job.export_16bit_image();
    // let image = image
    //     .into_par_iter()
    //     .map(|x| tonemap_f32(x as f32 / 65536.))
    //     .collect::<Vec<_>>();

    // // Construct rgb image
    // let x = RgbImage::from_raw(width as u32, height as u32, image)
    //     .context("can't decode raw output as image")?;
    // // make it a Dynamic image
    // Ok(DynamicImage::ImageRgb8(x).to_rgba8())
}

/// A decoder at a page of a TIFF, the first is 0
fn tiff_decoder(path: &Path, page: usize) -> Result<tiff::decoder::Decoder<BufReader<File>>> {
    let mut decoder = tiff::decoder::Decoder::new(BufReader::new(File::open(path)?))?
        .with_limits(Limits::unlimited());
    if page > 0 {
        decoder.seek_to_image(page)?;
    }
    Ok(decoder)
}

/// The pixels of a TIFF image. The strips of a file are compressed one by one,
/// so they are decoded at the same time, each thread with a decoder of its own:
/// a photo of 24 megapixels in LZW takes 43 ms with 20 threads instead of 348.
fn read_tiff_image(
    decoder: &mut tiff::decoder::Decoder<BufReader<File>>,
    path: &Path,
    page: usize,
) -> Result<tiff::decoder::DecodingResult> {
    use rayon::prelude::*;
    use tiff::decoder::{ChunkType, DecodingResult};

    let layout = decoder.image_buffer_layout()?;
    let (width, height) = decoder.dimensions()?;
    let (pixels, height) = (width as usize * height as usize, height as usize);
    if layout.planes > 1 {
        // Each color stored after the other, read alone they gave only red.
        let mut result = DecodingResult::U8(vec![]);
        let layout = decoder.read_image_to_buffer(&mut result)?;
        let sample = layout.len / pixels.max(1);
        let plane_stride = layout.plane_stride.map_or(layout.len, |s| s.get());
        let mut buffer = result.as_buffer(0);
        let bytes = buffer.as_bytes_mut();
        let planar_len = plane_stride * (layout.planes - 1) + layout.len;
        if sample == 0 || layout.len != pixels * sample || bytes.len() < planar_len {
            bail!("Planar TIFF of this layout is not supported");
        }
        let planar = bytes[..planar_len].to_vec();
        let interleaved = &mut bytes[..pixels * sample * layout.planes];
        for (pixel, out) in interleaved
            .chunks_exact_mut(sample * layout.planes)
            .enumerate()
        {
            for (plane, out) in out.chunks_exact_mut(sample).enumerate() {
                let at = plane * plane_stride + pixel * sample;
                out.copy_from_slice(&planar[at..at + sample]);
            }
        }
        return Ok(result);
    }
    // Tiles are decoded in one go
    let strips = if decoder.get_chunk_type() == ChunkType::Strip {
        decoder.strip_count()?
    } else {
        0
    };
    if strips < 2 || height == 0 || layout.len % height != 0 {
        return Ok(decoder.read_image()?);
    }
    let row_bytes = layout.len / height;

    let mut result = DecodingResult::U8(vec![]);
    result.resize_to(&layout, &Limits::unlimited())?;
    let mut buffer = result.as_buffer(0);
    let mut rest = &mut buffer.as_bytes_mut()[..layout.len];
    let mut pieces = Vec::with_capacity(strips as usize);
    for strip in 0..strips {
        let rows = decoder.chunk_data_dimensions(strip).1 as usize;
        if rows * row_bytes > rest.len() {
            return Ok(decoder.read_image()?);
        }
        let (piece, after) = std::mem::take(&mut rest).split_at_mut(rows * row_bytes);
        pieces.push(piece);
        rest = after;
    }
    if !rest.is_empty() {
        return Ok(decoder.read_image()?);
    }
    pieces.into_par_iter().enumerate().try_for_each_init(
        || tiff_decoder(path, page),
        |decoder, (strip, piece)| -> Result<()> {
            let decoder = decoder.as_mut().map_err(|e| anyhow!("{e}"))?;
            decoder.read_chunk_bytes(strip as u32, piece)?;
            Ok(())
        },
    )?;
    Ok(result)
}

fn load_tiff(img_location: &Path, page: usize) -> Result<DynamicImage> {
    use tiff::decoder::DecodingResult;
    // TODO: Probe if dng
    let mut decoder = tiff_decoder(img_location, page)?;
    let dim = decoder.dimensions()?;
    debug!("Color type: {:?}", decoder.colortype());
    let result = read_tiff_image(&mut decoder, img_location, page)?;
    // A container for the low dynamic range image
    let ldr_img: Vec<u8> = match result {
        DecodingResult::U8(contents) => contents,
        DecodingResult::U16(contents) => autoscale(&contents, |p| p as f32, (0., u16::MAX as f32)),
        DecodingResult::F16(contents) => autoscale(&contents, f32::from, (0., 1.)),
        DecodingResult::U32(contents) => autoscale(&contents, |p| p as f32, (0., u32::MAX as f32)),
        DecodingResult::U64(contents) => autoscale(&contents, |p| p as f32, (0., u64::MAX as f32)),
        DecodingResult::F32(contents) => autoscale(&contents, |p| p, (0., 1.)),
        DecodingResult::F64(contents) => autoscale(&contents, |p| p as f32, (0., 1.)),
        DecodingResult::I8(contents) => {
            autoscale(&contents, |p| p as f32, (i8::MIN as f32, i8::MAX as f32))
        }
        DecodingResult::I16(contents) => {
            autoscale(&contents, |p| p as f32, (i16::MIN as f32, i16::MAX as f32))
        }
        DecodingResult::I32(contents) => {
            autoscale(&contents, |p| p as f32, (i32::MIN as f32, i32::MAX as f32))
        }
        DecodingResult::I64(contents) => {
            autoscale(&contents, |p| p as f32, (i64::MIN as f32, i64::MAX as f32))
        }
    };

    match decoder.colortype()? {
        tiff::ColorType::Gray(_) => {
            debug!("Loading gray color");
            let i =
                image::GrayImage::from_raw(dim.0, dim.1, ldr_img).context("Can't load gray img")?;
            Ok(DynamicImage::ImageLuma8(i))
        }
        tiff::ColorType::RGB(_) => {
            debug!("Loading rgb color");
            let i =
                image::RgbImage::from_raw(dim.0, dim.1, ldr_img).context("Can't load RGB img")?;
            Ok(DynamicImage::ImageRgb8(i))
        }
        tiff::ColorType::RGBA(_) => {
            debug!("Loading rgba color");
            let i =
                image::RgbaImage::from_raw(dim.0, dim.1, ldr_img).context("Can't load RGBA img")?;
            Ok(image::DynamicImage::ImageRgba8(i))
        }
        tiff::ColorType::GrayA(_) => {
            debug!("Loading gray color with alpha");
            let i = image::GrayAlphaImage::from_raw(dim.0, dim.1, ldr_img)
                .context("Can't load gray alpha img")?;
            Ok(image::DynamicImage::ImageLumaA8(i))
        }
        _ => {
            bail!(
                "Error: This TIFF image type is unsupported, please open a ticket! {:?}",
                decoder.colortype()
            )
        }
    }
}

/// Stretches the values from the lowest to the highest to 0 to 255. An image
/// with a single value has nothing to stretch, it is shown at its level in the
/// range of its sample type. It came out black, from a division by zero.
/// Stretch the values from the lowest to the highest over 8 bit. Straight from the samples
/// and in parallel: the two float copies this made took 576 MB for a 24 MP 16 bit TIFF.
fn autoscale<T: Copy + Send + Sync>(
    values: &[T],
    as_f32: impl Fn(T) -> f32 + Sync,
    type_range: (f32, f32),
) -> Vec<u8> {
    // comparisons, not f32::min, so NaN never wins, as before
    let lower = |a: f32, b: f32| if b < a { b } else { a };
    let higher = |a: f32, b: f32| if b > a { b } else { a };
    let (mut lowest, mut highest) = values
        .par_iter()
        .fold(
            || (f32::MAX, f32::MIN),
            |(low, high), v| (lower(low, as_f32(*v)), higher(high, as_f32(*v))),
        )
        .reduce(
            || (f32::MAX, f32::MIN),
            |a, b| (lower(a.0, b.0), higher(a.1, b.1)),
        );
    if highest <= lowest {
        (lowest, highest) = type_range;
    }
    values
        .par_iter()
        .map(|v| fit(as_f32(*v), lowest, highest, 0., 255.).clamp(0., 255.) as u8)
        .collect()
}

fn load_jxl(img_location: &Path, frame_sender: Sender<Frame>) -> Result<()> {
    let mut image = JxlImage::builder()
        .open(img_location)
        .map_err(|e| anyhow!("{e}"))?;
    //TODO: Disable when colormanagement support exists
    let colorencoding = jxl_oxide::EnumColourEncoding::srgb(jxl_oxide::RenderingIntent::Perceptual);
    image.request_color_encoding(colorencoding);

    debug!("{:#?}", image.image_header().metadata);
    let is_jxl_anim = image.image_header().metadata.animation.is_some();
    // Durations are counted in ticks. A tick can be shorter than a millisecond,
    // so this stays a fraction until a frame's duration is computed.
    let ms_per_tick = image
        .image_header()
        .metadata
        .animation
        .as_ref()
        .map(|hdr| 1000.0 * hdr.tps_denominator as f64 / hdr.tps_numerator as f64)
        .unwrap_or(40.0);
    debug!("ms per tick: {ms_per_tick}");

    for keyframe_idx in 0..image.num_loaded_keyframes() {
        // create a mutable image to hold potential decoding results. We can then use this only once at the end of the loop/
        let image_result: DynamicImage;
        let render = image
            .render_frame(keyframe_idx)
            // .render_next_frame()
            .map_err(|e| anyhow!("{e}"))
            .context("Can't render JXL")?;

        let frame_duration = (render.duration() as f64 * ms_per_tick).round() as u32;
        debug!("duration {frame_duration} ms");
        let framebuffer = render.image_all_channels();
        debug!("{:?}", image.pixel_format());
        match image.pixel_format() {
            PixelFormat::Graya => {
                let float_image = GrayAlphaImage::from_raw(
                    framebuffer.width() as u32,
                    framebuffer.height() as u32,
                    framebuffer
                        .buf()
                        .par_iter()
                        .map(|x| x * 255. + 0.5)
                        .map(|x| x as u8)
                        .collect::<Vec<_>>(),
                )
                .context("Can't decode gray alpha buffer")?;
                image_result = DynamicImage::ImageLumaA8(float_image);
            }
            PixelFormat::Gray => {
                let float_image = image::GrayImage::from_raw(
                    framebuffer.width() as u32,
                    framebuffer.height() as u32,
                    framebuffer
                        .buf()
                        .par_iter()
                        .map(|x| x * 255. + 0.5)
                        .map(|x| x as u8)
                        .collect::<Vec<_>>(),
                )
                .context("Can't decode gray buffer")?;
                image_result = DynamicImage::ImageLuma8(float_image);
            }
            PixelFormat::Rgba => {
                let float_image = RgbaImage::from_raw(
                    framebuffer.width() as u32,
                    framebuffer.height() as u32,
                    framebuffer
                        .buf()
                        .par_iter()
                        .map(|x| x * 255. + 0.5)
                        .map(|x| x as u8)
                        .collect::<Vec<_>>(),
                )
                .context("Can't decode rgba buffer")?;
                image_result = DynamicImage::ImageRgba8(float_image);
            }
            PixelFormat::Rgb => {
                let float_image = RgbImage::from_raw(
                    framebuffer.width() as u32,
                    framebuffer.height() as u32,
                    framebuffer
                        .buf()
                        .par_iter()
                        .map(|x| x * 255. + 0.5)
                        .map(|x| x as u8)
                        .collect::<Vec<_>>(),
                )
                .context("Can't decode rgb buffer")?;
                image_result = DynamicImage::ImageRgb8(float_image);
            }
            _ => {
                bail!("JXL: Pixel format: {:?}", image.pixel_format())
            }
        }

        // Dispatch to still or animation
        if is_jxl_anim {
            _ = frame_sender.send(Frame::new_animation(image_result, frame_duration));
        } else {
            _ = frame_sender.send(Frame::new_still(image_result));
        }
    }
    debug!("Done decoding JXL");
    if is_jxl_anim {
        // 0 loops is forever
        let loops = image
            .image_header()
            .metadata
            .animation
            .as_ref()
            .map_or(0, |animation| animation.num_loops);
        _ = frame_sender.send(Frame::AnimationEnd((loops > 0).then_some(loops)));
    }

    Ok(())
}

/// How often an animation is to be played, `None` for forever
fn plays(count: image::metadata::LoopCount) -> Option<u32> {
    match count {
        image::metadata::LoopCount::Infinite => None,
        image::metadata::LoopCount::Finite(n) => Some(n.get()),
    }
}

/// One name for the extensions of a format, so the extension of a file and the
/// format found in its content can be compared
fn canonical_extension(extension: &str) -> String {
    let extension = extension.to_lowercase();
    match extension.as_str() {
        "tiff" => "tif",
        "jpeg" | "jfif" => "jpg",
        "ima" => "dcm",
        "heic" | "hif" => "heif",
        "apng" => "png",
        // the Netpbm family, its content is named pam
        "pbm" | "pgm" | "ppm" | "pnm" | "pam" => "pnm",
        other => other,
    }
    .to_string()
}

/// The decoded frames of an animated PNG
struct Apng {
    /// Every frame with its delay in milliseconds
    frames: Vec<(DynamicImage, u32)>,
    /// How often it is to be played, `None` for forever
    plays: Option<u32>,
}

fn decode_apng<R: std::io::BufRead + std::io::Seek>(
    decoder: image::codecs::png::PngDecoder<R>,
) -> Result<Apng> {
    let apng = decoder.apng()?;
    let plays = plays(apng.loop_count());
    let frames = apng
        .into_frames()
        .map(|frame| {
            let frame = frame.context("Can't decode APNG frame")?;
            let (delay_numer, delay_denom) = frame.delay().numer_denom_ms();
            let delay_ms = delay_numer.checked_div(delay_denom).unwrap_or(0);
            Ok((DynamicImage::ImageRgba8(frame.into_buffer()), delay_ms))
        })
        .collect::<Result<_>>()?;
    Ok(Apng { frames, plays })
}

/// How HEIF images are decoded. heic-rs refuses images above 256 megapixels unless
/// told otherwise, 0.9.6 had a setting to raise that limit of libheif (#683). The
/// other formats are read without a limit, so HEIF is too.
#[cfg(feature = "heif_native")]
fn heif_options(layout: heic_rs::PixelLayout) -> heic_rs::DecodeOptions {
    heic_rs::DecodeOptions {
        layout,
        max_pixels: None,
        ..Default::default()
    }
}

/// Decode a HEIC or HEIF image with `heic-rs`, in the layout the file has:
/// with alpha if there is any, and 16 bit if it has more than 8.
///
/// Rotation and mirroring stored in the file are applied by the decoder.
#[cfg(feature = "heif_native")]
/// One of the images of a HEIF file, 0 is the primary one
pub fn load_heif_native(path: &Path, item: usize) -> Result<DynamicImage> {
    use heic_rs::PixelLayout;

    let bytes = std::fs::read(path)?;
    let id = *heic_rs::images(&bytes)
        .context("Can't read HEIF file")?
        .get(item)
        .with_context(|| format!("HEIF file has no image {item}"))?;
    let info = heic_rs::probe_image(&bytes, id).context("Can't read HEIF file")?;
    let layout = match (info.bit_depth > 8, info.has_alpha) {
        (false, false) => PixelLayout::Rgb8,
        (false, true) => PixelLayout::Rgba8,
        (true, false) => PixelLayout::Rgb16,
        (true, true) => PixelLayout::Rgba16,
    };
    let img = heic_rs::decode_image(&bytes, id, &heif_options(layout))
        .context("Can't decode HEIF image")?;
    let (w, h) = (img.width, img.height);
    // 16 bit samples come as pairs of bytes in the byte order of the machine
    let wide = || -> Vec<u16> {
        img.data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_ne_bytes([b[0], b[1]]))
            .collect()
    };
    let decoded = match layout {
        PixelLayout::Rgb8 => image::RgbImage::from_raw(w, h, img.data).map(DynamicImage::ImageRgb8),
        PixelLayout::Rgba8 => {
            image::RgbaImage::from_raw(w, h, img.data).map(DynamicImage::ImageRgba8)
        }
        PixelLayout::Rgb16 => {
            image::ImageBuffer::from_raw(w, h, wide()).map(DynamicImage::ImageRgb16)
        }
        _ => image::ImageBuffer::from_raw(w, h, wide()).map(DynamicImage::ImageRgba16),
    };
    decoded.context("HEIF decoder returned a buffer of the wrong size")
}

pub fn rotate_dynimage(di: &mut DynamicImage, path: &Path) -> Result<()> {
    reorient(di, orientation_of(path)?);
    Ok(())
}

/// The EXIF orientation of a file
fn orientation_of(path: &Path) -> Result<image::metadata::Orientation> {
    use image::metadata::Orientation;
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    if reader.format() == Some(image::ImageFormat::Jpeg) {
        // The decoder reads the whole file for this, the EXIF reader only up to
        // the EXIF block
        let mut file = BufReader::new(File::open(path)?);
        let orientation = exif::Reader::new()
            .read_from_container(&mut file)
            .ok()
            .and_then(|exif| {
                exif.get_field(exif::Tag::Orientation, exif::In::PRIMARY)?
                    .value
                    .get_uint(0)
            })
            .and_then(|value| Orientation::from_exif(value as u8));
        return Ok(orientation.unwrap_or(Orientation::NoTransforms));
    }
    let mut reader = reader;
    reader.no_limits();
    Ok(reader.into_decoder()?.orientation()?)
}

/// Decodes with the image library, which by default refuses images that need
/// more than 512 MiB, such as a photo of 200 megapixels. The user opened the
/// image, and large images are what Oculante is for.
fn decode_without_limits<R: std::io::BufRead + std::io::Seek>(
    mut reader: ImageReader<R>,
) -> Result<DynamicImage> {
    reader.no_limits();
    Ok(reader.decode()?)
}

/// Like `DynamicImage::apply_orientation`. The orientations that swap width
/// and height make a new image, its rows are filled in parallel.
pub fn reorient(image: &mut DynamicImage, orientation: image::metadata::Orientation) {
    use image::metadata::Orientation::*;
    if matches!(
        orientation,
        Rotate90 | Rotate270 | Rotate90FlipH | Rotate270FlipH
    ) {
        macro_rules! reoriented {
            ($($variant:ident),*) => {
                match image {
                    $(DynamicImage::$variant(buffer) => {
                        *image = DynamicImage::$variant(reoriented(buffer, orientation))
                    })*
                    _ => image.apply_orientation(orientation),
                }
            };
        }
        reoriented!(
            ImageLuma8,
            ImageLumaA8,
            ImageRgb8,
            ImageRgba8,
            ImageLuma16,
            ImageLumaA16,
            ImageRgb16,
            ImageRgba16,
            ImageRgb32F,
            ImageRgba32F
        );
    } else {
        // flips and half turns are done in place
        image.apply_orientation(orientation);
    }
}

/// A copy of the image turned by a quarter, with or without a flip
fn reoriented<P>(
    source: &image::ImageBuffer<P, Vec<P::Subpixel>>,
    orientation: image::metadata::Orientation,
) -> image::ImageBuffer<P, Vec<P::Subpixel>>
where
    P: image::Pixel + Send + Sync,
    P::Subpixel: Send + Sync,
{
    use image::metadata::Orientation::*;
    use rayon::prelude::*;
    let (width, height) = (source.width() as usize, source.height() as usize);
    let channels = P::CHANNEL_COUNT as usize;
    let samples = source.as_raw();
    // the new image is as wide as the old one is high
    let mut target = image::ImageBuffer::<P, Vec<P::Subpixel>>::new(height as u32, width as u32);
    target
        .par_chunks_mut(height * channels)
        .enumerate()
        .for_each(|(y, row)| {
            for x in 0..height {
                // where the pixel at x, y of the new image comes from
                let (from_x, from_y) = match orientation {
                    Rotate90 => (y, height - 1 - x),
                    Rotate270 => (width - 1 - y, x),
                    Rotate90FlipH => (y, x),
                    _ => (width - 1 - y, height - 1 - x),
                };
                let from = (from_y * width + from_x) * channels;
                row[x * channels..(x + 1) * channels]
                    .copy_from_slice(&samples[from..from + channels]);
            }
        });
    target
}

/// Krita and OpenRaster files are zips with a copy of all layers in one picture
fn load_merged_image(path: &Path) -> Result<DynamicImage> {
    let f = File::open(path)?;
    let mut archive = zip::ZipArchive::new(f)?;
    // https://docs.krita.org/en/general_concepts/file_formats/file_kra.html
    // https://www.openraster.org/baseline/file-layout-spec.html
    let mut merged_image = archive.by_name("mergedimage.png")?;
    let mut image_bytes = Vec::<u8>::new();
    merged_image.read_to_end(&mut image_bytes)?;
    decode_without_limits(
        ImageReader::new(std::io::Cursor::new(image_bytes)).with_guessed_format()?,
    )
}

#[cfg(feature = "turbo")]
fn load_jpeg_turbojpeg(img_location: &Path) -> Result<DynamicImage> {
    debug!("Loading jpeg using turbojpeg");
    let jpeg_data = std::fs::read(img_location)?;
    let mut decompressor = turbojpeg::Decompressor::new()?;
    let header = decompressor.read_header(&jpeg_data)?;
    let (width, height) = (header.width, header.height);
    let mut image = turbojpeg::Image {
        pixels: vec![0; 3 * width * height],
        width,
        pitch: 3 * width, // we use no padding between rows
        height,
        format: turbojpeg::PixelFormat::RGB,
    };
    decompressor.decompress(&jpeg_data, image.as_deref_mut())?;
    let i = RgbImage::from_raw(width as u32, height as u32, image.pixels)
        .context("Can't load RgbImage from decompressed TurboJPEG")?;
    Ok(DynamicImage::ImageRgb8(i))
}

#[cfg(test)]
mod tests {
    /// The tiled photo of M31 in #683 has 416 megapixels, heic-rs stops at 256 by default
    #[cfg(feature = "heif_native")]
    #[test]
    fn heif_has_no_pixel_limit() {
        let options = super::heif_options(heic_rs::PixelLayout::Rgb8);
        assert_eq!(options.max_pixels, None);
        const { assert!(heic_rs::DEFAULT_MAX_PIXELS < 42208 * 9870) };
    }

    #[test]
    fn ci_autoscale_stretches_to_8_bit() {
        use super::autoscale;
        // the lowest becomes 0, the highest 255
        let a = autoscale(&[1000u16, 2000, 3000], |p| p as f32, (0., 65535.));
        assert_eq!(a, [0, 127, 255]);
        // NaN does not count for the range and comes out as 0
        assert_eq!(
            autoscale(&[f32::NAN, 0.25, 0.75], |p| p, (0., 1.)),
            [0, 0, 255]
        );
        // all the same: the range of the type
        assert_eq!(autoscale(&[-1i8; 3], |p| p as f32, (-128., 127.)), [127; 3]);
        // enough values to be split over threads, the lowest at the end
        let many: Vec<u32> = (1000..201_000).rev().collect();
        let a = autoscale(&many, |p| p as f32, (0., u32::MAX as f32));
        assert_eq!((a[0], a[a.len() - 1]), (255, 0));
    }

    #[test]
    fn ci_gamma_table_matches_powf() {
        use super::{gamma_table, tonemap_f32, tonemap_f32_powf};
        let same = |bits: u32| {
            let f = f32::from_bits(bits);
            assert_eq!(tonemap_f32(f), tonemap_f32_powf(f), "{f:?} ({bits:#x})");
        };
        // a spread over all floats, both signs, NaN and infinity
        for bits in (0..=u32::MAX).step_by(4099) {
            if bits != f32::NEG_INFINITY.to_bits() {
                same(bits);
            }
        }
        for bits in [
            0,
            1,
            f32::INFINITY.to_bits(),
            f32::NAN.to_bits(),
            1f32.to_bits(),
        ] {
            same(bits);
        }
        // every float close to a step
        for step in gamma_table().steps {
            let bits = step.to_bits();
            (bits.saturating_sub(256)..bits + 256).for_each(same);
        }
        assert_eq!(tonemap_f32(f32::NEG_INFINITY), 0);
    }

    /// The strips of a TIFF file are decoded in parallel. The pixels are the
    /// same as when they are decoded one after the other, for every compression,
    /// several layouts, and a last strip that is shorter than the others.
    #[test]
    fn ci_tiff_strips_in_parallel() {
        use super::{read_tiff_image, tiff_decoder};
        use tiff::encoder::{Compression, TiffEncoder, colortype, compression::DeflateLevel};
        use tiff::tags::Predictor;

        let (w, h) = (101u32, 67u32);
        let n = (w * h) as usize;
        let dir = std::env::temp_dir().join("oculante_test_tiff_strips");
        _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rgb8: Vec<u8> = (0..n * 3).map(|i| (i * 7 % 251) as u8).collect();
        let rgba8: Vec<u8> = (0..n * 4).map(|i| (i * 11 % 241) as u8).collect();
        let rgb16: Vec<u16> = (0..n * 3).map(|i| (i * 263 % 65521) as u16).collect();
        let gray16: Vec<u16> = (0..n).map(|i| (i * 97 % 65521) as u16).collect();
        let float: Vec<f32> = (0..n).map(|i| i as f32 / 100.0).collect();

        // name, rows per strip, the pixels as bytes
        let mut written = vec![];
        macro_rules! write {
            ($name:expr, $color:ty, $data:expr, $compression:expr, $predictor:expr, $rows:expr) => {{
                let path = dir.join($name);
                let file = std::fs::File::create(&path).unwrap();
                let mut encoder = TiffEncoder::new(file)
                    .unwrap()
                    .with_compression($compression)
                    .with_predictor($predictor);
                let mut image = encoder.new_image::<$color>(w, h).unwrap();
                image.rows_per_strip($rows).unwrap();
                image.write_data(&$data).unwrap();
                let bytes: Vec<u8> = $data.iter().flat_map(|v| v.to_ne_bytes()).collect();
                written.push((path, $rows, bytes));
            }};
        }
        write!(
            "lzw.tif",
            colortype::RGB8,
            rgb8,
            Compression::Lzw,
            Predictor::Horizontal,
            8
        );
        write!(
            "one_row.tif",
            colortype::RGBA8,
            rgba8,
            Compression::Uncompressed,
            Predictor::None,
            1
        );
        let deflate = Compression::Deflate(DeflateLevel::Balanced);
        write!(
            "deflate.tif",
            colortype::Gray16,
            gray16,
            deflate,
            Predictor::Horizontal,
            5
        );
        write!(
            "float.tif",
            colortype::Gray32Float,
            float,
            Compression::Lzw,
            Predictor::None,
            10
        );
        write!(
            "packbits.tif",
            colortype::RGB16,
            rgb16,
            Compression::Packbits,
            Predictor::None,
            20
        );
        write!(
            "one_strip.tif",
            colortype::RGB8,
            rgb8,
            Compression::Lzw,
            Predictor::None,
            h
        );

        for (path, rows, bytes) in written {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let mut decoder = tiff_decoder(&path, 0).unwrap();
            assert_eq!(decoder.strip_count().unwrap(), h.div_ceil(rows), "{name}");
            let mut serial = tiff_decoder(&path, 0).unwrap().read_image().unwrap();
            assert_eq!(
                serial.as_buffer(0).as_bytes(),
                &bytes[..],
                "{name} as written"
            );
            let mut parallel = read_tiff_image(&mut decoder, &path, 0).unwrap();
            assert!(
                parallel.as_buffer(0).as_bytes() == serial.as_buffer(0).as_bytes(),
                "{name}: the strips decoded in parallel differ"
            );
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A planar TIFF stores each color after the other. Only the first was
    /// read, and loading failed. The file is written by hand, the encoder
    /// writes no planar files.
    #[test]
    fn ci_planar_tiff() {
        use super::{load_tiff, read_tiff_image, tiff_decoder};
        let (w, h) = (7u32, 5u32);
        let n = (w * h) as usize;
        let dir = std::env::temp_dir().join("oculante_test_planar_tiff");
        _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for bits in [8u16, 16] {
            let value = |pixel: usize, plane: usize| ((pixel * 31 + plane * 80) % 256) as u16;
            let sample = |v: u16| {
                if bits == 8 {
                    vec![v as u8]
                } else {
                    (v * 257).to_le_bytes().to_vec()
                }
            };
            let planes: Vec<Vec<u8>> = (0..3)
                .map(|plane| {
                    (0..n)
                        .flat_map(|pixel| sample(value(pixel, plane)))
                        .collect()
                })
                .collect();
            let plane_len = planes[0].len() as u32;
            // header, 10 entries, bits per sample, strip offsets, strip byte counts
            let data_start = 8 + 2 + 10 * 12 + 4 + 6 + 12 + 12;
            let mut tiff = b"II\x2a\x00\x08\x00\x00\x00".to_vec();
            tiff.extend(10u16.to_le_bytes());
            let mut entry = |tag: u16, kind: u16, count: u32, value: u32| {
                tiff.extend(tag.to_le_bytes());
                tiff.extend(kind.to_le_bytes());
                tiff.extend(count.to_le_bytes());
                tiff.extend(value.to_le_bytes());
            };
            let (short, long) = (3, 4);
            entry(256, long, 1, w);
            entry(257, long, 1, h);
            entry(258, short, 3, 134);
            entry(259, short, 1, 1); // uncompressed
            entry(262, short, 1, 2); // RGB
            entry(273, long, 3, 140);
            entry(277, short, 1, 3);
            entry(278, long, 1, h);
            entry(279, long, 3, 152);
            entry(284, short, 1, 2); // planar
            tiff.extend(0u32.to_le_bytes());
            for _ in 0..3 {
                tiff.extend(bits.to_le_bytes());
            }
            for plane in 0..3 {
                tiff.extend((data_start + plane * plane_len).to_le_bytes());
            }
            for _ in 0..3 {
                tiff.extend(plane_len.to_le_bytes());
            }
            assert_eq!(tiff.len() as u32, data_start);
            for plane in &planes {
                tiff.extend(plane);
            }
            let path = dir.join(format!("planar{bits}.tif"));
            std::fs::write(&path, tiff).unwrap();

            let expected: Vec<u8> = (0..n)
                .flat_map(|pixel| (0..3).map(move |plane| value(pixel, plane)))
                .flat_map(|v| {
                    if bits == 8 {
                        vec![v as u8]
                    } else {
                        (v * 257).to_ne_bytes().to_vec()
                    }
                })
                .collect();
            let mut result =
                read_tiff_image(&mut tiff_decoder(&path, 0).unwrap(), &path, 0).unwrap();
            assert!(
                result.as_buffer(0).as_bytes() == &expected[..],
                "{bits} bit"
            );
            if bits == 8 {
                let image = load_tiff(&path, 0).unwrap();
                assert!(image.as_bytes() == &expected[..], "8 bit, loaded");
            }
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
