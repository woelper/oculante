//! Arrows, frames, text, pen lines and hidden areas on top of the edited image.
//!
//! Annotations are kept in pixels of the image as it was loaded. Crops, resizes,
//! rotations and flips take them along with what they point at. Each one keeps its
//! distance to the nearest corner, edge middle or the centre of the image, so it
//! stays there on an image of another size. They are drawn after all other edits.

use std::ops::RangeInclusive;
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
    /// A line this wide around what it draws, 0 for none
    #[serde(default)]
    pub outline: u32,
    #[serde(default = "white")]
    pub outline_color: [u8; 4],
    /// How round the corners of a frame are
    #[serde(default)]
    pub radius: u32,
    /// Of a text
    #[serde(default)]
    pub font: Font,
    /// Kept in the list, but not drawn
    #[serde(default)]
    pub hidden: bool,
}

fn white() -> [u8; 4] {
    [255; 4]
}

/// The font of a text
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub struct Font {
    /// Empty for Inter
    pub family: String,
    /// 400 is regular, 700 bold
    pub weight: u16,
}

impl Default for Font {
    fn default() -> Self {
        Self {
            family: String::new(),
            weight: 400,
        }
    }
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
            outline: 0,
            outline_color: white(),
            radius: 0,
            font: Font::default(),
            hidden: false,
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
            Shape::Text(text) => text_layout(text, self).map(|l| l.bounds),
        }
    }

    /// How far its lines and its outline reach out of its bounds, in its own
    /// coordinates
    fn reach(&self) -> f32 {
        let half = self.line_width() / 2.;
        let outline = self.outline_width();
        match &self.shape {
            Shape::Arrow(to) => {
                let length = (to[0] as f32).hypot(to[1] as f32);
                half.max(arrow_head(self.line_width(), length).1) + outline
            }
            Shape::Rect(_) | Shape::Ellipse(_) | Shape::Pen(_) => half + outline,
            Shape::Text(_) => outline,
            Shape::Redact { .. } => 0.,
        }
    }

    fn line_width(&self) -> f32 {
        self.width.max(1) as f32
    }

    /// A redaction has none
    fn outline_width(&self) -> f32 {
        match self.shape {
            Shape::Redact { .. } => 0.,
            _ => self.outline as f32,
        }
    }

    /// Of a frame, at most half of its shorter side
    fn corner_radius(&self) -> f32 {
        match self.shape {
            Shape::Rect(size) => (self.radius as f32)
                .min(size[0].abs() as f32 / 2.)
                .min(size[1].abs() as f32 / 2.),
            _ => 0.,
        }
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
    // the bold one calls itself "Inter 18pt", here it is the bold of Inter
    let bold: Vec<_> = fonts
        .faces()
        .filter(|face| face.families.iter().any(|(name, _)| name == "Inter 18pt"))
        .cloned()
        .collect();
    for mut face in bold {
        fonts.remove_face(face.id);
        face.families = vec![("Inter".into(), usvg::fontdb::Language::English_UnitedStates)];
        fonts.push_face_info(face);
    }
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
    font: Font,
    outline: u32,
    outline_color: [u8; 4],
    fonts: u32,
}

/// Text laid out lately, the newest first
static TEXTS: Mutex<Vec<(TextKey, Arc<TextLayout>)>> = Mutex::new(Vec::new());

/// Distance of the lines of a text, in sizes of the text
const LINE_HEIGHT: f32 = 1.25;

/// The text of an annotation, in its size, colors and font
fn text_layout(text: &str, a: &Annotation) -> Option<Arc<TextLayout>> {
    // first, so the text is laid out with the fonts this key says
    let fonts = fonts();
    let key = TextKey {
        text: text.into(),
        size: a.width,
        color: a.color,
        font: a.font.clone(),
        outline: a.outline,
        outline_color: a.outline_color,
        fonts: FONTS_VERSION.load(Ordering::Relaxed),
    };
    let mut texts = TEXTS.lock().ok()?;
    if let Some(i) = texts.iter().position(|(k, _)| *k == key) {
        let found = texts.remove(i);
        let layout = found.1.clone();
        texts.insert(0, found);
        return Some(layout);
    }
    let layout = Arc::new(lay_out(&key, fonts)?);
    texts.insert(0, (key, layout.clone()));
    texts.truncate(32);
    Some(layout)
}

