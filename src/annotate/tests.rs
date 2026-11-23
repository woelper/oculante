use super::*;
use image::{Rgba, RgbaImage};

pub(crate) fn item(operation: ImageOperation) -> ImgOpItem {
    ImgOpItem::new(operation)
}

/// Different colors everywhere, so a moved pixel shows
fn gradient(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        Rgba([
            (x * 255 / w) as u8,
            (y * 255 / h) as u8,
            ((x + y) % 256) as u8,
            255,
        ])
    })
}

pub(crate) fn arrow_at(start: [i32; 2], to: [i32; 2]) -> Annotation {
    Annotation::new(Shape::Arrow(to), [255, 0, 0, 255], 6, start)
}

/// All annotations drawn at once on a fresh copy of the image
fn drawn_at_once(img: &RgbaImage, annotations: &[Annotation], mapping: &Mapping) -> RgbaImage {
    let mut fresh = img.clone();
    AnnotationLayer::default().update(&mut fresh, annotations, mapping);
    fresh
}

#[test]
fn points_follow_crop_resize_rotation_and_flip() {
    let ops = [
        // a quarter off the left and the top
        item(ImageOperation::Crop([2500, 2500, 0, 0])),
        item(ImageOperation::Resize {
            dimensions: (150, 75),
            aspect: false,
            filter: crate::image_editing::ScaleFilter::Box,
        }),
        item(ImageOperation::Rotate(90)),
        item(ImageOperation::Flip(true)),
    ];
    let mapping = Mapping::new(&ops, (400, 200));
    // cropped to 300x150, halved to 150x75, turned to 75x150
    assert_eq!(mapping.result, (75, 150));
    // (200, 100) is (100, 50) cropped, (50, 25) halved, (50, 50) turned, (50, 100) flipped
    let at = mapping.to_result([200., 100.]);
    assert!(
        (at[0] - 50.).abs() < 0.01 && (at[1] - 100.).abs() < 0.01,
        "{at:?}"
    );
    let back = mapping.to_original(at);
    assert!(
        (back[0] - 200.).abs() < 0.01 && (back[1] - 100.).abs() < 0.01,
        "{back:?}"
    );
}

#[test]
fn annotations_stay_on_what_they_point_at_when_cropped() {
    let img = gradient(400, 200);
    let arrow = arrow_at([150, 100], [100, 0]);
    let whole = drawn_at_once(
        &img,
        std::slice::from_ref(&arrow),
        &Mapping::new(&[], (400, 200)),
    );
    let crop = [item(ImageOperation::Crop([2500, 0, 0, 0]))];
    let mapping = Mapping::new(&crop, (400, 200));
    let cropped_img = image::imageops::crop_imm(&img, 100, 0, 300, 200).to_image();
    let cropped = drawn_at_once(&cropped_img, &[arrow], &mapping);
    for y in 0..200 {
        for x in 0..300 {
            assert_eq!(
                whole.get_pixel(x + 100, y),
                cropped.get_pixel(x, y),
                "at {x},{y}"
            );
        }
    }
}

#[test]
fn a_box_selects_what_is_mostly_in_it() {
    let mapping = Mapping::new(&[], (400, 300));
    let pick = |list: &[Annotation], a: [f32; 2], b: [f32; 2]| in_box(list, &mapping, a, b);
    let mut annotations = vec![
        // Covers 10,10 to 50,40
        arrow_at([10, 10], [40, 30]),
        // Covers 200,100 to 300,200
        arrow_at([200, 100], [100, 100]),
    ];

    // How much of the second arrow is in the box
    // A quarter: half across, half down
    assert_eq!(pick(&annotations, [0., 0.], [250., 150.]), vec![0]);
    // Slightly less than half: 0.49 across, all down
    assert_eq!(pick(&annotations, [0., 0.], [249., 200.]), vec![0]);
    // Half: all across, half down
    assert_eq!(pick(&annotations, [0., 0.], [300., 150.]), vec![0, 1]);
    // Half: half across, all down
    assert_eq!(pick(&annotations, [0., 0.], [250., 200.]), vec![0, 1]);
    // All: both arrows entirely in the box
    assert_eq!(pick(&annotations, [0., 0.], [400., 300.]), vec![0, 1]);

    // A flat arrow has no height, so it is either in or out vertically
    // Covers 20,50 to 120,50
    let flat = [arrow_at([20, 50], [100, 0])];
    // Half of its length, within the box vertically
    assert_eq!(pick(&flat, [0., 0.], [70., 100.]), vec![0]);
    // Slightly less than half of its length
    assert!(pick(&flat, [0., 0.], [69., 100.]).is_empty());
    // All of its length, but the box is below it
    assert!(pick(&flat, [0., 60.], [200., 100.]).is_empty());

    // The same for an upright arrow, which has no width
    // Covers 50,20 to 50,120
    let upright = [arrow_at([50, 20], [0, 100])];
    // Half of its length, within the box horizontally
    assert_eq!(pick(&upright, [0., 0.], [100., 70.]), vec![0]);
    // Slightly less than half of its length
    assert!(pick(&upright, [0., 0.], [100., 69.]).is_empty());
    // All of its length, but the box is to the right of it
    assert!(pick(&upright, [60., 0.], [100., 200.]).is_empty());

    // The corners of the box can be given in any order
    assert_eq!(pick(&annotations, [400., 300.], [0., 0.]), vec![0, 1]);

    // Hidden annotations are left out
    annotations[1].hidden = true;
    assert_eq!(pick(&annotations, [0., 0.], [400., 300.]), vec![0]);
    annotations[1].hidden = false;

    // The box is in the edited image, here cropped a quarter off the left
    let crop = [item(ImageOperation::Crop([2500, 0, 0, 0]))];
    let cropped = Mapping::new(&crop, (400, 300));
    assert_eq!(
        in_box(&annotations, &cropped, [90., 90.], [210., 210.]),
        vec![1]
    );
}

#[test]
fn annotations_keep_to_their_corner_on_other_sizes() {
    let mut a = arrow_at([350, 250], [30, 20]);
    a.reanchor((400, 300));
    assert_eq!(a.anchor, Anchor::BottomRight);
    // 50 from the right, 50 from the bottom on the smaller image too
    assert_eq!(a.start((200, 150)), [150, 100]);
    // the centre of the image is in the middle third
    let mut b = arrow_at([190, 140], [20, 20]);
    b.reanchor((400, 300));
    assert_eq!(b.anchor, Anchor::Centre);
    assert_eq!(b.start((400, 300)), [190, 140]);
}

#[test]
fn drawn_a_change_at_a_time_is_drawn_at_once() {
    let img = gradient(320, 240);
    let mapping = Mapping::new(&[], (320, 240));
    let mut annotations = vec![
        Annotation::new(
            Shape::Redact {
                size: [80, 60],
                style: Redaction::Pixelate,
            },
            [0, 0, 0, 255],
            12,
            [40, 40],
        ),
        arrow_at([20, 200], [150, -120]),
        Annotation::new(Shape::Rect([90, 50]), [0, 255, 0, 160], 5, [150, 30]),
        Annotation::new(
            Shape::Text("Hello\nthere".into()),
            [255, 255, 255, 255],
            24,
            [60, 150],
        ),
        Annotation::new(
            Shape::Pen(vec![[0, 0], [30, 10], [60, 40]]),
            [0, 0, 255, 255],
            4,
            [200, 150],
        ),
    ];
    let mut img_now = img.clone();
    let mut layer = AnnotationLayer::default();
    layer.update(&mut img_now, &annotations, &mapping);
    assert_eq!(img_now, drawn_at_once(&img, &annotations, &mapping));
    assert!(
        layer.update(&mut img_now, &annotations, &mapping).is_none(),
        "nothing changed"
    );

    type Change = Box<dyn Fn(&mut Vec<Annotation>)>;
    let changes: Vec<Change> = vec![
        // the arrow over the redaction moves
        Box::new(|a| a[1].offset = [60, 180]),
        // the redaction below the arrow goes
        Box::new(|a| {
            a.remove(0);
        }),
        Box::new(|a| a[2].shape = Shape::Text("Changed".into())),
        Box::new(|a| a[1].color = [10, 10, 10, 90]),
        Box::new(|a| a.push(arrow_at([300, 230], [-200, -200]))),
        Box::new(|a| {
            a.push(Annotation::new(
                Shape::Ellipse([70, 50]),
                [255, 255, 0, 200],
                7,
                [100, 100],
            ))
        }),
        // to the front
        Box::new(|a| {
            let first = a.remove(0);
            a.push(first);
        }),
        // outlines, round corners and a font
        Box::new(|a| a[1].outline = 4),
        Box::new(|a| {
            a[0].radius = 20;
            a[0].outline = 3;
        }),
        Box::new(|a| {
            a[5].outline = 6;
            a[5].outline_color = [255, 0, 255, 255];
        }),
        Box::new(|a| a[1].offset = [10, 10]),
        Box::new(|a| a[1].font.weight = 700),
        Box::new(|a| a[1].outline = 0),
        // a stamp, another symbol, larger, with an outline
        Box::new(|a| {
            a.push(Annotation::new(
                Shape::Stamp(Stamp::Check),
                [255, 0, 255, 255],
                50,
                [230, 20],
            ))
        }),
        Box::new(|a| a[6].shape = Shape::Stamp(Stamp::Warning)),
        Box::new(|a| a[6].width = 80),
        Box::new(|a| a[6].outline = 5),
        // a line, moved, then with an outline
        Box::new(|a| {
            a.push(Annotation::new(
                Shape::Line([-120, 60]),
                [0, 200, 255, 255],
                5,
                [260, 60],
            ))
        }),
        Box::new(|a| a[7].offset = [200, 100]),
        Box::new(|a| a[7].outline = 3),
    ];
    for (i, change) in changes.iter().enumerate() {
        change(&mut annotations);
        let area = layer.update(&mut img_now, &annotations, &mapping);
        assert!(area.is_some(), "change {i} changed nothing");
        let expected = drawn_at_once(&img, &annotations, &mapping);
        let wrong = img_now
            .pixels()
            .zip(expected.pixels())
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(
            wrong, 0,
            "after change {i}, {wrong} pixels differ from drawing at once"
        );
    }
    // all gone, the image as it was
    layer.update(&mut img_now, &[], &mapping);
    assert_eq!(img_now, img);
}

/// Drawn on an image without alpha, the annotations give it the colors they give
/// it with alpha, also changed one at a time
#[test]
fn drawn_without_alpha() {
    let with = gradient(320, 240);
    let mut without = image::DynamicImage::ImageRgba8(with.clone()).to_rgb8();
    let mapping = Mapping::new(&[], (320, 240));
    let mut annotations = vec![
        Annotation::new(
            Shape::Redact {
                size: [80, 60],
                style: Redaction::Pixelate,
            },
            [0, 0, 0, 255],
            12,
            [40, 40],
        ),
        Annotation::new(
            Shape::Redact {
                size: [30, 30],
                style: Redaction::Fill,
            },
            [200, 10, 10, 255],
            1,
            [250, 10],
        ),
        arrow_at([20, 200], [150, -120]),
        Annotation::new(Shape::Rect([90, 50]), [0, 255, 0, 160], 5, [150, 30]),
        Annotation::new(
            Shape::Text("Hello\nthere".into()),
            [255, 255, 255, 255],
            24,
            [60, 150],
        ),
        Annotation::new(Shape::Ellipse([70, 50]), [255, 255, 0, 200], 7, [100, 100]),
    ];
    annotations[3].outline = 4;
    let mut layer = AnnotationLayer::default();
    layer.update(&mut without, &annotations, &mapping);
    let same = |without: &image::RgbImage, annotations: &[Annotation]| {
        let expected = drawn_at_once(&with, annotations, &mapping);
        image::DynamicImage::ImageRgba8(expected).to_rgb8() == *without
    };
    assert!(same(&without, &annotations));
    // the arrow over the redaction moves, the frame goes
    annotations[2].offset = [60, 180];
    layer.update(&mut without, &annotations, &mapping);
    assert!(same(&without, &annotations), "moved");
    annotations.remove(3);
    layer.update(&mut without, &annotations, &mapping);
    assert!(same(&without, &annotations), "removed");
    layer.update(&mut without, &[], &mapping);
    assert!(image::DynamicImage::ImageRgba8(with.clone()).to_rgb8() == without);
}

