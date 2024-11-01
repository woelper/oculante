use crate::ktx2_loader::CompressedImageFormats;
use crate::utils::{fit, Frame};
use crate::{appstate::Message, ktx2_loader, FONT};
use log::{debug, error, info};
use psd::Psd;

use anyhow::{anyhow, bail, Context, Result};
use dds::DDS;
use exr::prelude as exrs;
use exr::prelude::*;
use image::{
    DynamicImage, EncodableLayout, GrayAlphaImage, GrayImage, ImageDecoder, ImageReader,
    Rgb32FImage, RgbImage, RgbaImage,
};
use jxl_oxide::{JxlImage, PixelFormat};
use quickraw::{data, DemosaicingMethod, Export, Input, Output, OutputType};
use rayon::prelude::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rgb::*;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use tiff::decoder::Limits;
use webp_animation::prelude::*;
use zune_png::zune_core::options::DecoderOptions;
use zune_png::zune_core::result::DecodingResult;

/// Open an image from disk and send it somewhere
pub fn open_image(
    img_location: &Path,
    message_sender: Option<Sender<Message>>,
) -> Result<Receiver<Frame>> {
    let (sender, receiver): (Sender<Frame>, Receiver<Frame>) = channel();
    let img_location = (*img_location).to_owned();

    use file_format::FileFormat;

    let mut extension = img_location
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
        .to_lowercase()
        .replace("tiff", "tif")
        .replace("jpeg", "jpg");

    let unchecked_extensions = ["svg", "kra", "tga"];

    if let Ok(fmt) = FileFormat::from_file(&img_location) {
        debug!("Detected as {:?} {}", fmt.name(), fmt.extension());
        if fmt
            .extension()
            .replace("tiff", "tif")
            .replace("apng", "png")
            != extension
        {
            if unchecked_extensions.contains(&extension.as_str()) {
                info!("Extension {extension} skipped check.")
            } else {
                message_sender.map(|s| {
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
            if let Some(main_layer) = dds.layers.get(0) {
                let buf = main_layer.as_bytes();
                let buf =
                    image::ImageBuffer::from_raw(dds.header.width, dds.header.height, buf.into())
                        .context("Can't create DDS ImageBuffer with given res")?;
                let d = DynamicImage::ImageRgba8(buf);
                _ = sender.send(Frame::new_still(d));
                return Ok(receiver);
            }
        }
        "ktx2" => {
            // let file = File::open(img_location)?;
            let data = std::fs::read(img_location)?;

            let ktx = ktx2_loader::ktx2_buffer_to_image(
                data.as_bytes(),
                CompressedImageFormats::all(),
                true,
            )?;
            let d = ktx.try_into_dynamic()?;
            _ = sender.send(Frame::new_still(d));
            return Ok(receiver);
        }
        #[cfg(feature = "dav1d")]
        "avif" | "avifs" => {
            use std::io::Read;
            let mut file = File::open(img_location)?;
            let mut buf = vec![];
            file.read_to_end(&mut buf)?;
            let i = libavif_image::read(buf.as_slice())?;
            _ = sender.send(Frame::new_still(i));
            return Ok(receiver);

            // col.add_still(i.to_rgba8());
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
        #[cfg(feature = "heif")]
        "heif" | "heic" => {
            // Built on work in https://github.com/rsuu/rmg - thanks!
            use libheif_rs::{ColorSpace, HeifContext, LibHeif, RgbChroma};

            let lib_heif = LibHeif::new();
            let ctx = HeifContext::read_from_file(&img_location.to_string_lossy().to_string())?;
            let handle = ctx.primary_image_handle()?;
            let img = lib_heif.decode(&handle, ColorSpace::Rgb(RgbChroma::Rgba), None)?;
            let planes = img.planes();
            let interleaved = planes
                .interleaved
                .context("Can't create interleaved plane")?;

            let data = interleaved.data;
            let width = interleaved.width;
            let height = interleaved.height;
            let stride = interleaved.stride;

            let mut res: Vec<u8> = Vec::new();
            for y in 0..height {
                let mut step = y as usize * stride;

                for _ in 0..width {
                    res.extend_from_slice(&[
                        data[step],
                        data[step + 1],
                        data[step + 2],
                        data[step + 3],
                    ]);
                    step += 4;
                }
            }
            let buf = image::ImageBuffer::from_vec(width as u32, height as u32, res)
                .context("Can't create HEIC/HEIF ImageBuffer with given res")?;
            let i = DynamicImage::ImageRgba8(buf);

            _ = sender.send(Frame::new_still(i));
            return Ok(receiver);
        }
        #[cfg(feature = "avif_native")]
        #[cfg(not(feature = "dav1d"))]
        "avif" => {
            let mut file = File::open(img_location)?;
            let avif = avif_decode::Decoder::from_reader(&mut file)?.to_image()?;
            match avif {
                avif_decode::Image::Rgb8(img) => {
                    let mut img_buffer = vec![];
                    let (buf, width, height) = img.into_contiguous_buf();
                    for b in buf {
                        img_buffer.push(b.r);
                        img_buffer.push(b.g);
                        img_buffer.push(b.b);
                        img_buffer.push(255);
                    }
                    let buf = image::ImageBuffer::from_vec(width as u32, height as u32, img_buffer)
                        .context("Can't create avif ImageBuffer with given res")?;
                    let i = DynamicImage::ImageRgba8(buf);

                    _ = sender.send(Frame::new_still(i));
                    return Ok(receiver);
                }
                avif_decode::Image::Rgba8(img) => {
                    let mut img_buffer = vec![];
                    let (buf, width, height) = img.into_contiguous_buf();
                    for b in buf {
                        img_buffer.push(b.r);
                        img_buffer.push(b.g);
                        img_buffer.push(b.b);
                        img_buffer.push(b.a);
                    }
                    let buf = image::ImageBuffer::from_vec(width as u32, height as u32, img_buffer)
                        .context("Can't create avif ImageBuffer with given res")?;
                    let i = DynamicImage::ImageRgba8(buf);

                    _ = sender.send(Frame::new_still(i));
                    return Ok(receiver);
                }
                avif_decode::Image::Rgb16(img) => {
                    let mut img_buffer = vec![];
                    let (buf, width, height) = img.into_contiguous_buf();
                    for b in buf {
                        img_buffer.push(u16_to_u8(b.r));
                        img_buffer.push(u16_to_u8(b.g));
                        img_buffer.push(u16_to_u8(b.b));
                        img_buffer.push(255);
                    }
                    let buf = image::ImageBuffer::from_vec(width as u32, height as u32, img_buffer)
                        .context("Can't create avif ImageBuffer with given res")?;
                    let i = DynamicImage::ImageRgba8(buf);

                    _ = sender.send(Frame::new_still(i));
                    return Ok(receiver);
                }
                avif_decode::Image::Rgba16(_) => {
                    anyhow::bail!("This avif is not yet supported (Rgba16).")
                }
                avif_decode::Image::Gray8(_) => {
                    anyhow::bail!("This avif is not yet supported (Gray8).")
                }
                avif_decode::Image::Gray16(_) => {
                    anyhow::bail!("This avif is not yet supported (Gray16).")
                }
            }
        }
        "svg" => {
            // TODO: Should the svg be scaled? if so by what number?
            // This should be specified in a smarter way, maybe resolution * x?

            let render_scale = 2.;
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
                    .context("Can't get SVG size")?;

                if let Some(mut pixmap) =
                    tiny_skia::Pixmap::new(pixmap_size.width(), pixmap_size.height())
                {
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
                                let gray_image = GrayImage::from_raw(
                                    size.width() as u32,
                                    size.height() as u32,
                                    f.par_iter().map(|x| tonemap_f32(*x)).collect::<Vec<_>>(),
                                )
                                .context("Can't decode gray alpha buffer")?;

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
            let meta = hdr_decoder.metadata();

            #[cfg(feature = "hdr")]
            {
                let d = DynamicImage::from_decoder(hdr_decoder)?;
                _ = sender.send(Frame::new_still(d));
                return Ok(receiver);
            }

            #[cfg(not(feature = "hdr"))]
            {
                let hdr_img: Rgb32FImage = match DynamicImage::from_decoder(hdr_decoder)? {
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
                let img = image::ImageReader::with_format(
                    std::io::Cursor::new(contents),
                    image::ImageFormat::WebP,
                )
                .decode()?;
                _ = sender.send(Frame::new_still(img));
                return Ok(receiver);
            }

            let buffer = std::fs::read(img_location)?;
            let mut decoder = Decoder::new(&buffer)?.into_iter();
            let mut last_timestamp = 0;

            loop {
                if let Some(frame) = decoder.next() {
                    let buf = image::ImageBuffer::from_raw(
                        frame.dimensions().0,
                        frame.dimensions().1,
                        frame.data().to_vec(),
                    )
                    .context("Can't create imagebuffer from webp")?;
                    let t = frame.timestamp();
                    let delay = t - last_timestamp;
                    debug!("time {t} {delay}");
                    last_timestamp = t;
                    let i = DynamicImage::ImageRgba8(buf);
                    let frame = Frame::new_animation(i, delay as u16);
                    _ = sender.send(frame);
                } else {
                    break;
                }
            }

            // TODO: Use thread for animation and return receiver immediately, but this needs error handling
            return Ok(receiver);
        }
        "png" | "apng" => {
            use zune_png::zune_core::bytestream::ZCursor;
            use zune_png::zune_core::options::EncoderOptions;
            use zune_png::PngDecoder;

            let contents = std::fs::read(&img_location)?;
            let mut decoder = PngDecoder::new(ZCursor::new(contents));
            decoder.set_options(
                DecoderOptions::new_fast()
                    .set_max_height(128000)
                    .set_max_width(128000),
            );

            //animation
            decoder.decode_headers()?;
            if decoder.is_animated() {
                info!("Image is animated");
                decoder.decode_headers()?;

                let colorspace = decoder.colorspace().context("Can't get color space")?;
                let depth = decoder.depth().context("Can't get decoder depth")?;
                //  get decoder information,we clone this because we need a standalone
                // info since we mutably modify decoder struct below
                let info = decoder.info().context("Can't get decoder info")?.clone();
                // set up our background variable. Soon it will contain the data for the previous
                // frame, the first frame has no background hence why this is None
                let mut background: Option<Vec<u8>> = None;
                // the output, since we know that no frame will be bigger than the width and height, we can
                // set this up outside of the loop.
                let mut output = vec![
                    0;
                    info.width
                        * info.height
                        * decoder
                            .colorspace()
                            .context("Can't get decoder color depth")?
                            .num_components()
                ];

                while decoder.more_frames() {
                    // decode the header, in case we haven't processed a frame header
                    decoder.decode_headers()?;
                    // then decode the current frame information,
                    // NB: Frame information is for current frame hence should be accessed before decoding the frame
                    // as it will change on subsequent frames
                    let frame = decoder.frame_info().context("Can't get frame info")?;
                    debug!("Frame: {:?}", frame);

                    // decode the raw pixels, even on smaller frames, we only allocate frame_info.width*frame_info.height
                    let pix = decoder.decode_raw()?;
                    // call post process
                    zune_png::post_process_image(
                        &info,
                        colorspace,
                        &frame,
                        &pix,
                        background.as_ref().map(|x| x.as_slice()),
                        &mut output,
                        None,
                    )?;
                    // create encoder parameters
                    let encoder_opts =
                        EncoderOptions::new(info.width, info.height, colorspace, depth);

                    let mut out = vec![];
                    _ = zune_png::PngEncoder::new(&output, encoder_opts).encode(&mut out);
                    let img = image::load_from_memory(&out)?;

                    let delay = frame.delay_num as f32 / frame.delay_denom as f32 * 1000.;

                    _ = sender.send(Frame::new_animation(img, delay as u16));
                    background = Some(output.clone());
                }
                return Ok(receiver);
            }

            debug!("Image is not animated");
            match decoder.decode().map_err(|e| anyhow!("{:?}", e))? {
                // 16 bpp data
                DecodingResult::U16(imgdata) => {
                    //convert to 8bpp
                    let imgdata_8bpp = imgdata
                        .par_iter()
                        .map(|x| *x as f32 / u16::MAX as f32)
                        .map(|p| p.powf(2.2))
                        .map(|p| tonemap_f32(p))
                        // .map(|x| x as u8)
                        .collect::<Vec<_>>();

                    let (width, height) =
                        decoder.dimensions().context("Can't get png dimensions")?;
                    let colorspace = decoder.colorspace().context("Can't get colorspace")?;

                    if colorspace.is_grayscale() {
                        let buf: GrayImage =
                            image::ImageBuffer::from_raw(width as u32, height as u32, imgdata_8bpp)
                                .context("Can't interpret image as grayscale")?;
                        let image_result = DynamicImage::ImageLuma8(buf);
                        _ = sender.send(Frame::new_still(image_result));
                        return Ok(receiver);
                    }

                    if colorspace.has_alpha() {
                        let float_image =
                            RgbaImage::from_raw(width as u32, height as u32, imgdata_8bpp)
                                .context("Can't decode rgba buffer")?;
                        _ = sender.send(Frame::new_still(DynamicImage::ImageRgba8(float_image)));
                        return Ok(receiver);
                    } else {
                        let float_image =
                            RgbImage::from_raw(width as u32, height as u32, imgdata_8bpp)
                                .context("Can't decode rgba buffer")?;
                        _ = sender.send(Frame::new_still(DynamicImage::ImageRgb8(float_image)));
                        return Ok(receiver);
                    }
                }
                // 8bpp
                DecodingResult::U8(value) => {
                    let (width, height) =
                        decoder.dimensions().context("Can't get png dimensions")?;

                    let colorspace = decoder.colorspace().context("Can't get colorspace")?;
                    if colorspace.is_grayscale() && !colorspace.has_alpha() {
                        let buf: GrayImage =
                            image::ImageBuffer::from_raw(width as u32, height as u32, value)
                                .context("Can't interpret image as grayscale")?;
                        let image_result = DynamicImage::ImageLuma8(buf);
                        _ = sender.send(Frame::new_still(image_result));
                        return Ok(receiver);
                    }

                    if colorspace.is_grayscale() && colorspace.has_alpha() {
                        let buf: GrayAlphaImage =
                            image::ImageBuffer::from_raw(width as u32, height as u32, value)
                                .context("Can't interpret image as grayscale")?;
                        let image_result = DynamicImage::ImageLumaA8(buf);
                        _ = sender.send(Frame::new_still(image_result));
                        return Ok(receiver);
                    }

                    if colorspace.has_alpha() && !colorspace.is_grayscale() {
                        let buf: RgbaImage =
                            image::ImageBuffer::from_raw(width as u32, height as u32, value)
                                .context("Can't interpret image as rgba")?;
                        let i = DynamicImage::ImageRgba8(buf);

                        _ = sender.send(Frame::new_still(i));
                        return Ok(receiver);
                    } else {
                        let buf: RgbImage =
                            image::ImageBuffer::from_raw(width as u32, height as u32, value)
                                .context("Can't interpret image as rgb")?;
                        let image_result = DynamicImage::ImageRgb8(buf);
                        _ = sender.send(Frame::new_still(image_result));
                        return Ok(receiver);
                    }
                }
                _ => {}
            }
        }
        "gif" => {
            let file = File::open(img_location)?;

            // Below is a workaround for partially corrupt gifs.
            let mut gif_opts = gif::DecodeOptions::new();
            gif_opts.set_color_output(gif::ColorOutput::Indexed);
            let mut decoder = gif_opts.read_info(file)?;
            let dim = (decoder.width() as u32, decoder.height() as u32);
            let mut screen = gif_dispose::Screen::new_decoder(&decoder);
            loop {
                if let Ok(i) = decoder.read_next_frame() {
                    debug!("decoded frame");
                    if let Some(frame) = i {
                        screen.blit_frame(&frame)?;
                        let buf: Option<image::RgbaImage> = image::ImageBuffer::from_raw(
                            dim.0,
                            dim.1,
                            screen.pixels_rgba().buf().as_bytes().to_vec(),
                        );
                        let buf = buf.context("Can't read gif frame")?;
                        let i = DynamicImage::ImageRgba8(buf);
                        _ = sender.send(Frame::new_animation(
                            i,
                            frame.delay * 10,
                        ));
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            debug!("Done decoding Gif!");

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
                Err(e) => {
                    error!(
                        "Could not load using turbojpeg: {e}. Trying to load with image library."
                    );
                    let img = image::open(img_location)?;
                    _ = sender.send(Frame::new_still(img));
                }
            }
            return Ok(receiver);
        }
        "kra" => {
            let i = load_kra(&img_location)?;
            _ = sender.send(Frame::new_still(i));
            return Ok(receiver);
        }
        "icns" => {
            let file = BufReader::new(File::open(img_location)?);
            let icon_family = icns::IconFamily::read(file)?;

            // loop over the largest icons, take the largest one and return
            for icon_type in [
                icns::IconType::RGBA32_512x512_2x,
                icns::IconType::RGBA32_512x512,
                icns::IconType::RGBA32_256x256,
                icns::IconType::RGBA32_128x128,
            ] {
                let mut target = vec![];
                let image = icon_family.get_icon_with_type(icon_type)?;
                image.write_png(&mut target)?;
                let d = image::load_from_memory(&target).context("Load icns mem")?;
                _ = sender.send(Frame::new_still(d));
                return Ok(receiver);
            }
        }
        "tif" | "tiff" => match load_tiff(&img_location) {
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
                    bail!("Could not load tiff: {tiff_error}, tried as raw and still got error: {raw_error}")
                }
            },
        },
        _ => {
            // All other supported image files are handled by using `image`
            debug!("Loading using generic image library");
            let img = image::open(img_location)?;
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

pub fn tonemap_f32(px: f32) -> u8 {
    (px.powf(1.0 / 2.2).max(0.0).min(1.0) * 255.0) as u8
    // (px.filmic() * 255.) as u8
}

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
    let export_job = Export::new(
        Input::ByFile(&img_location.to_string_lossy()),
        Output::new(
            DemosaicingMethod::SuperPixel,
            data::XYZ2SRGB,
            data::GAMMA_SRGB,
            OutputType::Raw16,
            true,
            true,
        ),
    )?;

    let (image, width, height) = export_job.export_16bit_image();
    let image = image
        .into_par_iter()
        .map(|x| tonemap_f32(x as f32 / 65536.))
        .collect::<Vec<_>>();

    // Construct rgb image
    let x = RgbImage::from_raw(width as u32, height as u32, image)
        .context("can't decode raw output as image")?;
    // make it a Dynamic image
    Ok(DynamicImage::ImageRgb8(x).to_rgba8())
}

fn load_tiff(img_location: &Path) -> Result<DynamicImage> {
    // TODO: Probe if dng
    let data = File::open(img_location)?;

    let mut decoder = tiff::decoder::Decoder::new(&data)?.with_limits(Limits::unlimited());
    let dim = decoder.dimensions()?;
    debug!("Color type: {:?}", decoder.colortype());
    let result = decoder.read_image()?;
    // A container for the low dynamic range image
    let ldr_img: Vec<u8>;

    match result {
        tiff::decoder::DecodingResult::U8(contents) => {
            debug!("TIFF U8");
            ldr_img = contents;
        }
        tiff::decoder::DecodingResult::U16(contents) => {
            debug!("TIFF U16");
            let values = contents.par_iter().map(|p| *p as f32).collect::<Vec<_>>();
            ldr_img = autoscale(&values).par_iter().map(|x| *x as u8).collect();
        }
        tiff::decoder::DecodingResult::U32(contents) => {
            debug!("TIFF U32");
            let values = contents.par_iter().map(|p| *p as f32).collect::<Vec<_>>();
            ldr_img = autoscale(&values).par_iter().map(|x| *x as u8).collect();
        }
        tiff::decoder::DecodingResult::U64(contents) => {
            debug!("TIFF U64");
            let values = contents.par_iter().map(|p| *p as f32).collect::<Vec<_>>();
            ldr_img = autoscale(&values).par_iter().map(|x| *x as u8).collect();
        }
        tiff::decoder::DecodingResult::F32(contents) => {
            debug!("TIFF F32");
            ldr_img = autoscale(&contents).par_iter().map(|x| *x as u8).collect();
        }
        tiff::decoder::DecodingResult::F64(contents) => {
            debug!("TIFF F64");
            let values = contents.par_iter().map(|p| *p as f32).collect::<Vec<_>>();
            ldr_img = autoscale(&values).par_iter().map(|x| *x as u8).collect();
        }
        tiff::decoder::DecodingResult::I8(contents) => {
            debug!("TIFF I8");
            let values = contents.par_iter().map(|p| *p as f32).collect::<Vec<_>>();
            ldr_img = autoscale(&values).par_iter().map(|x| *x as u8).collect();
        }
        tiff::decoder::DecodingResult::I16(contents) => {
            debug!("TIFF I16");
            let values = contents.par_iter().map(|p| *p as f32).collect::<Vec<_>>();
            ldr_img = autoscale(&values).par_iter().map(|x| *x as u8).collect();
        }
        tiff::decoder::DecodingResult::I32(contents) => {
            debug!("TIFF I32");
            let values = contents.par_iter().map(|p| *p as f32).collect::<Vec<_>>();
            ldr_img = autoscale(&values).par_iter().map(|x| *x as u8).collect();
        }
        tiff::decoder::DecodingResult::I64(contents) => {
            debug!("TIFF I64");
            let values = contents.par_iter().map(|p| *p as f32).collect::<Vec<_>>();
            ldr_img = autoscale(&values).par_iter().map(|x| *x as u8).collect();
        }
    }

    match decoder.colortype()? {
        tiff::ColorType::Gray(_) => {
            debug!("Loading gray color");
            let i =
                image::GrayImage::from_raw(dim.0, dim.1, ldr_img).context("Can't load gray img")?;
            return Ok(DynamicImage::ImageLuma8(i));
        }
        tiff::ColorType::RGB(_) => {
            debug!("Loading rgb color");
            let i =
                image::RgbImage::from_raw(dim.0, dim.1, ldr_img).context("Can't load RGB img")?;
            return Ok(DynamicImage::ImageRgb8(i));
        }
        tiff::ColorType::RGBA(_) => {
            debug!("Loading rgba color");
            let i =
                image::RgbaImage::from_raw(dim.0, dim.1, ldr_img).context("Can't load RGBA img")?;
            return Ok(image::DynamicImage::ImageRgba8(i));
        }
        tiff::ColorType::GrayA(_) => {
            debug!("Loading gray color with alpha");
            let i = image::GrayAlphaImage::from_raw(dim.0, dim.1, ldr_img)
                .context("Can't load gray alpha img")?;
            return Ok(image::DynamicImage::ImageLumaA8(i));
        }
        _ => {
            bail!(
                "Error: This TIFF image type is unsupported, please open a ticket! {:?}",
                decoder.colortype()
            )
        }
    }
}

fn autoscale(values: &Vec<f32>) -> Vec<f32> {
    let mut lowest = f32::MAX;
    let mut highest = f32::MIN;

    for v in values {
        if *v < lowest {
            lowest = *v
        }
        if *v > highest {
            highest = *v
        }
    }

    values
        .into_iter()
        .map(|v| fit(*v, lowest, highest, 0., 255.))
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
    let ticks_ms = image
        .image_header()
        .metadata
        .animation
        .as_ref()
        .map(|hdr| hdr.tps_numerator as f32 / hdr.tps_denominator as f32)
        // map this into milliseconds
        .map(|x| 1000. / x)
        .map(|x| x as u16)
        .unwrap_or(40);
    debug!("TPS: {ticks_ms}");

    for keyframe_idx in 0..image.num_loaded_keyframes() {
        // create a mutable image to hold potential decoding results. We can then use this only once at the end of the loop/
        let image_result: DynamicImage;
        let render = image
            .render_frame(keyframe_idx)
            // .render_next_frame()
            .map_err(|e| anyhow!("{e}"))
            .context("Can't render JXL")?;

        let frame_duration = render.duration() as u16 * ticks_ms;
        debug!("duration {frame_duration} ms");
        let framebuffer = render.image();
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
            _ = frame_sender.send(Frame::new_animation(
                image_result,
                frame_duration
            ));
        } else {
            _ = frame_sender.send(Frame::new_still(image_result));
        }
    }
    debug!("Done decoding JXL");

    Ok(())
}

pub fn rotate_dynimage(di: &mut DynamicImage, path: &Path) -> Result<()> {
    let mut decoder = ImageReader::open(path)?.into_decoder()?;
    di.apply_orientation(decoder.orientation()?);
    Ok(())
}

#[allow(unused)]
pub fn rotate_rgbaimage(di: &RgbaImage, path: &Path) -> Result<RgbaImage> {
    let mut decoder = ImageReader::open(path)?.into_decoder()?;
    let orientation = decoder.orientation()?;
    if orientation != image::metadata::Orientation::NoTransforms {
        let mut dynimage = DynamicImage::ImageRgba8(di.clone());
        dynimage.apply_orientation(orientation);
        Ok(dynimage.to_rgba8())
    } else {
        bail!("This image needs no rotation.")
    }
}

fn load_kra(path: &Path) -> Result<DynamicImage> {
    let f = File::open(path)?;
    let mut archive = zip::ZipArchive::new(f)?;
    // https://docs.krita.org/en/general_concepts/file_formats/file_kra.html
    let mut merged_image = archive.by_name("mergedimage.png")?;
    let mut image_bytes = Vec::<u8>::new();
    merged_image.read_to_end(&mut image_bytes)?;
    Ok(image::load_from_memory(&image_bytes)?)
}

#[cfg(feature = "turbo")]
fn load_jpeg_turbojpeg(img_location: &Path) -> Result<DynamicImage> {
    debug!("Loading jpeg using turbojpeg");
    let jpeg_data = std::fs::read(&img_location)?;
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
