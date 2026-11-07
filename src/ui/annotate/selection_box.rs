//! The box around what is selected: handles that resize it, buttons above it that turn and
//! flip it, a button off a corner that turns it by hand, and tooltips for all of them.

use super::*;

/// The line of the box around what is selected, on the screen
const FRAME_WIDTH: f32 = 2.;
/// From what is drawn to the box
pub(super) const FRAME_GAP: f32 = 5.;
const HANDLE_RADIUS: f32 = 5.5;
/// The mouse may be this much further off a handle and still grab it, enough to cover the line
/// of a frame a few points inside the box
const GRAB_SLACK: f32 = 7.;
/// A side needs to be this long for the handle in its middle
const MIDDLE_HANDLE_FROM: f32 = 40.;
const BUTTON_RADIUS: f32 = 12.;
const BUTTON_SPACING: f32 = 3.;
/// From the box to the buttons
const BUTTON_GAP: f32 = 12.;
const ROTATE_RADIUS: f32 = 9.;
/// From a corner to the middle of the button that turns
const ROTATE_OFFSET: f32 = 20.;
/// Seconds the mouse rests on a handle or a button before its tooltip shows
pub(super) const TOOLTIP_DELAY: f64 = 1.;
/// How much larger a handle or a button gets under the mouse, and how fast
const GROW: f32 = 0.4;
const GROW_TIME: f32 = 0.1;

/// What the buttons above what is selected do
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Action {
    TurnLeft,
    TurnRight,
    FlipHorizontally,
    FlipVertically,
}

impl Action {
    /// In the order of the buttons
    const ALL: [Action; 4] = [
        Action::TurnLeft,
        Action::TurnRight,
        Action::FlipHorizontally,
        Action::FlipVertically,
    ];

    fn hint(self) -> &'static str {
        match self {
            Action::TurnLeft => "Rotate 90° counter-clockwise",
            Action::TurnRight => "Rotate 90° clockwise",
            Action::FlipHorizontally => "Flip horizontally",
            Action::FlipVertically => "Flip vertically",
        }
    }
}

/// What the mouse can grab on the box around what is selected
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    Button(Action),
    /// The handle at a corner or in the middle of a side: -1 left or top, 0 the middle, 1
    /// right or bottom
    Handle([i8; 2]),
    /// The button that turns, next to the corner the mouse is over
    Rotate([i8; 2]),
}

impl Target {
    pub(super) fn hint(self) -> &'static str {
        match self {
            Target::Button(action) => action.hint(),
            Target::Handle(_) => "Resize, Shift keeps the proportions",
            Target::Rotate(_) => "Rotate, Shift snaps to 15°",
        }
    }

    pub(super) fn cursor(self) -> CursorIcon {
        match self {
            Target::Button(_) => CursorIcon::PointingHand,
            Target::Rotate(_) => CursorIcon::Grab,
            Target::Handle(side) => resize_cursor(side),
        }
    }
}

pub(super) fn resize_cursor(side: [i8; 2]) -> CursorIcon {
    match side[0] * side[1] {
        0 if side[0] == 0 => CursorIcon::ResizeVertical,
        0 => CursorIcon::ResizeHorizontal,
        1 => CursorIcon::ResizeNwSe,
        _ => CursorIcon::ResizeNeSw,
    }
}

/// A corner, the middle of a side or the middle of a box on the screen
fn on_box(rect: egui::Rect, side: [i8; 2]) -> Pos2 {
    let along = |lo: f32, hi: f32, s: i8| lo + (hi - lo) * (s as f32 + 1.) / 2.;
    pos2(
        along(rect.left(), rect.right(), side[0]),
        along(rect.top(), rect.bottom(), side[1]),
    )
}

/// The button that turns, off the corner, away from the box
fn rotate_button_at(rect: egui::Rect, corner: [i8; 2]) -> Pos2 {
    let away = egui::vec2(corner[0] as f32, corner[1] as f32).normalized();
    on_box(rect, corner) + away * ROTATE_OFFSET
}