#[test]
fn redactions_hide() {
    let img = gradient(100, 100);
    let mapping = Mapping::new(&[], (100, 100));
    let redact = |style| {
        Annotation::new(
            Shape::Redact {
                size: [40, 40],
                style,
            },
            [0, 0, 0, 255],
            10,
            [20, 20],
        )
    };
    let pixelated = drawn_at_once(&img, &[redact(Redaction::Pixelate)], &mapping);
    // one color in each block
    for (bx, by) in [(20, 20), (30, 50), (50, 30)] {
        let first = pixelated.get_pixel(bx, by);
        for y in by..by + 10 {
            for x in bx..bx + 10 {
                assert_eq!(pixelated.get_pixel(x, y), first, "at {x},{y}");
            }
        }
    }
    assert_eq!(
        pixelated.get_pixel(10, 10),
        img.get_pixel(10, 10),
        "outside is as it was"
    );
    let filled = drawn_at_once(&img, &[redact(Redaction::Fill)], &mapping);
    assert!((20..60).all(|y| (20..60).all(|x| filled.get_pixel(x, y).0 == [0, 0, 0, 255])));
}

#[test]
fn text_is_drawn_inside_its_box() {
    let hello = Annotation::new(Shape::Text("Hello".into()), [255; 4], 40, [0, 0]);
    let layout = text_layout("Hello", &hello).expect("Inter lays out text");
    let [l, t, r, b] = layout.bounds;
    assert!(
        r - l > 60. && r - l < 140.,
        "5 letters at 40 px are {} wide",
        r - l
    );
    assert!(
        b - t > 30. && b - t < 70.,
        "a line at 40 px is {} high",
        b - t
    );
    let img = RgbaImage::from_pixel(300, 100, Rgba([0, 0, 0, 255]));
    let text = Annotation::new(
        Shape::Text("Hello".into()),
        [255, 255, 255, 255],
        40,
        [10, 10],
    );
    let drawn = drawn_at_once(&img, &[text], &Mapping::new(&[], (300, 100)));
    let lit = drawn.pixels().filter(|p| p[0] > 128).count();
    assert!(lit > 300, "only {lit} pixels of the text are light");
    let outside = drawn
        .enumerate_pixels()
        .filter(|(x, y, p)| {
            p[0] > 0
                && !(*x as f32 >= 10. + l - 2.
                    && *x as f32 <= 10. + r + 2.
                    && *y as f32 >= 10. + t - 2.
                    && *y as f32 <= 10. + b + 2.)
        })
        .count();
    assert_eq!(outside, 0, "the text draws outside its box");
}

#[test]
fn text_reads_the_right_way_on_a_turned_image() {
    let ops = [
        item(ImageOperation::Rotate(90)),
        item(ImageOperation::Flip(false)),
    ];
    let mapping = Mapping::new(&ops, (200, 100));
    let mut text = Annotation::new(Shape::Text("A".into()), [0; 4], 20, [50, 50]);
    text.stand_upright(&mapping);
    let t = text.transform(&mapping);
    assert!(
        t.sx > 0.99 && t.sy > 0.99 && t.kx.abs() < 0.01 && t.ky.abs() < 0.01,
        "{t:?}"
    );
}

#[test]
fn transparent_pixels_get_the_color_of_the_annotation() {
    let img = RgbaImage::new(60, 60);
    let rect = Annotation::new(Shape::Rect([40, 40]), [255, 0, 0, 255], 6, [10, 10]);
    let drawn = drawn_at_once(&img, &[rect], &Mapping::new(&[], (60, 60)));
    assert_eq!(drawn.get_pixel(10, 30).0, [255, 0, 0, 255]);
    assert_eq!(
        drawn.get_pixel(30, 30).0,
        [0, 0, 0, 0],
        "inside the frame is as it was"
    );
}

#[test]
fn clicks_hit_what_is_drawn() {
    let mapping = Mapping::new(&[], (300, 300));
    let arrow = arrow_at([50, 50], [100, 0]);
    assert!(hits(&arrow, &mapping, [100., 52.], 2.));
    assert!(!hits(&arrow, &mapping, [100., 80.], 2.));
    let frame = Annotation::new(Shape::Rect([100, 100]), [0; 4], 4, [100, 100]);
    assert!(hits(&frame, &mapping, [101., 150.], 2.), "on the frame");
    assert!(!hits(&frame, &mapping, [150., 150.], 2.), "inside it");
    // the head is hit to the ends of its lines, not past its tip
    let head = arrow_head_corners([100, 0], 6.).expect("a head");
    assert!(hits(
        &arrow,
        &mapping,
        [head[1].x + 50.5, head[1].y + 50.],
        1.
    ));
    assert!(!hits(&arrow, &mapping, [153., 50.], 1.));
    let text = Annotation::new(Shape::Text("Hi".into()), [0; 4], 30, [20, 200]);
    assert!(hits(&text, &mapping, [30., 220.], 0.));
}

/// A line is an arrow without its head, drawn, hit and saved as one
#[test]
fn a_line_is_an_arrow_without_a_head() {
    let img = RgbaImage::from_pixel(200, 100, Rgba([0, 0, 0, 255]));
    let mapping = Mapping::new(&[], (200, 100));
    let red = [255, 0, 0, 255];
    let line = Annotation::new(Shape::Line([100, 0]), red, 6, [50, 50]);
    let arrow = Annotation::new(Shape::Arrow([100, 0]), red, 6, [50, 50]);
    let drawn = drawn_at_once(&img, std::slice::from_ref(&line), &mapping);
    let with_head = drawn_at_once(&img, std::slice::from_ref(&arrow), &mapping);
    for x in [50, 100, 149] {
        assert_eq!(drawn.get_pixel(x, 50).0, red, "no line at {x},50");
    }
    // halfway along a line of the head, where the line has nothing
    let [tip, left, _] = arrow_head_corners([100, 0], 6.).expect("a head");
    let (x, y) = (50. + (tip.x + left.x) / 2., 50. + (tip.y + left.y) / 2.);
    assert_eq!(
        with_head.get_pixel(x as u32, y as u32).0,
        red,
        "the arrow has a head"
    );
    assert_eq!(
        drawn.get_pixel(x as u32, y as u32).0,
        [0, 0, 0, 255],
        "the line has none"
    );
    assert!(hits(&line, &mapping, [100., 52.], 2.));
    assert!(!hits(&line, &mapping, [100., 80.], 2.));
    assert!(!hits(&line, &mapping, [x, y], 1.), "no head to hit");
    assert!(!hits(&line, &mapping, [160., 50.], 2.), "not past its end");
    assert_eq!(line.label(), "Line");
    assert!(Annotation::new(Shape::Line([1, -1]), red, 6, [50, 50]).is_empty());
    assert_eq!(
        serde_json::to_string(&line.shape).unwrap(),
        r#"{"Line":[100,0]}"#
    );
    let op = ImageOperation::Annotate(vec![line]);
    let json = serde_json::to_string(&op).expect("saved");
    let back: ImageOperation = serde_json::from_str(&json).expect("read");
    assert_eq!(op, back);
}

#[test]
fn circles_are_drawn_and_hit() {
    let img = RgbaImage::from_pixel(100, 100, Rgba([0, 0, 0, 255]));
    let mapping = Mapping::new(&[], (100, 100));
    let circle = Annotation::new(Shape::Ellipse([60, 40]), [255, 0, 0, 255], 4, [20, 30]);
    let drawn = drawn_at_once(&img, std::slice::from_ref(&circle), &mapping);
    for (x, y) in [(20, 50), (79, 50), (50, 30), (50, 69)] {
        assert!(drawn.get_pixel(x, y)[0] > 200, "no line at {x},{y}");
    }
    assert_eq!(drawn.get_pixel(50, 50).0, [0, 0, 0, 255], "inside it");
    assert_eq!(
        drawn.get_pixel(22, 32).0,
        [0, 0, 0, 255],
        "in the corner of its box"
    );
    assert!(hits(&circle, &mapping, [21., 50.], 2.));
    assert!(hits(&circle, &mapping, [50., 31.], 2.));
    assert!(!hits(&circle, &mapping, [50., 50.], 2.), "inside it");
    assert!(
        !hits(&circle, &mapping, [22., 32.], 2.),
        "in the corner of its box"
    );
    for (rx, ry, dx, dy, distance) in [
        (10., 10., 13., 0., 3.),
        (10., 10., 3., -4., 5.),
        (40., 4., 0., 0., 4.),
        (40., 4., 45., 0., 5.),
        // nearer above than at the end
        (40., 4., -30., 0., 2.63),
    ] {
        let d = ellipse_distance(rx, ry, dx, dy);
        assert!(
            (d - distance).abs() < 0.05,
            "{dx},{dy} is {d} from an ellipse of {rx} by {ry}, not {distance}"
        );
    }
}

#[test]
fn saved_with_the_edits() {
    let mut dressed = Annotation::new(Shape::Text("Hi".into()), [1, 2, 3, 4], 5, [6, 7]);
    dressed.outline = 3;
    dressed.outline_color = [9, 8, 7, 6];
    dressed.radius = 4;
    dressed.font = Font {
        family: "Some 'font'".into(),
        weight: 700,
    };
    dressed.hidden = true;
    let op = ImageOperation::Annotate(vec![
        arrow_at([1, 2], [3, 4]),
        Annotation::new(Shape::Ellipse([8, 9]), [1, 2, 3, 4], 5, [6, 7]),
        Annotation::new(Shape::Text("ü & <b>".into()), [1, 2, 3, 4], 5, [6, 7]),
        dressed,
    ]);
    let json = serde_json::to_string(&op).expect("saved");
    let back: ImageOperation = serde_json::from_str(&json).expect("read");
    assert_eq!(op, back);
}

#[test]
fn edits_saved_before_outlines_still_load() {
    let op = ImageOperation::Annotate(vec![arrow_at([1, 2], [3, 4])]);
    let mut json = serde_json::to_value(&op).expect("saved");
    let annotation = json["Annotate"][0].as_object_mut().expect("an annotation");
    for newer in [
        "rotation",
        "flip",
        "outline",
        "outline_color",
        "radius",
        "font",
        "hidden",
        "number_color",
        "shadow",
        "shadow_size",
    ] {
        assert!(annotation.remove(newer).is_some(), "{newer} is saved");
    }
    let back: ImageOperation = serde_json::from_value(json).expect("read");
    assert_eq!(op, back);
}

