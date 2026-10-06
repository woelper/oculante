use std::path::PathBuf;
use std::time::Duration;

use crate::image_loader::open_image;
use crate::utils::Frame;

/// Helper: load an image and return the first frame, with timeout
fn load_first_frame(path: &str) -> Frame {
    let p = PathBuf::from(path);
    assert!(p.exists(), "Test file not found: {path}");
    let rx = open_image(&p, None, None).expect("open_image failed");
    rx.recv_timeout(Duration::from_secs(30))
        .expect("Timed out waiting for image")
}

/// Helper: assert the frame contains an image with nonzero dimensions
fn assert_valid_image(frame: &Frame) {
    match frame {
        Frame::Still(img)
        | Frame::AnimationStart(img)
        | Frame::Animation(img, _)
        | Frame::EditResult(img)
        | Frame::CompareResult(img, _)
        | Frame::ImageCollectionMember(img) => {
            assert!(
                img.width() > 0 && img.height() > 0,
                "Image has zero dimensions"
            );
        }
        Frame::UpdateTexture | Frame::AnimationEnd(_) => {
            panic!("Expected an image frame, got {frame}")
        }
    }
}

// === Format loading tests ===

#[test]
fn ci_load_jpg() {
    let frame = load_first_frame("res/tests/test.jpg");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_png() {
    let frame = load_first_frame("res/tests/test.png");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_png_16bit() {
    let frame = load_first_frame("res/tests/pngtest_16bit.png");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_png_gray() {
    let frame = load_first_frame("res/tests/gray_8bpp.png");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_webp() {
    let frame = load_first_frame("res/tests/mohsen-karimi.webp");
    assert_valid_image(&frame);
}

/// Writes an animated GIF with the given frame delays in milliseconds. Without
/// a repeat, the GIF has no loop block.
fn write_gif(
    path: &std::path::Path,
    delays_ms: &[u32],
    repeat: Option<image::codecs::gif::Repeat>,
) {
    use image::codecs::gif::GifEncoder;
    use image::{Delay, Frame as ImageFrame, Rgba, RgbaImage};
    let mut encoder = GifEncoder::new(std::fs::File::create(path).unwrap());
    if let Some(repeat) = repeat {
        encoder.set_repeat(repeat).unwrap();
    }
    for (i, delay) in delays_ms.iter().enumerate() {
        let color = Rgba([(i * 80) as u8, 0, 0, 255]);
        let buffer = RgbaImage::from_pixel(8, 8, color);
        encoder
            .encode_frame(ImageFrame::from_parts(
                buffer,
                0,
                0,
                Delay::from_numer_denom_ms(*delay, 1),
            ))
            .unwrap();
    }
}

/// GIF delays go up to more than ten minutes. A frame of 100 seconds kept
/// its delay instead of overflowing.
#[test]
fn ci_load_gif_long_delay() {
    let path = std::env::temp_dir().join("oculante_test_long_delay.gif");
    write_gif(
        &path,
        &[100, 100_000, 20],
        Some(image::codecs::gif::Repeat::Infinite),
    );
    let receiver = open_image(&path, None, None).expect("open_image failed");
    let delays: Vec<u32> = receiver
        .iter()
        .filter_map(|frame| match frame {
            Frame::Animation(_, delay) => Some(delay),
            _ => None,
        })
        .collect();
    _ = std::fs::remove_file(&path);
    assert_eq!(delays, vec![100, 100_000, 20]);
}

/// How often the loader says an animation is to be played
fn plays_of(path: &std::path::Path) -> Option<u32> {
    let receiver = open_image(path, None, None).expect("open_image failed");
    receiver
        .iter()
        .find_map(|frame| match frame {
            Frame::AnimationEnd(plays) => Some(plays),
            _ => None,
        })
        .expect("the animation has no end")
}

/// A GIF without a loop block plays once, a loop count of n repeats it n more
/// times, 0 is forever.
#[test]
fn ci_gif_play_count() {
    use image::codecs::gif::Repeat;
    let path = std::env::temp_dir().join("oculante_test_play_count.gif");
    for (repeat, plays) in [
        (None, Some(1)),
        (Some(Repeat::Finite(2)), Some(3)),
        (Some(Repeat::Infinite), None),
    ] {
        write_gif(&path, &[100, 100], repeat);
        assert_eq!(plays_of(&path), plays, "for {repeat:?}");
    }
    _ = std::fs::remove_file(&path);
}

/// An APNG says how often it is played, 0 is forever
#[test]
fn ci_apng_play_count() {
    let path = std::env::temp_dir().join("oculante_test_play_count.png");
    for (num_plays, plays) in [(1, Some(1)), (3, Some(3)), (0, None)] {
        let file = std::io::BufWriter::new(std::fs::File::create(&path).unwrap());
        let mut encoder = png::Encoder::new(file, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_animated(2, num_plays).unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[255; 16]).unwrap();
        writer.write_image_data(&[0; 16]).unwrap();
        writer.finish().unwrap();
        assert_eq!(plays_of(&path), plays, "for num_plays {num_plays}");
    }
    _ = std::fs::remove_file(&path);
}

/// An animated PNG of 4x4 pixels with 16 bit colors: a red frame, then a blue one
fn write_apng_16bit(path: &std::path::Path) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    let mut encoder = png::Encoder::new(file, 4, 4);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Sixteen);
    encoder.set_animated(2, 0).unwrap();
    encoder.set_frame_delay(1, 10).unwrap();
    let mut writer = encoder.write_header().unwrap();
    let red = [0xff, 0xff, 0, 0, 0, 0, 0xff, 0xff].repeat(16);
    let blue = [0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff].repeat(16);
    writer.write_image_data(&red).unwrap();
    writer.write_image_data(&blue).unwrap();
    writer.finish().unwrap();
}

/// The image crate can not composite the frames of a 16 bit APNG. The default
/// image is shown instead, with a warning, as the APNG specification recommends
/// for animations that can not be played.
#[test]
fn ci_load_apng_16bit_shows_default_image() {
    let path = std::env::temp_dir().join("oculante_test_apng_16bit.png");
    write_apng_16bit(&path);
    let (message_sender, messages) = std::sync::mpsc::channel();
    let receiver = open_image(&path, Some(message_sender), None).expect("open_image failed");
    let frames: Vec<Frame> = receiver.iter().collect();
    _ = std::fs::remove_file(&path);
    assert_eq!(frames.len(), 1, "expected the default image only");
    let Frame::Still(image) = &frames[0] else {
        panic!("expected a still image, got {}", frames[0]);
    };
    assert_eq!(image.to_rgba8().get_pixel(0, 0).0, [255, 0, 0, 255]);
    assert!(
        messages
            .try_iter()
            .any(|m| matches!(m, crate::appstate::Message::Warning(_))),
        "no warning that the animation can not be played"
    );
}

/// Pixel operations work on every layout they support, and refuse the others
/// instead of crashing. 32 bit float RGBA crashed.
#[test]
fn ci_pixel_operations_on_every_layout() {
    use crate::image_editing::{ImageOperation, process_pixels};
    use image::{DynamicImage, Rgba32FImage, RgbaImage};
    let ops = vec![ImageOperation::Invert];
    let base = RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]));
    let layouts = [
        DynamicImage::ImageLuma8(DynamicImage::ImageRgba8(base.clone()).to_luma8()),
        DynamicImage::ImageLumaA8(DynamicImage::ImageRgba8(base.clone()).to_luma_alpha8()),
        DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(base.clone()).to_rgb8()),
        DynamicImage::ImageRgba8(base.clone()),
        DynamicImage::ImageRgb32F(DynamicImage::ImageRgba8(base.clone()).to_rgb32f()),
        DynamicImage::ImageRgba32F(Rgba32FImage::from(DynamicImage::ImageRgba8(base.clone()))),
    ];
    for mut image in layouts {
        let color = image.color();
        process_pixels(&mut image, &ops).unwrap_or_else(|e| panic!("{color:?}: {e}"));
        // every pixel, including the last one, was inverted
        let last = image.to_rgba8().get_pixel(2, 1).0;
        assert!(last[0] > 200, "{color:?} was not inverted: {last:?}");
    }
    let mut sixteen = DynamicImage::ImageRgba16(DynamicImage::ImageRgba8(base).to_rgba16());
    assert!(process_pixels(&mut sixteen, &ops).is_err());
}

