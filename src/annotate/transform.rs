//! Transforming the selection as a whole: its bounding box, resizing it by a side or corner, turning it, and mirroring it.

use super::*;

/// The box around what is selected in the edited image, as left, top, right and bottom
#[derive(Debug, Clone, Copy)]
pub(crate) struct Bounds {
    /// Around the shapes, their lines not counted, they are resized by it
    pub(crate) shape: [f32; 4],
    /// Around all that is drawn of them, it is shown
    pub(crate) drawn: [f32; 4],
}

/// The bounding box of the visible selected annotations, or `None` if none are visible.
pub(crate) fn selection_bounds(
    annotations: &[Annotation],
    selected: &[usize],
    mapping: &Mapping,
) -> Option<Bounds> {
    let around = |b: Option<[f32; 4]>, e: [f32; 4]| {
        Some(b.map_or(e, |b| {
            [
                b[0].min(e[0]),
                b[1].min(e[1]),
                b[2].max(e[2]),
                b[3].max(e[3]),
            ]
        }))
    };
    let (mut shape, mut drawn) = (None, None);
    for a in selected
        .iter()
        .filter_map(|i| annotations.get(*i))
        .filter(|a| !a.hidden)
    {
        let t = a.transform(mapping);
        if let (Some(s), Some(d)) = (extent(a, t, 0.), extent(a, t, a.reach(t))) {
            shape = around(shape, s);
            drawn = around(drawn, d);
        }
    }
    Some(Bounds {
        shape: shape?,
        drawn: drawn?,
    })
}

pub(crate) fn middle_of(b: [f32; 4]) -> [f32; 2] {
    [(b[0] + b[2]) / 2., (b[1] + b[3]) / 2.]
}

pub(crate) fn middle_in(a: &Annotation, mapping: &Mapping) -> Option<[f32; 2]> {
    let b = a.bounds()?;
    let mut middle = Point::from_xy((b[0] + b[2]) / 2., (b[1] + b[3]) / 2.);
    a.transform(mapping).map_point(&mut middle);
    Some([middle.x, middle.y])
}

pub(crate) fn wrap(degrees: f32) -> f32 {
    let d = degrees.rem_euclid(360.);
    if d > 180. { d - 360. } else { d }
}

/// Degrees clockwise from `pivot` to `p`, from the right
pub(crate) fn angle_of(p: [f32; 2], pivot: [f32; 2]) -> f32 {
    (p[1] - pivot[1]).atan2(p[0] - pivot[0]).to_degrees()
}

/// Clockwise degrees the annotation appears turned in the edited image. A mirror is
/// counted as a flip plus a turn, so a flip alone is not a turn.
pub(crate) fn shown_angle(a: &Annotation, mapping: &Mapping) -> f32 {
    let t = a.transform(mapping);
    let degrees = |x: f32, y: f32| wrap(y.atan2(x).to_degrees());
    if !mirrors(t) {
        return degrees(t.sx, t.ky);
    }
    let left_to_right = degrees(-t.sx, -t.ky);
    let top_to_bottom = degrees(t.sx, t.ky);
    if left_to_right.abs() <= top_to_bottom.abs() {
        left_to_right
    } else {
        top_to_bottom
    }
}

/// How much a drag by `by` in the edited image on the handle at `side` of the box `shape`
/// stretches what is in it along each side, and the point that stays where it is: the
/// opposite corner or side. Dragged past the opposite side it is mirrored, the scale is below
/// 0 then. With `keep` it keeps its proportions, from a side it grows as much across, around
/// the middle. It never gets flat.
pub(crate) fn stretch(
    side: [i8; 2],
    shape: [f32; 4],
    by: [f32; 2],
    keep: bool,
) -> ([f32; 2], [f32; 2]) {
    let mut scale = [1f32; 2];
    let mut fixed = [0f32; 2];
    // from what stays to the side that is dragged
    let mut length = [0f32; 2];
    for axis in 0..2 {
        let (lo, hi) = (shape[axis], shape[axis + 2]);
        fixed[axis] = match side[axis] {
            -1 => hi,
            1 => lo,
            _ => (lo + hi) / 2.,
        };
        if side[axis] == 0 {
            continue;
        }
        length[axis] = if side[axis] < 0 { lo - hi } else { hi - lo };
        // one with no width or height stays so
        if length[axis].abs() > 0.01 {
            scale[axis] = (length[axis] + by[axis]) / length[axis];
        }
    }
    // at least a pixel, on the side it is dragged to
    let least = |axis: usize| {
        let size = shape[axis + 2] - shape[axis];
        if size > 1. { 1. / size } else { 0.01 }
    };
    let at_least = |s: f32, least: f32| {
        if s.abs() >= least {
            s
        } else if s < 0. {
            -least
        } else {
            least
        }
    };
    if keep {
        let least = least(0).max(least(1));
        if side[0] != 0 && side[1] != 0 {
            // the dragged corner moves along the diagonal, as far as the mouse
            let (wide, high) = (length[0] * length[0], length[1] * length[1]);
            let uniform = if wide + high > 0.0001 {
                (wide * scale[0] + high * scale[1]) / (wide + high)
            } else {
                1.
            };
            scale = [at_least(uniform, least); 2];
        } else {
            // across as much, not mirrored
            let along = if side[0] != 0 { 0 } else { 1 };
            let s = at_least(scale[along], least);
            scale[along] = s;
            scale[1 - along] = s.abs();
        }
    } else {
        scale = [at_least(scale[0], least(0)), at_least(scale[1], least(1))];
        // a flat one is tilted by a corner instead, see `corner_tilt`: dragged past its line
        // it is mirrored
        for axis in 0..2 {
            if tilts(side, shape, axis) && by[axis] * f32::from(side[axis]) < 0. {
                scale[axis] = -1.;
            }
        }
    }
    (scale, fixed)
}