#[test]
fn outlines_go_around_what_is_drawn() {
    let mapping = Mapping::new(&[], (200, 200));
    let img = RgbaImage::from_pixel(200, 200, Rgba([0, 0, 0, 255]));
    let (red, white, black) = ([255, 0, 0, 255], [255; 4], [0, 0, 0, 255]);
    let mut frame = Annotation::new(Shape::Rect([100, 60]), red, 6, [50, 50]);
    frame.outline = 4;
    let drawn = drawn_at_once(&img, &[frame], &mapping);
    // the line is 47 to 53, its outline 4 more on both sides
    assert_eq!(drawn.get_pixel(100, 50).0, red, "on the line");
    assert_eq!(drawn.get_pixel(100, 45).0, white, "the outline outside");
    assert_eq!(drawn.get_pixel(100, 55).0, white, "the outline inside");
    assert_eq!(drawn.get_pixel(100, 41).0, black, "past the outline");

    let mut arrow = arrow_at([20, 100], [160, 0]);
    arrow.outline = 3;
    let drawn = drawn_at_once(&img, &[arrow], &mapping);
    assert_eq!(drawn.get_pixel(100, 100).0, red, "on the line");
    assert_eq!(drawn.get_pixel(100, 95).0, white, "the outline of the line");
    assert_eq!(drawn.get_pixel(100, 92).0, black, "past it");
    assert_eq!(drawn.get_pixel(181, 100).0, white, "the outline at the tip");
    assert_eq!(drawn.get_pixel(185, 100).0, black, "past the tip");

    let mut text = Annotation::new(Shape::Text("Hi".into()), red, 40, [20, 20]);
    let plain = drawn_at_once(&img, &[text.clone()], &mapping);
    text.outline = 3;
    let outlined = drawn_at_once(&img, &[text.clone()], &mapping);
    let count = |img: &RgbaImage, color: [u8; 4]| img.pixels().filter(|p| p.0 == color).count();
    assert_eq!(count(&plain, white), 0);
    assert!(count(&outlined, white) > 200, "the letters have an outline");
    assert!(count(&outlined, red) > 200, "and are red inside");
    assert_eq!(
        text.bounds(),
        {
            text.outline = 0;
            text.bounds()
        },
        "the box of the text stays"
    );
}

#[test]
fn outlines_are_not_cut_off() {
    let mapping = Mapping::new(&[], (300, 300));
    let img = RgbaImage::from_pixel(300, 300, Rgba([0, 0, 0, 255]));
    let whole = Rect {
        x: 0,
        y: 0,
        width: 300,
        height: 300,
    };
    let shapes = [
        Shape::Arrow([120, -60]),
        Shape::Line([-90, 70]),
        Shape::Rect([80, 50]),
        Shape::Ellipse([80, 50]),
        Shape::Pen(vec![[0, 0], [40, 30], [90, -10]]),
        Shape::Pen(vec![[0, 0]]),
        Shape::Text("Hi".into()),
        Shape::Stamp(Stamp::Warning),
    ];
    for shape in shapes {
        // as large as a stamp is, the width of a line for the others
        let width = if matches!(shape, Shape::Stamp(_)) {
            60
        } else {
            6
        };
        let mut a = Annotation::new(shape, [255, 0, 0, 255], width, [100, 100]);
        a.outline = 5;
        a.radius = 10;
        let t = a.transform(&mapping);
        let area = area(&a, t, (300, 300)).expect("an area");
        let mut everywhere = img.clone();
        draw(&mut everywhere, &a, t, whole);
        for (x, y, p) in everywhere.enumerate_pixels() {
            let inside =
                x >= area.x && y >= area.y && x < area.x + area.width && y < area.y + area.height;
            assert!(
                inside || p.0 == [0, 0, 0, 255],
                "{:?} draws at {x},{y}, out of {area:?}",
                a.shape
            );
        }
    }
}

#[test]
fn frames_get_round_corners() {
    let mapping = Mapping::new(&[], (200, 200));
    let img = RgbaImage::from_pixel(200, 200, Rgba([0, 0, 0, 255]));
    let red = [255, 0, 0, 255];
    let mut frame = Annotation::new(Shape::Rect([100, 60]), red, 4, [50, 50]);
    frame.radius = 20;
    let drawn = drawn_at_once(&img, &[frame.clone()], &mapping);
    assert_eq!(
        drawn.get_pixel(50, 50).0,
        [0, 0, 0, 255],
        "the corner is round"
    );
    // halfway round the corner around 70,70
    assert_eq!(drawn.get_pixel(55, 55).0, red, "on the curve");
    assert_eq!(drawn.get_pixel(100, 50).0, red, "on the straight part");
    // a click hits where it is drawn
    assert!(touches(&frame, Point::from_xy(5.86, 5.86), 0.));
    assert!(!touches(&frame, Point::zero(), 1.));
    // no rounder than half of the shorter side
    frame.radius = 500;
    assert_eq!(frame.corner_radius(), 30.);
}

#[test]
fn text_in_its_font_and_weight() {
    let mut text = Annotation::new(Shape::Text("Hello".into()), [255; 4], 40, [0, 0]);
    let regular = text.bounds().expect("laid out");
    text.font.weight = 700;
    let bold = text.bounds().expect("laid out");
    assert!(
        bold[2] - bold[0] > regular[2] - regular[0] + 2.,
        "bold is wider: {regular:?} {bold:?}"
    );
    // Inter, when the font is not there
    text.font = Font {
        family: "No such font".into(),
        weight: 400,
    };
    assert_eq!(text.bounds(), Some(regular));
    // Inter first, with its two weights
    let families = font_families();
    assert_eq!(
        families.first(),
        Some(&("Inter".to_string(), vec![400, 700]))
    );
}

#[test]
fn every_stamp_fills_the_middle_of_its_square() {
    for stamp in Stamp::ALL {
        let path = stamp_path(stamp, 100.).unwrap_or_else(|| panic!("{stamp:?} has no glyph"));
        let b = path.bounds();
        let (w, h) = (b.width(), b.height());
        assert!((w.max(h) - 100.).abs() < 0.5, "{stamp:?} is {w}x{h}");
        assert!(w.min(h) > 30., "{stamp:?} is {w}x{h}");
        let middle = ((b.left() + b.right()) / 2., (b.top() + b.bottom()) / 2.);
        assert!(
            (middle.0 - 50.).abs() < 0.5 && (middle.1 - 50.).abs() < 0.5,
            "{stamp:?} is around {middle:?}"
        );
    }
}

#[test]
fn stamps_are_drawn_in_their_square_with_their_outline() {
    let img = RgbaImage::from_pixel(100, 100, Rgba([255; 4]));
    let mapping = Mapping::new(&[], (100, 100));
    let mut stamp = Annotation::new(Shape::Stamp(Stamp::Warning), [255, 0, 0, 255], 60, [20, 20]);
    let count = |drawn: &RgbaImage, color: [u8; 4], inside: [u32; 2]| {
        let mut count = [0, 0];
        for (x, y, p) in drawn.enumerate_pixels() {
            if p.0 == color {
                let within =
                    (inside[0]..inside[1]).contains(&x) && (inside[0]..inside[1]).contains(&y);
                count[within as usize] += 1;
            }
        }
        count
    };
    let drawn = drawn_at_once(&img, std::slice::from_ref(&stamp), &mapping);
    let [outside, inside] = count(&drawn, [255, 0, 0, 255], [20, 80]);
    assert!(inside > 800, "only {inside} pixels in its color");
    assert_eq!(outside, 0, "pixels in its color outside of its square");
    let changed = drawn
        .enumerate_pixels()
        .filter(|(x, y, p)| p.0 != [255; 4] && !((19..81).contains(x) && (19..81).contains(y)));
    assert_eq!(changed.count(), 0, "changed outside of its square");
    // a black line 4 pixels wide around it
    stamp.outline = 4;
    stamp.outline_color = [0, 0, 0, 255];
    let outlined = drawn_at_once(&img, &[stamp], &mapping);
    let [outside, inside] = count(&outlined, [0, 0, 0, 255], [16, 84]);
    assert!(inside > 400, "only {inside} pixels of its outline");
    assert_eq!(outside, 0, "its outline reaches out too far");
    let [_, red] = count(&outlined, [255, 0, 0, 255], [20, 80]);
    assert!(
        red > 400,
        "the outline covers the stamp, {red} pixels in its color are left"
    );
}

#[test]
fn stamps_are_saved_by_name() {
    let shape = Shape::Stamp(Stamp::Warning);
    assert_eq!(
        serde_json::to_string(&shape).unwrap(),
        r#"{"Stamp":"Warning"}"#
    );
    let op = ImageOperation::Annotate(vec![Annotation::new(shape, [1, 2, 3, 4], 50, [6, 7])]);
    let json = serde_json::to_string(&op).expect("saved");
    let back: ImageOperation = serde_json::from_str(&json).expect("read");
    assert_eq!(op, back);
}

#[test]
fn stamps_are_hit_in_their_square_and_stand_upright() {
    let mapping = Mapping::new(&[], (300, 300));
    let stamp = Annotation::new(Shape::Stamp(Stamp::Check), [0; 4], 40, [100, 100]);
    assert!(hits(&stamp, &mapping, [120., 120.], 0.), "the middle");
    assert!(
        hits(&stamp, &mapping, [101., 139.], 0.),
        "a corner, where a check has no line"
    );
    assert!(!hits(&stamp, &mapping, [145., 120.], 2.), "besides it");
    // on a turned image it reads the right way, its middle where it was put
    let ops = [item(ImageOperation::Rotate(90))];
    let turned = Mapping::new(&ops, (200, 100));
    let mut stamp = Annotation::new(Shape::Stamp(Stamp::Check), [0; 4], 40, [0, 0]);
    stamp.stand_upright(&turned);
    stamp.centre_at([50., 30.], turned.original);
    let t = stamp.transform(&turned);
    assert!(
        t.sx > 0.99 && t.sy > 0.99 && t.kx.abs() < 0.01 && t.ky.abs() < 0.01,
        "{t:?}"
    );
    let b = stamp.bounds().unwrap();
    let mut middle = Point::from_xy((b[0] + b[2]) / 2., (b[1] + b[3]) / 2.);
    stamp.placement(turned.original).map_point(&mut middle);
    assert!(
        (middle.x - 50.).abs() < 1. && (middle.y - 30.).abs() < 1.,
        "its middle is at {middle:?}"
    );
}

#[test]
fn turned_four_times_by_a_quarter_is_as_before() {
    let mapping = Mapping::new(&[], (300, 300));
    let frame = Annotation::new(Shape::Rect([40, 20]), [0; 4], 4, [100, 100]);
    let bounds = selection_bounds(std::slice::from_ref(&frame), &[0], &mapping).unwrap();
    let centre = middle_of(bounds.shape);
    assert_eq!(centre, [120., 110.]);
    let mut turned = frame.clone();
    for quarter in 1..=4 {
        turned = rotated(&turned, &mapping, centre, 90);
        let middle = middle_in(&turned, &mapping).unwrap();
        assert!(
            (middle[0] - 120.).abs() < 1. && (middle[1] - 110.).abs() < 1.,
            "turned around its middle, {quarter}: {middle:?}"
        );
    }
    assert_eq!(turned.rotation, 0);
    assert_eq!(
        turned.start(mapping.original),
        frame.start(mapping.original)
    );
    // the box is upright after a quarter
    let quarter = rotated(&frame, &mapping, centre, 90);
    let shape = selection_bounds(&[quarter], &[0], &mapping).unwrap().shape;
    assert!(
        (shape[2] - shape[0] - 20.).abs() < 0.01 && (shape[3] - shape[1] - 40.).abs() < 0.01,
        "{shape:?}"
    );
}