const CORNERS: [[i8; 2]; 4] = [[-1, -1], [1, -1], [1, 1], [-1, 1]];

pub(super) fn rotate_corner(
    rect: egui::Rect,
    shown: Option<[i8; 2]>,
    pointer: Option<Pos2>,
) -> Option<[i8; 2]> {
    let pos = pointer?;
    if let Some(corner) = CORNERS
        .into_iter()
        .find(|c| on_box(rect, *c).distance(pos) <= HANDLE_RADIUS + GRAB_SLACK)
    {
        return Some(corner);
    }
    shown.filter(|c| {
        let (a, b) = (on_box(rect, *c), rotate_button_at(rect, *c));
        let along = b - a;
        let t = ((pos - a).dot(along) / along.length_sq().max(1e-6)).clamp(0., 1.);
        (a + along * t).distance(pos) <= ROTATE_RADIUS + 4.
    })
}

/// The box around what is selected on the screen, with its handles and buttons
pub(super) struct Handles {
    pub(super) rect: egui::Rect,
    handles: Vec<([i8; 2], Pos2)>,
    buttons: Vec<(Action, Pos2)>,
    rotate: Option<([i8; 2], Pos2)>,
}

impl Handles {
    /// The buttons above the box, below it when there is no room, inside it at the top when
    /// there is none either. `view` is the part of the screen that shows the image.
    pub(super) fn new(rect: egui::Rect, rotate: Option<[i8; 2]>, view: egui::Rect) -> Self {
        let mut handles = vec![];
        for y in -1..=1i8 {
            for x in -1..=1i8 {
                // none in the middle of a short side, the corners are close
                let crowded = |side: i8, length: f32| side == 0 && length < MIDDLE_HANDLE_FROM;
                if (x, y) == (0, 0) || crowded(x, rect.width()) || crowded(y, rect.height()) {
                    continue;
                }
                handles.push(([x, y], on_box(rect, [x, y])));
            }
        }
        let step = BUTTON_RADIUS * 2. + BUTTON_SPACING;
        let half = step * Action::ALL.len() as f32 / 2.;
        let above = rect.top() - BUTTON_GAP - BUTTON_RADIUS;
        let below = rect.bottom() + BUTTON_GAP + BUTTON_RADIUS;
        let y = if above - BUTTON_RADIUS >= view.top() {
            above
        } else if below + BUTTON_RADIUS <= view.bottom() {
            below
        } else {
            rect.top().max(view.top()) + BUTTON_GAP + BUTTON_RADIUS
        };
        let x = rect
            .center()
            .x
            .max(view.left() + half)
            .min(view.right() - half);
        let first = (Action::ALL.len() as f32 - 1.) / 2.;
        let buttons = Action::ALL
            .iter()
            .enumerate()
            .map(|(i, action)| (*action, pos2(x + (i as f32 - first) * step, y)))
            .collect();
        Self {
            rect,
            handles,
            buttons,
            rotate: rotate.map(|corner| (corner, rotate_button_at(rect, corner))),
        }
    }

