//! Where annotations in the edited image are. Persists through crops, resizes, rotations, and flips, finds what a point or box touches.

use super::*;

/// Maps the loaded image to the edited one, applying the edits crops, resizes, rotations and flips in order.
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

    /// Carries a transform through to the loaded image. A perspective crop moves
    /// `centre`, in the transform's coordinates, and everything around it.
    pub(super) fn transform(&self, mut t: Transform, centre: Point) -> Transform {
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

    /// Maps a point in the edited image back to the loaded image.
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

    /// Maps a point in the loaded image to the edited image.
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
pub(crate) fn scale_of(t: Transform) -> f32 {
    (t.sx * t.sy - t.kx * t.ky).abs().sqrt()
}

/// Left, top, right and bottom in the edited image of its bounds, grown by `grow` in its own coordinates
pub(super) fn extent(a: &Annotation, t: Transform, grow: f32) -> Option<[f32; 4]> {
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

/// The ones that are shown and have at least half of their area inside the box between two
/// corners in the edited image, with their lines not counted. The area is that of the bounding
/// box of the points they cover. One with no height or no width is in or out along that side,
/// depending on whether it lies within the box.
pub(crate) fn in_box(annotations: &[Annotation], mapping: &Mapping, a: [f32; 2], b: [f32; 2]) -> Vec<usize> {
    let (left, top) = (a[0].min(b[0]), a[1].min(b[1]));
    let (right, bottom) = (a[0].max(b[0]), a[1].max(b[1]));
    // the part of lo..hi within from..to, 0 to 1; a flat one is in or out
    let covered = |lo: f32, hi: f32, from: f32, to: f32| {
        let overlap = hi.min(to) - lo.max(from);
        if hi > lo {
            overlap.max(0.) / (hi - lo)
        } else {
            f32::from(overlap >= 0.)
        }
    };
    (0..annotations.len())
        .filter(|&i| {
            let x = &annotations[i];
            !x.hidden
                && extent(x, x.transform(mapping), 0.).is_some_and(|e| {
                    covered(e[0], e[2], left, right) * covered(e[1], e[3], top, bottom) >= 0.5
                })
        })
        .collect()
}

/// The lines that show an annotation is pointed at or selected, in its own
/// coordinates, each closed: `gap` around its lines, its arrow, its text or the
/// area it hides. `resolution` is how large it is on the screen.
pub(crate) fn outline(a: &Annotation, gap: f32, resolution: f32) -> Vec<Vec<[f32; 2]>> {
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
        Shape::Text(_) | Shape::Redact { .. } | Shape::Stamp(_) => {
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
pub(crate) fn hits(a: &Annotation, mapping: &Mapping, p: [f32; 2], slack: f32) -> bool {
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
pub(super) fn ellipse_distance(rx: f32, ry: f32, dx: f32, dy: f32) -> f32 {
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
pub(super) fn touches(a: &Annotation, q: Point, slack: f32) -> bool {
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
        Shape::Text(_) | Shape::Redact { .. } | Shape::Stamp(_) => a.bounds().is_some_and(|b| {
            q.x >= b[0] - slack && q.y >= b[1] - slack && q.x <= b[2] + slack && q.y <= b[3] + slack
        }),
    }
}