#[test]
fn flipped_twice_is_as_before() {
    let mapping = Mapping::new(&[], (300, 300));
    let mut arrow = arrow_at([100, 100], [40, 10]);
    arrow.rotation = 30;
    let centre = [150., 150.];
    for axis in 0..2 {
        let once = flipped(&arrow, &mapping, centre, axis);
        assert_ne!(once.flip, arrow.flip);
        let twice = flipped(&once, &mapping, centre, axis);
        assert_eq!(twice.rotation, 30);
        assert_eq!(twice.flip, [false; 2]);
        let (a, b) = (twice.start(mapping.original), arrow.start(mapping.original));
        assert!(
            (a[0] - b[0]).abs() <= 1 && (a[1] - b[1]).abs() <= 1,
            "{a:?} {b:?}"
        );
    }
    // left to right: the tip goes to the other side of the middle
    let mirrored = flipped(&arrow_at([100, 100], [40, 0]), &mapping, [150., 100.], 0);
    let mut tip = Point::from_xy(40., 0.);
    mirrored.transform(&mapping).map_point(&mut tip);
    assert!(
        (tip.x - 160.).abs() < 1. && (tip.y - 100.).abs() < 1.,
        "{tip:?}"
    );
}

#[test]
fn flipped_on_a_turned_image_as_it_is_seen() {
    // on an image turned clockwise an arrow to the right points down
    let ops = [item(ImageOperation::Rotate(90))];
    let mapping = Mapping::new(&ops, (200, 100));
    let arrow = arrow_at([50, 50], [40, 0]);
    let middle = middle_in(&arrow, &mapping).unwrap();
    // left to right, it still points down
    let mirrored = flipped(&arrow, &mapping, middle, 0);
    let t = mirrored.transform(&mapping);
    assert!(t.sx.abs() < 0.01 && t.ky > 0.99, "{t:?}");
    // top to bottom it points up
    let mirrored = flipped(&arrow, &mapping, middle, 1);
    let t = mirrored.transform(&mapping);
    assert!(t.sx.abs() < 0.01 && t.ky < -0.99, "{t:?}");
}

#[test]
fn turned_on_a_mirrored_image_as_it_is_seen() {
    let ops = [item(ImageOperation::Flip(false))];
    let mapping = Mapping::new(&ops, (200, 100));
    let arrow = arrow_at([50, 50], [40, 0]);
    let middle = middle_in(&arrow, &mapping).unwrap();
    assert!(shown_angle(&arrow, &mapping).abs() < 0.01);
    let turned = rotated(&arrow, &mapping, middle, 30);
    assert!(
        (shown_angle(&turned, &mapping) - 30.).abs() < 0.5,
        "{}",
        shown_angle(&turned, &mapping)
    );
}

#[test]
fn stretched_by_the_handles() {
    let mapping = Mapping::new(&[], (400, 400));
    let frame = Annotation::new(Shape::Rect([40, 20]), [0; 4], 4, [100, 100]);
    let shape = [100., 100., 140., 120.];
    // the bottom right corner twice as far, the top left stays
    let (scale, fixed) = stretch([1, 1], shape, [40., 20.], false);
    assert_eq!((scale, fixed), ([2., 2.], [100., 100.]));
    let larger = resized(&frame, &mapping, fixed, scale, [0., 0.]);
    assert_eq!(larger.shape, Shape::Rect([80, 40]));
    assert_eq!(larger.start(mapping.original), [100, 100]);
    assert_eq!(larger.width, 4, "lines keep their width");
    // the left side, the right one stays
    let (scale, fixed) = stretch([-1, 0], shape, [-40., 30.], false);
    assert_eq!((scale, fixed), ([2., 1.], [140., 110.]));
    let wider = resized(&frame, &mapping, fixed, scale, [0., 0.]);
    assert_eq!(wider.shape, Shape::Rect([80, 20]));
    assert_eq!(wider.start(mapping.original), [60, 100]);
    // with Shift it keeps its proportions, the corner along the diagonal
    let (scale, _) = stretch([1, 1], shape, [40., 0.], true);
    assert!(
        (scale[0] - 1.8).abs() < 0.001 && scale[0] == scale[1],
        "{scale:?}"
    );
    // from a side as much across, around the middle
    let (scale, fixed) = stretch([0, 1], shape, [0., 20.], true);
    assert_eq!((scale, fixed), ([2., 2.], [120., 100.]));
    // past the opposite side it is mirrored
    let (scale, fixed) = stretch([1, 0], shape, [-100., 0.], false);
    assert_eq!((scale, fixed), ([-1.5, 1.], [100., 110.]));
    // never flat
    let (scale, _) = stretch([1, 0], shape, [-40., 0.], false);
    assert!(scale[0] != 0. && scale[1] == 1., "{scale:?}");
    // from a side with Shift it is mirrored only along it
    let (scale, _) = stretch([0, 1], shape, [0., -60.], true);
    assert_eq!(scale, [2., -2.]);
}

#[test]
fn mirrored_when_dragged_past_the_other_side() {
    let mapping = Mapping::new(&[], (400, 400));
    let arrow = arrow_at([100, 100], [40, 20]);
    let shape = [100., 100., 140., 120.];
    // the right side dragged 60 to the left: 20 wide on the other side of 100
    let (scale, fixed) = stretch([1, 0], shape, [-60., 0.], false);
    let mut list = vec![resized(&arrow, &mapping, fixed, scale, [0., 0.])];
    keep_in_place(&mut list, &[0], &mapping, [1, 0], fixed, scale);
    let after = selection_bounds(&list, &[0], &mapping).unwrap().shape;
    assert!(
        (after[0] - 80.).abs() < 0.01 && (after[2] - 100.).abs() < 0.01,
        "{after:?}"
    );
    assert!((after[1] - 100.).abs() < 0.01 && (after[3] - 120.).abs() < 0.01);
    // it points the other way, to the bottom left
    let Shape::Arrow(to) = list[0].shape else {
        panic!("still an arrow")
    };
    let mut tip = Point::from_xy(to[0] as f32, to[1] as f32);
    list[0].transform(&mapping).map_point(&mut tip);
    assert!(
        (tip.x - 80.).abs() < 1. && (tip.y - 120.).abs() < 1.,
        "{tip:?}"
    );
}

/// Where the ends of a line or an arrow are in the edited image
fn ends_of(a: &Annotation, mapping: &Mapping) -> [[f32; 2]; 2] {
    let (Shape::Line(to) | Shape::Arrow(to)) = a.shape else {
        panic!("{:?} has no ends", a.shape)
    };
    let mut ends = [Point::zero(), Point::from_xy(to[0] as f32, to[1] as f32)];
    a.transform(mapping).map_points(&mut ends);
    ends.map(|p| [p.x, p.y])
}

fn near(a: [[f32; 2]; 2], b: [[f32; 2]; 2]) -> bool {
    a.iter()
        .flatten()
        .zip(b.iter().flatten())
        .all(|(a, b)| (a - b).abs() <= 1.)
}

/// A level line has a box with no height: a corner could only make it longer. It tilts it,
/// the end at that corner follows the mouse, the other stays, also past the line.
#[test]
fn a_level_line_is_tilted_by_a_corner() {
    let mapping = Mapping::new(&[], (400, 400));
    let line = Annotation::new(Shape::Line([80, 0]), [0; 4], 4, [100, 100]);
    let shape = selection_bounds(std::slice::from_ref(&line), &[0], &mapping)
        .unwrap()
        .shape;
    assert_eq!(shape, [100., 100., 180., 100.]);
    for (side, by, ends) in [
        // the top right corner up and to the right
        ([1, -1], [20., -30.], [[100., 100.], [200., 70.]]),
        // and down, past the line
        ([1, -1], [0., 30.], [[100., 100.], [180., 130.]]),
        // the bottom left corner down and to the left, the right end stays
        ([-1, 1], [-10., 40.], [[90., 140.], [180., 100.]]),
        // the middle of a side only makes it longer
        ([1, 0], [20., 15.], [[100., 100.], [200., 100.]]),
    ] {
        let (scale, fixed) = stretch(side, shape, by, false);
        let tilt = corner_tilt(side, shape, by, false);
        let mut list = vec![resized(&line, &mapping, fixed, scale, tilt)];
        keep_in_place(&mut list, &[0], &mapping, side, fixed, scale);
        let got = ends_of(&list[0], &mapping);
        assert!(near(got, ends), "{side:?} by {by:?}: {got:?}, not {ends:?}");
    }
    // with Shift it keeps its proportions, it stays level
    assert_eq!(corner_tilt([1, -1], shape, [20., -30.], true), [0., 0.]);
    // an upright one tilts left and right
    let upright = Annotation::new(Shape::Line([0, 80]), [0; 4], 4, [100, 100]);
    let shape = [100., 100., 100., 180.];
    let (scale, fixed) = stretch([1, 1], shape, [25., 0.], false);
    let tilt = corner_tilt([1, 1], shape, [25., 0.], false);
    let tilted = resized(&upright, &mapping, fixed, scale, tilt);
    let got = ends_of(&tilted, &mapping);
    assert!(near(got, [[100., 100.], [125., 180.]]), "{got:?}");
}

/// Stretched along its own sides, a turned line could only get longer or shorter. Its ends go
/// where the box takes them, it keeps its turn.
#[test]
fn a_turned_line_gets_steeper() {
    let mapping = Mapping::new(&[], (400, 400));
    let mut line = Annotation::new(Shape::Line([40, 0]), [0; 4], 4, [100, 100]);
    line.rotation = 45;
    let [start, end] = ends_of(&line, &mapping);
    let shape = [start[0], start[1], end[0], end[1]];
    // the bottom right corner twice as far down
    let (scale, fixed) = stretch([1, 1], shape, [0., end[1] - start[1]], false);
    let steeper = resized(&line, &mapping, fixed, scale, [0., 0.]);
    let want = [start, [end[0], start[1] + 2. * (end[1] - start[1])]];
    let got = ends_of(&steeper, &mapping);
    assert!(near(got, want), "{got:?}, not {want:?}");
    assert_eq!(steeper.rotation, 45, "it keeps its turn");
    // an arrow too, and its head points where its end went
    let arrow = Annotation::new(Shape::Arrow([40, 0]), [0; 4], 4, [100, 100]);
    let (scale, fixed) = stretch([1, 1], [100., 100., 140., 100.], [0., 30.], false);
    let tilt = corner_tilt([1, 1], [100., 100., 140., 100.], [0., 30.], false);
    let tilted = resized(&arrow, &mapping, fixed, scale, tilt);
    assert!(near(
        ends_of(&tilted, &mapping),
        [[100., 100.], [140., 130.]]
    ));
}

#[test]
fn what_stays_stays_where_it_is() {
    let mapping = Mapping::new(&[], (600, 600));
    let mut list = vec![
        Annotation::new(Shape::Rect([40, 20]), [0; 4], 4, [100, 100]),
        Annotation::new(Shape::Stamp(Stamp::Check), [0; 4], 30, [150, 100]),
        arrow_at([110, 160], [60, 10]),
    ];
    let before = list.clone();
    let shape = selection_bounds(&list, &[0, 1, 2], &mapping).unwrap().shape;
    for (side, by) in [
        ([1, 1], [37., 13.]),
        ([-1, -1], [-23., 7.]),
        ([1, 0], [55., 0.]),
        ([0, -1], [0., -31.]),
        ([-1, 1], [200., 170.]),
    ] {
        for keep in [false, true] {
            let (scale, fixed) = stretch(side, shape, by, keep);
            for i in 0..list.len() {
                list[i] = resized(&before[i], &mapping, fixed, scale, [0., 0.]);
            }
            keep_in_place(&mut list, &[0, 1, 2], &mapping, side, fixed, scale);
            let after = selection_bounds(&list, &[0, 1, 2], &mapping).unwrap().shape;
            for axis in 0..2 {
                let (lo, hi) = (after[axis], after[axis + 2]);
                let stays = match side[axis] {
                    0 => (lo + hi) / 2.,
                    s if (s < 0) == (scale[axis] > 0.) => hi,
                    _ => lo,
                };
                assert!(
                    (stays - fixed[axis]).abs() <= 1.,
                    "{side:?} by {by:?}, Shift {keep}: {stays} is not {}",
                    fixed[axis]
                );
            }
        }
    }
}

