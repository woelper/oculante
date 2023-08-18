use crate::utils::{fit, Frame, FrameSource};
use crate::FONT;
use libwebp_sys::{WebPDecodeRGBA, WebPGetInfo};
use log::{debug, error, info};
use psd::Psd;

use anyhow::{anyhow, bail, Context, Result};
use dds::DDS;
use exr::prelude as exrs;
use exr::prelude::*;
use image::{DynamicImage, GrayAlphaImage, GrayImage, RgbImage, RgbaImage};
use jxl_oxide::{JxlImage, PixelFormat, RenderResult};
use quickraw::{data, DemosaicingMethod, Export, Input, Output, OutputType};
use rayon::prelude::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rgb::*;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};
use tiff::decoder::Limits;
use usvg::{TreeParsing, TreeTextToPath};
use zune_png::zune_core::options::DecoderOptions;
use zune_png::zune_core::result::DecodingResult;
use zune_png::PngDecoder;

/// Open an image from disk and send it somewhere
pub fn open_image(img_location: &Path) -> Result<Receiver<Frame>> {
    let (sender, receiver): (Sender<Frame>, Receiver<Frame>) = channel();
    let img_location = (*img_location).to_owned();

    match img_location
        .extension()
        .unwrap_or_default()
        .to_str()
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "dds" => {
            let file = File::open(img_location)?;
            let mut reader = BufReader::new(file);
            let dds = DDS::decode(&mut reader).map_err(|e| anyhow!("{:?}", e))?;
            if let Some(main_layer) = dds.layers.get(0) {
                let buf = main_layer.as_bytes();
                let buf =
                    image::ImageBuffer::from_raw(dds.header.width, dds.header.height, buf.into())
                        .context("Can't create DDS ImageBuffer with given res")?;
                _ = sender.send(Frame::new_still(buf));
                return Ok(receiver);
            }
        }
        #[cfg(feature = "dav1d")]
        "avif" => {
            let mut file = File::open(img_location)?;
            let mut buf = vec![];
            file.read_to_end(&mut buf)?;
            let i = libavif_image::read(buf.as_slice())?;
            _ = sender.send(Frame::new_still(i.to_rgba8()));
            return Ok(receiver);

            // col.add_still(i.to_rgba8());
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
                    _ = sender.send(Frame::new_still(buf));
                    return Ok(receiver);

                    // col.add_still(buf);
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
                    _ = sender.send(Frame::new_still(buf));
                    return Ok(receiver);

                    // col.add_still(buf);
                }
                _ => {
                    anyhow::bail!("This avif is not yet supported.")
                }
            }
        }
        "svg" => {
            // TODO: Should the svg be scaled? if so by what number?
            // This should be specified in a smarter way, maybe resolution * x?

            let render_scale = 2.;
            let mut opt = usvg::Options::default();
            opt.font_family = "Inter".into();
            opt.font_size = 6.;

            let svg_data = std::fs::read(img_location)?;
            if let Ok(mut tree) = usvg::Tree::from_data(&svg_data, &opt) {
                let pixmap_size = resvg::IntSize::from_usvg(tree.size);

                let scaled_size = (
                    (pixmap_size.width() as f32 * render_scale) as u32,
                    (pixmap_size.height() as f32 * render_scale) as u32,
                );

                if let Some(mut pixmap) = tiny_skia::Pixmap::new(scaled_size.0, scaled_size.1) {
                    let mut fontdb = usvg::fontdb::Database::new();
                    fontdb.load_system_fonts();
                    fontdb.load_font_data(FONT.to_vec());
                    // for f in fontdb.faces() {
                    //     info!("{:?}",f.post_script_name);
                    // }
                    fontdb.set_cursive_family("Inter");
                    fontdb.set_sans_serif_family("Inter");
                    fontdb.set_serif_family("Inter");
                    tree.convert_text(&fontdb);

                    let rtree = resvg::Tree::from_usvg(&tree);

                    rtree.render(
                        tiny_skia::Transform::from_scale(render_scale, render_scale),
                        &mut pixmap.as_mut(),
                    );
                    let buf: RgbaImage = image::ImageBuffer::from_raw(
                        scaled_size.0,
                        scaled_size.1,
                        pixmap.data().to_vec(),
                    )
                    .context("Can't create image buffer from SVG render")?;

                    _ = sender.send(Frame::new_still(buf));
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
                    _ = sender.send(Frame::new_still(buf));
                    return Ok(receiver);
                    // return Ok(OpenResult::still(buf));

                    // col.add_still(png_buffer);
                }
                Err(e) => error!("{} from {:?}", e, img_location),
            }
        }
        "nef" | "cr2" | "dng" | "mos" | "erf" | "raf" | "arw" | "3fr" | "ari" | "srf" | "sr2"
        | "braw" | "r3d" | "nrw" | "raw" => {
            debug!("Loading RAW");

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
            let buf = DynamicImage::ImageRgb8(x).to_rgba8();
            // return Ok(OpenResult::still(d.to_rgba8()));
            _ = sender.send(Frame::new_still(buf));
            return Ok(receiver);

            // col.add_still(d.to_rgba8());
        }
        "jxl" => {
            //TODO this needs to be a thread

            fn foo(img_location: &Path, frame_sender: Sender<Frame>) -> Result<()> {
                let mut image = JxlImage::open(img_location).map_err(|e| anyhow!("{e}"))?;
                let mut renderer = image.renderer();

                debug!("{:#?}", renderer.image_header().metadata);
                let is_jxl_anim = renderer.image_header().metadata.animation.is_some();
                let ticks_ms = renderer
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
                loop {
                    // create a mutable image to hold potential decoding results. We can then use this only once at the end of the loop/
                    let image_result: DynamicImage;
                    let result = renderer
                        .render_next_frame()
                        .map_err(|e| anyhow!("{e}"))
                        .context("Can't render JXL")?;
                    match result {
                        RenderResult::Done(render) => {
                            let frame_duration = render.duration() as u16 * ticks_ms;
                            debug!("duration {frame_duration} ms");
                            let framebuffer = render.image();
                            debug!("{:?}", renderer.pixel_format());
                            match renderer.pixel_format() {
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
                                    bail!("JXL: Pixel format: {:?}", renderer.pixel_format())
                                }
                            }

                            // Dispatch to still or animation
                            if is_jxl_anim {
                                // col.add_anim_frame(image_result.to_rgba8(), frame_duration);
                                _ = frame_sender.send(Frame::new(
                                    image_result.to_rgba8(),
                                    frame_duration,
                                    FrameSource::Animation,
                                ));
                            } else {
                                // col.add_still(image_result.to_rgba8());
                                _ = frame_sender.send(Frame::new_still(image_result.to_rgba8()));
                            }
                        }
                        RenderResult::NeedMoreData => {
                            info!("Need more data in JXL");
                        }
                        RenderResult::NoMoreFrames => break,
                    }
                }
                debug!("Done decoding JXL");

                Ok(())
            }

            std::thread::spawn(move || {
                if let Err(e) = foo(&img_location, sender) {
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
            let mut ldr_img: Vec<image::Rgba<u8>> = vec![];

            let hdr_img = hdr_decoder.read_image_hdr()?;
            for pixel in hdr_img {
                let tp = image::Rgba(tonemap_rgb(pixel.0));
                ldr_img.push(tp);
            }
            let mut s: Vec<u8> = vec![];
            let l = ldr_img.clone();
            for p in l {
                let mut x = vec![p.0[0], p.0[1], p.0[2], 255];
                s.append(&mut x);
            }

            let buf = RgbaImage::from_raw(meta.width, meta.height, s)
                .context("Failed to create RgbaImage with given dimensions")?;
            // col.add_still(buf);
            _ = sender.send(Frame::new_still(buf));
            return Ok(receiver);
        }
        "psd" => {
            let contents = std::fs::read(img_location)?;
            let psd = Psd::from_bytes(&contents).map_err(|e| anyhow!("{:?}", e))?;
            let buf = image::ImageBuffer::from_raw(psd.width(), psd.height(), psd.rgba())
                .context("Can't create imagebuffer from PSD")?;

            _ = sender.send(Frame::new_still(buf));
            return Ok(receiver);
        }
        "webp" => {
            let contents = std::fs::read(img_location)?;
            let buf = decode_webp(&contents).context("Can't decode webp")?;
            _ = sender.send(Frame::new_still(buf));
            return Ok(receiver);
        }
        "png" => {
            let contents = std::fs::read(&img_location)?;
            let mut decoder = PngDecoder::new(&contents);
            decoder.set_options(
                DecoderOptions::new_fast()
                    .set_max_height(50000)
                    .set_max_width(50000),
            );
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

                    let (width, height) = decoder
                        .get_dimensions()
                        .context("Can't get png dimensions")?;
                    let colorspace = decoder.get_colorspace().context("Can't get colorspace")?;

                    if colorspace.is_grayscale() {
                        let buf: GrayImage =
                            image::ImageBuffer::from_raw(width as u32, height as u32, imgdata_8bpp)
                                .context("Can't interpret image as grayscale")?;
                        let image_result = DynamicImage::ImageLuma8(buf);
                        _ = sender.send(Frame::new_still(image_result.to_rgba8()));
                        return Ok(receiver);
                    }

                    if colorspace.has_alpha() {
                        let float_image =
                            RgbaImage::from_raw(width as u32, height as u32, imgdata_8bpp)
                                .context("Can't decode rgba buffer")?;
                        _ = sender.send(Frame::new_still(
                            DynamicImage::ImageRgba8(float_image).to_rgba8(),
                        ));
                        return Ok(receiver);
                    } else {
                        let float_image =
                            RgbImage::from_raw(width as u32, height as u32, imgdata_8bpp)
                                .context("Can't decode rgba buffer")?;
                        _ = sender.send(Frame::new_still(
                            DynamicImage::ImageRgb8(float_image).to_rgba8(),
                        ));
                        return Ok(receiver);
                    }
                }
                // 8bpp
                DecodingResult::U8(value) => {
                    let (width, height) = decoder
                        .get_dimensions()
                        .context("Can't get png dimensions")?;

                    let colorspace = decoder.get_colorspace().context("Can't get colorspace")?;
                    if colorspace.is_grayscale() && !colorspace.has_alpha() {
                        let buf: GrayImage =
                            image::ImageBuffer::from_raw(width as u32, height as u32, value)
                                .context("Can't interpret image as grayscale")?;
                        let image_result = DynamicImage::ImageLuma8(buf);
                        _ = sender.send(Frame::new_still(image_result.to_rgba8()));
                        return Ok(receiver);
                    }

                    if colorspace.is_grayscale() && colorspace.has_alpha() {
                        let buf: GrayAlphaImage =
                            image::ImageBuffer::from_raw(width as u32, height as u32, value)
                                .context("Can't interpret image as grayscale")?;
                        let image_result = DynamicImage::ImageLumaA8(buf);
                        _ = sender.send(Frame::new_still(image_result.to_rgba8()));
                        return Ok(receiver);
                    }

                    if colorspace.has_alpha() && !colorspace.is_grayscale() {
                        let buf: RgbaImage =
                            image::ImageBuffer::from_raw(width as u32, height as u32, value)
                                .context("Can't interpret image as rgba")?;
                        _ = sender.send(Frame::new_still(buf));
                        return Ok(receiver);
                    } else {
                        let buf: RgbImage =
                            image::ImageBuffer::from_raw(width as u32, height as u32, value)
                                .context("Can't interpret image as rgb")?;
                        let image_result = DynamicImage::ImageRgb8(buf);
                        _ = sender.send(Frame::new_still(image_result.to_rgba8()));
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
                            screen.pixels.buf().as_bytes().to_vec(),
                        );
                        _ = sender.send(Frame::new(
                            buf.context("Can't read gif frame")?,
                            frame.delay * 10,
                            FrameSource::Animation,
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
            let jpeg_data = std::fs::read(img_location)?;
            let buf: RgbaImage = turbojpeg::decompress_image(&jpeg_data)?;
            _ = sender.send(Frame::new_still(buf));
            return Ok(receiver);
            // col.add_still(img);
        }
        "tif" | "tiff" => {
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
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, u16::MIN as f32, u16::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::U32(contents) => {
                    debug!("TIFF U32");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, u32::MIN as f32, u32::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::U64(contents) => {
                    debug!("TIFF U64");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, u64::MIN as f32, u64::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::F32(contents) => {
                    debug!("TIFF F32");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p, 0.0, 1.0, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::F64(contents) => {
                    debug!("TIFF F64");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, 0.0, 1.0, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::I8(contents) => {
                    debug!("TIFF I8");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, i8::MIN as f32, i8::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::I16(contents) => {
                    debug!("TIFF I16");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, i16::MIN as f32, i16::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::I32(contents) => {
                    debug!("TIFF I32");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, i32::MIN as f32, i32::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
                tiff::decoder::DecodingResult::I64(contents) => {
                    debug!("TIFF I64");
                    ldr_img = contents
                        .par_iter()
                        .map(|p| fit(*p as f32, i64::MIN as f32, i64::MAX as f32, 0., 255.) as u8)
                        .collect();
                }
            }

            match decoder.colortype()? {
                tiff::ColorType::Gray(_) => {
                    debug!("Loading gray color");
                    let i = image::GrayImage::from_raw(dim.0, dim.1, ldr_img)
                        .context("Can't load gray img")?;
                    // col.add_still(DynamicImage::ImageLuma8(i).into_rgba8());
                    _ = sender.send(Frame::new_still(DynamicImage::ImageLuma8(i).into_rgba8()));
                    return Ok(receiver);
                }
                tiff::ColorType::RGB(_) => {
                    debug!("Loading rgb color");
                    let i = image::RgbImage::from_raw(dim.0, dim.1, ldr_img)
                        .context("Can't load RGB img")?;
                    // col.add_still(DynamicImage::ImageRgb8(i).into_rgba8());
                    _ = sender.send(Frame::new_still(DynamicImage::ImageRgb8(i).into_rgba8()));
                    return Ok(receiver);
                }
                tiff::ColorType::RGBA(_) => {
                    debug!("Loading rgba color");
                    let i = image::RgbaImage::from_raw(dim.0, dim.1, ldr_img)
                        .context("Can't load RGBA img")?;
                    // col.add_still(i);
                    _ = sender.send(Frame::new_still(i));
                    return Ok(receiver);
                }
                tiff::ColorType::GrayA(_) => {
                    debug!("Loading gray color with alpha");
                    let i = image::GrayAlphaImage::from_raw(dim.0, dim.1, ldr_img)
                        .context("Can't load gray alpha img")?;
                    // col.add_still(image::DynamicImage::ImageLumaA8(i).into_rgba8());
                    _ = sender.send(Frame::new_still(
                        image::DynamicImage::ImageLumaA8(i).into_rgba8(),
                    ));
                    return Ok(receiver);
                }
                _ => {
                    bail!(
                        "Error: This TIFF image type is unsupported, please open a ticket! {:?}",
                        decoder.colortype()
                    )
                }
            }
        }
        _ => {
            // All other supported image files are handled by using `image`
            let img = image::open(img_location)?;
            // col.add_still(img.to_rgba8());
            _ = sender.send(Frame::new_still(img.to_rgba8()));
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

fn tonemap_f32(px: f32) -> u8 {
    (px.powf(1.0 / 2.2).max(0.0).min(1.0) * 255.0) as u8
    // (px.filmic() * 255.) as u8
}

fn tonemap_rgb(px: [f32; 3]) -> [u8; 4] {
    let mut tm = tonemap_rgba([px[0], px[1], px[2], 1.0]);
    tm[3] = 255;
    tm
}

// Unsafe webp decoding using webp-sys
fn decode_webp(buf: &[u8]) -> Option<RgbaImage> {
    let mut width = 0;
    let mut height = 0;
    let len = buf.len();
    let webp_buffer: Vec<u8>;
    unsafe {
        WebPGetInfo(buf.as_ptr(), len, &mut width, &mut height);
        let out_buf = WebPDecodeRGBA(buf.as_ptr(), len, &mut width, &mut height);
        let len = width * height * 4;
        webp_buffer = Vec::from_raw_parts(out_buf, len as usize, len as usize);
    }
    image::ImageBuffer::from_raw(width as u32, height as u32, webp_buffer)
}