fn lay_out(key: &TextKey, fonts: Arc<usvg::fontdb::Database>) -> Option<TextLayout> {
    let size = key.size.max(1) as f32;
    let escape = |line: &str| {
        line.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let mut lines = String::new();
    for (i, line) in key.text.split('\n').enumerate() {
        let dy = if i == 0 { 0. } else { size * LINE_HEIGHT };
        // an empty line still takes its place
        let line = if line.is_empty() {
            "\u{a0}".into()
        } else {
            escape(line)
        };
        lines += &format!(r#"<tspan x="0" dy="{dy}">{line}</tspan>"#);
    }
    let [r, g, b, a] = key.color;
    // Inter where the font lacks a letter, or is not there
    let family = key.font.family.replace(['\'', '"'], "");
    let family = if family.is_empty() {
        "Inter".into()
    } else {
        format!("'{}', Inter", escape(&family))
    };
    // half of it is under the letters
    let outline = match key.outline {
        0 => String::new(),
        width => {
            let [r, g, b, a] = key.outline_color;
            format!(
                r#" stroke="rgb({r},{g},{b})" stroke-opacity="{}" stroke-width="{}" stroke-linejoin="round" paint-order="stroke""#,
                a as f32 / 255.,
                width * 2
            )
        }
    };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><text y="{size}" font-family="{family}" font-weight="{}" font-size="{size}" fill="rgb({r},{g},{b})" fill-opacity="{}"{outline} xml:space="preserve">{lines}</text></svg>"#,
        key.font.weight,
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

/// Left, top, right and bottom in the edited image of its bounds, grown by `grow`
/// in its own coordinates
fn extent(a: &Annotation, t: Transform, grow: f32) -> Option<[f32; 4]> {
    let b = a.bounds()?;
    let mut corners = [
        Point::from_xy(b[0] - grow, b[1] - grow),
        Point::from_xy(b[2] + grow, b[1] - grow),
        Point::from_xy(b[0] - grow, b[3] + grow),
        Point::from_xy(b[2] + grow, b[3] + grow),
    ];
    t.map_points(&mut corners);
    Some(
        corners
            .iter()
            .fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |e, c| {
                [e[0].min(c.x), e[1].min(c.y), e[2].max(c.x), e[3].max(c.y)]
            }),
    )
}

/// The ones that are shown and lie wholly inside the box between two corners in the
/// edited image, their lines not counted
fn in_box(annotations: &[Annotation], mapping: &Mapping, a: [f32; 2], b: [f32; 2]) -> Vec<usize> {
    let (left, top) = (a[0].min(b[0]), a[1].min(b[1]));
    let (right, bottom) = (a[0].max(b[0]), a[1].max(b[1]));
    (0..annotations.len())
        .filter(|i| {
            let x = &annotations[*i];
            !x.hidden
                && extent(x, x.transform(mapping), 0.)
                    .is_some_and(|e| e[0] >= left && e[1] >= top && e[2] <= right && e[3] <= bottom)
        })
        .collect()
}

/// The part of the edited image an annotation draws on
fn area(a: &Annotation, t: Transform, size: (u32, u32)) -> Option<Rect> {
    let [left, top, right, bottom] = extent(a, t, a.reach())?;
    // smoothed edges reach a little further
    let smooth = if matches!(a.shape, Shape::Redact { .. }) {
        0.
    } else {
        2.
    };
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
    let paint = |[r, g, b, alpha]: [u8; 4]| {
        let mut paint = Paint::default();
        paint.set_color_rgba8(r, g, b, alpha);
        paint.anti_alias = true;
        paint
    };
    // the outline first, the annotation over it
    let outline = a.outline_width();
    let mut passes = vec![(paint(a.color), 0.)];
    if outline > 0. {
        passes.insert(0, (paint(a.outline_color), outline));
    }
    for (paint, outline) in passes {
        let stroke = tiny_skia::Stroke {
            width: a.line_width() + outline * 2.,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        match &a.shape {
            Shape::Arrow(to) => {
                if let Some((line, tip)) = arrow(*to, a.line_width()) {
                    layer.stroke_path(&line, &paint, &stroke, t, None);
                    layer.fill_path(&tip, &paint, FillRule::Winding, t, None);
                    if outline > 0. {
                        let around = tiny_skia::Stroke {
                            width: outline * 2.,
                            ..stroke.clone()
                        };
                        layer.stroke_path(&tip, &paint, &around, t, None);
                    }
                }
            }
            Shape::Rect(size) => {
                if let Some(frame) = frame_path(*size, a.corner_radius()) {
                    let stroke = tiny_skia::Stroke {
                        line_join: LineJoin::Miter,
                        ..stroke
                    };
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
                        PathBuilder::from_circle(only[0] as f32, only[1] as f32, stroke.width / 2.);
                    if let Some(dot) = dot {
                        layer.fill_path(&dot, &paint, FillRule::Winding, t, None);
                    }
                } else if let Some(line) = polyline(points) {
                    layer.stroke_path(&line, &paint, &stroke, t, None);
                }
            }
            // the outline of a text is a part of it
            Shape::Text(text) if outline == 0. => {
                if let Some(layout) = text_layout(text, a) {
                    resvg::render(&layout.tree, t, &mut layer.as_mut());
                }
            }
            Shape::Text(_) | Shape::Redact { .. } => {}
        }
    }
    blend(img, &layer, area);
}

/// A frame this large, its corners rounded by `radius`
fn frame_path(size: [i32; 2], radius: f32) -> Option<Path> {
    let (w, h) = (size[0] as f32, size[1] as f32);
    let mut frame = PathBuilder::new();
    if radius <= 0. {
        frame.move_to(0., 0.);
        frame.line_to(w, 0.);
        frame.line_to(w, h);
        frame.line_to(0., h);
    } else {
        // a quarter of a circle, as a curve
        let r = radius;
        let k = r * (1. - 0.552_284_8);
        frame.move_to(r, 0.);
        frame.line_to(w - r, 0.);
        frame.cubic_to(w - k, 0., w, k, w, r);
        frame.line_to(w, h - r);
        frame.cubic_to(w, h - k, w - k, h, w - r, h);
        frame.line_to(r, h);
        frame.cubic_to(k, h, 0., h - k, 0., h - r);
        frame.line_to(0., r);
        frame.cubic_to(0., k, k, 0., r, 0.);
    }
    frame.close();
    frame.finish()
}

/// A line through the points of a pen
fn polyline(points: &[[i32; 2]]) -> Option<Path> {
    let mut line = PathBuilder::new();
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            line.move_to(p[0] as f32, p[1] as f32);
        } else {
            line.line_to(p[0] as f32, p[1] as f32);
        }
    }
    line.finish()
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
                .filter(|(i, a)| state.typing != Some(*i) && !a.hidden)
                .map(|(_, a)| a.clone())
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// Selects and moves, draws nothing
    Select,
    Arrow,
    Frame,
    Circle,
    Text,
    Pen,
    Redact,
}

impl Tool {
    /// Those that draw, a tile each
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
            Tool::Select => "Select",
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
            Tool::Select => "Click what to change, drag a box around what to select",
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
            Tool::Select => Icon::Pointer,
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
    Pointer,
    Arrow,
    Frame,
    Circle,
    Text,
    Pen,
    Pixelate,
    Fill,
}

/// Draws an icon in the middle of `rect`, in lines as thick as those of the icon
/// font, `scale` times as large
fn paint_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: Color32, scale: f32) {
    // in a box of 16 by 16
    let at = |x: f32, y: f32| rect.center() + egui::vec2(x - 8., y - 8.) * scale;
    let stroke = Stroke::new(1.6 * scale, color);
    let square = egui::Rect::from_min_max(at(2.5, 3.5), at(13.5, 12.5));
    match icon {
        Icon::Pointer => {
            let tip = [
                (4., 1.5),
                (4., 13.5),
                (7., 10.6),
                (9.2, 14.8),
                (11.1, 13.9),
                (9., 9.8),
                (13., 9.6),
            ];
            let points = tip.into_iter().map(|(x, y)| at(x, y)).collect();
            painter.add(egui::Shape::closed_line(points, stroke));
        }
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
            painter.circle_stroke(at(8., 8.), 5.8 * scale, stroke);
        }
        Icon::Text => {
            painter.text(
                at(8., 8.),
                egui::Align2::CENTER_CENTER,
                "Aa",
                egui::FontId::proportional(13. * scale),
                color,
            );
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
                        egui::Rect::from_min_size(min, egui::vec2(4., 4.) * scale),
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
    /// A box from here in the edited image selects what is in it, besides what was
    /// selected before
    Select { from: [f32; 2], before: Vec<usize> },
}

/// What the mouse and the keys do with annotations
#[derive(Debug, Clone)]
pub struct AnnotateUi {
    /// What a drag over the image does. With none, the image is moved as always.
    pub tool: Option<Tool>,
    /// The tools are shown in the panel, while they are folded away there is no
    /// annotating
    open: bool,
    pub color: [u8; 4],
    /// For new lines and arrows, 0 until an image tells how wide
    pub width: u32,
    /// For new text
    pub text_size: u32,
    pub redaction: Redaction,
    /// The rest of what new annotations look like, as the one changed last
    outline: u32,
    outline_color: [u8; 4],
    radius: u32,
    font: Font,
    /// Of new redactions
    blocks: u32,
    redact_color: [u8; 4],
    /// The card in the panel the mouse is over
    pointed_card: Option<usize>,
    /// A list or a color picker was open when the frame began, Esc closes it
    popup_open: bool,
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
            open: false,
            color: [235, 40, 40, 255],
            width: 0,
            text_size: 0,
            redaction: Redaction::Pixelate,
            outline: 0,
            outline_color: white(),
            radius: 0,
            font: Font::default(),
            blocks: 0,
            redact_color: [0, 0, 0, 255],
            pointed_card: None,
            popup_open: false,
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
            self.blocks = (self.width * 3).max(8);
        }
    }

    /// A new annotation for the tool, at the top left of the image, if it draws one
    fn template(&self, tool: Tool) -> Option<Annotation> {
        let (shape, color, width) = match tool {
            Tool::Select => return None,
            Tool::Arrow => (Shape::Arrow([0, 0]), self.color, self.width),
            Tool::Frame => (Shape::Rect([0, 0]), self.color, self.width),
            Tool::Circle => (Shape::Ellipse([0, 0]), self.color, self.width),
            Tool::Pen => (Shape::Pen(vec![[0, 0]]), self.color, self.width),
            Tool::Text => (Shape::Text(String::new()), self.color, self.text_size),
            Tool::Redact => {
                let style = self.redaction;
                let shape = Shape::Redact {
                    size: [0, 0],
                    style,
                };
                return Some(Annotation::new(
                    shape,
                    self.redact_color,
                    self.blocks,
                    [0, 0],
                ));
            }
        };
        let mut a = Annotation::new(shape, color, width, [0, 0]);
        a.outline = self.outline;
        a.outline_color = self.outline_color;
        match tool {
            Tool::Frame => a.radius = self.radius,
            Tool::Text => a.font = self.font.clone(),
            _ => {}
        }
        Some(a)
    }

    /// The next annotations look like this one
    fn remember(&mut self, a: &Annotation) {
        match &a.shape {
            Shape::Redact { style, .. } => {
                self.redaction = *style;
                self.blocks = a.width;
                self.redact_color = a.color;
                return;
            }
            Shape::Text(_) => {
                self.text_size = a.width;
                self.font = a.font.clone();
            }
            Shape::Rect(_) => {
                self.width = a.width;
                self.radius = a.radius;
            }
            Shape::Arrow(_) | Shape::Ellipse(_) | Shape::Pen(_) => self.width = a.width,
        }
        self.color = a.color;
        self.outline = a.outline;
        self.outline_color = a.outline_color;
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

/// The annotations part of the edit panel: a button to select and a tile for each
/// tool in a box that folds away, then a card for each annotation, the one in front
/// first. Like the cards of the filters they stay when it is folded, as they are
/// still on the image. Returns whether the edits changed.
pub fn panel_ui(ui: &mut Ui, stack: &mut Vec<ImgOpItem>, state: &mut AnnotateUi) -> bool {
    let mut changed = false;
    state.pointed_card = None;
    // before the lists of this frame are shown, which close on Esc
    state.popup_open = egui::Popup::is_any_open(ui.ctx());
    let mut toggled = false;
    ui.scope(|ui| {
        ui.style_mut().visuals.collapsing_header_frame = true;
        ui.style_mut().visuals.indent_has_left_vline = false;
        let header = egui::CollapsingHeader::new("Annotations")
            .icon(crate::ui::caret_icon)
            .open(Some(state.open))
            .show_unindented(ui, |ui| {
                crate::ui::dark_panel(ui, |ui| {
                    let mut picked = None;
                    if select_button(ui, state.tool == Some(Tool::Select))
                        .on_hover_text(Tool::Select.hint())
                        .clicked()
                    {
                        picked = Some(Tool::Select);
                    }
                    let spacing = ui.spacing().item_spacing.x;
                    let width = ((ui.available_width() - spacing * 2.) / 3.).floor();
                    for row in Tool::ALL.chunks(3) {
                        ui.horizontal(|ui| {
                            for &tool in row {
                                let button = egui::Button::new("").min_size(egui::vec2(width, 40.));
                                let response = lit(ui, state.tool == Some(tool), button);
                                let color = ui.style().interact(&response).fg_stroke.color;
                                paint_icon(ui.painter(), response.rect, tool.icon(), color, 1.5);
                                if response
                                    .on_hover_text(format!("{}\n{}", tool.name(), tool.hint()))
                                    .clicked()
                                {
                                    picked = Some(tool);
                                }
                            }
                        });
                    }
                    if let Some(tool) = picked {
                        let (annotations, created) = annotations_or_new(stack);
                        changed |= created;
                        state.finish_typing(annotations);
                        state.tool = (state.tool != Some(tool)).then_some(tool);
                    }
                });
                match state.tool {
                    Some(Tool::Select) => {
                        ui.small(
                            "Click to select, Shift+click selects more, drag a box around \
                             what to select. Drag what you point at to move it, double click \
                             a text to change it. Del removes what is selected, Esc stops \
                             annotating.",
                        );
                    }
                    Some(_) => {
                        ui.small(
                            "Drag what you point at to move it, Shift+click selects more, \
                             double click a text to change it. Del removes what is selected, \
                             Esc goes back to selecting.",
                        );
                    }
                    None => {}
                }
            });
        toggled = header.header_response.clicked();
    });
    if toggled {
        state.open = !state.open;
        ui.ctx().request_repaint();
        let any = annotations_mut(stack).is_some_and(|a| !a.is_empty());
        if !state.open {
            state.tool = None;
        } else if any {
            // there is something to select
            state.tool.get_or_insert(Tool::Select);
        }
    }
    if let Some(annotations) = annotations_mut(stack) {
        cards_ui(ui, annotations, state);
    }
    changed
}

/// As wide as the box of the tools, tinted while it is on
fn select_button(ui: &mut Ui, on: bool) -> egui::Response {
    let accent = crate::ui::accent_color(ui.ctx());
    let mut text = egui::RichText::new(Tool::Select.name());
    if on {
        text = text.color(accent);
    }
    let mut button = egui::Button::new(text).min_size(egui::vec2(0., 32.));
    if on {
        button = button.fill(tint(ui));
    }
    // the text in the middle
    let response = ui.vertical_centered_justified(|ui| ui.add(button)).inner;
    let color = if on {
        accent
    } else {
        ui.style().interact(&response).fg_stroke.color
    };
    let icon = egui::Rect::from_center_size(
        egui::pos2(response.rect.left() + 22., response.rect.center().y),
        egui::vec2(16., 16.),
    );
    paint_icon(ui.painter(), icon, Icon::Pointer, color, 1.2);
    response
}

/// The color of the select button while it is on and of the cards of what is
/// selected
fn tint(ui: &Ui) -> Color32 {
    card_fill(ui).lerp_to_gamma(crate::ui::accent_color(ui.ctx()), 0.2)
}

/// The card of what comes next while nothing is selected, then one for each
/// annotation, the one in front first
fn cards_ui(ui: &mut Ui, annotations: &mut Vec<Annotation>, state: &mut AnnotateUi) {
    // what comes next, while nothing is selected
    if let Some(tool) = state.tool
        && state.selected.is_empty()
        && state.typing.is_none()
        && let Some(mut next) = state.template(tool)
    {
        card(ui, false, |ui| {
            ui.label(format!("Next {}", tool.name().to_lowercase()));
            if options_ui(ui, &mut next, usize::MAX) {
                state.remember(&next);
            }
        });
    }

    let names = names(annotations);
    let count = annotations.len();
    let mut swap = None;
    let mut remove = None;
    for i in (0..count).rev() {
        let selected = state.selected.contains(&i);
        let response = card(ui, selected, |ui| {
            ui.horizontal(|ui| {
                let size = 10.;
                let small = |text: &str| {
                    egui::Button::new(egui::RichText::new(text).size(size)).frame(false)
                };
                // the list goes from the front to the back
                ui.add_enabled_ui(i + 1 < count, |ui| {
                    if ui
                        .add(small(crate::icons::CHEVRON_UP))
                        .on_hover_text("Bring forward")
                        .clicked()
                    {
                        swap = Some((i, i + 1));
                    }
                });
                ui.add_enabled_ui(i > 0, |ui| {
                    if ui
                        .add(small(crate::icons::CHEVRON_DOWN))
                        .on_hover_text("Send backward")
                        .clicked()
                    {
                        swap = Some((i - 1, i));
                    }
                });
                let a = &mut annotations[i];
                let eye = if a.hidden {
                    crate::icons::EYEOFF
                } else {
                    crate::icons::EYE
                };
                if ui
                    .add(small(eye))
                    .on_hover_text(if a.hidden { "Show" } else { "Hide" })
                    .clicked()
                {
                    a.hidden = !a.hidden;
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new(crate::icons::X).size(size * 1.5))
                            .frame(false),
                    )
                    .on_hover_text("Remove")
                    .clicked()
                {
                    remove = Some(i);
                }
                ui.add_space(size / 2.);
                let mut name = egui::RichText::new(&names[i]);
                if selected {
                    name = name.color(crate::ui::accent_color(ui.ctx()));
                } else if a.hidden {
                    name = name.weak();
                }
                if ui
                    .add(
                        egui::Label::new(name)
                            .truncate()
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_text("Select")
                    .clicked()
                {
                    state.finish_typing(annotations);
                    state.selected = vec![i];
                    // the selection lasts while annotating
                    state.tool.get_or_insert(Tool::Select);
                    state.open = true;
                }
            });
            if selected
                && let Some(a) = annotations.get_mut(i)
                && options_ui(ui, a, i)
            {
                state.remember(a);
            }
        });
        if response.hovered() {
            state.pointed_card = Some(i);
        }
    }
    if let Some((a, b)) = swap {
        annotations.swap(a, b);
        let other = |i: usize| match i {
            i if i == a => b,
            i if i == b => a,
            i => i,
        };
        for s in &mut state.selected {
            *s = other(*s);
        }
        state.typing = state.typing.map(other);
    }
    if let Some(i) = remove {
        state.remove(annotations, vec![i]);
    }
}

/// The annotations of the edits, which get them if they have none
fn annotations_or_new(stack: &mut Vec<ImgOpItem>) -> (&mut Vec<Annotation>, bool) {
    let found = stack
        .iter()
        .position(|op| matches!(op.operation, ImageOperation::Annotate(_)));
    let created = found.is_none();
    let i = found.unwrap_or_else(|| {
        stack.push(ImgOpItem::new(ImageOperation::Annotate(vec![])));
        stack.len() - 1
    });
    let op = &mut stack[i];
    op.active = true;
    match &mut op.operation {
        ImageOperation::Annotate(annotations) => (annotations, created),
        _ => unreachable!("found as one"),
    }
}

/// The names in the list, numbered for each kind
fn names(annotations: &[Annotation]) -> Vec<String> {
    let mut counts = std::collections::HashMap::new();
    annotations
        .iter()
        .map(|a| match &a.shape {
            Shape::Text(_) => a.label(),
            _ => {
                let label = a.label();
                let n = counts.entry(label.clone()).or_insert(0);
                *n += 1;
                format!("{label} {n}")
            }
        })
        .collect()
}

/// A card as those of the other edits
/// Tinted for what is selected
fn card(ui: &mut Ui, tinted: bool, add_contents: impl FnOnce(&mut Ui)) -> egui::Response {
    egui::Frame::new()
        .fill(if tinted { tint(ui) } else { card_fill(ui) })
        .corner_radius(ui.visuals().widgets.active.corner_radius)
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.allocate_space(egui::vec2(ui.available_width(), 0.));
            add_contents(ui);
        })
        .response
}