/// Every encoder saves every layout an image can have, and the file opens
/// again with the same size. Lossless formats keep the pixels.
#[test]
fn ci_save_every_layout_with_every_encoder() {
    use crate::file_encoder::{CompressionLevel, FileEncoder};
    use image::{DynamicImage, GenericImageView, RgbaImage};
    let base = DynamicImage::ImageRgba8(RgbaImage::from_fn(5, 3, |x, y| {
        image::Rgba([(x * 50) as u8, (y * 80) as u8, 90, 255])
    }));
    let layouts = [
        base.to_luma8().into(),
        base.to_luma_alpha8().into(),
        base.to_rgb8().into(),
        base.clone(),
        base.to_luma16().into(),
        base.to_rgba16().into(),
        base.to_rgb32f().into(),
        DynamicImage::ImageRgba32F(base.to_rgba32f()),
    ];
    let encoders = [
        FileEncoder::Png {
            compressionlevel: CompressionLevel::Default,
        },
        FileEncoder::Jpg { quality: 90 },
        FileEncoder::Bmp,
        FileEncoder::WebP,
        FileEncoder::Avif,
    ];
    let dir = std::env::temp_dir().join("oculante_test_save");
    _ = std::fs::create_dir_all(&dir);
    let mut failures = vec![];
    for encoder in &encoders {
        for image in &layouts {
            let path = dir.join(format!("{:?}.{}", image.color(), encoder.ext()));
            let result = std::panic::catch_unwind(|| encoder.save(image, &path));
            let saved = match result {
                Ok(Ok(())) => open_image(&path, None, None).and_then(|receiver| {
                    receiver
                        .recv_timeout(Duration::from_secs(30))?
                        .get_image()
                        .ok_or(anyhow::anyhow!("no image"))
                }),
                Ok(Err(e)) => {
                    failures.push(format!("{encoder} {:?}: {e}", image.color()));
                    continue;
                }
                Err(_) => {
                    failures.push(format!("{encoder} {:?}: panicked", image.color()));
                    continue;
                }
            };
            match saved {
                Ok(saved) if saved.dimensions() != (5, 3) => failures.push(format!(
                    "{encoder} {:?}: size {:?}",
                    image.color(),
                    saved.dimensions()
                )),
                Ok(saved) => {
                    let lossless = matches!(
                        encoder,
                        FileEncoder::Png { .. } | FileEncoder::Bmp | FileEncoder::WebP
                    );
                    if lossless && saved.to_rgb8() != image.to_rgb8() {
                        failures.push(format!("{encoder} {:?}: pixels differ", image.color()));
                    }
                }
                Err(e) => failures.push(format!(
                    "{encoder} {:?}: can not open the file: {e}",
                    image.color()
                )),
            }
        }
    }
    _ = std::fs::remove_dir_all(&dir);
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Edits saved for an image or a folder are found again, older files are
/// upgraded. They were written and never read since the move to egui.
#[test]
fn ci_saved_edits_are_found() {
    use crate::image_editing::{
        EditState, ImageOperation, ImgOpItem, LegacyEditState, saved_edits,
    };
    let dir = std::env::temp_dir().join("oculante_test_saved_edits");
    _ = std::fs::remove_dir_all(&dir);
    for sub in ["own", "folder", "legacy", "broken", "none"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    let edits = EditState {
        pixel_op_stack: vec![ImgOpItem::new(ImageOperation::Invert)],
        ..Default::default()
    };
    let json = serde_json::to_string(&edits).unwrap();
    std::fs::write(dir.join("own/a.oculante"), &json).unwrap();
    std::fs::write(dir.join("folder/.oculante"), &json).unwrap();
    let legacy = LegacyEditState {
        painting: false,
        non_destructive_painting: false,
        paint_strokes: vec![],
        paint_fade: false,
        pixel_op_stack: vec![ImageOperation::Invert],
        image_op_stack: vec![],
        export_extension: "png".into(),
    };
    std::fs::write(
        dir.join("legacy/a.oculante"),
        serde_json::to_string(&legacy).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("broken/a.oculante"), "not json").unwrap();

    let found = |sub: &str| saved_edits(&dir.join(sub).join("a.png"));
    let (own, message) = found("own").unwrap().unwrap();
    assert_eq!(own.pixel_op_stack.len(), 1);
    assert!(message.contains("this image"));
    let (folder, message) = found("folder").unwrap().unwrap();
    assert_eq!(folder.pixel_op_stack.len(), 1);
    assert!(message.contains("Directory"));
    let (upgraded, _) = found("legacy").unwrap().unwrap();
    assert_eq!(upgraded.pixel_op_stack.len(), 1);
    // and saved in the current format
    let rewritten = std::fs::read_to_string(dir.join("legacy/a.oculante")).unwrap();
    assert!(serde_json::from_str::<EditState>(&rewritten).is_ok());
    assert!(found("broken").unwrap().is_err());
    assert!(found("none").is_none());
    _ = std::fs::remove_dir_all(&dir);
}

/// EXIF data with only an orientation tag, as TIFF data the way JPEG files hold it
pub(crate) fn exif_with_orientation(orientation: u16) -> Vec<u8> {
    let mut exif = vec![b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0];
    // tag 0x0112, type SHORT, one value
    exif.extend_from_slice(&[0x12, 0x01, 3, 0, 1, 0, 0, 0]);
    exif.extend_from_slice(&orientation.to_le_bytes());
    exif.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    exif
}

/// A JPEG of 4x2 pixels with a red pixel at the top left and the given EXIF
/// orientation
pub(crate) fn write_jpeg_with_orientation(path: &std::path::Path, orientation: u16) {
    let image = image::RgbImage::from_fn(4, 2, |x, y| {
        if (x, y) == (0, 0) {
            image::Rgb([255, 0, 0])
        } else {
            image::Rgb([0, 0, 255])
        }
    });
    image.save(path).unwrap();
    crate::utils::fix_exif(path, Some(exif_with_orientation(orientation).into())).unwrap();
}

/// EXIF is read for the info panel, and kept when a copy is saved in another
/// format, also now that it is taken from the file at the time of saving
#[test]
fn ci_exif_is_read_and_kept_when_saving() {
    use crate::file_encoder::{CompressionLevel, FileEncoder};
    use crate::utils::{ExtendedImageInfo, fix_exif, raw_exif};
    let dir = std::env::temp_dir().join("oculante_test_exif");
    _ = std::fs::create_dir_all(&dir);
    let source = dir.join("photo.jpg");
    write_jpeg_with_orientation(&source, 6);

    let mut info = ExtendedImageInfo::default();
    info.with_exif(&source).unwrap();
    assert!(
        // orientation 6, as the EXIF reader describes it
        info.exif
            .get("Orientation")
            .is_some_and(|o| o.contains("row 0 at right")),
        "{:?}",
        info.exif
    );

    let exif = raw_exif(&source).expect("no EXIF in the source");
    let copy = dir.join("copy.png");
    let image = image::open(&source).unwrap();
    FileEncoder::Png {
        compressionlevel: CompressionLevel::Default,
    }
    .save(&image, &copy)
    .unwrap();
    fix_exif(&copy, Some(exif.clone())).unwrap();
    assert_eq!(raw_exif(&copy), Some(exif));
    _ = std::fs::remove_dir_all(&dir);
}

/// The warnings the loader sends while opening a file
fn warnings_for(path: &str) -> Vec<String> {
    let (message_sender, messages) = std::sync::mpsc::channel();
    if let Ok(receiver) = open_image(&PathBuf::from(path), Some(message_sender), None) {
        _ = receiver.recv_timeout(Duration::from_secs(30));
    }
    messages
        .try_iter()
        .filter_map(|m| match m {
            crate::appstate::Message::Warning(w) => Some(w),
            _ => None,
        })
        .collect()
}

/// A HEIC file is a HEIF file. Every one of them was reported as having the
/// wrong extension.
#[cfg(any(feature = "heif", feature = "heif_native"))]
#[test]
fn ci_heic_has_the_right_extension() {
    assert_eq!(warnings_for("res/tests/orange.heic"), Vec::<String>::new());
    // a file that really has the wrong extension is still reported
    assert_eq!(warnings_for("res/tests/mp4_ex-signature.gif").len(), 1);
}

#[test]
fn ci_load_misnamed_mp4_as_gif() {
    // This file is actually an MP4 with a .gif extension.
    // It should fail gracefully (not panic).
    let p = PathBuf::from("res/tests/mp4_ex-signature.gif");
    assert!(p.exists());
    let result = open_image(&p, None, None);
    // Either open_image returns an error, or the frame it sends is an error.
    // The point is it doesn't panic.
    if let Ok(rx) = result {
        // If it sends something, that's fine too — format detection may reclassify it
        let _ = rx.recv_timeout(Duration::from_secs(5));
    }
}

#[test]
fn ci_load_exr() {
    let frame = load_first_frame("res/tests/test.exr");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_exr_float() {
    let frame = load_first_frame("res/tests/512x512_float.exr");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_psd() {
    let frame = load_first_frame("res/tests/test.psd");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_svg() {
    let frame = load_first_frame("res/tests/johnny_automatic_lobster.svg");
    assert_valid_image(&frame);
    // 165x282 in the file, drawn at twice that size by default
    let img = frame.get_image().unwrap();
    assert_eq!((img.width(), img.height()), (330, 564));
}

#[test]
fn ci_load_jxl() {
    let frame = load_first_frame("res/tests/test.jxl");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_dds() {
    let frame = load_first_frame("res/tests/test.dds");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_ktx2_r8g8b8a8() {
    let frame = load_first_frame("res/tests/test_R8G8B8A8_SRGB.ktx2");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_ktx2_r16g16b16a16() {
    let frame = load_first_frame("res/tests/test_R16G16B16A16_SFLOAT.ktx2");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_avif() {
    let frame = load_first_frame("res/tests/red-at-12-oclock-with-color-profile-8bpc.avif");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_large_jpg() {
    let frame = load_first_frame("res/tests/large_image.jpg");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_no_extension() {
    // File with no extension — format detected from content
    let frame = load_first_frame("res/tests/pngtest_16bit_no_ext");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_unicode_path() {
    let frame = load_first_frame("res/tests/AR-اختبار.png");
    assert_valid_image(&frame);
}

#[cfg(feature = "j2k")]
#[test]
fn ci_load_jp2() {
    let frame = load_first_frame("res/tests/test.jp2");
    assert_valid_image(&frame);
}

/// HEIC without libheif. The mean color also checks that the decoder reads the
/// range and the matrix from the video stream: this photo has no colr box, and
/// a decoder that ignores the stream gets about 173, 139, 107.
#[cfg(feature = "heif_native")]
#[test]
fn ci_load_heic_native() {
    let frame = load_first_frame("res/tests/orange.heic");
    assert_valid_image(&frame);
    let img = frame.get_image().unwrap();
    // The file stores 4000x3000 and a rotation, which the decoder applies
    assert_eq!((img.width(), img.height()), (3000, 4000));
    let rgb = img.to_rgb8();
    let pixels = (rgb.width() * rgb.height()) as f64;
    let mut sum = [0f64; 3];
    for p in rgb.pixels() {
        for (total, channel) in sum.iter_mut().zip(p.0) {
            *total += channel as f64;
        }
    }
    // What libheif gets for this file
    for (mean, expected) in sum.iter().map(|s| s / pixels).zip([162.0, 134.0, 104.0]) {
        assert!(
            (mean - expected).abs() < 2.0,
            "mean color {mean:.1} is not near {expected}"
        );
    }
}
