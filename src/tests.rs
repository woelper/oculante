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

/// The parallel reorientation gives the same pixels as the image crate, for
/// every orientation and layout
#[test]
fn ci_reorient_like_the_image_crate() {
    use crate::image_loader::reorient;
    use image::{DynamicImage, RgbaImage, metadata::Orientation};
    let base = DynamicImage::ImageRgba8(RgbaImage::from_fn(5, 3, |x, y| {
        image::Rgba([(x * 40) as u8, (y * 70) as u8, (x * y * 9) as u8, 200])
    }));
    let layouts = [
        DynamicImage::ImageLuma8(base.to_luma8()),
        DynamicImage::ImageRgb8(base.to_rgb8()),
        base.clone(),
        DynamicImage::ImageLumaA16(base.to_luma_alpha16()),
        DynamicImage::ImageRgba32F(base.to_rgba32f()),
    ];
    for exif in 1..=8 {
        let orientation = Orientation::from_exif(exif).unwrap();
        for layout in &layouts {
            let mut expected = layout.clone();
            expected.apply_orientation(orientation);
            let mut ours = layout.clone();
            reorient(&mut ours, orientation);
            assert_eq!(ours, expected, "{orientation:?} {:?}", layout.color());
        }
    }
}

/// A photo with an EXIF orientation comes out turned, through the decoder the
/// app uses. None of the test images had an orientation.
#[test]
fn ci_exif_orientation_is_applied() {
    use crate::image_loader::rotate_dynimage;
    let dir = std::env::temp_dir().join("oculante_test_orientation");
    _ = std::fs::create_dir_all(&dir);
    for (orientation, size, red_at) in [
        (1, (4, 2), (0, 0)),
        (6, (2, 4), (1, 0)),
        (8, (2, 4), (0, 3)),
    ] {
        let path = dir.join(format!("turned_{orientation}.jpg"));
        write_jpeg_with_orientation(&path, orientation);
        let mut image = load_first_frame(path.to_str().unwrap())
            .get_image()
            .unwrap();
        rotate_dynimage(&mut image, &path).unwrap();
        let rgb = image.to_rgb8();
        assert_eq!(rgb.dimensions(), size, "orientation {orientation}");
        let red = rgb.get_pixel(red_at.0, red_at.1).0;
        assert!(
            red[0] > 180 && red[2] < 100,
            "orientation {orientation}: {red:?} at {red_at:?}"
        );
    }
    _ = std::fs::remove_dir_all(&dir);
}

/// The mean difference of two images of the same size, per channel
fn mean_difference(a: &image::RgbImage, b: &image::RgbImage) -> f64 {
    assert_eq!(a.dimensions(), b.dimensions());
    let sum: u64 = a
        .as_raw()
        .iter()
        .zip(b.as_raw())
        .map(|(x, y)| x.abs_diff(*y) as u64)
        .sum();
    sum as f64 / a.as_raw().len() as f64
}

/// Lossless rotations and flips of a JPEG whose size is not a multiple of its
/// blocks: the result is the turned image, without strips of the old one at an
/// edge. Blocks that can not be moved are trimmed, a few pixels at most.
#[cfg(feature = "turbo")]
#[test]
fn ci_lossless_jpeg_transforms() {
    use crate::image_editing::lossless_tx;
    use image::GenericImageView;
    use turbojpeg::{Transform, TransformOp};
    let dir = std::env::temp_dir().join("oculante_test_lossless");
    _ = std::fs::create_dir_all(&dir);
    // 1000x750 in blocks of 16: 8 and 14 pixels are left over
    let original = image::open("res/tests/moss.jpg").unwrap();
    for (op, expected) in [
        (TransformOp::Rot90, original.rotate90()),
        (TransformOp::Rot180, original.rotate180()),
        (TransformOp::Rot270, original.rotate270()),
        (TransformOp::Hflip, original.fliph()),
        (TransformOp::Vflip, original.flipv()),
    ] {
        let path = dir.join(format!("{op:?}.jpg"));
        std::fs::copy("res/tests/moss.jpg", &path).unwrap();
        lossless_tx(&path, Transform::op(op)).unwrap();
        let turned = image::open(&path).unwrap();
        let (w, h) = turned.dimensions();
        assert!(
            expected.width() - w < 16 && expected.height() - h < 16,
            "{op:?}: {w}x{h} from {:?}",
            expected.dimensions()
        );
        // trimmed blocks were at the start of the turned image when the edge moved
        // to the left or the top
        let x = expected.width() - w;
        let y = expected.height() - h;
        let best = [(0, 0), (x, 0), (0, y), (x, y)]
            .into_iter()
            .map(|(x, y)| {
                mean_difference(&turned.to_rgb8(), &expected.crop_imm(x, y, w, h).to_rgb8())
            })
            .fold(f64::MAX, f64::min);
        assert!(
            best < 1.5,
            "{op:?} differs from the turned image by {best:.2} on average"
        );
    }

    // a crop that does not start on a block keeps to the image
    let path = dir.join("crop.jpg");
    std::fs::copy("res/tests/moss.jpg", &path).unwrap();
    let mut crop = Transform::default();
    crop.crop = Some(turbojpeg::TransformCrop {
        x: 13,
        y: 21,
        width: Some(300),
        height: Some(200),
    });
    lossless_tx(&path, crop).unwrap();
    let cropped = image::open(&path).unwrap();
    assert_eq!(cropped.dimensions(), (300, 200));
    _ = std::fs::remove_dir_all(&dir);
}

