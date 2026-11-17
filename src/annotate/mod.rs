//! Arrows, lines, frames, circles, stamps, text, pen lines and hidden areas drawn over the edited image.
//! They are stored in pixels of the loaded image, so crops, resizes, rotations and flips carry them along.
//! `geometry` places them, `text` lays out their text, and `render` draws them. Editing UI is in `crate::ui::annotate`.

mod geometry;
mod glyphs;
mod render;
#[cfg(test)]
pub(crate) mod tests;
mod text;
mod transform;

pub(crate) use geometry::*;
use glyphs::*;
pub(crate) use render::*;
pub(crate) use text::*;
pub(crate) use transform::*;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use image::Pixel;
use image::math::Rect;
use imageproc::geometric_transformations::Projection;
use serde::{Deserialize, Serialize};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Point, Transform};

use crate::image_editing::{
    ColorTypeExt, ImageOperation, ImgOpItem, convert_color_type, cropped_range, perspective,
};
use crate::paint::{Base, Pixels, channels, keep_base, keep_base_bytes, restore, restore_bytes};
use crate::utils::rect_union;

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
    /// Of the number of a counter
    #[serde(default = "white")]
    pub number_color: [u8; 4],
    /// How dark its shadow is at its edge, in percent, 0 for none
    #[serde(default)]
    pub shadow: u32,
    /// How far its shadow reaches out of what it draws, 0 for none
    #[serde(default)]
    pub shadow_size: u32,
}

pub(crate) fn white() -> [u8; 4] {
    [255; 4]
}

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
    Arrow([i32; 2]),
    /// A straight line from where it starts to this point, an arrow without a head
    Line([i32; 2]),
    Rect([i32; 2]),
    Ellipse([i32; 2]),
    Text(String),
    /// A line through these points, the first one is where it starts
    Pen(Vec<[i32; 2]>),
    Redact {
        size: [i32; 2],
        style: Redaction,
    },
    /// Dims the image outside a spotlight: an oval or a rounded box. The dim colour
    /// is the shade, with the coverage as its alpha. All spotlights dim together, down
    /// to the layer furthest back. Anything in front of that is not dimmed.
    Spotlight {
        size: [i32; 2],
        oval: bool,
    },
    /// A symbol of the icon font in a square as large as the annotation is wide
    Stamp(Stamp),
    /// A number in a round as large as the annotation is wide, in its color. New ones count on
    /// from the highest there is, and those after a removed one count on without a gap.
    Counter(u32),
}

/// Stamp symbols, saved by name since the icon font may change.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum Stamp {
    Check,
    Cross,
    Warning,
    Error,
    Info,
    Plus,
    Pin,
    Clock,
    Eye,
    Search,
}

impl Stamp {
    pub const ALL: [Stamp; 10] = [
        Stamp::Check,
        Stamp::Cross,
        Stamp::Warning,
        Stamp::Error,
        Stamp::Info,
        Stamp::Plus,
        Stamp::Pin,
        Stamp::Clock,
        Stamp::Eye,
        Stamp::Search,
    ];