/// Whether the box is flat along `axis`, so a corner `side` tilts it instead of stretching it.
fn tilts(side: [i8; 2], shape: [f32; 4], axis: usize) -> bool {
    let other = 1 - axis;
    let length = |axis: usize| shape[axis + 2] - shape[axis];
    side[axis] != 0 && side[other] != 0 && length(axis) <= 0.01 && length(other) > 0.01
}

/// Tilt factor for a flat box dragged from a corner, or zero if nothing tilts. The dragged end
/// moves with the mouse, and the other end stays put. `keep` disables tilting.
pub(crate) fn corner_tilt(side: [i8; 2], shape: [f32; 4], by: [f32; 2], keep: bool) -> [f32; 2] {
    let mut tilt = [0f32; 2];
    if keep {
        return tilt;
    }
    for axis in 0..2 {
        let other = 1 - axis;
        if tilts(side, shape, axis) {
            // from what stays to the dragged side
            let (lo, hi) = (shape[other], shape[other + 2]);
            let along = if side[other] < 0 { lo - hi } else { hi - lo };
            tilt[axis] = by[axis] / along;
        }
    }
    tilt
}

/// Stretches the annotation by `scale` on each side, anchored at `fixed`, and tilts it by `tilt`
/// as a corner does a flat box (see `corner_tilt`, only lines and arrows tilt: what else is in a
/// flat box is flat as well). A negative scale mirrors it. Line widths stay put, while text,
/// stamps and counters grow with the side that stretches most.
pub(crate) fn resized(
    a: &Annotation,
    mapping: &Mapping,
    fixed: [f32; 2],
    scale: [f32; 2],
    tilt: [f32; 2],
) -> Annotation {
    let moved = with_ends_moved(a, mapping, |p| {
        let d = [p[0] - fixed[0], p[1] - fixed[1]];
        [
            fixed[0] + d[0] * scale[0] + d[1] * tilt[0],
            fixed[1] + d[1] * scale[1] + d[0] * tilt[1],
        ]
    });
    if let Some(moved) = moved {
        return moved;
    }
    let mut a = a.clone();
    let Some(middle) = middle_in(&a, mapping) else {
        return a;
    };
    let size = [scale[0].abs(), scale[1].abs()];
    let t = a.transform(mapping);
    // how much longer its own sides get
    let longer = |x: f32, y: f32| {
        let length = x.hypot(y);
        if length < 1e-6 {
            1.
        } else {
            (x * size[0]).hypot(y * size[1]) / length
        }
    };
    let k = [longer(t.sx, t.ky), longer(t.kx, t.sy)];
    let most = if k[0].ln().abs() >= k[1].ln().abs() {
        k[0]
    } else {
        k[1]
    };
    let times = |v: i32, k: f32| (v as f32 * k).round() as i32;
    match &mut a.shape {
        Shape::Arrow(to) | Shape::Line(to) => *to = [times(to[0], k[0]), times(to[1], k[1])],
        Shape::Rect(s)
        | Shape::Ellipse(s)
        | Shape::Redact { size: s, .. }
        | Shape::Spotlight { size: s, .. } => {
            *s = [times(s[0], k[0]).max(2), times(s[1], k[1]).max(2)]
        }
        Shape::Pen(points) => {
            for p in points.iter_mut() {
                *p = [times(p[0], k[0]), times(p[1], k[1])];
            }
        }
        Shape::Text(_) | Shape::Stamp(_) | Shape::Counter(_) => {}
    }
    if matches!(
        a.shape,
        Shape::Text(_) | Shape::Stamp(_) | Shape::Counter(_)
    ) {
        a.width = ((a.width as f32 * most).round() as u32).max(2);
    }
    let moved = [
        fixed[0] + (middle[0] - fixed[0]) * size[0],
        fixed[1] + (middle[1] - fixed[1]) * size[1],
    ];
    a.centre_at(mapping.to_original(moved), mapping.original);
    for (axis, s) in scale.iter().enumerate() {
        if *s < 0. {
            a = flipped(&a, mapping, fixed, axis);
        }
    }
    a
}