    /// What the mouse grabs at `pos`. Inside a small box, handles shrink their reach so they don't crowd out what's in it.
    pub(super) fn target(&self, pos: Pos2) -> Option<Target> {
        if let Some((action, _)) = self
            .buttons
            .iter()
            .find(|(_, c)| c.distance(pos) <= BUTTON_RADIUS)
        {
            return Some(Target::Button(*action));
        }
        if let Some((corner, c)) = self.rotate
            && c.distance(pos) <= ROTATE_RADIUS + 2.
        {
            return Some(Target::Rotate(corner));
        }
        let reach = if self.rect.contains(pos) {
            let room = self.rect.width().min(self.rect.height()) / 4.;
            (HANDLE_RADIUS + GRAB_SLACK).min(room)
        } else {
            HANDLE_RADIUS + GRAB_SLACK
        };
        self.handles
            .iter()
            .filter(|(_, c)| c.distance(pos) <= reach)
            .min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)))
            .map(|(side, _)| Target::Handle(*side))
    }

    /// The box, its handles, and while idle its buttons. The `hot` target grows and turns red,
    /// and is drawn last so it sits in front of its neighbors.
    pub(super) fn paint(
        &self,
        ctx: &egui::Context,
        out: &mut Vec<egui::Shape>,
        hot: Option<Target>,
        idle: bool,
    ) {
        frame_line(out, self.rect, FRAME_WIDTH);
        let grow = |id: Id, on: bool| ctx.animate_bool_with_time(id, on, GROW_TIME);
        let mut handles: Vec<(f32, Pos2)> = self
            .handles
            .iter()
            .map(|(side, centre)| {
                let on = hot == Some(Target::Handle(*side));
                (grow(Id::new(("annotation_handle", *side)), on), *centre)
            })
            .collect();
        handles.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (t, centre) in handles {
            dot(out, centre, HANDLE_RADIUS, t);
        }
        if !idle {
            return;
        }
        let mut buttons: Vec<(f32, Action, Pos2)> = self
            .buttons
            .iter()
            .map(|(action, centre)| {
                let on = hot == Some(Target::Button(*action));
                (
                    grow(Id::new(("annotation_button", *action)), on),
                    *action,
                    *centre,
                )
            })
            .collect();
        buttons.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (t, action, centre) in buttons {
            paint_button(out, centre, action, t);
        }
        if let Some((corner, centre)) = self.rotate {
            let t = grow(
                Id::new("annotation_rotate"),
                hot == Some(Target::Rotate(corner)),
            );
            let scale = 1. + GROW * t;
            dot(out, centre, ROTATE_RADIUS, t);
            paint_turn(
                out,
                centre,
                4.2 * scale,
                false,
                symbol_color(t),
                1.6 * scale,
            );
        }
    }
}

pub(super) fn frame_line(out: &mut Vec<egui::Shape>, rect: egui::Rect, width: f32) {
    let corners = vec![
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ];
    out.push(egui::Shape::closed_line(
        corners.clone(),
        Stroke::new(width + 2., OVER_IMAGE_EDGE),
    ));
    out.push(egui::Shape::closed_line(
        corners,
        Stroke::new(width, YELLOW),
    ));
}

/// A handle or the round of a button, `t` from 0 to 1 as far as it grew under the mouse
fn dot(out: &mut Vec<egui::Shape>, centre: Pos2, radius: f32, t: f32) {
    let radius = radius * (1. + GROW * t);
    out.push(egui::Shape::circle_filled(
        centre,
        radius,
        fade(YELLOW, RED, t),
    ));
    out.push(egui::Shape::circle_stroke(
        centre,
        radius,
        Stroke::new(1., OVER_IMAGE_EDGE),
    ));
}

/// Dark on the round of a button, white once it is red
fn symbol_color(t: f32) -> Color32 {
    fade(BLACK, WHITE, t)
}

/// Filled, in either order of its corners
fn triangle(out: &mut Vec<egui::Shape>, mut corners: [Pos2; 3], color: Color32) {
    let (a, b) = (corners[1] - corners[0], corners[2] - corners[0]);
    // clockwise on the screen
    if a.x * b.y - a.y * b.x < 0. {
        corners.swap(1, 2);
    }
    out.push(egui::Shape::convex_polygon(
        corners.to_vec(),
        color,
        Stroke::NONE,
    ));
}

/// A round arrow, open at the top, its head pointing the way it turns
fn paint_turn(
    out: &mut Vec<egui::Shape>,
    centre: Pos2,
    radius: f32,
    clockwise: bool,
    color: Color32,
    width: f32,
) {
    let side = if clockwise { 1. } else { -1. };
    let direction = |degrees: f32| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        egui::vec2(cos * side, sin)
    };
    // from the right round the bottom and the left to the top, clockwise
    let end = -70f32;
    let start = end - 280.;
    let steps = 24;
    let points = (0..=steps)
        .map(|i| centre + direction(start + (end - start) * i as f32 / steps as f32) * radius)
        .collect();
    out.push(egui::Shape::line(points, Stroke::new(width, color)));
    let tip = centre + direction(end) * radius;
    let along = direction(end + 90.);
    let outward = direction(end);
    let head = width * 1.8;
    triangle(
        out,
        [
            tip + along * head * 0.9,
            tip - along * head * 0.4 + outward * head,
            tip - along * head * 0.4 - outward * head,
        ],
        color,
    );
}