fn card_fill(ui: &Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_gray(33)
    } else {
        Color32::from_gray(242)
    }
}

/// What can be set for an annotation: what only its kind has, a line, then its
/// colors and outline. `id` keeps the lists of fonts of the cards apart. Returns
/// whether it changed.
fn options_ui(ui: &mut Ui, a: &mut Annotation, id: usize) -> bool {
    use crate::icons::{ARROWS_OUT, ARROWS_OUT_SIMPLE, PALETTE, PENCIL_SIMPLE_LINE};
    let before = a.clone();
    match &mut a.shape {
        Shape::Arrow(_) | Shape::Ellipse(_) | Shape::Pen(_) => {
            slider_row(
                ui,
                format!("{PENCIL_SIMPLE_LINE} Width"),
                &mut a.width,
                1..=100,
            );
        }
        Shape::Rect(_) => {
            slider_row(
                ui,
                format!("{PENCIL_SIMPLE_LINE} Width"),
                &mut a.width,
                1..=100,
            );
            slider_row(
                ui,
                format!("{ARROWS_OUT_SIMPLE} Radius"),
                &mut a.radius,
                0..=100,
            );
        }
        Shape::Text(_) => {
            slider_row(
                ui,
                format!("{PENCIL_SIMPLE_LINE} Size"),
                &mut a.width,
                4..=400,
            );
            font_ui(ui, &mut a.font, id);
        }
        Shape::Redact { style, .. } => {
            ui.horizontal(|ui| {
                for (way, icon, name) in [
                    (Redaction::Pixelate, Icon::Pixelate, "Pixelate"),
                    (Redaction::Fill, Icon::Fill, "Fill with the color"),
                ] {
                    if icon_button(ui, *style == way, icon)
                        .on_hover_text(name)
                        .clicked()
                    {
                        *style = way;
                    }
                }
            });
            if *style == Redaction::Pixelate {
                slider_row(
                    ui,
                    format!("{PENCIL_SIMPLE_LINE} Blocks"),
                    &mut a.width,
                    2..=100,
                );
            } else {
                color_row(ui, format!("{PALETTE} Color"), &mut a.color);
            }
            return *a != before;
        }
    }
    ui.separator();
    slider_row(ui, format!("{ARROWS_OUT} Outline"), &mut a.outline, 0..=40);
    color_row(ui, format!("{PALETTE} Color"), &mut a.color);
    ui.add_enabled_ui(a.outline > 0, |ui| {
        color_row(ui, format!("{PALETTE} Outline color"), &mut a.outline_color);
    });
    *a != before
}

