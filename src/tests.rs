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
        Frame::UpdateTexture | Frame::AnimationEnd => {
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