    /// On the tiles until others are picked
    pub const TILES: [Stamp; 3] = [Stamp::Check, Stamp::Cross, Stamp::Warning];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Stamp::Check => "Check",
            Stamp::Cross => "Cross",
            Stamp::Warning => "Warning",
            Stamp::Error => "Error",
            Stamp::Info => "Info",
            Stamp::Plus => "Plus",
            Stamp::Pin => "Pin",
            Stamp::Clock => "Clock",
            Stamp::Eye => "Eye",
            Stamp::Search => "Search",
        }
    }

    pub(crate) fn glyph(self) -> &'static str {
        use crate::icons::*;
        match self {
            Stamp::Check => CHECK,
            Stamp::Cross => X,
            Stamp::Warning => WARNING_CIRCLE,
            Stamp::Error => ERROR_CIRCLE,
            Stamp::Info => INFO,
            Stamp::Plus => PLUS,
            Stamp::Pin => LOCATION_PIN,
            Stamp::Clock => CLOCK,
            Stamp::Eye => EYE,
            Stamp::Search => SEARCH,
        }
    }
}

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
    pub(crate) fn new(shape: Shape, color: [u8; 4], width: u32, start: [i32; 2]) -> Self {
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
            number_color: white(),
            shadow: 0,
            shadow_size: 0,
        }
    }

    /// Where it starts, in the image as it was loaded
    pub(crate) fn start(&self, original: (u32, u32)) -> [i32; 2] {
        let anchor = self.anchor.point(original);
        [anchor[0] + self.offset[0], anchor[1] + self.offset[1]]
    }

    pub(crate) fn set_start(&mut self, start: [i32; 2], original: (u32, u32)) {
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

    /// From its own coordinates to the edited image.
    ///
    /// It's turned after the image is stretched, so a turned frame stays a rectangle. Text, stamps
    /// and counters aren't stretched at all, only moved, turned and scaled evenly.
    pub fn transform(&self, mapping: &Mapping) -> Transform {
        let centre = self.middle();
        let mut middle = centre;
        mapping
            .transform(self.placement(mapping.original), centre)
            .map_point(&mut middle);
        let linear = self.linear(mapping);
        let mut moved = centre;
        linear.map_point(&mut moved);
        let t = linear.post_translate(middle.x - moved.x, middle.y - moved.y);
        if self.keeps_its_shape() {
            undistorted(t, centre)
        } else {
            t
        }
    }

    fn middle(&self) -> Point {
        self.bounds().map_or(Point::zero(), |b| {
            Point::from_xy((b[0] + b[2]) / 2., (b[1] + b[3]) / 2.)
        })
    }

    /// Its flip, the image's edits and its turn, without moving it. A mirrored image turns it the
    /// other way.
    fn linear(&self, mapping: &Mapping) -> Transform {
        let image = mapping.linear();
        let turn = if mirrors(image) {
            -self.rotation
        } else {
            self.rotation
        };
        let flip = |flipped: bool| if flipped { -1. } else { 1. };
        image
            .pre_scale(flip(self.flip[0]), flip(self.flip[1]))
            .post_rotate(turn as f32)
    }

    /// Moves it so its start lands on `at` in the edited image
    pub(crate) fn start_at(&mut self, at: [f32; 2], mapping: &Mapping) {
        // The stored start assumes it's turned before the image is stretched. It's turned after,
        // so correct by how far apart the two put its middle.
        let centre = self.middle();
        let as_placed = mapping
            .linear()
            .pre_concat(Transform::from_rotate(self.rotation as f32))
            .pre_scale(
                if self.flip[0] { -1. } else { 1. },
                if self.flip[1] { -1. } else { 1. },
            );
        let (mut placed, mut turned) = (centre, centre);
        as_placed.map_point(&mut placed);
        self.linear(mapping).map_point(&mut turned);
        let start = mapping.to_original([at[0] + turned.x - placed.x, at[1] + turned.y - placed.y]);
        self.set_start(
            [start[0].round() as i32, start[1].round() as i32],
            mapping.original,
        );
    }

    /// Text, stamps and counters aren't stretched with the image
    fn keeps_its_shape(&self) -> bool {
        matches!(
            self.shape,
            Shape::Text(_) | Shape::Stamp(_) | Shape::Counter(_)
        )
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
            Shape::Arrow(to) | Shape::Line(to) => around(&[[0, 0], *to]),
            Shape::Rect(size)
            | Shape::Ellipse(size)
            | Shape::Redact { size, .. }
            | Shape::Spotlight { size, .. } => around(&[[0, 0], *size]),
            Shape::Pen(points) => around(points),
            Shape::Text(text) => text_layout(text, self).map(|l| l.bounds),
            Shape::Stamp(_) | Shape::Counter(_) => around(&[[0, 0], [self.width as i32; 2]]),
        }
    }

    /// How far its lines and its outline reach out of its bounds, in its own
    /// coordinates. Takes `t` because an arrowhead's size depends on how long the arrow looks.
    fn reach(&self, t: Transform) -> f32 {
        let half = self.line_width() / 2.;
        let outline = self.outline_width();
        match &self.shape {
            Shape::Arrow(to) => {
                // the lines of the head reach out to the sides
                let (x, y) = (to[0] as f32, to[1] as f32);
                let shown = (t.sx * x + t.kx * y).hypot(t.ky * x + t.sy * y);
                let length = shown / scale_of(t).max(1e-6);
                arrow_head(self.line_width(), length).1 + half + outline
            }
            Shape::Line(_) | Shape::Rect(_) | Shape::Ellipse(_) | Shape::Pen(_) => half + outline,
            Shape::Text(_) | Shape::Stamp(_) | Shape::Counter(_) => outline,
            Shape::Redact { .. } | Shape::Spotlight { .. } => 0.,
        }
    }

    fn line_width(&self) -> f32 {
        self.width.max(1) as f32
    }

    /// A redaction and a spotlight have none
    fn outline_width(&self) -> f32 {
        match self.shape {
            Shape::Redact { .. } | Shape::Spotlight { .. } => 0.,
            _ => self.outline as f32,
        }
    }

    /// Of a frame or a spotlight that is a box, at most half of its shorter side
    #[cfg(test)]
    fn corner_radius(&self) -> f32 {
        match self.shape {
            Shape::Rect(size) | Shape::Spotlight { size, oval: false } => (self.radius as f32)
                .min(size[0].abs() as f32 / 2.)
                .min(size[1].abs() as f32 / 2.),
            _ => 0.,
        }
    }

    /// Too small to be seen, or a text without any
    pub(crate) fn is_empty(&self) -> bool {
        match &self.shape {
            Shape::Arrow(to) | Shape::Line(to) => to[0].abs().max(to[1].abs()) < 2,
            Shape::Rect(size)
            | Shape::Ellipse(size)
            | Shape::Redact { size, .. }
            | Shape::Spotlight { size, .. } => size[0] < 2 || size[1] < 2,
            Shape::Pen(points) => points.is_empty(),
            Shape::Text(text) => text.trim().is_empty(),
            Shape::Stamp(_) | Shape::Counter(_) => self.width < 2,
        }
    }

    /// Puts its middle at `centre` in the image as it was loaded
    pub(crate) fn centre_at(&mut self, centre: [f32; 2], original: (u32, u32)) {
        let Some(b) = self.bounds() else { return };
        self.set_start([0, 0], original);
        let mut middle = Point::from_xy((b[0] + b[2]) / 2., (b[1] + b[3]) / 2.);
        self.placement(original).map_point(&mut middle);
        let start = [centre[0] - middle.x, centre[1] - middle.y];
        self.set_start([start[0].round() as i32, start[1].round() as i32], original);
    }

    /// After it was drawn or moved: the anchor nearest to it, with the same start
    pub(crate) fn reanchor(&mut self, original: (u32, u32)) {
        let Some(b) = self.bounds() else { return };
        let mut centre = Point::from_xy((b[0] + b[2]) / 2., (b[1] + b[3]) / 2.);
        self.placement(original).map_point(&mut centre);
        let start = self.start(original);
        self.anchor = Anchor::nearest([centre.x, centre.y], original);
        self.set_start(start, original);
    }

    /// Turned and mirrored so that it reads the right way round in the edited image,
    /// where `at` is: the image may be rotated or flipped after it was loaded
    pub(crate) fn stand_upright(&mut self, mapping: &Mapping) {
        let mut best = None;
        for rotation in [0, 90, 180, 270] {
            for flip in [false, true] {
                self.rotation = rotation;
                self.flip = [flip, false];
                let t = self.linear(mapping);
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

    pub(crate) fn is_spotlight(&self) -> bool {
        matches!(self.shape, Shape::Spotlight { .. })
    }

    /// A redaction and a spotlight have none
    pub(crate) fn casts_shadow(&self) -> bool {
        self.shadow > 0
            && self.shadow_size > 0
            && !matches!(self.shape, Shape::Redact { .. } | Shape::Spotlight { .. })
    }

    /// How far its shadow reaches out of what it draws, in its own coordinates
    fn shadow_reach(&self) -> f32 {
        if self.casts_shadow() {
            self.shadow_size as f32
        } else {
            0.
        }
    }

    /// Its name in the list of annotations
    pub(crate) fn label(&self) -> String {
        match &self.shape {
            Shape::Arrow(_) => "Arrow".into(),
            Shape::Line(_) => "Line".into(),
            Shape::Rect(_) => "Frame".into(),
            Shape::Ellipse(_) => "Circle".into(),
            Shape::Pen(_) => "Pen".into(),
            Shape::Redact { .. } => "Redacted".into(),
            Shape::Spotlight { .. } => "Spotlight".into(),
            Shape::Stamp(stamp) => stamp.name().into(),
            Shape::Counter(number) => format!("Counter {number}"),
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

/// One more than the highest counter there is, starts at 1
pub(crate) fn next_counter(annotations: &[Annotation]) -> u32 {
    annotations
        .iter()
        .filter_map(|a| match a.shape {
            Shape::Counter(number) => Some(number),
            _ => None,
        })
        .max()
        .map_or(1, |n| n.saturating_add(1))
}

/// Renumbers the counters after a removed one so there are no gaps.
/// Numbers still used by another counter are kept.
pub(crate) fn renumber(annotations: &mut [Annotation], gone: &mut Vec<u32>) {
    gone.sort_unstable();
    gone.dedup();
    gone.retain(|n| {
        !annotations
            .iter()
            .any(|a| matches!(a.shape, Shape::Counter(m) if m == *n))
    });
    for a in annotations.iter_mut() {
        if let Shape::Counter(n) = &mut a.shape {
            let below = gone.iter().filter(|g| **g < *n).count() as u32;
            *n -= below;
        }
    }
}

/// The annotations applied to the edit, if annotations are enabled.
pub fn annotations_mut(stack: &mut [ImgOpItem]) -> Option<&mut Vec<Annotation>> {
    stack
        .iter_mut()
        .filter(|op| op.active)
        .find_map(|op| match &mut op.operation {
            ImageOperation::Annotate(annotations) => Some(annotations),
            _ => None,
        })
}
