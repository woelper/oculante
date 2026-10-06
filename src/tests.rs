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
