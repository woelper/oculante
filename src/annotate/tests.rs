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
    // the head is hit to its corners, not before its tip
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
fn what_is_outlined_is_what_a_click_hits() {
    let ops = [
        item(ImageOperation::Rotate(90)),
        item(ImageOperation::Resize {
            dimensions: (150, 300),
            aspect: false,
            filter: crate::image_editing::ScaleFilter::Box,
        }),
    ];
    let mapping = Mapping::new(&ops, (200, 100));
    let shapes = [
        Shape::Arrow([60, -30]),
        Shape::Arrow([500, -375]),
        Shape::Rect([50, 30]),
        Shape::Ellipse([50, 30]),
        Shape::Pen(vec![[0, 0], [20, 10], [40, -5]]),
        // as drawn by hand, in small steps that turn a lot
        Shape::Pen(
            (0..60)
                .map(|i| [i, ((i as f32 * 0.7).sin() * 6.) as i32])
                .collect(),
        ),
        Shape::Pen(vec![[0, 0]]),
        Shape::Ellipse([80, 8]),
        Shape::Text("Hi".into()),
        Shape::Redact {
            size: [30, 20],
            style: Redaction::Fill,
        },
        Shape::Stamp(Stamp::Check),
    ];
    let looks = [(0, 0), (4, 0), (0, 10), (4, 100)];
    for (shape, (thick, radius)) in shapes
        .into_iter()
        .flat_map(|shape| looks.map(|look| (shape.clone(), look)))
    {
        // only frames have corners to round
        if radius > 0 && !matches!(shape, Shape::Rect(_)) {
            continue;
        }
        let mut a = Annotation::new(shape, [0; 4], 6, [80, 40]);
        a.outline = thick;
        a.radius = radius;
        let t = a.transform(&mapping);
        // 2 pixels around it in the edited image
        let gap = 2. / scale_of(t);
        let lines = outline(&a, gap, scale_of(t));
        assert!(!lines.is_empty(), "{:?} has no outline", a.shape);
        // all of it is inside
        let b = a.bounds().expect("bounds");
        let all = lines.iter().flatten();
        let (left, top, right, bottom) = all.fold(
            (f32::MAX, f32::MAX, f32::MIN, f32::MIN),
            |(l, t, r, b), p| (l.min(p[0]), t.min(p[1]), r.max(p[0]), b.max(p[1])),
        );
        assert!(
            left < b[0] - gap / 2.
                && top < b[1] - gap / 2.
                && right > b[2] + gap / 2.
                && bottom > b[3] + gap / 2.,
            "the outline of {:?} from {left},{top} to {right},{bottom} leaves out some of {b:?}",
            a.shape
        );
        // egui draws a spike where a line turns back
        for line in &lines {
            let n = line.len();
            for i in 0..n {
                let (before, at, after) = (line[(i + n - 1) % n], line[i], line[(i + 1) % n]);
                let (ax, ay) = (at[0] - before[0], at[1] - before[1]);
                let (bx, by) = (after[0] - at[0], after[1] - at[1]);
                assert!(
                    ax * bx + ay * by >= -0.2 * ax.hypot(ay) * bx.hypot(by),
                    "the outline of {:?} turns back at {at:?}",
                    a.shape
                );
            }
        }
        for p in lines.into_iter().flatten() {
            let mut at = Point::from_xy(p[0], p[1]);
            t.map_point(&mut at);
            assert!(
                hits(&a, &mapping, [at.x, at.y], 2.5),
                "{:?} is not hit at {p:?} of its outline",
                a.shape
            );
            // and not much closer
            assert!(
                !hits(&a, &mapping, [at.x, at.y], 1.),
                "{:?} is outlined too closely at {p:?}",
                a.shape
            );
        }
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
            let inside = x >= area.x
                && y >= area.y
                && x < area.x + area.width
                && y < area.y + area.height;
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
    let mut stamp =
        Annotation::new(Shape::Stamp(Stamp::Warning), [255, 0, 0, 255], 60, [20, 20]);
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
