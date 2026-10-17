//! Arrows, frames, text, pen lines and hidden areas on top of the edited image.
//!
//! Annotations are kept in pixels of the image as it was loaded. Crops, resizes,
//! rotations and flips take them along with what they point at. Each one keeps its
//! distance to the nearest corner, edge middle or the centre of the image, so it
//! stays there on an image of another size. They are drawn after all other edits.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use egui::{Color32, CursorIcon, Id, Key, Pos2, Stroke, Ui, pos2};
use image::math::Rect;
use image::{Rgba, RgbaImage};
use imageproc::geometric_transformations::Projection;
use serde::{Deserialize, Serialize};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Point, Transform};

use crate::appstate::ImageGeometry;
use crate::image_editing::{ImageOperation, ImgOpItem, canvas_rect, cropped_range, perspective};
use crate::paint::{keep_base, restore};
use crate::utils::rect_union;

/// One annotation. Its own coordinates start where it starts, in pixels of the
/// image as it was loaded.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub shape: Shape,
    pub color: [u8; 4],
    /// How wide its lines are, how large its text or the blocks that hide an area
    pub width: u32,
    /// The point of the image it keeps its distance to
    pub anchor: Anchor,
    /// From the anchor to where it starts
    pub offset: [i32; 2],
    /// Degrees clockwise, around where it starts
    #[serde(default)]
    pub rotation: i32,
    /// Mirrored left to right, top to bottom, at where it starts
    #[serde(default)]
    pub flip: [bool; 2],
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub enum Shape {
    /// Points from where it starts to here
    Arrow([i32; 2]),
    /// A frame this large
    Rect([i32; 2]),
    /// A circle, or an ellipse, in a box this large
    Ellipse([i32; 2]),
    /// Lines of text, the first one starts at the top left
    Text(String),
    /// A line through these points, the first one is where it starts
    Pen(Vec<[i32; 2]>),
    /// Hides what is below it
    Redact { size: [i32; 2], style: Redaction },
}

/// How a redaction hides what is below it
#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum Redaction {
    /// In blocks of one color each
    #[default]
    Pixelate,
    /// In the color of the annotation
    Fill,
}

/// A corner, the middle of an edge or the centre of the image
#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Centre,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    /// Row by row
    const ALL: [Anchor; 9] = [
        Anchor::TopLeft,
        Anchor::Top,
        Anchor::TopRight,
        Anchor::Left,
        Anchor::Centre,
        Anchor::Right,
        Anchor::BottomLeft,
        Anchor::Bottom,
        Anchor::BottomRight,
    ];

    /// Where it is on an image this large, in whole pixels
    pub fn point(self, size: (u32, u32)) -> [i32; 2] {
        let i = Self::ALL
            .iter()
            .position(|a| *a == self)
            .unwrap_or_default();
        let at = |length: u32, part: usize| (length as usize * part / 2) as i32;
        [at(size.0, i % 3), at(size.1, i / 3)]
    }

    /// The one in the same third of the image as the point
    fn nearest(p: [f32; 2], size: (u32, u32)) -> Anchor {
        let third =
            |v: f32, length: u32| ((v / length.max(1) as f32 * 3.).floor() as i32).clamp(0, 2);
        Self::ALL[(third(p[1], size.1) * 3 + third(p[0], size.0)) as usize]
    }
}

impl Annotation {
    fn new(shape: Shape, color: [u8; 4], width: u32, start: [i32; 2]) -> Self {
        Self {
            shape,
            color,
            width,
            anchor: Anchor::TopLeft,
            offset: start,
            rotation: 0,
            flip: [false; 2],
        }
    }

    /// Where it starts, in the image as it was loaded
    fn start(&self, original: (u32, u32)) -> [i32; 2] {
        let anchor = self.anchor.point(original);
        [anchor[0] + self.offset[0], anchor[1] + self.offset[1]]
    }

    fn set_start(&mut self, start: [i32; 2], original: (u32, u32)) {
        let anchor = self.anchor.point(original);
        self.offset = [start[0] - anchor[0], start[1] - anchor[1]];
    }

    /// From its own coordinates to the image as it was loaded
    fn placement(&self, original: (u32, u32)) -> Transform {
        let start = self.start(original);
        let flip = |flipped: bool| if flipped { -1. } else { 1. };
        Transform::from_translate(start[0] as f32, start[1] as f32)
            .pre_rotate(self.rotation as f32)
            .pre_scale(flip(self.flip[0]), flip(self.flip[1]))
    }

    /// From its own coordinates to the edited image
    pub fn transform(&self, mapping: &Mapping) -> Transform {
        let centre = self.bounds().map_or(Point::zero(), |b| {
            Point::from_xy((b[0] + b[2]) / 2., (b[1] + b[3]) / 2.)
        });
        mapping.transform(self.placement(mapping.original), centre)
    }

    /// Left, top, right and bottom of what it covers in its own coordinates, its
    /// lines not counted
    fn bounds(&self) -> Option<[f32; 4]> {
        let around = |points: &[[i32; 2]]| {
            points.iter().fold(None, |b: Option<[f32; 4]>, p| {
                let (x, y) = (p[0] as f32, p[1] as f32);
                Some(b.map_or([x, y, x, y], |b| {
                    [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)]
                }))
            })
        };
        match &self.shape {
            Shape::Arrow(to) => around(&[[0, 0], *to]),
            Shape::Rect(size) | Shape::Ellipse(size) | Shape::Redact { size, .. } => {
                around(&[[0, 0], *size])
            }
            Shape::Pen(points) => around(points),
            Shape::Text(text) => text_layout(text, self.width, self.color).map(|l| l.bounds),
        }
    }

    /// How far its lines reach out of its bounds, in its own coordinates
    fn reach(&self) -> f32 {
        let half = self.line_width() / 2.;
        match &self.shape {
            Shape::Arrow(to) => {
                let length = (to[0] as f32).hypot(to[1] as f32);
                half.max(arrow_head(self.line_width(), length).1)
            }
            Shape::Rect(_) | Shape::Ellipse(_) | Shape::Pen(_) => half,
            Shape::Text(_) | Shape::Redact { .. } => 0.,
        }
    }

    fn line_width(&self) -> f32 {
        self.width.max(1) as f32
    }

    /// Too small to be seen, or a text without any
    fn is_empty(&self) -> bool {
        match &self.shape {
            Shape::Arrow(to) => to[0].abs().max(to[1].abs()) < 2,
            Shape::Rect(size) | Shape::Ellipse(size) | Shape::Redact { size, .. } => {
                size[0] < 2 || size[1] < 2
            }
            Shape::Pen(points) => points.is_empty(),
            Shape::Text(text) => text.trim().is_empty(),
        }
    }

    /// After it was drawn or moved: the anchor nearest to it, with the same start
    fn reanchor(&mut self, original: (u32, u32)) {
        let Some(b) = self.bounds() else { return };
        let mut centre = Point::from_xy((b[0] + b[2]) / 2., (b[1] + b[3]) / 2.);
        self.placement(original).map_point(&mut centre);
        let start = self.start(original);
        self.anchor = Anchor::nearest([centre.x, centre.y], original);
        self.set_start(start, original);
    }

    /// Turned and mirrored so that it reads the right way round in the edited image,
    /// where `at` is: the image may be rotated or flipped after it was loaded
    fn stand_upright(&mut self, mapping: &Mapping) {
        let mut best = None;
        for rotation in [0, 90, 180, 270] {
            for flip in [false, true] {
                self.rotation = rotation;
                self.flip = [flip, false];
                let t = mapping.transform(self.placement(mapping.original), Point::zero());
                // how close to upright, without mirroring
                let straight = t.sx + t.sy - t.kx.abs() - t.ky.abs();
                if best.is_none_or(|(s, _, _)| straight > s) {
                    best = Some((straight, rotation, flip));
                }
            }
        }
        if let Some((_, rotation, flip)) = best {
            self.rotation = rotation;
            self.flip = [flip, false];
        }
    }

    /// Its name in the list of annotations
    fn label(&self) -> String {
        match &self.shape {
            Shape::Arrow(_) => "Arrow".into(),
            Shape::Rect(_) => "Frame".into(),
            Shape::Ellipse(_) => "Circle".into(),
            Shape::Pen(_) => "Pen".into(),
            Shape::Redact { .. } => "Redacted".into(),
            Shape::Text(text) => {
                let line = text.lines().next().unwrap_or_default();
                let mut short: String = line.chars().take(16).collect();
                if short.len() < text.len() {
                    short.push('…');
                }
                format!("Text: {short}")
            }
        }
    }
}