/// All frames the loader sends for a file, and the warnings
fn load_all(path: &std::path::Path) -> anyhow::Result<(Vec<Frame>, Vec<String>)> {
    let (message_sender, messages) = std::sync::mpsc::channel();
    let receiver = open_image(path, Some(message_sender), None)?;
    let mut frames = vec![];
    while let Ok(frame) = receiver.recv_timeout(Duration::from_secs(60)) {
        frames.push(frame);
    }
    let warnings = messages
        .try_iter()
        .filter_map(|m| match m {
            crate::appstate::Message::Warning(w) => Some(w),
            _ => None,
        })
        .collect();
    Ok((frames, warnings))
}

/// The size a file name gives, like 600x300 in "600x300_float.exr"
fn size_in_name(name: &str) -> Option<(u32, u32)> {
    name.split(|c: char| !c.is_ascii_alphanumeric())
        .find_map(|part| {
            let (w, h) = part.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
}

/// Every file in res/tests loads, with the size its name gives, and the
/// animated ones as animations. About 30 of them were loaded by no test.
#[test]
fn ci_load_every_test_file() {
    // not supported, or not an image
    let mut expected_errors = vec![
        "mp4_ex-signature.gif",
        "test_R16G16B16_SFLOAT.ktx2",
        "test_R32G32B32_SFLOAT.ktx2",
    ];
    if cfg!(not(any(feature = "heif", feature = "heif_native"))) {
        expected_errors.push("orange.heic");
    }
    let animated = [
        "APNG_throbber.png",
        "Animated_PNG_example_bouncing_beach_ball.png",
        "3d2.png",
    ];
    let mut paths: Vec<_> = std::fs::read_dir("res/tests")
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .collect();
    paths.sort();
    let mut failures = vec![];
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let frames = match load_all(&path) {
            Ok((frames, _)) => frames,
            Err(e) => {
                if !expected_errors.contains(&name.as_str()) {
                    failures.push(format!("{name}: {e}"));
                }
                continue;
            }
        };
        let images: Vec<_> = frames.iter().filter_map(|f| f.get_image()).collect();
        let Some(first) = images.first() else {
            if !expected_errors.contains(&name.as_str()) {
                failures.push(format!("{name}: no image"));
            }
            continue;
        };
        if expected_errors.contains(&name.as_str()) {
            failures.push(format!("{name}: loads now, update the test"));
        }
        if first.width() == 0 || first.height() == 0 {
            failures.push(format!("{name}: empty"));
        }
        if let Some(size) = size_in_name(&name)
            && (first.width(), first.height()) != size
        {
            failures.push(format!("{name}: {}x{}", first.width(), first.height()));
        }
        if animated.contains(&name.as_str()) {
            let ended = frames.iter().any(|f| matches!(f, Frame::AnimationEnd(_)));
            if images.len() < 2 || !ended {
                failures.push(format!(
                    "{name}: {} frames, end sent: {ended}",
                    images.len()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Formats without a test file are written here and loaded, with the right
/// size, a pixel that is right, and no warning about the extension
#[test]
fn ci_load_generated_formats() {
    use image::{DynamicImage, GrayImage, ImageBuffer, Luma, RgbaImage};
    let dir = std::env::temp_dir().join("oculante_test_formats");
    _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let rgba = DynamicImage::ImageRgba8(RgbaImage::from_fn(5, 3, |x, y| {
        image::Rgba([(x * 50) as u8, (y * 80) as u8, 90, 255])
    }));
    // the pixel at 4,2
    let corner = [200, 160, 90];
    let gray16 = DynamicImage::ImageLuma16(ImageBuffer::from_fn(5, 3, |x, y| {
        Luma([(x * 10000 + y * 500) as u16])
    }));
    let written = [
        ("a.bmp", rgba.to_rgb8().into()),
        ("a.ico", rgba.clone()),
        ("a.tga", rgba.clone()),
        ("a.qoi", rgba.clone()),
        ("a.ppm", rgba.to_rgb8().into()),
        ("a.ff", rgba.to_rgba16().into()),
        ("a.jpeg", rgba.to_rgb8().into()),
        ("a.tif", rgba.to_rgb8().into()),
        ("rgba.tiff", rgba.clone()),
        (
            "gray.tif",
            DynamicImage::ImageLuma8(GrayImage::from_fn(5, 3, |x, _| Luma([(x * 50) as u8]))),
        ),
        ("gray16.tif", gray16),
        ("float.hdr", rgba.to_rgb32f().into()),
    ];
    let mut failures = vec![];
    for (name, image) in written {
        let path = dir.join(name);
        image.save(&path).unwrap();
        let (frames, warnings) = match load_all(&path) {
            Ok(loaded) => loaded,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        let Some(loaded) = frames.first().and_then(|f| f.get_image()) else {
            failures.push(format!("{name}: no image"));
            continue;
        };
        if (loaded.width(), loaded.height()) != (5, 3) {
            failures.push(format!("{name}: {}x{}", loaded.width(), loaded.height()));
        }
        if !warnings.is_empty() {
            failures.push(format!("{name}: {warnings:?}"));
        }
        let pixel = loaded.to_rgb8().get_pixel(4, 2).0;
        let lossless =
            !name.ends_with(".jpeg") && !name.ends_with(".hdr") && !name.starts_with("gray");
        if lossless && pixel != corner {
            failures.push(format!("{name}: {pixel:?} at 4,2"));
        }
    }

    // A 16 bit TIFF with a single value has its level, it came out black
    let flat = DynamicImage::ImageLuma16(ImageBuffer::from_pixel(4, 4, Luma([30000u16])));
    let path = dir.join("flat.tif");
    flat.save(&path).unwrap();
    let level = load_all(&path).unwrap().0[0]
        .get_image()
        .unwrap()
        .to_luma8()
        .get_pixel(0, 0)
        .0[0];
    if level.abs_diff(117) > 1 {
        failures.push(format!("flat.tif: level {level}"));
    }

    // formats the image crate can not write: written by hand
    let by_hand: [(&str, &[u8]); 3] = [
        (
            "a.xbm",
            b"#define a_width 5\n#define a_height 3\nstatic unsigned char a_bits[] = { 0x01, 0x02, 0x04 };\n",
        ),
        (
            "a.xpm",
            b"/* XPM */\nstatic char *a[] = {\n\"5 3 2 1\",\n\"r c #FF0000\",\n\"b c #0000FF\",\n\"rrbbb\",\n\"bbbbb\",\n\"rrrrr\"\n};\n",
        ),
        // type 0, no extended header, 5x3, one byte per row
        ("a.wbmp", &[0, 0, 5, 3, 0b1000_0000, 0b0100_0000, 0b0010_0000]),
    ];
    for (name, bytes) in by_hand {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        match load_all(&path).map(|(frames, _)| frames.first().and_then(|f| f.get_image())) {
            Ok(Some(image)) if (image.width(), image.height()) == (5, 3) => {}
            Ok(Some(image)) => {
                failures.push(format!("{name}: {}x{}", image.width(), image.height()))
            }
            Ok(None) => failures.push(format!("{name}: no image")),
            Err(e) => failures.push(format!("{name}: {e}")),
        }
    }
    _ = std::fs::remove_dir_all(&dir);
    assert!(failures.is_empty(), "{failures:#?}");
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