#[test]
fn texts_and_stamps_grow_as_they_are_stretched() {
    let mapping = Mapping::new(&[], (400, 400));
    let stamp = Annotation::new(Shape::Stamp(Stamp::Check), [0; 4], 40, [100, 100]);
    let larger = resized(&stamp, &mapping, [100., 100.], [1.5, 1.], [0., 0.]);
    assert_eq!(larger.width, 60, "as much as the side that stretches most");
    // a turned frame is stretched along its own sides
    let mut frame = Annotation::new(Shape::Rect([40, 20]), [0; 4], 4, [100, 100]);
    frame.rotation = 90;
    let wider = resized(&frame, &mapping, [0., 0.], [1., 2.], [0., 0.]);
    assert_eq!(wider.shape, Shape::Rect([80, 20]));
}

/// Flipped top to bottom was taken as turned by 180: then "Reset rotation" turned a flipped
/// arrow around
#[test]
fn a_flip_is_not_a_turn() {
    for ops in [
        vec![],
        vec![item(ImageOperation::Rotate(90))],
        vec![item(ImageOperation::Flip(false))],
    ] {
        let mapping = Mapping::new(&ops, (200, 200));
        let mut arrow = arrow_at([50, 50], [40, 0]);
        arrow.stand_upright(&mapping);
        assert!(
            shown_angle(&arrow, &mapping).abs() < 0.01,
            "{ops:?}: upright"
        );
        let middle = middle_in(&arrow, &mapping).unwrap();
        for axis in 0..2 {
            let mirrored = flipped(&arrow, &mapping, middle, axis);
            let angle = shown_angle(&mirrored, &mapping);
            assert!(angle.abs() < 0.01, "{ops:?}, axis {axis}: {angle}");
        }
        // turned, then flipped: turned the other way
        let turned = rotated(&arrow, &mapping, middle, 30);
        assert!(
            (shown_angle(&turned, &mapping) - 30.).abs() < 0.5,
            "{ops:?}"
        );
        let mirrored = flipped(&turned, &mapping, middle, 1);
        assert!(
            (shown_angle(&mirrored, &mapping) + 30.).abs() < 0.5,
            "{ops:?}"
        );
    }
}

/// The line went under the head, drawn on their own: see-through, where they met was darker
#[test]
fn a_see_through_arrow_is_one_piece() {
    let mapping = Mapping::new(&[], (200, 200));
    let img = RgbaImage::from_pixel(200, 200, Rgba([0, 0, 0, 255]));
    let mut arrow = Annotation::new(Shape::Arrow([160, 0]), [255, 0, 0, 128], 8, [20, 100]);
    for outline in [0, 3] {
        arrow.outline = outline;
        let drawn = drawn_at_once(&img, std::slice::from_ref(&arrow), &mapping);
        let line = drawn.get_pixel(60, 100).0;
        assert_ne!(line, [0, 0, 0, 255], "it is drawn");
        // the line, both lines of the head and the round tip are all there
        assert_eq!(drawn.get_pixel(172, 100).0, line, "where it meets the head");
        assert_eq!(drawn.get_pixel(175, 100).0, line, "at the tip");
        // the head is open: between its lines the image shows
        assert_eq!(
            drawn.get_pixel(155, 108).0,
            [0, 0, 0, 255],
            "inside the head"
        );
    }
    // it ends where it points
    arrow.color = [255, 0, 0, 255];
    arrow.outline = 0;
    let drawn = drawn_at_once(&img, &[arrow], &mapping);
    assert_eq!(drawn.get_pixel(178, 100).0, [255, 0, 0, 255], "at its end");
    assert_eq!(drawn.get_pixel(181, 100).0, [0, 0, 0, 255], "past its end");
}

fn spotlight_at(start: [i32; 2], size: [i32; 2], oval: bool) -> Annotation {
    Annotation::new(Shape::Spotlight { size, oval }, [0, 0, 0, 128], 0, start)
}

#[test]
fn a_spotlight_dims_all_but_its_spot() {
    let mapping = Mapping::new(&[], (200, 200));
    let gray = Rgba([100, 100, 100, 255]);
    let img = RgbaImage::from_pixel(200, 200, gray);
    let one = drawn_at_once(&img, &[spotlight_at([20, 20], [60, 40], false)], &mapping);
    assert_eq!(*one.get_pixel(50, 40), gray, "the spot is as it was");
    // black at half over 100
    let dimmed = one.get_pixel(150, 150).0;
    assert!(
        dimmed[0] > 40 && dimmed[0] < 60 && dimmed[3] == 255,
        "{dimmed:?}"
    );
    // two dim once together, each keeps its spot
    let two = drawn_at_once(
        &img,
        &[
            spotlight_at([20, 20], [60, 40], false),
            spotlight_at([120, 120], [60, 40], true),
        ],
        &mapping,
    );
    assert_eq!(*two.get_pixel(50, 40), gray);
    assert_eq!(*two.get_pixel(150, 140), gray, "the middle of the oval");
    assert_eq!(two.get_pixel(10, 180).0, dimmed, "dimmed once, not twice");
    assert_eq!(
        two.get_pixel(121, 121).0,
        dimmed,
        "the corner besides the oval"
    );
    // one that is too small dims nothing
    let empty = drawn_at_once(&img, &[spotlight_at([20, 20], [0, 0], false)], &mapping);
    assert_eq!(*empty.get_pixel(150, 150), gray);
}

#[test]
fn what_is_in_front_of_a_spotlight_is_not_dimmed() {
    let mapping = Mapping::new(&[], (200, 200));
    let img = RgbaImage::from_pixel(200, 200, Rgba([100, 100, 100, 255]));
    let mut list = vec![
        arrow_at([20, 100], [150, 0]),
        spotlight_at([10, 10], [40, 40], false),
        arrow_at([20, 150], [150, 0]),
    ];
    let drawn = drawn_at_once(&img, &list, &mapping);
    assert_eq!(drawn.get_pixel(80, 150).0, [255, 0, 0, 255], "in front");
    let behind = drawn.get_pixel(80, 100).0;
    assert!(
        behind[0] > 100 && behind[0] < 200,
        "behind is dimmed: {behind:?}"
    );
    // drawn a change at a time it is the same as at once
    let mut changing = img.clone();
    let mut layer = AnnotationLayer::default();
    layer.update(&mut changing, &list, &mapping);
    list[0] = arrow_at([20, 120], [150, 0]);
    list[2] = arrow_at([30, 170], [140, 0]);
    layer.update(&mut changing, &list, &mapping);
    assert!(
        changing == drawn_at_once(&img, &list, &mapping),
        "arrows moved"
    );
    list[1].set_start([60, 60], mapping.original);
    layer.update(&mut changing, &list, &mapping);
    assert!(
        changing == drawn_at_once(&img, &list, &mapping),
        "the spotlight moved"
    );
    list[1].color = [0, 0, 80, 200];
    layer.update(&mut changing, &list, &mapping);
    assert!(
        changing == drawn_at_once(&img, &list, &mapping),
        "its shade changed"
    );
}

/// Moved, a spotlight changes the image only where its spot was and where it is: its shade is as
/// it was everywhere else. It dimmed all of the image again, on a large photo at every frame of
/// a drag.
#[test]
fn a_moved_spotlight_redraws_only_around_its_spot() {
    let mapping = Mapping::new(&[], (400, 400));
    let img = RgbaImage::from_pixel(400, 400, Rgba([100, 100, 100, 255]));
    let mut list = vec![
        arrow_at([250, 350], [120, 0]),
        spotlight_at([20, 20], [60, 40], true),
    ];
    let mut changing = img.clone();
    let mut layer = AnnotationLayer::default();
    layer.update(&mut changing, &list, &mapping);
    list[1].set_start([40, 30], mapping.original);
    let area = layer
        .update(&mut changing, &list, &mapping)
        .expect("it moved");
    // where it was and where it is, not the arrow far from it
    assert!(
        area.x + area.width <= 110 && area.y + area.height <= 80,
        "{area:?}"
    );
    assert!(changing == drawn_at_once(&img, &list, &mapping), "moved");
    // turned and larger, the same
    list[1].rotation = 30;
    list[1].shape = Shape::Spotlight {
        size: [80, 40],
        oval: true,
    };
    let area = layer
        .update(&mut changing, &list, &mapping)
        .expect("it changed");
    assert!(area.width < 200 && area.height < 200, "{area:?}");
    assert!(
        changing == drawn_at_once(&img, &list, &mapping),
        "turned and larger"
    );
    // a new shade is everywhere
    list[1].color = [0, 0, 80, 200];
    let area = layer
        .update(&mut changing, &list, &mapping)
        .expect("its shade changed");
    assert_eq!((area.width, area.height), (400, 400));
    assert!(
        changing == drawn_at_once(&img, &list, &mapping),
        "its shade changed"
    );
}

/// What did not change is not drawn again, not copied and not measured again either
#[test]
fn the_layer_draws_again_only_what_changed() {
    let mapping = Mapping::new(&[], (200, 200));
    let img = RgbaImage::from_pixel(200, 200, Rgba([100, 100, 100, 255]));
    let mut list = vec![arrow_at([10, 10], [40, 0]), arrow_at([120, 150], [60, 0])];
    let mut changing = img.clone();
    let mut layer = AnnotationLayer::default();
    assert!(layer.update(&mut changing, &list, &mapping).is_some());
    assert_eq!(
        layer.update(&mut changing, &list, &mapping),
        None,
        "nothing changed"
    );
    list[1].width = 9;
    let area = layer
        .update(&mut changing, &list, &mapping)
        .expect("one changed");
    assert!(
        area.y > 100,
        "around the one that changed, not the other: {area:?}"
    );
    assert!(changing == drawn_at_once(&img, &list, &mapping));
    // the last one removed: only where it was
    list.pop();
    let area = layer
        .update(&mut changing, &list, &mapping)
        .expect("one removed");
    assert!(area.y > 100, "where it was: {area:?}");
    assert!(changing == drawn_at_once(&img, &list, &mapping));
}

#[test]
fn a_click_in_a_spot_goes_to_what_is_in_it() {
    let mapping = Mapping::new(&[], (200, 200));
    let spot = spotlight_at([10, 10], [100, 100], false);
    assert!(hits(&spot, &mapping, [60., 60.], 0.), "in its spot");
    assert!(!hits(&spot, &mapping, [150., 150.], 2.), "where it dims");
    // an oval only in its oval, not in the corners of its box
    let oval = spotlight_at([10, 10], [100, 100], true);
    assert!(hits(&oval, &mapping, [60., 60.], 0.), "in the oval");
    assert!(
        !hits(&oval, &mapping, [14., 14.], 0.),
        "in a corner of its box"
    );
}

#[test]
fn counter_digits_fit_in_their_round() {
    for number in [1, 2, 3, 4, 7, 10, 11, 42, 100, 999, 12345] {
        let digits = counter_digits(number, 100.).expect("digits");
        let [l, t, r, b] = ink(&digits).expect("ink");
        assert!(
            r - l <= 70.1 && b - t <= 50.1,
            "{number}: {l},{t} to {r},{b}"
        );
        assert!(
            l >= 8. && r <= 92.,
            "{number} stays in the round: {l} to {r}"
        );
        assert!(
            (t + b - 100.).abs() < 0.6,
            "{number} is in the middle up and down"
        );
        let across = weight_middle(&digits).expect("a weight");
        assert!(
            (across - 50.).abs() < 0.3,
            "{number}: its weight at {across}"
        );
    }
    // the stem of a 1 is in the middle, the flag on the left takes its box there
    let [l, _, r, _] = ink(&counter_digits(1, 100.).unwrap()).unwrap();
    assert!((l + r) / 2. < 49.5, "{l} to {r}");
}

