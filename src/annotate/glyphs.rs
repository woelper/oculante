//! Letters and symbols of fonts as paths: the symbols of stamps and the numbers of counters.

use super::*;

/// The icon font, read once
static ICONS: LazyLock<Option<ttf_parser::Face<'static>>> =
    LazyLock::new(|| ttf_parser::Face::parse(crate::ICON_FONT, 0).ok());

/// Bold Inter, for the numbers of counters, read once
static BOLD: LazyLock<Option<ttf_parser::Face<'static>>> =
    LazyLock::new(|| ttf_parser::Face::parse(crate::BOLD_FONT, 0).ok());

/// The outlines of letters of a font as a path, in the units of the font, up from its
/// baseline, moved right by `dx`
struct Outline {
    path: PathBuilder,
    dx: f32,
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(x + self.dx, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(x + self.dx, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.quad_to(x1 + self.dx, y1, x + self.dx, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path
            .cubic_to(x1 + self.dx, y1, x2 + self.dx, y2, x + self.dx, y);
    }
    fn close(&mut self) {
        self.path.close();
    }
}

/// A stamp in a square this large, in the middle of it, as wide or as high
pub(super) fn stamp_path(stamp: Stamp, size: f32) -> Option<Path> {
    let face = ICONS.as_ref()?;
    let glyph = face.glyph_index(stamp.glyph().chars().next()?)?;
    let mut outline = Outline {
        path: PathBuilder::new(),
        dx: 0.,
    };
    let b = face.outline_glyph(glyph, &mut outline)?;
    let (w, h) = (b.width() as f32, b.height() as f32);
    let scale = size / w.max(h).max(1.);
    // the font goes up from its baseline
    let fit = Transform::from_row(
        scale,
        0.,
        0.,
        -scale,
        (size - w * scale) / 2. - b.x_min as f32 * scale,
        (size - h * scale) / 2. + b.y_max as f32 * scale,
    );
    outline.path.finish()?.transform(fit)
}

/// The digits of a counter in a round as wide as `size`: in its middle, half as high as it,
/// narrower where there are many
pub(super) fn counter_digits(number: u32, size: f32) -> Option<Path> {
    let face = BOLD.as_ref()?;
    let mut outline = Outline {
        path: PathBuilder::new(),
        dx: 0.,
    };
    for digit in number.to_string().chars() {
        let glyph = face.glyph_index(digit)?;
        face.outline_glyph(glyph, &mut outline);
        outline.dx += face.glyph_hor_advance(glyph).unwrap_or(0) as f32;
    }
    let path = outline.path.finish()?;
    // as large as what is drawn, not the box of the font that also holds the points that pull
    // the curves
    let [l, bottom, r, top] = ink(&path)?;
    let (w, h) = ((r - l).max(1.), (top - bottom).max(1.));
    let scale = (size * 0.5 / h).min(size * 0.7 / w);
    // across, where its weight is in the middle, as the eye sees it: the stem of a 1 in the
    // middle, not its box with the flag on the left. Up and down the middle of its height, all
    // digits are as high.
    let across = weight_middle(&path).unwrap_or(l + w / 2.);
    let fit = Transform::from_row(
        scale,
        0.,
        0.,
        -scale,
        size / 2. - across * scale,
        size / 2. + (bottom + h / 2.) * scale,
    );
    path.transform(fit)
}

/// Along a path, its curves in short lines
fn flattened(path: &Path) -> Vec<Vec<Point>> {
    let mut lines: Vec<Vec<Point>> = vec![];
    let mut last = Point::zero();
    let steps = 16;
    for segment in path.segments() {
        match segment {
            tiny_skia::PathSegment::MoveTo(p) => {
                lines.push(vec![p]);
                last = p;
            }
            tiny_skia::PathSegment::LineTo(p) => {
                if let Some(line) = lines.last_mut() {
                    line.push(p);
                }
                last = p;
            }
            tiny_skia::PathSegment::QuadTo(c, p) => {
                if let Some(line) = lines.last_mut() {
                    for i in 1..=steps {
                        let (t, u) = (i as f32 / steps as f32, 1. - i as f32 / steps as f32);
                        line.push(Point::from_xy(
                            u * u * last.x + 2. * u * t * c.x + t * t * p.x,
                            u * u * last.y + 2. * u * t * c.y + t * t * p.y,
                        ));
                    }
                }
                last = p;
            }
            tiny_skia::PathSegment::CubicTo(c1, c2, p) => {
                if let Some(line) = lines.last_mut() {
                    for i in 1..=steps {
                        let (t, u) = (i as f32 / steps as f32, 1. - i as f32 / steps as f32);
                        let at = |a: f32, b: f32, c: f32, d: f32| {
                            u * u * u * a + 3. * u * u * t * b + 3. * u * t * t * c + t * t * t * d
                        };
                        line.push(Point::from_xy(
                            at(last.x, c1.x, c2.x, p.x),
                            at(last.y, c1.y, c2.y, p.y),
                        ));
                    }
                }
                last = p;
            }
            tiny_skia::PathSegment::Close => {}
        }
    }
    lines
}

/// Where the area a path fills has as much of it to the left as to the right. The holes of
/// letters go the other way round, they take away.
pub(super) fn weight_middle(path: &Path) -> Option<f32> {
    let (mut area, mut moment) = (0f32, 0f32);
    for line in flattened(path) {
        let n = line.len();
        for i in 0..n {
            let (a, b) = (line[i], line[(i + 1) % n]);
            let cross = a.x * b.y - b.x * a.y;
            area += cross;
            moment += (a.x + b.x) * cross;
        }
    }
    (area.abs() > 1e-3).then(|| moment / (3. * area))
}

/// Left, top, right and bottom of the lines and curves of a path, the points that pull the
/// curves not counted
pub(super) fn ink(path: &Path) -> Option<[f32; 4]> {
    flattened(path).iter().flatten().fold(None, |b, p| {
        Some(b.map_or([p.x, p.y, p.x, p.y], |b: [f32; 4]| {
            [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)]
        }))
    })
}