/// The labels of the sliders take this much room, so the sliders line up
const LABEL_WIDTH: f32 = 96.;

fn slider_row(ui: &mut Ui, label: String, value: &mut u32, range: RangeInclusive<u32>) {
    use crate::ui::EguiExt;
    // a value set elsewhere stays in reach
    let range = *range.start()..=(*range.end()).max(*value);
    ui.horizontal(|ui| {
        let height = ui.spacing().interact_size.y;
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_WIDTH, height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| ui.label_i(label),
        );
        // the value takes 40 points at the right
        let spacing = ui.spacing().item_spacing.x;
        ui.spacing_mut().slider_width = (ui.available_width() - 40. - 2. * spacing).max(40.);
        ui.styled_slider(value, range);
    });
}

fn color_row(ui: &mut Ui, label: String, color: &mut [u8; 4]) {
    use crate::ui::EguiExt;
    ui.horizontal(|ui| {
        ui.label_i(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.color_edit_button_srgba_unmultiplied(color);
        });
    });
}

/// The font and its weight
fn font_ui(ui: &mut Ui, font: &mut Font, id: usize) {
    let families = font_families();
    let shown = |family: &str| {
        if family.is_empty() {
            "Inter".to_string()
        } else {
            family.to_string()
        }
    };
    ui.label("Font");
    egui::ComboBox::from_id_salt(("annotation font", id))
        .selected_text(shown(&font.family))
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for (family, _) in families.iter() {
                // Inter is the one without a name
                let value = if family == "Inter" {
                    String::new()
                } else {
                    family.clone()
                };
                ui.selectable_value(&mut font.family, value, family);
            }
        });
    let weights = families
        .iter()
        .find(|(family, _)| *family == shown(&font.family))
        .map(|(_, weights)| weights.clone())
        .unwrap_or_else(|| vec![400]);
    ui.label("Weight");
    egui::ComboBox::from_id_salt(("annotation weight", id))
        .selected_text(weight_name(font.weight))
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for weight in weights {
                ui.selectable_value(&mut font.weight, weight, weight_name(weight));
            }
        });
}