#[test]
fn a_counter_is_a_number_in_a_round_that_reads_upright() {
    let white = |p: &Rgba<u8>| p.0[0] > 240 && p.0[1] > 240 && p.0[2] > 240;
    let ground = [40, 40, 40, 255];
    for ops in [
        vec![],
        vec![item(ImageOperation::Rotate(90))],
        vec![item(ImageOperation::Flip(false))],
    ] {
        let mapping = Mapping::new(&ops, (200, 200));
        let img = RgbaImage::from_pixel(200, 200, Rgba(ground));
        let mut one = Annotation::new(Shape::Counter(1), [229, 72, 74, 255], 60, [0, 0]);
        one.stand_upright(&mapping);
        one.centre_at(mapping.to_original([100., 100.]), mapping.original);
        let drawn = drawn_at_once(&img, std::slice::from_ref(&one), &mapping);
        let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
        for (x, y, _) in drawn.enumerate_pixels().filter(|(_, _, p)| white(p)) {
            let from_middle = (x as f32 + 0.5 - 100.).hypot(y as f32 + 0.5 - 100.);
            assert!(
                from_middle < 30.,
                "{ops:?}: white at {x},{y} is out of the round"
            );
            (left, top) = (left.min(x), top.min(y));
            (right, bottom) = (right.max(x), bottom.max(y));
        }
        let (wide, high) = (right + 1 - left, bottom + 1 - top);
        assert!(
            high > wide * 2,
            "{ops:?}: a 1 stands upright, {wide} by {high}"
        );
        assert!(
            high.abs_diff(30) <= 3,
            "{ops:?}: half as high as the round, {high}"
        );
        assert_eq!(
            drawn.get_pixel(100, 73).0,
            [229, 72, 74, 255],
            "{ops:?}: the round"
        );
        assert_eq!(
            drawn.get_pixel(100, 60).0,
            ground,
            "{ops:?}: not further out"
        );
        assert_eq!(drawn.get_pixel(5, 5).0, ground, "{ops:?}: nothing far away");
    }
}

#[test]
fn a_counter_turns_but_never_reads_mirrored() {
    // how it is turned on the screen, and whether its number is mirrored
    let shown = |a: &Annotation, mapping: &Mapping| {
        (shown_angle(a, mapping), mirrors(a.transform(mapping)))
    };
    for ops in [
        vec![],
        vec![item(ImageOperation::Rotate(90))],
        vec![item(ImageOperation::Flip(false))],
        vec![item(ImageOperation::Flip(true))],
    ] {
        let mapping = Mapping::new(&ops, (200, 200));
        let mut counter = Annotation::new(Shape::Counter(1), [0; 4], 40, [0, 0]);
        counter.stand_upright(&mapping);
        counter.centre_at([100., 100.], mapping.original);
        let (angle, mirrored) = shown(&counter, &mapping);
        assert!(
            angle.abs() < 0.5 && !mirrored,
            "{ops:?}: placed upright, {angle}"
        );
        let middle = middle_in(&counter, &mapping).unwrap();
        let turned = rotated(&counter, &mapping, middle, 30);
        let (angle, mirrored) = shown(&turned, &mapping);
        assert!(
            (angle - 30.).abs() < 0.5 && !mirrored,
            "{ops:?}: turned, {angle}"
        );
        for axis in 0..2 {
            let flipped = flipped(&turned, &mapping, middle, axis);
            let (angle, mirrored) = shown(&flipped, &mapping);
            assert!(
                (angle + 30.).abs() < 0.5 && !mirrored,
                "{ops:?}: flipped along {axis}, turned the other way, {angle}"
            );
        }
    }
}

/// Its number in its own colour, turned with the round
#[test]
fn a_counter_draws_its_number_in_its_colour() {
    let mapping = Mapping::new(&[], (200, 200));
    let ground = [40, 40, 40, 255];
    let img = RgbaImage::from_pixel(200, 200, Rgba(ground));
    let mut one = Annotation::new(Shape::Counter(8), [0, 90, 200, 255], 60, [70, 70]);
    one.number_color = [0, 0, 0, 255];
    let drawn = drawn_at_once(&img, &[one], &mapping);
    assert!(
        drawn.pixels().any(|p| p.0 == [0, 0, 0, 255]),
        "a black number"
    );
    assert!(
        !drawn.pixels().any(|p| p.0[0] > 240 && p.0[1] > 240),
        "nothing white"
    );
    assert_eq!(
        drawn.get_pixel(100, 132).0,
        ground,
        "nothing besides the round"
    );
    // turned by hand, its number turns with it: a 1 lies on its side
    let mut lying = Annotation::new(Shape::Counter(1), [229, 72, 74, 255], 60, [70, 70]);
    let middle = middle_in(&lying, &mapping).unwrap();
    lying = rotated(&lying, &mapping, middle, 90);
    let drawn = drawn_at_once(&img, &[lying], &mapping);
    let white: Vec<(u32, u32)> = drawn
        .enumerate_pixels()
        .filter(|(_, _, p)| p.0[0] > 240 && p.0[1] > 240 && p.0[2] > 240)
        .map(|(x, y, _)| (x, y))
        .collect();
    let wide = white.iter().map(|p| p.0).max().unwrap() - white.iter().map(|p| p.0).min().unwrap();
    let high = white.iter().map(|p| p.1).max().unwrap() - white.iter().map(|p| p.1).min().unwrap();
    assert!(wide > high * 2, "{wide} by {high}");
}

#[test]
fn shadows_are_round_and_smooth() {
    let mapping = Mapping::new(&[], (300, 300));
    let ground = [200, 200, 200, 255];
    let img = RgbaImage::from_pixel(300, 300, Rgba(ground));
    for size in [3, 12, 40] {
        let mut counter = Annotation::new(Shape::Counter(1), [0, 0, 0, 255], 100, [100, 100]);
        (counter.shadow, counter.shadow_size) = (100, size);
        let drawn = drawn_at_once(&img, std::slice::from_ref(&counter), &mapping);
        // out of the round to the right, and as much the other ways
        let xs = 200..=200 + size + 3;
        let row: Vec<u8> = xs.clone().map(|x| drawn.get_pixel(x, 150).0[0]).collect();
        for (i, x) in xs.enumerate() {
            let around = [
                drawn.get_pixel(299 - x, 149).0[0],
                drawn.get_pixel(150, x).0[0],
                drawn.get_pixel(149, 299 - x).0[0],
            ];
            assert!(
                around.iter().all(|v| v.abs_diff(row[i]) <= 2),
                "size {size}: at {x} {} but {around:?}",
                row[i]
            );
        }
        assert!(row[0] < 150, "size {size}: dark next to the round, {row:?}");
        for pair in row.windows(2) {
            assert!(pair[1] >= pair[0], "size {size}: lighter outwards, {row:?}");
        }
        assert_eq!(*row.last().unwrap(), 200, "size {size}: it ends, {row:?}");
        if size == 40 {
            let steps = row.windows(2).map(|p| p[1] - p[0]).max().unwrap();
            assert!(steps <= 16, "smooth when large: {row:?}");
        }
    }
}

/// Drawn around an arrow, not where nothing is near; a redaction casts none
#[test]
fn shadows_are_drawn_around_what_casts_them() {
    // drawn around an arrow, not where nothing is near
    let mapping = Mapping::new(&[], (200, 200));
    let ground = [200, 200, 200, 255];
    let img = RgbaImage::from_pixel(200, 200, Rgba(ground));
    let mut arrow = arrow_at([20, 100], [150, 0]);
    (arrow.shadow, arrow.shadow_size) = (60, 10);
    let drawn = drawn_at_once(&img, &[arrow], &mapping);
    let near = drawn.get_pixel(80, 106).0;
    assert!(near[0] < 190 && near[1] < 190, "below the line {near:?}");
    assert_eq!(drawn.get_pixel(80, 130).0, ground, "far below");
    // a redaction has none
    let mut hidden = Annotation::new(
        Shape::Redact {
            size: [40, 40],
            style: Redaction::Fill,
        },
        [0, 0, 0, 255],
        8,
        [50, 50],
    );
    (hidden.shadow, hidden.shadow_size) = (100, 10);
    assert!(!hidden.casts_shadow());
}

/// The layer was cut at the edge of the image, so what was drawn beyond it cast no shadow and
/// the shadow faded there
#[test]
fn a_shadow_is_as_dark_at_the_edge_of_the_image() {
    let mut counter = Annotation::new(Shape::Counter(1), [0, 0, 0, 255], 60, [70, 70]);
    (counter.shadow, counter.shadow_size) = (100, 20);
    let ground = Rgba([200, 200, 200, 255]);
    // the whole of it, and cut by the bottom edge at 115
    let whole = drawn_at_once(
        &RgbaImage::from_pixel(300, 300, ground),
        std::slice::from_ref(&counter),
        &Mapping::new(&[], (300, 300)),
    );
    let cut = drawn_at_once(
        &RgbaImage::from_pixel(300, 115, ground),
        std::slice::from_ref(&counter),
        &Mapping::new(&[], (300, 115)),
    );
    // a few pixels out of the round, near the edge of the image
    for (x, y) in [(132, 110), (130, 114), (68, 112)] {
        let (a, b) = (whole.get_pixel(x, y).0, cut.get_pixel(x, y).0);
        assert!(a[0].abs_diff(b[0]) <= 1, "at {x},{y}: {a:?} and {b:?}");
        assert!(a[0] < 150, "a shadow at {x},{y}: {a:?}");
    }
}

/// An arrow with transparency and an outline with its own color in the middle and the outline is around
/// it. the outline does not show through it.
#[test]
fn an_outline_does_not_show_through_what_it_is_around() {
    let mapping = Mapping::new(&[], (200, 200));
    let black = [0, 0, 0, 255];
    let img = RgbaImage::from_pixel(200, 200, Rgba(black));
    let mut arrow = Annotation::new(Shape::Arrow([160, 0]), [255, 0, 0, 128], 8, [20, 100]);
    let without = drawn_at_once(&img, std::slice::from_ref(&arrow), &mapping);
    arrow.outline = 3;
    arrow.outline_color = [255, 255, 255, 255];
    let with = drawn_at_once(&img, std::slice::from_ref(&arrow), &mapping);
    // half red over black, with an outline or without
    let middle = with.get_pixel(60, 100).0;
    assert_eq!(middle, without.get_pixel(60, 100).0);
    assert!(
        middle[0] > 120 && middle[1] < 5 && middle[2] < 5,
        "{middle:?}"
    );
    // the outline is white, at the side of the line
    assert_eq!(with.get_pixel(60, 106).0, [255, 255, 255, 255]);
    // a see-through outline is the outline's color over the image only
    arrow.outline_color = [255, 255, 255, 128];
    let faint = drawn_at_once(&img, std::slice::from_ref(&arrow), &mapping);
    assert_eq!(faint.get_pixel(60, 100).0, middle);
    let rim = faint.get_pixel(60, 106).0;
    assert!(rim[0].abs_diff(128) <= 1 && rim[0] == rim[1], "{rim:?}");
}