/// How the image as it was loaded turns into the edited one: the crops, resizes,
/// rotations and flips of the edits, in their order
#[derive(Debug, Clone, Default)]
pub struct Mapping {
    /// Size of the image as it was loaded
    pub original: (u32, u32),
    /// Size of the edited image
    pub result: (u32, u32),
    steps: Vec<Step>,
}

#[derive(Debug, Clone)]
enum Step {
    Affine(Transform),
    /// Annotations are moved along, not bent
    Perspective(Projection),
}

impl Mapping {
    pub fn new(ops: &[ImgOpItem], original: (u32, u32)) -> Self {
        let mut steps = vec![];
        let mut size = original;
        for op in ops.iter().filter(|op| op.active) {
            let (w, h) = (size.0 as f32, size.1 as f32);
            match &op.operation {
                ImageOperation::Crop(crop) if *crop != [0, 0, 0, 0] => {
                    // as the crop keeps its window inside the image
                    let window = cropped_range(crop, &size);
                    let (x, y) = (window[0].min(size.0), window[1].min(size.1));
                    steps.push(Step::Affine(Transform::from_translate(
                        -(x as f32),
                        -(y as f32),
                    )));
                    size = (window[2].min(size.0 - x), window[3].min(size.1 - y));
                }
                ImageOperation::CropPerspective { points, .. } => {
                    if let Some((projection, cropped)) = perspective(points) {
                        steps.push(Step::Perspective(projection));
                        size = cropped;
                    }
                }
                ImageOperation::Resize { dimensions, .. }
                    if *dimensions != (0, 0) && size.0 > 0 && size.1 > 0 =>
                {
                    steps.push(Step::Affine(Transform::from_scale(
                        dimensions.0 as f32 / w,
                        dimensions.1 as f32 / h,
                    )));
                    size = *dimensions;
                }
                ImageOperation::Rotate(angle) => {
                    let turn = match angle {
                        90 => Transform::from_row(0., 1., -1., 0., h, 0.),
                        -90 | 270 => Transform::from_row(0., -1., 1., 0., 0., w),
                        180 => Transform::from_row(-1., 0., 0., -1., w, h),
                        _ => continue,
                    };
                    steps.push(Step::Affine(turn));
                    if *angle != 180 {
                        size = (size.1, size.0);
                    }
                }
                ImageOperation::Flip(vertical) => {
                    steps.push(Step::Affine(if *vertical {
                        Transform::from_row(1., 0., 0., -1., 0., h)
                    } else {
                        Transform::from_row(-1., 0., 0., 1., w, 0.)
                    }));
                }
                _ => {}
            }
        }
        Self {
            original,
            result: size,
            steps,
        }
    }

    /// Follows a transform into the image as it was loaded on into the edited
    /// image. A perspective crop moves `centre`, in the transform's coordinates,
    /// to where it belongs and everything around with it.
    fn transform(&self, mut t: Transform, centre: Point) -> Transform {
        for step in &self.steps {
            match step {
                Step::Affine(a) => t = t.post_concat(*a),
                Step::Perspective(projection) => {
                    let mut at = centre;
                    t.map_point(&mut at);
                    let (x, y) = *projection * (at.x, at.y);
                    t = t.post_translate(x - at.x, y - at.y);
                }
            }
        }
        t
    }

    /// A point of the edited image in the image as it was loaded
    pub fn to_original(&self, p: [f32; 2]) -> [f32; 2] {
        let mut p = Point::from_xy(p[0], p[1]);
        for step in self.steps.iter().rev() {
            match step {
                Step::Affine(a) => {
                    if let Some(back) = a.invert() {
                        back.map_point(&mut p)
                    }
                }
                Step::Perspective(projection) => {
                    let (x, y) = projection.invert() * (p.x, p.y);
                    p = Point::from_xy(x, y);
                }
            }
        }
        [p.x, p.y]
    }

    /// A point of the image as it was loaded in the edited image
    pub fn to_result(&self, p: [f32; 2]) -> [f32; 2] {
        let mut p = Point::from_xy(p[0], p[1]);
        for step in &self.steps {
            match step {
                Step::Affine(a) => a.map_point(&mut p),
                Step::Perspective(projection) => {
                    let (x, y) = *projection * (p.x, p.y);
                    p = Point::from_xy(x, y);
                }
            }
        }
        [p.x, p.y]
    }
}

/// How much larger a transform makes things
fn scale_of(t: Transform) -> f32 {
    (t.sx * t.sy - t.kx * t.ky).abs().sqrt()
}

/// Length and half the width of the head of an arrow
fn arrow_head(width: f32, length: f32) -> (f32, f32) {
    let head = (width * 3. + 10.).min(length * 0.6);
    (head, head * 0.5)
}

/// The tip and the two back corners of the head of an arrow from where it starts
/// to `to`
fn arrow_head_corners(to: [i32; 2], width: f32) -> Option<[Point; 3]> {
    let (x, y) = (to[0] as f32, to[1] as f32);
    let length = x.hypot(y);
    if length < 1. {
        return None;
    }
    let (dx, dy) = (x / length, y / length);
    let (head, half) = arrow_head(width, length);
    let (bx, by) = (x - dx * head, y - dy * head);
    Some([
        Point::from_xy(x, y),
        Point::from_xy(bx - dy * half, by + dx * half),
        Point::from_xy(bx + dy * half, by - dx * half),
    ])
}

/// The line and the head of an arrow from where it starts to `to`
fn arrow(to: [i32; 2], width: f32) -> Option<(Path, Path)> {
    let [tip, left, right] = arrow_head_corners(to, width)?;
    let length = tip.x.hypot(tip.y);
    let head = arrow_head(width, length).0;
    let mut line = PathBuilder::new();
    line.move_to(0., 0.);
    // into the head, which covers the round end
    let into = 1. - head * 0.5 / length;
    line.line_to(tip.x * into, tip.y * into);
    let mut corners = PathBuilder::new();
    corners.move_to(tip.x, tip.y);
    corners.line_to(left.x, left.y);
    corners.line_to(right.x, right.y);
    corners.close();
    Some((line.finish()?, corners.finish()?))
}

/// The fonts for text annotations: Inter right away, the fonts of the system for
/// the scripts Inter lacks once they are found
static FONTS: LazyLock<Mutex<Arc<usvg::fontdb::Database>>> = LazyLock::new(|| {
    let mut fonts = usvg::fontdb::Database::new();
    fonts.load_font_data(crate::FONT.to_vec());
    fonts.load_font_data(crate::BOLD_FONT.to_vec());
    fonts.set_sans_serif_family("Inter");
    // tests draw with Inter only, the same on every machine
    if !cfg!(test) {
        let mut with_system = fonts.clone();
        std::thread::spawn(move || {
            with_system.load_system_fonts();
            if let Ok(mut fonts) = FONTS.lock() {
                *fonts = Arc::new(with_system);
            }
            FONTS_VERSION.fetch_add(1, Ordering::Relaxed);
            crate::utils::request_repaint();
        });
    }
    Mutex::new(Arc::new(fonts))
});

/// Counts up when the fonts change, text is laid out again then
static FONTS_VERSION: AtomicU32 = AtomicU32::new(0);

fn fonts() -> Arc<usvg::fontdb::Database> {
    FONTS
        .lock()
        .map(|fonts| fonts.clone())
        .unwrap_or_else(|_| Arc::new(usvg::fontdb::Database::new()))
}

/// Text ready to draw, with the box its lines take
struct TextLayout {
    tree: usvg::Tree,
    bounds: [f32; 4],
}