/// A line with a head at both ends, along `along`
fn paint_both_ways(
    out: &mut Vec<egui::Shape>,
    centre: Pos2,
    along: egui::Vec2,
    half: f32,
    color: Color32,
    width: f32,
) {
    let across = egui::vec2(-along.y, along.x);
    let head = width * 2.;
    let wide = width * 1.7;
    let (a, b) = (centre - along * half, centre + along * half);
    out.push(egui::Shape::line_segment(
        [a + along * head * 0.8, b - along * head * 0.8],
        Stroke::new(width, color),
    ));
    for (tip, back) in [(a, along), (b, -along)] {
        let base = tip + back * head;
        triangle(
            out,
            [tip, base + across * wide, base - across * wide],
            color,
        );
    }
}

/// A button above the box with its symbol, grown by `t` from 0 to 1
fn paint_button(out: &mut Vec<egui::Shape>, centre: Pos2, action: Action, t: f32) {
    dot(out, centre, BUTTON_RADIUS, t);
    let scale = 1. + GROW * t;
    let (color, width) = (symbol_color(t), 2. * scale);
    match action {
        Action::TurnLeft => paint_turn(out, centre, 6. * scale, false, color, width),
        Action::TurnRight => paint_turn(out, centre, 6. * scale, true, color, width),
        Action::FlipHorizontally => {
            paint_both_ways(out, centre, egui::vec2(1., 0.), 7.5 * scale, color, width)
        }
        Action::FlipVertically => {
            paint_both_ways(out, centre, egui::vec2(0., 1.), 7.5 * scale, color, width)
        }
    }
}

/// A small box with text next to the mouse, kept on the part of the screen that shows the image
pub(super) fn tooltip(ctx: &egui::Context, pointer: Pos2, text: &str) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        Id::new("annotation_tooltip"),
    ));
    let dark = ctx.global_style().visuals.dark_mode;
    let galley = painter.layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(13.),
        TEXT.in_theme(dark),
    );
    let margin = egui::vec2(8., 5.);
    let size = galley.size() + margin * 2.;
    let view = canvas_rect(ctx);
    let mut at = pointer + egui::vec2(14., 18.);
    if at.x + size.x > view.right() {
        at.x = (view.right() - size.x).max(view.left());
    }
    if at.y + size.y > view.bottom() {
        at.y = pointer.y - 10. - size.y;
    }
    let rect = egui::Rect::from_min_size(at, size);
    painter.rect(
        rect,
        4.,
        PANEL.in_theme(dark),
        Stroke::new(1., BUTTON.in_theme(dark)),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + margin, galley, TEXT.in_theme(dark));
}

/// What a button above the box does to what is selected
pub(super) fn act(
    annotations: &mut [Annotation],
    selected: &[usize],
    mapping: &Mapping,
    action: Action,
) {
    let Some(bounds) = selection_bounds(annotations, selected, mapping) else {
        return;
    };
    let centre = middle_of(bounds.shape);
    for &i in selected {
        let Some(a) = annotations.get(i).filter(|a| !a.hidden) else {
            continue;
        };
        let mut changed = match action {
            Action::TurnLeft => rotated(a, mapping, centre, -90),
            Action::TurnRight => rotated(a, mapping, centre, 90),
            Action::FlipHorizontally => flipped(a, mapping, centre, 0),
            Action::FlipVertically => flipped(a, mapping, centre, 1),
        };
        changed.reanchor(mapping.original);
        annotations[i] = changed;
    }
}