fn weight_name(weight: u16) -> String {
    match weight {
        100 => "Thin".into(),
        200 => "Extra light".into(),
        300 => "Light".into(),
        400 => "Regular".into(),
        500 => "Medium".into(),
        600 => "Semibold".into(),
        700 => "Bold".into(),
        800 => "Extra bold".into(),
        900 => "Black".into(),
        other => other.to_string(),
    }
}

/// The families of the fonts with the weights each has, Inter first
type Families = Arc<Vec<(String, Vec<u16>)>>;

static FAMILIES: Mutex<Option<(u32, Families)>> = Mutex::new(None);

fn font_families() -> Families {
    let version = FONTS_VERSION.load(Ordering::Relaxed);
    let fonts = fonts();
    let Ok(mut known) = FAMILIES.lock() else {
        return Default::default();
    };
    if let Some((v, families)) = &*known
        && *v == version
    {
        return families.clone();
    }
    let mut weights = std::collections::BTreeMap::<String, Vec<u16>>::new();
    for face in fonts.faces() {
        if face.style != usvg::fontdb::Style::Normal {
            continue;
        }
        if let Some((family, _)) = face.families.first() {
            let list = weights.entry(family.clone()).or_default();
            if !list.contains(&face.weight.0) {
                list.push(face.weight.0);
            }
        }
    }
    let mut families: Vec<(String, Vec<u16>)> = weights
        .into_iter()
        .map(|(family, mut list)| {
            list.sort_unstable();
            (family, list)
        })
        .collect();
    if let Some(i) = families.iter().position(|(family, _)| family == "Inter") {
        let inter = families.remove(i);
        families.insert(0, inter);
    }
    let families = Arc::new(families);
    *known = Some((version, families.clone()));
    families
}