#[derive(PartialEq)]
struct TextKey {
    text: String,
    size: u32,
    color: [u8; 4],
    fonts: u32,
}

/// Text laid out lately, the newest first
static TEXTS: Mutex<Vec<(TextKey, Arc<TextLayout>)>> = Mutex::new(Vec::new());

/// Distance of the lines of a text, in sizes of the text
const LINE_HEIGHT: f32 = 1.25;

fn text_layout(text: &str, size: u32, color: [u8; 4]) -> Option<Arc<TextLayout>> {
    // first, so the text is laid out with the fonts this key says
    let fonts = fonts();
    let key = TextKey {
        text: text.into(),
        size,
        color,
        fonts: FONTS_VERSION.load(Ordering::Relaxed),
    };
    let mut texts = TEXTS.lock().ok()?;
    if let Some(i) = texts.iter().position(|(k, _)| *k == key) {
        let found = texts.remove(i);
        let layout = found.1.clone();
        texts.insert(0, found);
        return Some(layout);
    }
    let layout = Arc::new(lay_out(text, size, color, fonts)?);
    texts.insert(0, (key, layout.clone()));
    texts.truncate(32);
    Some(layout)
}

fn lay_out(
    text: &str,
    size: u32,
    color: [u8; 4],
    fonts: Arc<usvg::fontdb::Database>,
) -> Option<TextLayout> {
    let size = size.max(1) as f32;
    let escape = |line: &str| {
        line.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let mut lines = String::new();
    for (i, line) in text.split('\n').enumerate() {
        let dy = if i == 0 { 0. } else { size * LINE_HEIGHT };
        // an empty line still takes its place
        let line = if line.is_empty() {
            "\u{a0}".into()
        } else {
            escape(line)
        };
        lines += &format!(r#"<tspan x="0" dy="{dy}">{line}</tspan>"#);
    }
    let [r, g, b, a] = color;
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><text y="{size}" font-family="Inter" font-size="{size}" fill="rgb({r},{g},{b})" fill-opacity="{}" xml:space="preserve">{lines}</text></svg>"#,
        a as f32 / 255.
    );
    let options = usvg::Options {
        fontdb: fonts,
        font_family: "Inter".into(),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&svg, &options).ok()?;
    let bounds = text_bounds(tree.root())?;
    Some(TextLayout { tree, bounds })
}

fn text_bounds(group: &usvg::Group) -> Option<[f32; 4]> {
    group.children().iter().find_map(|node| match node {
        usvg::Node::Text(text) => {
            let b = text.bounding_box();
            Some([b.left(), b.top(), b.right(), b.bottom()])
        }
        usvg::Node::Group(group) => text_bounds(group),
        _ => None,
    })
}

/// The part of the edited image an annotation draws on
fn area(a: &Annotation, t: Transform, size: (u32, u32)) -> Option<Rect> {
    let b = a.bounds()?;
    let reach = a.reach();
    let mut corners = [
        Point::from_xy(b[0] - reach, b[1] - reach),
        Point::from_xy(b[2] + reach, b[1] - reach),
        Point::from_xy(b[0] - reach, b[3] + reach),
        Point::from_xy(b[2] + reach, b[3] + reach),
    ];
    t.map_points(&mut corners);
    // smoothed edges reach a little further
    let smooth = if matches!(a.shape, Shape::Redact { .. }) {
        0.
    } else {
        2.
    };
    let (mut left, mut top, mut right, mut bottom) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for c in corners {
        (left, top) = (left.min(c.x), top.min(c.y));
        (right, bottom) = (right.max(c.x), bottom.max(c.y));
    }
    let (left, top) = (
        (left - smooth).floor().max(0.),
        (top - smooth).floor().max(0.),
    );
    let (right, bottom) = (
        (right + smooth).ceil().min(size.0 as f32),
        (bottom + smooth).ceil().min(size.1 as f32),
    );
    (right > left && bottom > top).then_some(Rect {
        x: left as u32,
        y: top as u32,
        width: (right - left) as u32,
        height: (bottom - top) as u32,
    })
}

/// Draws an annotation, which reaches no further than `area`
fn draw(img: &mut RgbaImage, a: &Annotation, t: Transform, area: Rect) {
    if let Shape::Redact { style, .. } = &a.shape {
        match style {
            Redaction::Pixelate => {
                pixelate(img, area, (a.width as f32 * scale_of(t)).round() as u32)
            }
            Redaction::Fill => {
                let [r, g, b, _] = a.color;
                for y in area.y..area.y + area.height {
                    for x in area.x..area.x + area.width {
                        img.put_pixel(x, y, Rgba([r, g, b, 255]));
                    }
                }
            }
        }
        return;
    }
    let Some(mut layer) = Pixmap::new(area.width, area.height) else {
        return;
    };
    let t = t.post_translate(-(area.x as f32), -(area.y as f32));
    let mut paint = Paint::default();
    let [r, g, b, alpha] = a.color;
    paint.set_color_rgba8(r, g, b, alpha);
    paint.anti_alias = true;
    let mut stroke = tiny_skia::Stroke {
        width: a.line_width(),
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    match &a.shape {
        Shape::Arrow(to) => {
            if let Some((line, tip)) = arrow(*to, a.line_width()) {
                layer.stroke_path(&line, &paint, &stroke, t, None);
                layer.fill_path(&tip, &paint, FillRule::Winding, t, None);
            }
        }
        Shape::Rect(size) => {
            let (w, h) = (size[0] as f32, size[1] as f32);
            let mut frame = PathBuilder::new();
            frame.move_to(0., 0.);
            frame.line_to(w, 0.);
            frame.line_to(w, h);
            frame.line_to(0., h);
            frame.close();
            stroke.line_join = LineJoin::Miter;
            if let Some(frame) = frame.finish() {
                layer.stroke_path(&frame, &paint, &stroke, t, None);
            }
        }
        Shape::Ellipse(size) => {
            let oval = tiny_skia::Rect::from_xywh(0., 0., size[0] as f32, size[1] as f32)
                .and_then(PathBuilder::from_oval);
            if let Some(oval) = oval {
                layer.stroke_path(&oval, &paint, &stroke, t, None);
            }
        }
        Shape::Pen(points) => {
            if let [only] = points.as_slice() {
                let dot =
                    PathBuilder::from_circle(only[0] as f32, only[1] as f32, a.line_width() / 2.);
                if let Some(dot) = dot {
                    layer.fill_path(&dot, &paint, FillRule::Winding, t, None);
                }
            } else {
                let mut line = PathBuilder::new();
                for (i, p) in points.iter().enumerate() {
                    if i == 0 {
                        line.move_to(p[0] as f32, p[1] as f32);
                    } else {
                        line.line_to(p[0] as f32, p[1] as f32);
                    }
                }
                if let Some(line) = line.finish() {
                    layer.stroke_path(&line, &paint, &stroke, t, None);
                }
            }
        }
        Shape::Text(text) => {
            if let Some(layout) = text_layout(text, a.width, a.color) {
                resvg::render(&layout.tree, t, &mut layer.as_mut());
            }
        }
        Shape::Redact { .. } => {}
    }
    blend(img, &layer, area);
}

/// Hides an area in blocks, each the average color of its pixels
fn pixelate(img: &mut RgbaImage, area: Rect, block: u32) {
    let block = block.max(2);
    let (right, bottom) = (area.x + area.width, area.y + area.height);
    for top in (area.y..bottom).step_by(block as usize) {
        for left in (area.x..right).step_by(block as usize) {
            let (r, b) = ((left + block).min(right), (top + block).min(bottom));
            // weighted by alpha, transparent pixels have no color to give
            let mut sum = [0u64; 4];
            for y in top..b {
                for x in left..r {
                    let p = img.get_pixel(x, y).0;
                    let alpha = p[3] as u64;
                    for c in 0..3 {
                        sum[c] += p[c] as u64 * alpha;
                    }
                    sum[3] += alpha;
                }
            }
            let count = ((r - left) * (b - top)) as u64;
            let color = match sum[3] {
                0 => [0; 4],
                alpha => [
                    (sum[0] / alpha) as u8,
                    (sum[1] / alpha) as u8,
                    (sum[2] / alpha) as u8,
                    (alpha / count) as u8,
                ],
            };
            for y in top..b {
                for x in left..r {
                    img.put_pixel(x, y, Rgba(color));
                }
            }
        }
    }
}

/// Puts a layer, its colors multiplied by alpha, over a part of the image
fn blend(img: &mut RgbaImage, layer: &Pixmap, area: Rect) {
    let stride = img.width() as usize * 4;
    let row_length = area.width as usize * 4;
    for (row, src) in layer.data().chunks_exact(row_length).enumerate() {
        let start = (area.y as usize + row) * stride + area.x as usize * 4;
        let dst = &mut img.as_mut()[start..start + row_length];
        let (dst, _) = dst.as_chunks_mut::<4>();
        let (src, _) = src.as_chunks::<4>();
        for (d, s) in dst.iter_mut().zip(src) {
            match s[3] {
                0 => {}
                255 => *d = *s,
                alpha => {
                    let sa = alpha as f32 / 255.;
                    // what shows of the pixel below
                    let below = d[3] as f32 / 255. * (1. - sa);
                    let out = sa + below;
                    for c in 0..3 {
                        d[c] = ((s[c] as f32 + d[c] as f32 * below) / out).round() as u8;
                    }
                    d[3] = (out * 255.).round() as u8;
                }
            }
        }
    }
}

/// The annotations in the edited image, kept up to date by drawing only what changed
#[derive(Debug, Clone, Default)]
pub struct AnnotationLayer {
    /// What was under the annotations, where `base_area` says
    base: RgbaImage,
    base_area: Option<Rect>,
    /// The annotations as they are in the image, with the part each one drew on
    drawn: Vec<(Annotation, Option<Rect>)>,
    fonts: u32,
}

impl AnnotationLayer {
    /// The image was made anew, without annotations
    pub fn reset(&mut self) {
        self.base_area = None;
        self.drawn.clear();
    }

    /// Whether any annotation is in the image
    pub fn is_empty(&self) -> bool {
        self.drawn.iter().all(|(_, area)| area.is_none())
    }

    /// Brings the annotations in the image up to date. Returns the part of the image
    /// that changed.
    pub fn update(
        &mut self,
        img: &mut RgbaImage,
        annotations: &[Annotation],
        mapping: &Mapping,
    ) -> Option<Rect> {
        let size = img.dimensions();
        let now: Vec<(Annotation, Option<Rect>)> = annotations
            .iter()
            .map(|a| (a.clone(), area(a, a.transform(mapping), size)))
            .collect();
        let fonts = FONTS_VERSION.load(Ordering::Relaxed);
        let everything = fonts != self.fonts;
        let mut dirty: Option<Rect> = None;
        for i in 0..self.drawn.len().max(now.len()) {
            let (was, is) = (self.drawn.get(i), now.get(i));
            if everything || was != is {
                for area in [was, is].into_iter().flatten().filter_map(|e| e.1) {
                    dirty = Some(dirty.map_or(area, |d| rect_union(d, area)));
                }
            }
        }
        self.fonts = fonts;
        let Some(mut dirty) = dirty else {
            self.drawn = now;
            return None;
        };
        // all that is drawn again is drawn whole, a redaction needs all of its blocks
        loop {
            let mut grown = false;
            for area in now.iter().filter_map(|e| e.1) {
                if overlap(area, dirty) && rect_union(area, dirty) != dirty {
                    dirty = rect_union(area, dirty);
                    grown = true;
                }
            }
            if !grown {
                break;
            }
        }
        let kept = self.base_area.map_or(dirty, |b| rect_union(b, dirty));
        keep_base(&mut self.base, &mut self.base_area, img, kept);
        restore(img, &self.base, dirty);
        for (a, area) in &now {
            if let Some(area) = area.filter(|area| overlap(*area, dirty)) {
                draw(img, a, a.transform(mapping), area);
            }
        }
        self.drawn = now;
        Some(dirty)
    }
}

fn overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

/// The annotations of the edits, if they are switched on
pub fn annotations_mut(stack: &mut [ImgOpItem]) -> Option<&mut Vec<Annotation>> {
    stack
        .iter_mut()
        .filter(|op| op.active)
        .find_map(|op| match &mut op.operation {
            ImageOperation::Annotate(annotations) => Some(annotations),
            _ => None,
        })
}

/// The annotations that are drawn into the image: all but a text that is typed,
/// the box to type in shows it
pub fn visible(stack: &[ImgOpItem], state: &AnnotateUi) -> Vec<Annotation> {
    stack
        .iter()
        .filter(|op| op.active)
        .find_map(|op| match &op.operation {
            ImageOperation::Annotate(annotations) => Some(annotations),
            _ => None,
        })
        .map(|annotations| {
            annotations
                .iter()
                .enumerate()
                .filter(|(i, _)| state.typing != Some(*i))
                .map(|(_, a)| a.clone())
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Arrow,
    Frame,
    Circle,
    Text,
    Pen,
    Redact,
}

impl Tool {
    const ALL: [Tool; 6] = [
        Tool::Arrow,
        Tool::Frame,
        Tool::Circle,
        Tool::Text,
        Tool::Pen,
        Tool::Redact,
    ];

    fn name(self) -> &'static str {
        match self {
            Tool::Arrow => "Arrow",
            Tool::Frame => "Frame",
            Tool::Circle => "Circle",
            Tool::Text => "Text",
            Tool::Pen => "Pen",
            Tool::Redact => "Redact",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Tool::Arrow => "Drag from where the arrow starts to what it points at",
            Tool::Frame => "Drag a frame around something, with Shift a square",
            Tool::Circle => "Drag a circle around something, with Shift a round one",
            Tool::Text => "Click where the text goes, click a text to change it",
            Tool::Pen => "Draw freely",
            Tool::Redact => "Drag over what nobody should see",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Tool::Arrow => Icon::Arrow,
            Tool::Frame => Icon::Frame,
            Tool::Circle => Icon::Circle,
            Tool::Text => Icon::Text,
            Tool::Pen => Icon::Pen,
            Tool::Redact => Icon::Pixelate,
        }
    }
}

/// What the button of a tool or of a way to hide shows
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Icon {
    Arrow,
    Frame,
    Circle,
    Text,
    Pen,
    Pixelate,
    Fill,
}

/// Draws an icon in the middle of `rect`, in lines as thick as those of the icon font
fn paint_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: Color32) {
    // in a box of 16 by 16
    let at = |x: f32, y: f32| rect.center() + egui::vec2(x - 8., y - 8.);
    let stroke = Stroke::new(1.6, color);
    let square = egui::Rect::from_min_max(at(2.5, 3.5), at(13.5, 12.5));
    match icon {
        Icon::Arrow => {
            painter.line_segment([at(2.5, 13.5), at(10., 6.)], stroke);
            painter.add(egui::Shape::convex_polygon(
                vec![at(13.5, 2.5), at(11.4, 8.9), at(7.1, 4.6)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Frame => {
            painter.rect_stroke(square, 1., stroke, egui::StrokeKind::Middle);
        }
        Icon::Circle => {
            painter.circle_stroke(at(8., 8.), 5.8, stroke);
        }
        Icon::Text => {
            let stroke = Stroke::new(1.9, color);
            painter.line_segment([at(3., 3.), at(13., 3.)], stroke);
            painter.line_segment([at(8., 3.), at(8., 14.)], stroke);
        }
        Icon::Pen => {
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [at(2., 12.), at(5., 0.), at(10., 17.), at(14., 4.)],
                false,
                Color32::TRANSPARENT,
                stroke,
            ));
        }
        Icon::Pixelate => {
            // blocks of two shades, as a pixelated area
            for row in 0..3 {
                for column in 0..3 {
                    let shade = if (row + column) % 2 == 0 {
                        color
                    } else {
                        color.gamma_multiply(0.35)
                    };
                    let min = at(2. + column as f32 * 4., 2. + row as f32 * 4.);
                    painter.rect_filled(
                        egui::Rect::from_min_size(min, egui::vec2(4., 4.)),
                        0.,
                        shade,
                    );
                }
            }
        }
        Icon::Fill => {
            painter.rect_filled(square, 1., color);
        }
    }
}

#[derive(Debug, Clone)]
enum Drag {
    /// Drawing the annotation at this index, from here in the image as it was loaded
    Draw { index: usize, from: [f32; 2] },
    /// Moving the selected annotations, from here, where they started
    Move {
        from: [f32; 2],
        starts: Vec<[i32; 2]>,
    },
}

/// What the mouse and the keys do with annotations
#[derive(Debug, Clone)]
pub struct AnnotateUi {
    /// What a drag over the image does. With none, the image is moved as always.
    pub tool: Option<Tool>,
    pub color: [u8; 4],
    /// For new lines and arrows, 0 until an image tells how wide
    pub width: u32,
    /// For new text
    pub text_size: u32,
    pub redaction: Redaction,
    /// Indices of the selected annotations
    pub selected: Vec<usize>,
    drag: Option<Drag>,
    /// The text annotation that is typed
    typing: Option<usize>,
    /// The box to type in gets the keys
    focus_text: bool,
    /// Size of the image as it was loaded
    original: (u32, u32),
    /// The annotations as they are in the edited image
    pub layer: AnnotationLayer,
    /// The mouse and the keys belong to the annotations this frame
    pub grab: bool,
}

impl Default for AnnotateUi {
    fn default() -> Self {
        Self {
            tool: None,
            color: [235, 40, 40, 255],
            width: 0,
            text_size: 0,
            redaction: Redaction::Pixelate,
            selected: vec![],
            drag: None,
            typing: None,
            focus_text: false,
            original: (0, 0),
            layer: Default::default(),
            grab: false,
        }
    }
}

impl AnnotateUi {
    /// Before the panel and the canvas: the size of the image, lines as wide as
    /// suits it
    pub fn prepare(&mut self, original: (u32, u32)) {
        self.original = original;
        if self.width == 0 && original.0 > 0 && original.1 > 0 {
            self.width = (original.0.min(original.1) / 120).clamp(3, 80);
            self.text_size = self.width * 6;
        }
    }

    /// A new image: nothing of it is drawn, selected or typed
    pub fn new_image(&mut self) {
        self.layer.reset();
        self.selected.clear();
        self.drag = None;
        self.typing = None;
    }

    /// No annotations, or they are switched off
    pub fn stop(&mut self) {
        self.tool = None;
        self.new_image();
    }

    fn start_typing(&mut self, index: usize) {
        self.typing = Some(index);
        self.focus_text = true;
        self.selected = vec![index];
        self.drag = None;
    }

    fn finish_typing(&mut self, annotations: &mut Vec<Annotation>) {
        if let Some(i) = self.typing.take()
            && let Some(a) = annotations.get_mut(i)
        {
            if a.is_empty() {
                self.remove(annotations, vec![i]);
            } else {
                a.reanchor(self.original);
            }
        }
    }

    fn remove(&mut self, annotations: &mut Vec<Annotation>, mut indices: Vec<usize>) {
        indices.sort_unstable();
        indices.dedup();
        for i in indices.into_iter().rev() {
            if i < annotations.len() {
                annotations.remove(i);
            }
        }
        self.selected.clear();
        self.typing = None;
        self.drag = None;
    }
}

/// The part of the edit panel for the annotations
pub fn panel_ui(ui: &mut Ui, annotations: &mut Vec<Annotation>, state: &mut AnnotateUi) {
    ui.horizontal_wrapped(|ui| {
        for tool in Tool::ALL {
            if icon_button(ui, state.tool == Some(tool), tool.icon())
                .on_hover_text(format!("{}\n{}", tool.name(), tool.hint()))
                .clicked()
            {
                state.finish_typing(annotations);
                state.tool = (state.tool != Some(tool)).then_some(tool);
            }
        }
    });

    // what is selected, or else what comes next
    let selected = state.selected.clone();
    ui.horizontal(|ui| {
        let mut color = selected
            .first()
            .and_then(|i| annotations.get(*i))
            .map_or(state.color, |a| a.color);
        if ui
            .color_edit_button_srgba_unmultiplied(&mut color)
            .changed()
        {
            state.color = color;
            for i in &selected {
                if let Some(a) = annotations.get_mut(*i) {
                    a.color = color;
                }
            }
        }
        let next = if state.tool == Some(Tool::Text) {
            &mut state.text_size
        } else {
            &mut state.width
        };
        let mut size = selected
            .first()
            .and_then(|i| annotations.get(*i))
            .map_or(*next, |a| a.width);
        ui.label("Size");
        if ui
            .add(egui::DragValue::new(&mut size).range(1..=500))
            .on_hover_text("Width of lines, size of text or of the blocks that hide")
            .changed()
        {
            if selected.is_empty() {
                *next = size;
            }
            for i in &selected {
                if let Some(a) = annotations.get_mut(*i) {
                    a.width = size;
                }
            }
        }
    });

    let redactions: Vec<usize> = selected
        .iter()
        .copied()
        .filter(|i| {
            matches!(
                annotations.get(*i).map(|a| &a.shape),
                Some(Shape::Redact { .. })
            )
        })
        .collect();
    if state.tool == Some(Tool::Redact) || !redactions.is_empty() {
        ui.horizontal(|ui| {
            for (style, icon, name) in [
                (Redaction::Pixelate, Icon::Pixelate, "Pixelate"),
                (Redaction::Fill, Icon::Fill, "Fill with the color"),
            ] {
                if icon_button(ui, state.redaction == style, icon)
                    .on_hover_text(name)
                    .clicked()
                {
                    state.redaction = style;
                    for i in &redactions {
                        if let Some(Shape::Redact { style: s, .. }) =
                            annotations.get_mut(*i).map(|a| &mut a.shape)
                        {
                            *s = style;
                        }
                    }
                }
            }
        });
    }

    let mut delete = None;
    for (i, a) in annotations.iter().enumerate() {
        ui.horizontal(|ui| {
            if toggle(ui, state.selected.contains(&i), &a.label()).clicked() {
                state.selected = vec![i];
                // the selection lasts while annotating
                state.tool.get_or_insert(Tool::Arrow);
            }
            if ui
                .small_button(crate::icons::X)
                .on_hover_text("Remove")
                .clicked()
            {
                delete = Some(i);
            }
        });
    }
    if let Some(i) = delete {
        state.remove(annotations, vec![i]);
    }

    if state.tool.is_some() {
        ui.horizontal_wrapped(|ui| {
            ui.small(
                "Drag what you point at to move it, Shift+click selects more, double click a \
                 text to change it. Del removes what is selected, Esc stops annotating.",
            );
        });
    }
}

/// A button that is lit while `on`, as the one for paint mode
fn toggle(ui: &mut Ui, on: bool, text: &str) -> egui::Response {
    lit(ui, on, egui::Button::new(text))
}

fn icon_button(ui: &mut Ui, on: bool, icon: Icon) -> egui::Response {
    let response = lit(ui, on, egui::Button::new("").min_size(egui::vec2(28., 24.)));
    let color = ui.style().interact(&response).fg_stroke.color;
    paint_icon(ui.painter(), response.rect, icon, color);
    response
}

fn lit<'a>(ui: &mut Ui, on: bool, button: egui::Button<'a>) -> egui::Response {
    ui.add(if on {
        button.fill(ui.style().visuals.selection.bg_fill)
    } else {
        button
    })
}

/// Drawing, selecting and moving annotations with the mouse, and the keys for them.
/// Returns whether the image should be copied.
pub fn canvas_ui(
    ctx: &egui::Context,
    annotations: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    mapping: &Mapping,
    geo: &ImageGeometry,
    pointer_over_ui: bool,
) -> bool {
    // the list may have changed elsewhere
    state.selected.retain(|i| *i < annotations.len());
    if state
        .typing
        .is_some_and(|i| !matches!(annotations.get(i).map(|a| &a.shape), Some(Shape::Text(_))))
    {
        state.typing = None;
    }
    let Some(tool) = state.tool else {
        state.finish_typing(annotations);
        state.selected.clear();
        state.drag = None;
        state.grab = false;
        return false;
    };
    state.grab = true;
    let original = mapping.original;
    let to_result = |p: Pos2| {
        [
            (p.x - geo.offset.x) / geo.scale,
            (p.y - geo.offset.y) / geo.scale,
        ]
    };
    let to_screen = |p: Point| {
        pos2(
            p.x * geo.scale + geo.offset.x,
            p.y * geo.scale + geo.offset.y,
        )
    };
    let painter = ctx
        .layer_painter(egui::LayerId::new(
            egui::Order::Middle,
            Id::new("annotation_overlay"),
        ))
        .with_clip_rect(canvas_rect(ctx));
    // a thin line around it, for what the mouse points at, a stronger one for what
    // is selected
    let show = |a: &Annotation, strong: bool| {
        let t = a.transform(mapping);
        let on_screen = (scale_of(t) * geo.scale).max(0.001);
        let width = if strong { 1.5 } else { 1. };
        for line in outline(a, 3. / on_screen, on_screen) {
            let points: Vec<Pos2> = line
                .into_iter()
                .map(|p| {
                    let mut p = Point::from_xy(p[0], p[1]);
                    t.map_point(&mut p);
                    to_screen(p)
                })
                .collect();
            painter.add(egui::Shape::closed_line(
                points.clone(),
                Stroke::new(width + 2., Color32::from_black_alpha(120)),
            ));
            let yellow = Stroke::new(width, Color32::from_rgb(255, 200, 40));
            if strong {
                // what is selected is dashed, what is pointed at is not
                let mut points = points;
                points.push(points[0]);
                painter.extend(egui::Shape::dashed_line(&points, yellow, 6., 4.));
            } else {
                painter.add(egui::Shape::closed_line(points, yellow));
            }
        }
    };

    let typed = text_ui(ctx, annotations, state, mapping, geo);
    let mut copy = false;
    if state.typing.is_none() {
        let (delete, escape, shift) = ctx.input(|i| {
            (
                i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
                i.key_pressed(Key::Escape),
                i.modifiers.shift,
            )
        });
        copy = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)));
        if delete && !state.selected.is_empty() {
            let selected = state.selected.clone();
            state.remove(annotations, selected);
        }
        if escape {
            if state.selected.is_empty() {
                state.tool = None;
            }
            state.selected.clear();
            state.drag = None;
        }
        let step = if shift { 10. } else { 1. };
        let nudge = ctx.input(|i| {
            [
                (Key::ArrowLeft, [-step, 0.]),
                (Key::ArrowRight, [step, 0.]),
                (Key::ArrowUp, [0., -step]),
                (Key::ArrowDown, [0., step]),
            ]
            .into_iter()
            .filter(|(key, _)| i.key_pressed(*key))
            .fold([0., 0.], |d, (_, by)| [d[0] + by[0], d[1] + by[1]])
        });
        if nudge != [0., 0.] {
            for &i in &state.selected {
                let a = &mut annotations[i];
                let start = a.start(original);
                let from = [start[0] as f32, start[1] as f32];
                let at = mapping.to_result(from);
                let to = mapping.to_original([at[0] + nudge[0], at[1] + nudge[1]]);
                // at least a pixel, also on an image that was made smaller
                let by = |d: f32, wanted: f32| {
                    let d = d.round() as i32;
                    if d == 0 && wanted != 0. {
                        wanted.signum() as i32
                    } else {
                        d
                    }
                };
                let moved = [
                    start[0] + by(to[0] - from[0], nudge[0]),
                    start[1] + by(to[1] - from[1], nudge[1]),
                ];
                a.set_start(moved, original);
                a.reanchor(original);
            }
        }
    }

    let (pressed, down, released, double, shift) = ctx.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            // on the second release
            i.pointer
                .button_double_clicked(egui::PointerButton::Primary),
            i.modifiers.shift,
        )
    });
    // a drag goes on over the panels too
    let pointer = ctx.input(|i| i.pointer.hover_pos());
    // the mouse may be this far off a line on the screen and still hit it
    let slack = 6. / geo.scale.max(0.01);
    let hit = |annotations: &[Annotation], p: [f32; 2]| {
        (0..annotations.len())
            .rev()
            .find(|i| hits(&annotations[*i], mapping, p, slack))
    };
    let is_text = |a: &Annotation| matches!(a.shape, Shape::Text(_));

    if let Some(pos) = pointer {
        let p = to_result(pos);
        let o = mapping.to_original(p);
        let round = |v: [f32; 2]| [v[0].round() as i32, v[1].round() as i32];
        if pressed && !pointer_over_ui {
            if state.typing.is_some() || typed {
                // the click only ends the typing
                state.finish_typing(annotations);
            } else {
                let under = hit(annotations, p);
                let text_under = under.filter(|i| is_text(&annotations[*i]));
                match under {
                    // the text tool changes the text it clicks
                    Some(i) if tool == Tool::Text && text_under.is_some() => state.start_typing(i),
                    Some(i) if shift => {
                        if let Some(at) = state.selected.iter().position(|s| *s == i) {
                            state.selected.remove(at);
                        } else {
                            state.selected.push(i);
                        }
                    }
                    // what is pointed at is selected and moved, with what else is
                    // selected
                    Some(i) => {
                        if !state.selected.contains(&i) {
                            state.selected = vec![i];
                        }
                        state.drag = Some(Drag::Move {
                            from: o,
                            starts: state
                                .selected
                                .iter()
                                .map(|i| annotations[*i].start(original))
                                .collect(),
                        });
                    }
                    None if tool == Tool::Text => {
                        let mut a = Annotation::new(
                            Shape::Text(String::new()),
                            state.color,
                            state.text_size,
                            round(o),
                        );
                        a.stand_upright(mapping);
                        annotations.push(a);
                        state.start_typing(annotations.len() - 1);
                    }
                    None => {
                        let (shape, color, width) = match tool {
                            Tool::Arrow => (Shape::Arrow([0, 0]), state.color, state.width),
                            Tool::Frame => (Shape::Rect([0, 0]), state.color, state.width),
                            Tool::Circle => (Shape::Ellipse([0, 0]), state.color, state.width),
                            Tool::Pen => (Shape::Pen(vec![[0, 0]]), state.color, state.width),
                            Tool::Text | Tool::Redact => (
                                Shape::Redact {
                                    size: [0, 0],
                                    style: state.redaction,
                                },
                                [0, 0, 0, 255],
                                (state.width * 3).max(8),
                            ),
                        };
                        annotations.push(Annotation::new(shape, color, width, round(o)));
                        let index = annotations.len() - 1;
                        state.selected = vec![index];
                        state.drag = Some(Drag::Draw { index, from: o });
                    }
                }
            }
        }

        if down || released {
            match &state.drag {
                Some(Drag::Draw { index, from }) => {
                    if let Some(a) = annotations.get_mut(*index) {
                        let mut d = [o[0] - from[0], o[1] - from[1]];
                        if shift && matches!(a.shape, Shape::Rect(_) | Shape::Ellipse(_)) {
                            // as wide as high
                            let side = d[0].abs().max(d[1].abs());
                            d = [side.copysign(d[0]), side.copysign(d[1])];
                        }
                        let corner = [from[0] + d[0].min(0.), from[1] + d[1].min(0.)];
                        let size = round([d[0].abs(), d[1].abs()]);
                        match &mut a.shape {
                            Shape::Arrow(to) => *to = round(d),
                            Shape::Rect(s) | Shape::Ellipse(s) | Shape::Redact { size: s, .. } => {
                                *s = size;
                                a.set_start(round(corner), original);
                            }
                            Shape::Pen(points) => {
                                let at = round(d);
                                if points.last() != Some(&at) {
                                    points.push(at);
                                }
                            }
                            Shape::Text(_) => {}
                        }
                    }
                }
                Some(Drag::Move { from, starts }) => {
                    let d = round([o[0] - from[0], o[1] - from[1]]);
                    for (i, start) in state.selected.iter().zip(starts) {
                        annotations[*i].set_start([start[0] + d[0], start[1] + d[1]], original);
                    }
                }
                None => {}
            }
        }

        if !pointer_over_ui {
            // what a click would select and move
            let pointed = hit(annotations, p).filter(|_| state.drag.is_none());
            ctx.set_cursor_icon(match pointed {
                Some(i) if tool == Tool::Text && is_text(&annotations[i]) => CursorIcon::Text,
                Some(_) => CursorIcon::Move,
                None if matches!(state.drag, Some(Drag::Move { .. })) => CursorIcon::Move,
                None if tool == Tool::Text => CursorIcon::Text,
                None => CursorIcon::Crosshair,
            });
            if let Some(i) = pointed.filter(|i| !state.selected.contains(i)) {
                show(&annotations[i], false);
            }
        }
    }

    if released && let Some(drag) = state.drag.take() {
        match drag {
            Drag::Draw { index, .. } => {
                if annotations.get(index).is_some_and(|a| a.is_empty()) {
                    state.remove(annotations, vec![index]);
                } else if let Some(a) = annotations.get_mut(index) {
                    a.reanchor(original);
                }
            }
            Drag::Move { .. } => {
                for &i in &state.selected {
                    annotations[i].reanchor(original);
                }
            }
        }
    }
    // a double click changes a text
    if released
        && double
        && !pointer_over_ui
        && state.typing.is_none()
        && let Some(pos) = pointer
        && let Some(i) = hit(annotations, to_result(pos)).filter(|i| is_text(&annotations[*i]))
    {
        state.start_typing(i);
    }

    // not while it is typed or drawn
    let drawn = match state.drag {
        Some(Drag::Draw { index, .. }) => Some(index),
        _ => None,
    };
    for &i in &state.selected {
        if state.typing != Some(i) && drawn != Some(i) {
            show(&annotations[i], true);
        }
    }
    copy
}