/// A see-through frame with a shadow is as light where it is as without one, the shadow is only
/// around it. Its line was dark.
#[test]
fn a_shadow_does_not_show_through_what_casts_it() {
    let mapping = Mapping::new(&[], (200, 200));
    let ground = [200, 200, 200, 255];
    let img = RgbaImage::from_pixel(200, 200, Rgba(ground));
    let mut frame = Annotation::new(Shape::Rect([100, 100]), [255, 0, 0, 128], 10, [50, 50]);
    let without = drawn_at_once(&img, std::slice::from_ref(&frame), &mapping);
    (frame.shadow, frame.shadow_size) = (80, 12);
    let with = drawn_at_once(&img, std::slice::from_ref(&frame), &mapping);
    // on the line
    assert_eq!(with.get_pixel(100, 50).0, without.get_pixel(100, 50).0);
    assert_eq!(with.get_pixel(50, 100).0, without.get_pixel(50, 100).0);
    // just out of the line it is dark
    let out = with.get_pixel(100, 58).0;
    assert!(out[1] < 150, "{out:?}");
    // as with a counter, its number on top
    let mut counter = Annotation::new(Shape::Counter(1), [0, 0, 255, 128], 60, [70, 70]);
    let without = drawn_at_once(&img, std::slice::from_ref(&counter), &mapping);
    (counter.shadow, counter.shadow_size) = (100, 10);
    counter.outline = 3;
    let with_outline = {
        let mut c = counter.clone();
        (c.shadow, c.shadow_size) = (0, 0);
        drawn_at_once(&img, &[c], &mapping)
    };
    let with = drawn_at_once(&img, std::slice::from_ref(&counter), &mapping);
    for (x, y) in [(80, 100), (100, 80), (100, 100)] {
        assert_eq!(with.get_pixel(x, y).0, without.get_pixel(x, y).0, "{x},{y}");
        assert_eq!(
            with.get_pixel(x, y).0,
            with_outline.get_pixel(x, y).0,
            "{x},{y}"
        );
    }
}

/// A see-through text with an outline shows the outline around its letters only
#[test]
fn a_see_through_text_does_not_show_its_outline_through() {
    let mapping = Mapping::new(&[], (300, 120));
    let img = RgbaImage::from_pixel(300, 120, Rgba([0, 0, 0, 255]));
    let mut text = Annotation::new(Shape::Text("IIII".into()), [255, 0, 0, 128], 80, [20, 10]);
    let without = drawn_at_once(&img, std::slice::from_ref(&text), &mapping);
    text.outline = 4;
    let with = drawn_at_once(&img, std::slice::from_ref(&text), &mapping);
    // where the letters are filled the same, red over black
    let filled: Vec<(u32, u32)> = (0..300)
        .flat_map(|x| (0..120).map(move |y| (x, y)))
        .filter(|&(x, y)| without.get_pixel(x, y).0[0] >= 127)
        .filter(|&(x, y)| {
            // well inside a letter
            [(-2i32, 0i32), (2, 0), (0, -2), (0, 2)]
                .iter()
                .all(|(dx, dy)| {
                    let (x, y) = (x as i32 + dx, y as i32 + dy);
                    (0..300).contains(&x)
                        && (0..120).contains(&y)
                        && without.get_pixel(x as u32, y as u32).0[0] >= 127
                })
        })
        .collect();
    assert!(filled.len() > 50, "letters are drawn");
    for (x, y) in filled {
        let p = with.get_pixel(x, y).0;
        assert!(p[1] < 5 && p[2] < 5, "white shows at {x},{y}: {p:?}");
    }
    // and the outline is there
    assert!(with.pixels().any(|p| p.0 == [255, 255, 255, 255]));
}

/// After a color type operation the image may be gray, 16 bit or floating point. Annotations
/// are drawn there too: they were left out, and saved or copied the image showed what they hide.
#[test]
fn drawn_in_every_layout() {
    use image::DynamicImage;
    let rgba = gradient(120, 90);
    let mapping = Mapping::new(&[], (120, 90));
    let redaction = |at: [i32; 2]| {
        Annotation::new(
            Shape::Redact {
                size: [30, 20],
                style: Redaction::Fill,
            },
            [255, 255, 255, 255],
            1,
            at,
        )
    };
    let frame = Annotation::new(Shape::Rect([40, 30]), [255, 0, 0, 128], 4, [60, 40]);
    let layouts = [
        DynamicImage::ImageLuma8(DynamicImage::ImageRgba8(rgba.clone()).to_luma8()),
        DynamicImage::ImageLumaA8(DynamicImage::ImageRgba8(rgba.clone()).to_luma_alpha8()),
        DynamicImage::ImageLuma16(DynamicImage::ImageRgba8(rgba.clone()).to_luma16()),
        DynamicImage::ImageLumaA16(DynamicImage::ImageRgba8(rgba.clone()).to_luma_alpha16()),
        DynamicImage::ImageRgb16(DynamicImage::ImageRgba8(rgba.clone()).to_rgb16()),
        DynamicImage::ImageRgba16(DynamicImage::ImageRgba8(rgba.clone()).to_rgba16()),
        DynamicImage::ImageRgb32F(DynamicImage::ImageRgba8(rgba.clone()).to_rgb32f()),
        DynamicImage::ImageRgba32F(DynamicImage::ImageRgba8(rgba.clone()).to_rgba32f()),
    ];
    for mut original in layouts {
        let color = original.color();
        // depth that 8 bits do not have, to see that it is kept
        if let DynamicImage::ImageRgba16(p) = &mut original {
            p.pixels_mut().for_each(|p| p.0[0] |= 0x55);
        }
        if let DynamicImage::ImageRgb32F(p) = &mut original {
            p.pixels_mut().for_each(|p| p.0[1] += 0.0007);
        }
        let mut img = original.clone();
        let mut layer = AnnotationLayer::default();
        let mut annotations = vec![redaction([10, 10]), frame.clone()];
        let changed = layer.update_image(&mut img, &annotations, &mapping);
        assert!(changed.is_some(), "{color:?}");
        assert_eq!(img.color(), color);
        // hidden under white
        let white = img.to_rgba8().get_pixel(20, 20).0;
        assert_eq!(white[..3], [255, 255, 255], "{color:?}");
        // the frame is drawn
        assert_ne!(
            img.to_rgba8().get_pixel(60, 55),
            original.to_rgba8().get_pixel(60, 55),
            "{color:?}"
        );
        // besides them, all is as it was, to the last bit
        let pixel = |img: &DynamicImage, x: u32, y: u32| {
            let n = color.bytes_per_pixel() as usize;
            let at = (y as usize * img.width() as usize + x as usize) * n;
            img.as_bytes()[at..at + n].to_vec()
        };
        for (x, y) in [(5, 5), (50, 15), (80, 55), (110, 85)] {
            assert_eq!(
                pixel(&img, x, y),
                pixel(&original, x, y),
                "{color:?} at {x},{y}"
            );
        }
        // moved, it is drawn as a whole drawing at once would, and what it left is as it was
        annotations[0] = redaction([70, 5]);
        layer.update_image(&mut img, &annotations, &mapping);
        let mut at_once = original.clone();
        AnnotationLayer::default().update_image(&mut at_once, &annotations, &mapping);
        assert_eq!(img.as_bytes(), at_once.as_bytes(), "{color:?}");
        // gone, it is all as it was
        layer.update_image(&mut img, &[], &mapping);
        assert_eq!(img.as_bytes(), original.as_bytes(), "{color:?}");
    }
}

/// A 200×200 image resized to 400×200
pub(crate) fn stretched() -> Mapping {
    let resize = item(ImageOperation::Resize {
        dimensions: (400, 200),
        aspect: false,
        filter: crate::image_editing::ScaleFilter::Box,
    });
    Mapping::new(&[resize], (200, 200))
}

/// How many pixels in a row, starting at `from`, are roughly `color`
fn run_of(img: &RgbaImage, from: (u32, u32), step: (i32, i32), color: [u8; 3]) -> u32 {
    let (mut x, mut y, mut n) = (from.0 as i32, from.1 as i32, 0);
    while (0..img.width() as i32).contains(&x) && (0..img.height() as i32).contains(&y) {
        let p = img.get_pixel(x as u32, y as u32).0;
        if (0..3).any(|c| p[c].abs_diff(color[c]) > 100) {
            break;
        }
        n += 1;
        x += step.0;
        y += step.1;
    }
    n
}

/// On a stretched image a frame's lines are equally thick horizontally and vertically
#[test]
fn lines_are_as_wide_on_a_stretched_image() {
    let mapping = stretched();
    let img = RgbaImage::from_pixel(400, 200, Rgba([0, 0, 0, 255]));
    // shows as 100×100
    let frame = Annotation::new(Shape::Rect([50, 100]), [255, 255, 255, 255], 8, [50, 50]);
    let drawn = drawn_at_once(&img, &[frame], &mapping);
    let white = [255, 255, 255];
    // measure the top line and the left line
    let top = run_of(&drawn, (150, 40), (0, 1), [0, 0, 0]);
    let across_top = run_of(&drawn, (150, 40 + top), (0, 1), white);
    let left = run_of(&drawn, (90, 100), (1, 0), [0, 0, 0]);
    let across_left = run_of(&drawn, (90 + left, 100), (1, 0), white);
    assert!(
        across_top.abs_diff(across_left) <= 1,
        "{across_top} and {across_left}"
    );
    // 8 × √2, the image's average scale
    assert!((10..=13).contains(&across_top), "{across_top}");
    let reach = selection_bounds(&[frame_like()], &[0], &mapping).unwrap();
    assert!(reach.drawn[1] <= 50. - 5.6 && reach.drawn[0] <= 100. - 5.6);
}

fn frame_like() -> Annotation {
    Annotation::new(Shape::Rect([50, 100]), [255, 255, 255, 255], 8, [50, 50])
}

/// Counters, stamps and text aren't stretched with the image, so a counter stays round
#[test]
fn counters_stamps_and_texts_keep_their_shape_on_a_stretched_image() {
    let mapping = stretched();
    let img = RgbaImage::from_pixel(400, 200, Rgba([0, 0, 0, 255]));
    let counter = Annotation::new(Shape::Counter(1), [255, 255, 255, 255], 60, [70, 70]);
    let drawn = drawn_at_once(&img, std::slice::from_ref(&counter), &mapping);
    let white: Vec<(u32, u32)> = drawn
        .enumerate_pixels()
        .filter(|(_, _, p)| p.0[0] > 128)
        .map(|(x, y, _)| (x, y))
        .collect();
    let wide = white.iter().map(|p| p.0).max().unwrap() - white.iter().map(|p| p.0).min().unwrap();
    let high = white.iter().map(|p| p.1).max().unwrap() - white.iter().map(|p| p.1).min().unwrap();
    assert!(wide.abs_diff(high) <= 1, "{wide} by {high}");
    // and its middle is still where it was placed
    let middle = mapping.to_result([100., 100.]);
    let (x, y) = (
        white.iter().map(|p| p.0 as f32).sum::<f32>() / white.len() as f32,
        white.iter().map(|p| p.1 as f32).sum::<f32>() / white.len() as f32,
    );
    assert!(
        (x - middle[0]).abs() < 2. && (y - middle[1]).abs() < 2.,
        "{x},{y}"
    );
    for a in [
        counter,
        Annotation::new(Shape::Stamp(Stamp::Check), [255; 4], 40, [10, 10]),
        Annotation::new(Shape::Text("Hi".into()), [255; 4], 30, [10, 10]),
    ] {
        let t = a.transform(&mapping);
        let (x, y) = (t.sx.hypot(t.ky), t.kx.hypot(t.sy));
        assert!(
            (x - y).abs() < 1e-4 && (t.sx * t.kx + t.ky * t.sy).abs() < 1e-4,
            "{a:?}"
        );
    }
    // they still turn with the image
    let turned = Mapping::new(&[item(ImageOperation::Rotate(90))], (200, 200));
    let a = Annotation::new(Shape::Stamp(Stamp::Check), [255; 4], 40, [10, 10]);
    let full = turned.transform(a.placement((200, 200)), Point::zero());
    let t = a.transform(&turned);
    assert!((t.sx - full.sx).abs() < 1e-4 && (t.ky - full.ky).abs() < 1e-4);
    assert!((t.kx - full.kx).abs() < 1e-4 && (t.sy - full.sy).abs() < 1e-4);
}