fn icon_button(ui: &mut Ui, on: bool, icon: Icon) -> egui::Response {
    let response = lit(ui, on, egui::Button::new("").min_size(egui::vec2(28., 24.)));
    let color = ui.style().interact(&response).fg_stroke.color;
    paint_icon(ui.painter(), response.rect, icon, color, 1.);
    response
}

fn lit<'a>(ui: &mut Ui, on: bool, button: egui::Button<'a>) -> egui::Response {
    ui.add(if on {
        button.fill(crate::ui::accent_color(ui.ctx()))
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
                // that only closes a list
                i.key_pressed(Key::Escape) && !std::mem::take(&mut state.popup_open),
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
                // from drawing back to selecting, then no more annotating
                state.tool = (tool != Tool::Select).then_some(Tool::Select);
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
            .find(|i| !annotations[*i].hidden && hits(&annotations[*i], mapping, p, slack))
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
                    // a box selects what is in it
                    None if tool == Tool::Select => {
                        let before = if shift {
                            state.selected.clone()
                        } else {
                            vec![]
                        };
                        state.selected = before.clone();
                        state.drag = Some(Drag::Select { from: p, before });
                    }
                    None if tool == Tool::Text => {
                        if let Some(mut a) = state.template(tool) {
                            a.offset = round(o);
                            a.stand_upright(mapping);
                            annotations.push(a);
                            state.start_typing(annotations.len() - 1);
                        }
                    }
                    None => {
                        if let Some(mut a) = state.template(tool) {
                            a.offset = round(o);
                            annotations.push(a);
                            let index = annotations.len() - 1;
                            state.selected = vec![index];
                            state.drag = Some(Drag::Draw { index, from: o });
                        }
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
                Some(Drag::Select { from, before }) => {
                    let mut selected = before.clone();
                    for i in in_box(annotations, mapping, *from, p) {
                        if !selected.contains(&i) {
                            selected.push(i);
                        }
                    }
                    state.selected = selected;
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
                None if tool == Tool::Select => CursorIcon::Default,
                None if tool == Tool::Text => CursorIcon::Text,
                None => CursorIcon::Crosshair,
            });
            if let Some(i) = pointed.filter(|i| !state.selected.contains(i)) {
                show(&annotations[i], false);
            }
        }
    }

    // the box that selects
    if down
        && let Some(Drag::Select { from, .. }) = &state.drag
        && let Some(pos) = pointer
    {
        let accent = crate::ui::accent_color(ctx);
        let corner = to_screen(Point::from_xy(from[0], from[1]));
        painter.rect(
            egui::Rect::from_two_pos(corner, pos),
            0.,
            accent.gamma_multiply(0.15),
            Stroke::new(1., accent),
            egui::StrokeKind::Inside,
        );
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
            Drag::Select { .. } => {}
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
        if state.typing != Some(i) && drawn != Some(i) && !annotations[i].hidden {
            show(&annotations[i], true);
        }
    }
    // the one of the card the mouse is over in the panel
    if let Some(i) = state.pointed_card.filter(|i| !state.selected.contains(i))
        && let Some(a) = annotations.get(i).filter(|a| !a.hidden)
    {
        show(a, false);
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
    // as the text will be, other fonts than Inter aside
    let family = if a.font.weight >= 600 {
        egui::FontFamily::Name("bold".into())
    } else {
        egui::FontFamily::Proportional
    };
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
                .font(egui::FontId::new(size, family))
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
    // around the outline it may have
    let o = a.outline_width();
    let mut stroke = tiny_skia::Stroke {
        width: w + (o + gap) * 2.,
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
            stroke.width = (o + gap) * 2.;
            area = true;
            arrow.finish()
        }
        Shape::Rect(size) => {
            if a.corner_radius() <= 0. {
                stroke.line_join = LineJoin::Miter;
            }
            frame_path(*size, a.corner_radius())
        }
        Shape::Ellipse(size) => tiny_skia::Rect::from_xywh(0., 0., size[0] as f32, size[1] as f32)
            .and_then(PathBuilder::from_oval),
        Shape::Pen(points) => match points.as_slice() {
            // a dot
            [only] => {
                area = true;
                let mut dot = PathBuilder::new();
                dot.move_to(only[0] as f32, only[1] as f32);
                dot.line_to(only[0] as f32, only[1] as f32 + 0.01);
                dot.finish()
            }
            points => polyline(points),
        },
        Shape::Text(_) | Shape::Redact { .. } => {
            let Some(b) = a.bounds() else { return vec![] };
            let by = gap + o;
            let (l, t, r, b) = (b[0] - by, b[1] - by, b[2] + by, b[3] + by);
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
    // its outline is a part of it
    let slack = slack + a.outline_width();
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
            let r = a.corner_radius();
            if r <= 0. {
                let inside = |by: f32| q.x >= -by && q.y >= -by && q.x <= w + by && q.y <= h + by;
                return inside(near) && !inside(-near);
            }
            // from the line of a box with round corners, inside or outside
            let (dx, dy) = (
                (q.x - w / 2.).abs() - (w / 2. - r),
                (q.y - h / 2.).abs() - (h / 2. - r),
            );
            let from_line = dx.max(0.).hypot(dy.max(0.)) + dx.max(dy).min(0.) - r;
            from_line.abs() <= near
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
    fn a_box_selects_what_is_wholly_in_it() {
        let mapping = Mapping::new(&[], (400, 300));
        let mut annotations = vec![
            arrow_at([10, 10], [40, 30]),
            arrow_at([200, 100], [100, 100]),
        ];
        // around the first, over half of the second
        assert_eq!(
            in_box(&annotations, &mapping, [0., 0.], [250., 150.]),
            vec![0]
        );
        // from any corner
        assert_eq!(
            in_box(&annotations, &mapping, [400., 300.], [0., 0.]),
            vec![0, 1]
        );
        annotations[1].hidden = true;
        assert_eq!(
            in_box(&annotations, &mapping, [0., 0.], [400., 300.]),
            vec![0]
        );
        // the box is in the edited image, here a quarter narrower on the left
        annotations[1].hidden = false;
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
        ];
        for shape in shapes {
            let mut a = Annotation::new(shape, [255, 0, 0, 255], 6, [100, 100]);
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
    fn hidden_ones_are_not_drawn() {
        let mut hidden = arrow_at([1, 1], [50, 50]);
        hidden.hidden = true;
        let shown = arrow_at([5, 5], [60, 60]);
        let stack = vec![item(ImageOperation::Annotate(vec![hidden, shown.clone()]))];
        assert_eq!(visible(&stack, &AnnotateUi::default()), vec![shown]);
    }

    #[test]
    fn new_ones_look_like_the_one_changed_last() {
        let mut state = AnnotateUi::default();
        state.prepare((800, 600));
        let template = |state: &AnnotateUi, tool| state.template(tool).unwrap();
        let mut text = template(&state, Tool::Text);
        text.font.weight = 700;
        text.outline = 3;
        text.color = [0, 0, 255, 255];
        state.remember(&text);
        let frame = template(&state, Tool::Frame);
        assert_eq!((frame.outline, frame.color), (3, [0, 0, 255, 255]));
        assert_eq!(frame.font, Font::default(), "only texts have a font");
        assert_eq!(template(&state, Tool::Text).font.weight, 700);
        let redaction = template(&state, Tool::Redact);
        assert_eq!(redaction.outline, 0, "redactions have no outline");
        assert_eq!(redaction.color, [0, 0, 0, 255]);
        assert!(
            state.template(Tool::Select).is_none(),
            "selecting draws nothing"
        );
    }
}