/// The box to type a text in, over where the text is. Returns whether typing ended.
fn text_ui(
    ctx: &egui::Context,
    annotations: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    mapping: &Mapping,
    geo: &ImageGeometry,
) -> bool {
    let Some(i) = state.typing else {
        return false;
    };
    let Some(a) = annotations.get_mut(i) else {
        state.typing = None;
        return false;
    };
    let t = a.transform(mapping);
    let mut start = Point::zero();
    t.map_point(&mut start);
    let size = (a.width as f32 * scale_of(t) * geo.scale).max(4.);
    let [r, g, b, alpha] = a.color;
    let color = Color32::from_rgba_unmultiplied(r, g, b, alpha.max(80));
    let Shape::Text(text) = &mut a.shape else {
        return false;
    };
    let focus = std::mem::take(&mut state.focus_text);
    let mut done = false;
    let at = pos2(
        start.x * geo.scale + geo.offset.x,
        start.y * geo.scale + geo.offset.y,
    );
    // as far as the image is shown
    let width = (canvas_rect(ctx).right() - at.x).max(size * 3.);
    egui::Area::new(Id::new("annotation_text"))
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .constrain(false)
        .show(ctx, |ui| {
            let edit = egui::TextEdit::multiline(text)
                .font(egui::FontId::proportional(size))
                .text_color(color)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .desired_rows(1)
                .desired_width(width)
                // Enter ends the typing, with Shift a new line starts
                .return_key(egui::KeyboardShortcut::new(
                    egui::Modifiers::SHIFT,
                    Key::Enter,
                ));
            let response = ui.add(edit);
            if focus {
                response.request_focus();
            }
            ui.painter().rect_stroke(
                response.rect.expand(3.),
                2.,
                Stroke::new(1., Color32::from_white_alpha(150)),
                egui::StrokeKind::Outside,
            );
            let enter = response.has_focus()
                && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
            if enter || (response.lost_focus() && !focus) {
                done = true;
            }
        });
    if done {
        state.finish_typing(annotations);
    }
    done
}

