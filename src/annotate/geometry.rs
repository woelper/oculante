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

    /// How the edits turn, mirror and stretch the image as it was loaded, without moving it
    pub(crate) fn linear(&self) -> Transform {
        let t = self.transform(Transform::identity(), Point::zero());
        Transform::from_row(t.sx, t.ky, t.kx, t.sy, 0., 0.)
    }
}

pub(crate) fn mirrors(t: Transform) -> bool {
    t.sx * t.sy - t.kx * t.ky < 0.
}

/// How much larger a transform makes things on average. Lines use this for their width, so
/// they're equally thick in every direction even on a stretched image.
pub(crate) fn scale_of(t: Transform) -> f32 {
    (t.sx * t.sy - t.kx * t.ky).abs().sqrt()
}

/// `t` with the stretch taken out, closest transform that only moves, turns, mirrors and
/// scales evenly. `centre` stays where `t` puts it.
pub(crate) fn undistorted(t: Transform, centre: Point) -> Transform {
    let scale = scale_of(t);
    let mirrored = mirrors(t);
    // the closest rotation, or reflection if `t` mirrors
    let angle = if mirrored {
        (t.ky + t.kx).atan2(t.sx - t.sy)
    } else {
        (t.ky - t.kx).atan2(t.sx + t.sy)
    };
    let (sin, cos) = angle.sin_cos();
    let flip = if mirrored { -1. } else { 1. };
    let linear = Transform::from_row(
        cos * scale,
        sin * scale,
        -sin * scale * flip,
        cos * scale * flip,
        0.,
        0.,
    );
    let mut at = centre;
    t.map_point(&mut at);
    let mut moved = centre;
    linear.map_point(&mut moved);
    linear.post_translate(at.x - moved.x, at.y - moved.y)
}

/// Left, top, right and bottom in the edited image of its bounds, grown by `grow` along its
/// own sides as much as `t` scales lines (see `scale_of`). That covers its lines, sharp corners
/// included, also on a stretched image.
pub(super) fn extent(a: &Annotation, t: Transform, grow: f32) -> Option<[f32; 4]> {
    let b = a.bounds()?;
    let unit = |x: f32, y: f32| {
        let length = x.hypot(y).max(1e-9);
        (x / length, y / length)
    };
    let (u, v) = (unit(t.sx, t.ky), unit(t.kx, t.sy));
    let grow = grow * scale_of(t);
    let mut e = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for (x, along) in [(b[0], -1.), (b[2], 1.)] {
        for (y, across) in [(b[1], -1.), (b[3], 1.)] {
            let mut c = Point::from_xy(x, y);
            t.map_point(&mut c);
            let x = c.x + (u.0 * along + v.0 * across) * grow;
            let y = c.y + (u.1 * along + v.1 * across) * grow;
            e = [e[0].min(x), e[1].min(y), e[2].max(x), e[3].max(y)];
        }
    }
    Some(e)
}

/// The ones that are shown and have at least half of their area inside the box between two
/// corners in the edited image, with their lines not counted. The area is that of the bounding
/// box of the points they cover. One with no height or no width is in or out along that side,
/// depending on whether it lies within the box.
pub(crate) fn in_box(
    annotations: &[Annotation],
    mapping: &Mapping,
    a: [f32; 2],
    b: [f32; 2],
) -> Vec<usize> {
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

/// Whether a point of the edited image is on an annotation, `slack` pixels around
/// it count too
pub(crate) fn hits(a: &Annotation, mapping: &Mapping, p: [f32; 2], slack: f32) -> bool {
    let t = a.transform(mapping);
    let Some(back) = t.invert() else {
        return false;
    };
    let mut q = Point::from_xy(p[0], p[1]);
    back.map_point(&mut q);
    // along its own sides, as large as it's shown
    let axes = [t.sx.hypot(t.ky), t.kx.hypot(t.sy)];
    let q = Point::from_xy(q.x * axes[0], q.y * axes[1]);
    touches_shown(a, q, axes, scale_of(t), slack)
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
#[cfg(test)]
pub(super) fn touches(a: &Annotation, q: Point, slack: f32) -> bool {
    touches_shown(a, q, [1., 1.], 1., slack)
}

/// Like `touches`, with its own coordinates scaled by `axes` along each side and its lines by
/// `scale`, as it's shown. Its sides are at right angles in the edited image, so distances
/// here are distances on screen.
fn touches_shown(a: &Annotation, q: Point, axes: [f32; 2], scale: f32, slack: f32) -> bool {
    let at = |x: f32, y: f32| Point::from_xy(x * axes[0], y * axes[1]);
    let point = |p: [i32; 2]| at(p[0] as f32, p[1] as f32);
    // its outline is a part of it
    let slack = slack + a.outline_width() * scale;
    let near = a.line_width() * scale / 2. + slack;
    let to_line = |p0: [i32; 2], p1: [i32; 2]| segment_distance(q, point(p0), point(p1));
    match &a.shape {
        Shape::Arrow(to) => match arrow_corners(point([0, 0]), point(*to), a.line_width(), scale) {
            None => to_line([0, 0], *to) <= near,
            Some([tip, left, right]) => [(point([0, 0]), tip), (left, tip), (right, tip)]
                .iter()
                .any(|(p0, p1)| segment_distance(q, *p0, *p1) <= near),
        },
        Shape::Line(to) => to_line([0, 0], *to) <= near,
        Shape::Rect(size) => {
            let (w, h) = (size[0] as f32 * axes[0], size[1] as f32 * axes[1]);
            // as `box_path` rounds it
            let r = (a.radius as f32 * scale).min(w / 2.).min(h / 2.);
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
            let (rx, ry) = (size[0] as f32 * axes[0] / 2., size[1] as f32 * axes[1] / 2.);
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
            let (from, to) = (at(b[0], b[1]), at(b[2], b[3]));
            q.x >= from.x - slack
                && q.y >= from.y - slack
                && q.x <= to.x + slack
                && q.y <= to.y + slack
        }),
        Shape::Counter(_) => {
            let r = a.width as f32 / 2.;
            let centre = at(r, r);
            (q.x - centre.x).hypot(q.y - centre.y) <= r * axes[0] + slack
        }
        Shape::Spotlight { size, oval } => {
            let (rx, ry) = (size[0] as f32 * axes[0] / 2., size[1] as f32 * axes[1] / 2.);
            let (dx, dy) = (q.x - rx, q.y - ry);
            if *oval {
                let inside = (dx / rx.max(0.5)).powi(2) + (dy / ry.max(0.5)).powi(2) <= 1.;
                inside || ellipse_distance(rx, ry, dx, dy) <= slack
            } else {
                dx.abs() <= rx + slack && dy.abs() <= ry + slack
            }
        }
    }
}