// Returns the line or arrow moved by `place` in the edited image, or `None` for other shapes. Its ends follow the box exactly, including rotation, so it keeps its angle, flip, and line width.
fn with_ends_moved(
    a: &Annotation,
    mapping: &Mapping,
    place: impl Fn([f32; 2]) -> [f32; 2],
) -> Option<Annotation> {
    let (Shape::Arrow(to) | Shape::Line(to)) = a.shape else {
        return None;
    };
    let t = a.transform(mapping);
    let mut ends = [Point::zero(), Point::from_xy(to[0] as f32, to[1] as f32)];
    t.map_points(&mut ends);
    let [start, end] = ends.map(|p| place([p.x, p.y]));
    let back = Transform::from_row(t.sx, t.ky, t.kx, t.sy, 0., 0.).invert()?;
    let mut own = Point::from_xy(end[0] - start[0], end[1] - start[1]);
    back.map_point(&mut own);
    let mut a = a.clone();
    if let Shape::Arrow(to) | Shape::Line(to) = &mut a.shape {
        *to = [own.x.round() as i32, own.y.round() as i32];
    }
    a.start_at(start, mapping);
    Some(a)
}

/// Shifts the annotations at `indices` together so the side or corner that should
/// stay put sits at `fixed`. The shift is in whole loaded image pixels, so the point
/// can be off by a fraction of a pixel.
pub(crate) fn keep_in_place(
    annotations: &mut [Annotation],
    indices: &[usize],
    mapping: &Mapping,
    side: [i8; 2],
    fixed: [f32; 2],
    scale: [f32; 2],
) {
    let Some(bounds) = selection_bounds(annotations, indices, mapping) else {
        return;
    };
    let b = bounds.shape;
    let mut off = [0f32; 2];
    for axis in 0..2 {
        let (lo, hi) = (b[axis], b[axis + 2]);
        let now = if side[axis] == 0 {
            (lo + hi) / 2.
        } else if (side[axis] < 0) == (scale[axis] > 0.) {
            // what stays is on the far side, or on this one once mirrored
            hi
        } else {
            lo
        };
        off[axis] = fixed[axis] - now;
    }
    // the same whole pixels for all, in the image as it was loaded
    let at = middle_of(b);
    let from = mapping.to_original(at);
    let to = mapping.to_original([at[0] + off[0], at[1] + off[1]]);
    let by = [
        (to[0] - from[0]).round() as i32,
        (to[1] - from[1]).round() as i32,
    ];
    if by == [0, 0] {
        return;
    }
    for &i in indices {
        if let Some(a) = annotations.get_mut(i) {
            let start = a.start(mapping.original);
            a.set_start([start[0] + by[0], start[1] + by[1]], mapping.original);
        }
    }
}

/// Turned by `degrees` clockwise around `pivot` in the edited image
pub(crate) fn rotated(
    a: &Annotation,
    mapping: &Mapping,
    pivot: [f32; 2],
    degrees: i32,
) -> Annotation {
    let mut a = a.clone();
    let Some(middle) = middle_in(&a, mapping) else {
        return a;
    };
    let (sin, cos) = (degrees as f32).to_radians().sin_cos();
    let d = [middle[0] - pivot[0], middle[1] - pivot[1]];
    let moved = [
        pivot[0] + cos * d[0] - sin * d[1],
        pivot[1] + sin * d[0] + cos * d[1],
    ];
    // a mirrored image turns it the other way
    let turn = if mirrors(mapping.linear()) {
        -degrees
    } else {
        degrees
    };
    a.rotation = (a.rotation + turn).rem_euclid(360);
    a.centre_at(mapping.to_original(moved), mapping.original);
    a
}

/// Mirrored in the edited image at `centre`, left to right along `axis` 0, top to bottom along 1
pub(crate) fn flipped(
    a: &Annotation,
    mapping: &Mapping,
    centre: [f32; 2],
    axis: usize,
) -> Annotation {
    let mut a = a.clone();
    let Some(middle) = middle_in(&a, mapping) else {
        return a;
    };
    let mut moved = middle;
    moved[axis] = 2. * centre[axis] - middle[axis];
    if matches!(a.shape, Shape::Counter(_)) {
        // a counter is not mirrored, its number would read mirrored: it turns the other way as
        // it is seen
        let turn = (-2. * shown_angle(&a, mapping)).round() as i32;
        a.centre_at(mapping.to_original(moved), mapping.original);
        return rotated(&a, mapping, moved, turn);
    }
    // a turned image has its sides the other way round
    let l = mapping.linear();
    let x_stays_x = l.sx.abs() >= l.ky.abs();
    let own = if x_stays_x == (axis == 0) { 0 } else { 1 };
    // mirrored after it is turned is mirrored before it is turned back
    a.rotation = (-a.rotation).rem_euclid(360);
    a.flip[own] = !a.flip[own];
    a.centre_at(mapping.to_original(moved), mapping.original);
    a
}