/// The lines that show an annotation is pointed at or selected, in its own
/// coordinates, each closed: `gap` around its lines, its arrow, its text or the
/// area it hides. `resolution` is how large it is on the screen.
fn outline(a: &Annotation, gap: f32, resolution: f32) -> Vec<Vec<[f32; 2]>> {
    let w = a.line_width();
    let mut stroke = tiny_skia::Stroke {
        width: w + gap * 2.,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    // of an area, only the line outside of it
    let mut area = false;
    let path = match &a.shape {
        Shape::Arrow(to) => {
            let Some([tip, left, right]) = arrow_head_corners(*to, w) else {
                return vec![];
            };
            // the arrow as one area: along the line on one side, around the head
            // and back on the other side
            let length = tip.x.hypot(tip.y);
            let (nx, ny) = (-tip.y / length * w / 2., tip.x / length * w / 2.);
            let (bx, by) = ((left.x + right.x) / 2., (left.y + right.y) / 2.);
            let mut arrow = PathBuilder::new();
            arrow.move_to(nx, ny);
            arrow.line_to(bx + nx, by + ny);
            arrow.line_to(left.x, left.y);
            arrow.line_to(tip.x, tip.y);
            arrow.line_to(right.x, right.y);
            arrow.line_to(bx - nx, by - ny);
            arrow.line_to(-nx, -ny);
            // around the round end where it starts
            let (backx, backy) = (-tip.x / length * w / 2., -tip.y / length * w / 2.);
            for i in 1..8 {
                let (sin, cos) = (i as f32 / 8. * std::f32::consts::PI).sin_cos();
                arrow.line_to(-nx * cos + backx * sin, -ny * cos + backy * sin);
            }
            arrow.close();
            stroke.width = gap * 2.;
            area = true;
            arrow.finish()
        }
        Shape::Rect(size) => {
            stroke.line_join = LineJoin::Miter;
            tiny_skia::Rect::from_xywh(0., 0., size[0] as f32, size[1] as f32)
                .map(PathBuilder::from_rect)
        }
        Shape::Ellipse(size) => tiny_skia::Rect::from_xywh(0., 0., size[0] as f32, size[1] as f32)
            .and_then(PathBuilder::from_oval),
        Shape::Pen(points) => {
            let mut line = PathBuilder::new();
            for (i, p) in points.iter().enumerate() {
                if i == 0 {
                    line.move_to(p[0] as f32, p[1] as f32);
                } else {
                    line.line_to(p[0] as f32, p[1] as f32);
                }
            }
            if let [only] = points.as_slice() {
                // a dot
                line.line_to(only[0] as f32, only[1] as f32 + 0.01);
                area = true;
            }
            line.finish()
        }
        Shape::Text(_) | Shape::Redact { .. } => {
            let Some(b) = a.bounds() else { return vec![] };
            let (l, t, r, b) = (b[0] - gap, b[1] - gap, b[2] + gap, b[3] + gap);
            return vec![vec![[l, t], [r, t], [r, b], [l, b]]];
        }
    };
    let Some(around) = path.and_then(|path| path.stroke(&stroke, resolution)) else {
        return vec![];
    };
    let mut lines = contours(&around);
    // where it turns, a stroke also goes over itself, inside of what it draws
    for line in &mut lines {
        line.retain(|p| !touches(a, Point::from_xy(p[0], p[1]), gap * 0.5));
        without_sharp_turns(line);
    }
    lines.retain(|line| line.len() > 2);
    if area {
        // the widest one
        let extent = |line: &Vec<[f32; 2]>| {
            let (min, max) = line
                .iter()
                .fold(([f32::MAX; 2], [f32::MIN; 2]), |(min, max), p| {
                    (
                        [min[0].min(p[0]), min[1].min(p[1])],
                        [max[0].max(p[0]), max[1].max(p[1])],
                    )
                });
            (max[0] - min[0]) * (max[1] - min[1])
        };
        lines.sort_by(|a, b| extent(b).total_cmp(&extent(a)));
        lines.truncate(1);
    }
    lines
}

/// Takes out the points of a closed line where it turns back, a line drawn through
/// them would end in a long spike there
fn without_sharp_turns(line: &mut Vec<[f32; 2]>) {
    let mut i = 0;
    let mut kept = 0;
    // until it went once around without taking any out
    while line.len() > 2 && kept < line.len() {
        let n = line.len();
        let (before, at, after) = (line[(i + n - 1) % n], line[i % n], line[(i + 1) % n]);
        let (ax, ay) = (at[0] - before[0], at[1] - before[1]);
        let (bx, by) = (after[0] - at[0], after[1] - at[1]);
        let (a, b) = (ax.hypot(ay), bx.hypot(by));
        // twice the same point, or turned by more than about 100°
        if a < 0.001 || (b >= 0.001 && ax * bx + ay * by < -0.2 * a * b) {
            line.remove(i % n);
            kept = 0;
            // the one before may turn sharply now
            i = (i + n - 2) % (n - 1);
        } else {
            i = (i + 1) % n;
            kept += 1;
        }
    }
}

/// The lines a path is made of, its curves in short straight pieces
fn contours(path: &Path) -> Vec<Vec<[f32; 2]>> {
    let mut lines = vec![];
    let mut line: Vec<[f32; 2]> = vec![];
    let mut last = Point::zero();
    for segment in path.segments() {
        match segment {
            tiny_skia::PathSegment::MoveTo(p) => {
                if line.len() > 1 {
                    lines.push(std::mem::take(&mut line));
                }
                line = vec![[p.x, p.y]];
                last = p;
            }
            tiny_skia::PathSegment::LineTo(p) => {
                line.push([p.x, p.y]);
                last = p;
            }
            tiny_skia::PathSegment::QuadTo(c, p) => {
                for i in 1..=8 {
                    let (t, u) = (i as f32 / 8., 1. - i as f32 / 8.);
                    line.push([
                        u * u * last.x + 2. * u * t * c.x + t * t * p.x,
                        u * u * last.y + 2. * u * t * c.y + t * t * p.y,
                    ]);
                }
                last = p;
            }
            tiny_skia::PathSegment::CubicTo(c1, c2, p) => {
                for i in 1..=8 {
                    let (t, u) = (i as f32 / 8., 1. - i as f32 / 8.);
                    line.push([
                        u * u * u * last.x
                            + 3. * u * u * t * c1.x
                            + 3. * u * t * t * c2.x
                            + t * t * t * p.x,
                        u * u * u * last.y
                            + 3. * u * u * t * c1.y
                            + 3. * u * t * t * c2.y
                            + t * t * t * p.y,
                    ]);
                }
                last = p;
            }
            tiny_skia::PathSegment::Close => {
                if line.len() > 1 {
                    lines.push(std::mem::take(&mut line));
                }
            }
        }
    }
    if line.len() > 1 {
        lines.push(line);
    }
    lines
}

/// Whether a point of the edited image is on an annotation, `slack` pixels around
/// it count too
fn hits(a: &Annotation, mapping: &Mapping, p: [f32; 2], slack: f32) -> bool {
    let t = a.transform(mapping);
    let Some(back) = t.invert() else {
        return false;
    };
    let mut q = Point::from_xy(p[0], p[1]);
    back.map_point(&mut q);
    touches(a, q, slack / scale_of(t).max(0.001))
}

/// How far a point, from the centre of an ellipse with these radii, is from its line.
/// After github.com/0xfaded/ellipse_demo, which comes close in a few steps.
fn ellipse_distance(rx: f32, ry: f32, dx: f32, dy: f32) -> f32 {
    let (px, py) = (dx.abs(), dy.abs());
    let (mut tx, mut ty) = (
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
    );
    for _ in 0..4 {
        let (x, y) = (rx * tx, ry * ty);
        let ex = (rx * rx - ry * ry) * tx.powi(3) / rx;
        let ey = (ry * ry - rx * rx) * ty.powi(3) / ry;
        let r = (x - ex).hypot(y - ey);
        let (qx, qy) = (px - ex, py - ey);
        let q = qx.hypot(qy).max(f32::EPSILON);
        tx = ((qx * r / q + ex) / rx).clamp(0., 1.);
        ty = ((qy * r / q + ey) / ry).clamp(0., 1.);
        let t = tx.hypot(ty).max(f32::EPSILON);
        (tx, ty) = (tx / t, ty / t);
    }
    (px - rx * tx).hypot(py - ry * ty)
}

/// How far a point is from the straight line between two others
fn segment_distance(q: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx * dx + dy * dy;
    let along = if length == 0. {
        0.
    } else {
        (((q.x - a.x) * dx + (q.y - a.y) * dy) / length).clamp(0., 1.)
    };
    (q.x - a.x - along * dx).hypot(q.y - a.y - along * dy)
}

/// Whether a point in the annotation's own coordinates is on it, or `slack` off it
fn touches(a: &Annotation, q: Point, slack: f32) -> bool {
    let near = a.line_width() / 2. + slack;
    let point = |p: [i32; 2]| Point::from_xy(p[0] as f32, p[1] as f32);
    let to_line = |p0: [i32; 2], p1: [i32; 2]| segment_distance(q, point(p0), point(p1));
    match &a.shape {
        Shape::Arrow(to) => match arrow_head_corners(*to, a.line_width()) {
            None => to_line([0, 0], *to) <= near,
            Some([tip, left, right]) => {
                // the line ends below the head, as it is drawn
                let length = tip.x.hypot(tip.y);
                let into = 1. - arrow_head(a.line_width(), length).0 * 0.5 / length;
                let end = Point::from_xy(tip.x * into, tip.y * into);
                let sides = [(tip, left), (left, right), (right, tip)];
                // on the same side of all sides of the head is inside it
                let turns = sides
                    .map(|(p0, p1)| (p1.x - p0.x) * (q.y - p0.y) - (p1.y - p0.y) * (q.x - p0.x));
                segment_distance(q, Point::zero(), end) <= near
                    || turns.iter().all(|t| *t >= 0.)
                    || turns.iter().all(|t| *t <= 0.)
                    || sides
                        .iter()
                        .any(|(p0, p1)| segment_distance(q, *p0, *p1) <= slack)
            }
        },
        Shape::Rect(size) => {
            let (w, h) = (size[0] as f32, size[1] as f32);
            let inside = |by: f32| q.x >= -by && q.y >= -by && q.x <= w + by && q.y <= h + by;
            inside(near) && !inside(-near)
        }
        Shape::Ellipse(size) => {
            let (rx, ry) = (size[0] as f32 / 2., size[1] as f32 / 2.);
            if rx.min(ry) <= 0. {
                return to_line([0, 0], *size) <= near;
            }
            ellipse_distance(rx, ry, q.x - rx, q.y - ry) <= near
        }
        Shape::Pen(points) => match points.as_slice() {
            [only] => to_line(*only, *only) <= near,
            points => points.windows(2).any(|w| to_line(w[0], w[1]) <= near),
        },
        Shape::Text(_) | Shape::Redact { .. } => a.bounds().is_some_and(|b| {
            q.x >= b[0] - slack && q.y >= b[1] - slack && q.x <= b[2] + slack && q.y <= b[3] + slack
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(operation: ImageOperation) -> ImgOpItem {
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

    fn arrow_at(start: [i32; 2], to: [i32; 2]) -> Annotation {
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
        let layout = text_layout("Hello", 40, [255, 255, 255, 255]).expect("Inter lays out text");
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
        ];
        for shape in shapes {
            let a = Annotation::new(shape, [0; 4], 6, [80, 40]);
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
        let op = ImageOperation::Annotate(vec![
            arrow_at([1, 2], [3, 4]),
            Annotation::new(Shape::Ellipse([8, 9]), [1, 2, 3, 4], 5, [6, 7]),
            Annotation::new(Shape::Text("ü & <b>".into()), [1, 2, 3, 4], 5, [6, 7]),
        ]);
        let json = serde_json::to_string(&op).expect("saved");
        let back: ImageOperation = serde_json::from_str(&json).expect("read");
        assert_eq!(op, back);
    }
}