/// A turned redaction only hides the turned box, and clicks match it
#[test]
fn a_turned_redaction_hides_and_is_hit_where_it_is() {
    let mapping = Mapping::new(&[], (200, 200));
    let img = RgbaImage::from_pixel(200, 200, Rgba([0, 0, 0, 255]));
    for style in [Redaction::Fill, Redaction::Pixelate] {
        let mut redaction = Annotation::new(
            Shape::Redact {
                size: [80, 80],
                style,
            },
            [255, 255, 255, 255],
            8,
            [60, 60],
        );
        let middle = middle_in(&redaction, &mapping).unwrap();
        redaction = rotated(&redaction, &mapping, middle, 30);
        let mut white = gradient(200, 200);
        for p in white.pixels_mut() {
            p.0 = [255, 255, 255, 255];
        }
        let base = if style == Redaction::Fill {
            &img
        } else {
            &white
        };
        let drawn = drawn_at_once(base, std::slice::from_ref(&redaction), &mapping);
        if style == Redaction::Fill {
            assert_eq!(
                drawn.get_pixel(100, 100).0,
                [255; 4],
                "the middle is hidden"
            );
            // a corner of the upright box around it stays visible
            let e = extent(&redaction, redaction.transform(&mapping), 0.).unwrap();
            let corner = (e[0].ceil() as u32 + 1, e[1].ceil() as u32 + 1);
            assert_eq!(
                drawn.get_pixel(corner.0, corner.1).0,
                [0, 0, 0, 255],
                "{corner:?}"
            );
            // its own corner is hidden and can be clicked
            let mut own = Point::from_xy(4., 4.);
            redaction.transform(&mapping).map_point(&mut own);
            assert_eq!(drawn.get_pixel(own.x as u32, own.y as u32).0, [255; 4]);
            assert!(hits(&redaction, &mapping, [own.x, own.y], 0.));
            assert!(!hits(
                &redaction,
                &mapping,
                [corner.0 as f32, corner.1 as f32],
                0.
            ));
        } else {
            assert_eq!(drawn.get_pixel(100, 100).0, [255; 4]);
        }
    }
}

/// Caret positions follow the drawn letters
#[test]
fn the_caret_goes_where_the_letters_are() {
    let a = Annotation::new(Shape::Text("Hi there".into()), [255; 4], 40, [0, 0]);
    let positions = line_positions("Hi there", &a);
    assert_eq!(positions.len(), 9);
    assert_eq!(positions[0], 0.);
    assert!(positions.windows(2).all(|w| w[1] > w[0]), "{positions:?}");
    let bounds = text_layout("Hi there", &a).unwrap().bounds;
    assert!(
        (positions[8] - bounds[2]).abs() < 6.,
        "{positions:?} {bounds:?}"
    );
    // trailing spaces count, and bold is wider
    assert!(line_positions("Hi  ", &a)[4] > line_positions("Hi", &a)[2] + 10.);
    let mut bold = a.clone();
    bold.font.weight = 700;
    assert!(line_positions("Hi there", &bold)[8] > positions[8]);
    assert_eq!(*line_positions("", &a), vec![0.]);
}

/// On a stretched image the selection box has the same margin on every side
#[test]
fn the_selection_box_is_as_far_out_on_every_side_of_a_stretched_image() {
    let mapping = stretched();
    let width = 8;
    for a in [
        Annotation::new(Shape::Rect([50, 100]), [255; 4], width, [50, 50]),
        Annotation::new(Shape::Ellipse([50, 100]), [255; 4], width, [50, 50]),
        Annotation::new(Shape::Line([60, 40]), [255; 4], width, [50, 50]),
        {
            let mut framed = Annotation::new(Shape::Rect([50, 100]), [255; 4], width, [50, 50]);
            framed.outline = 3;
            framed
        },
    ] {
        let bounds = selection_bounds(std::slice::from_ref(&a), &[0], &mapping).unwrap();
        let (s, d) = (bounds.shape, bounds.drawn);
        let gaps = [s[0] - d[0], s[1] - d[1], d[2] - s[2], d[3] - s[3]];
        let reach = (width as f32 / 2. + a.outline as f32) * 2f32.sqrt();
        for gap in gaps {
            assert!(
                (gap - reach).abs() < 0.01,
                "{:?}: {gaps:?}, {reach}",
                a.shape
            );
        }
    }
    // unchanged without a stretch
    let plain = Mapping::new(&[], (200, 200));
    let a = Annotation::new(Shape::Rect([50, 100]), [255; 4], width, [50, 50]);
    let bounds = selection_bounds(std::slice::from_ref(&a), &[0], &plain).unwrap();
    assert_eq!(bounds.drawn, [46., 46., 104., 154.]);
}

/// A box's width and height sides as vectors, after `t`
fn sides(size: [f32; 2], t: Transform) -> [[f32; 2]; 2] {
    [
        [t.sx * size[0], t.ky * size[0]],
        [t.kx * size[1], t.sy * size[1]],
    ]
}

/// Turning a shape on a stretched image doesn't skew it
#[test]
fn turned_on_a_stretched_image_it_is_not_skewed() {
    let mapping = stretched();
    for shape in [
        Shape::Rect([50, 100]),
        Shape::Ellipse([50, 100]),
        Shape::Redact {
            size: [50, 100],
            style: Redaction::Fill,
        },
        Shape::Spotlight {
            size: [50, 100],
            oval: false,
        },
    ] {
        let a = Annotation::new(shape, [255; 4], 6, [50, 50]);
        let middle = middle_in(&a, &mapping).unwrap();
        for degrees in [30, 90, -45] {
            let turned = rotated(&a, &mapping, middle, degrees);
            let t = turned.transform(&mapping);
            let [w, h] = sides([50., 100.], t);
            let length = |v: [f32; 2]| v[0].hypot(v[1]);
            // still 100×100 with right angles
            assert!((length(w) - 100.).abs() < 0.01, "{degrees}: {w:?}");
            assert!((length(h) - 100.).abs() < 0.01, "{degrees}: {h:?}");
            assert!(
                (w[0] * h[0] + w[1] * h[1]).abs() < 0.01,
                "{degrees}: {w:?} {h:?}"
            );
            assert!((shown_angle(&turned, &mapping) - degrees as f32).abs() < 0.01);
            let now = middle_in(&turned, &mapping).unwrap();
            assert!((now[0] - middle[0]).abs() <= 1. && (now[1] - middle[1]).abs() <= 1.);
        }
    }
    // a turned line keeps its length
    let line = Annotation::new(Shape::Line([50, 0]), [255; 4], 6, [50, 100]);
    let middle = middle_in(&line, &mapping).unwrap();
    let turned = rotated(&line, &mapping, middle, 90);
    let ends = ends_of(&turned, &mapping);
    let (dx, dy) = (ends[1][0] - ends[0][0], ends[1][1] - ends[0][1]);
    assert!(dx.abs() < 0.01 && (dy - 100.).abs() < 0.01, "{ends:?}");
    // and its ends follow a resize
    let turned = rotated(&line, &mapping, middle, 30);
    let [start, end] = ends_of(&turned, &mapping);
    let stretched_line = resized(&turned, &mapping, start, [2., 2.], [0., 0.]);
    let [s, e] = ends_of(&stretched_line, &mapping);
    let wanted = [
        start[0] + 2. * (end[0] - start[0]),
        start[1] + 2. * (end[1] - start[1]),
    ];
    assert!(
        (s[0] - start[0]).abs() <= 1.5 && (s[1] - start[1]).abs() <= 1.5,
        "{s:?} {start:?}"
    );
    assert!(
        (e[0] - wanted[0]).abs() <= 2. && (e[1] - wanted[1]).abs() <= 2.,
        "{e:?} {wanted:?}"
    );
}

/// Rounded corners stay circular on a stretched image
#[test]
fn round_corners_stay_round_on_a_stretched_image() {
    let mapping = stretched();
    let img = RgbaImage::from_pixel(400, 200, Rgba([0, 0, 0, 255]));
    // shows as 100×100 at 100, 50
    let mut frame = Annotation::new(Shape::Rect([50, 100]), [255; 4], 4, [50, 50]);
    frame.radius = 20;
    let drawn = drawn_at_once(&img, &[frame], &mapping);
    // a circular corner is symmetric along its diagonal
    let mut worst = 0;
    for i in 0..40 {
        for j in 0..40 {
            let a = drawn.get_pixel(96 + i, 46 + j).0[0];
            let b = drawn.get_pixel(96 + j, 46 + i).0[0];
            worst = worst.max(a.abs_diff(b));
        }
    }
    assert!(worst <= 40, "{worst}");
    assert_eq!(drawn.get_pixel(101, 51).0, [0, 0, 0, 255]);
    assert_eq!(drawn.get_pixel(130, 50).0[0], 255, "the top line");
    assert_eq!(drawn.get_pixel(100, 80).0[0], 255, "the left line");
}

/// On a stretched image clicks go by the lines as shown: as far off them across the stretch as
/// along it, round corners included. Along the stretch they reached further.
#[test]
fn clicks_are_exact_on_a_stretched_image() {
    let mapping = stretched();
    // shows as 100×100 at 100, 50, its lines 8 × √2 wide
    let frame = Annotation::new(Shape::Rect([50, 100]), [255; 4], 8, [50, 50]);
    let half = 4. * 2f32.sqrt();
    let hit = |a: &Annotation, x: f32, y: f32| hits(a, &mapping, [x, y], 0.);
    assert!(hit(&frame, 100. - half + 0.5, 100.));
    assert!(
        !hit(&frame, 100. - half - 1.5, 100.),
        "left of the left line"
    );
    assert!(hit(&frame, 150., 50. - half + 0.5));
    assert!(!hit(&frame, 150., 50. - half - 1.5), "above the top line");
    // a round corner is hit on its curve, not in the corner it cuts off
    let mut round = frame.clone();
    round.radius = 20;
    let r = 20. * 2f32.sqrt();
    let on_curve = 100. + r - r / 2f32.sqrt();
    assert!(hit(&round, on_curve, 50. + on_curve - 100.));
    assert!(!hit(&round, 103., 53.), "the cut off corner");
}

/// On a stretched image what is redrawn and the box around what is selected reach as far as the
/// lines do, sharp corners of a turned frame included, and no further
#[test]
fn boxes_fit_the_lines_on_a_stretched_image() {
    let mapping = stretched();
    let half = 4. * 2f32.sqrt();
    let frame = Annotation::new(Shape::Rect([50, 100]), [255; 4], 8, [50, 50]);
    // what is redrawn: the lines and 2 pixels for their smooth edge
    let t = frame.transform(&mapping);
    let drawn = area(&frame, t, (400, 200)).unwrap();
    let left = (100. - half - 2.).floor() as u32;
    let right = (200. + half + 2.).ceil() as u32;
    assert_eq!((drawn.x, drawn.width), (left, right - left));
    // turned by 45° its sharp corners point left and right
    let middle = middle_in(&frame, &mapping).unwrap();
    let turned = rotated(&frame, &mapping, middle, 45);
    let bounds = selection_bounds(std::slice::from_ref(&turned), &[0], &mapping).unwrap();
    // turning keeps the start on whole pixels, so its middle moves a little
    let middle = middle_in(&turned, &mapping).unwrap();
    let corner = (50. + half) * 2f32.sqrt();
    assert!(
        (bounds.drawn[0] - (middle[0] - corner)).abs() < 0.5,
        "{bounds:?}"
    );
    assert!(
        (bounds.drawn[2] - (middle[0] + corner)).abs() < 0.5,
        "{bounds:?}"
    );
}
