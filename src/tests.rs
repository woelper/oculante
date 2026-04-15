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
            assert!(img.width() > 0 && img.height() > 0, "Image has zero dimensions");
        }
        Frame::UpdateTexture => panic!("Expected an image frame, got UpdateTexture"),
    }
}

// === Format loading tests ===

#[test]
fn ci_load_jpg() {
    let frame = load_first_frame("tests/test.jpg");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_png() {
    let frame = load_first_frame("tests/test.png");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_png_16bit() {
    let frame = load_first_frame("tests/pngtest_16bit.png");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_png_gray() {
    let frame = load_first_frame("tests/gray_8bpp.png");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_webp() {
    let frame = load_first_frame("tests/mohsen-karimi.webp");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_misnamed_mp4_as_gif() {
    // This file is actually an MP4 with a .gif extension.
    // It should fail gracefully (not panic).
    let p = PathBuf::from("tests/mp4_ex-signature.gif");
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
    let frame = load_first_frame("tests/test.exr");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_exr_float() {
    let frame = load_first_frame("tests/512x512_float.exr");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_psd() {
    let frame = load_first_frame("tests/test.psd");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_svg() {
    let frame = load_first_frame("tests/johnny_automatic_lobster.svg");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_jxl() {
    let frame = load_first_frame("tests/test.jxl");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_dds() {
    let frame = load_first_frame("tests/test.dds");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_ktx2_r8g8b8a8() {
    let frame = load_first_frame("tests/test_R8G8B8A8_SRGB.ktx2");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_ktx2_r16g16b16a16() {
    let frame = load_first_frame("tests/test_R16G16B16A16_SFLOAT.ktx2");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_avif() {
    let frame = load_first_frame("tests/red-at-12-oclock-with-color-profile-8bpc.avif");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_large_jpg() {
    let frame = load_first_frame("tests/large_image.jpg");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_no_extension() {
    // File with no extension — format detected from content
    let frame = load_first_frame("tests/pngtest_16bit_no_ext");
    assert_valid_image(&frame);
}

#[test]
fn ci_load_unicode_path() {
    let frame = load_first_frame("tests/AR-اختبار.png");
    assert_valid_image(&frame);
}

#[cfg(feature = "j2k")]
#[test]
fn ci_load_jp2() {
    let frame = load_first_frame("tests/test.jp2");
    assert_valid_image(&frame);
}
