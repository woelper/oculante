//! Drawing, selecting and moving annotations on the image with the mouse, and the keys for them.

use super::*;

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
    forget_what_is_gone(annotations, state);
    let Some(tool) = state.tool else {
        state.finish_typing(annotations);
        state.selected.clear();
        state.drag = None;
        state.grab = false;
        return false;
    };
    state.grab = true;
    let typed = text_ui(ctx, annotations, state, mapping, geo);
    let copy = state.typing.is_none() && keys(ctx, annotations, state, mapping, tool);

    let canvas = Canvas::new(ctx, mapping, geo, tool, pointer_over_ui);
    if let Some(pos) = canvas.pointer {
        let p = canvas.to_result(pos);
        if canvas.pressed && !pointer_over_ui {
            press(&canvas, annotations, state, p, typed);
        }
        if canvas.down || canvas.released {
            drag(&canvas, annotations, state, p);
        }
        if !pointer_over_ui {
            point(&canvas, annotations, state, p);
        }
    }
    if canvas.down {
        canvas.show_select_box(state);
    }
    if canvas.released {
        release(&canvas, annotations, state);
        double_click(&canvas, annotations, state);
    }
    show_selection(&canvas, annotations, state);
    copy
}

/// What the canvas works with in a frame: where the image is on the screen, the mouse, and the
/// layer it draws on over the image
struct Canvas<'a> {
    ctx: &'a egui::Context,
    mapping: &'a Mapping,
    geo: &'a ImageGeometry,
    tool: Tool,
    pointer_over_ui: bool,
    painter: egui::Painter,
    pressed: bool,
    down: bool,
    released: bool,
    /// On the second release
    double: bool,
    shift: bool,
    /// A drag goes on over the panels too
    pointer: Option<Pos2>,
    /// The mouse may be this far off a line on the screen and still hit it, in pixels of the
    /// edited image
    slack: f32,
}

impl<'a> Canvas<'a> {
    fn new(
        ctx: &'a egui::Context,
        mapping: &'a Mapping,
        geo: &'a ImageGeometry,
        tool: Tool,
        pointer_over_ui: bool,
    ) -> Self {
        let (pressed, down, released, double, shift) = ctx.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.primary_released(),
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary),
                i.modifiers.shift,
            )
        });
        let painter = ctx
            .layer_painter(egui::LayerId::new(
                egui::Order::Middle,
                Id::new("annotation_overlay"),
            ))
            .with_clip_rect(canvas_rect(ctx));
        Self {
            ctx,
            mapping,
            geo,
            tool,
            pointer_over_ui,
            painter,
            pressed,
            down,
            released,
            double,
            shift,
            pointer: ctx.input(|i| i.pointer.hover_pos()),
            slack: 6. / geo.scale.max(0.01),
        }
    }

    /// A point on the screen in the edited image
    fn to_result(&self, p: Pos2) -> [f32; 2] {
        [
            (p.x - self.geo.offset.x) / self.geo.scale,
            (p.y - self.geo.offset.y) / self.geo.scale,
        ]
    }

    /// A point of the edited image on the screen
    fn to_screen(&self, p: Point) -> Pos2 {
        pos2(
            p.x * self.geo.scale + self.geo.offset.x,
            p.y * self.geo.scale + self.geo.offset.y,
        )
    }

    /// The annotation in front at `p` in the edited image
    fn hit(&self, annotations: &[Annotation], p: [f32; 2]) -> Option<usize> {
        (0..annotations.len()).rev().find(|i| {
            !annotations[*i].hidden && hits(&annotations[*i], self.mapping, p, self.slack)
        })
    }

    /// A thin line around it, for what the mouse points at, a stronger one for what is selected
    fn show(&self, a: &Annotation, strong: bool) {
        let t = a.transform(self.mapping);
        let on_screen = (scale_of(t) * self.geo.scale).max(0.001);
        let width = if strong { 1.5 } else { 1. };
        for line in outline(a, 3. / on_screen, on_screen) {
            let points: Vec<Pos2> = line
                .into_iter()
                .map(|p| {
                    let mut p = Point::from_xy(p[0], p[1]);
                    t.map_point(&mut p);
                    self.to_screen(p)
                })
                .collect();
            self.painter.add(egui::Shape::closed_line(
                points.clone(),
                Stroke::new(width + 2., OVER_IMAGE_EDGE),
            ));
            let yellow = Stroke::new(width, YELLOW);
            if strong {
                // what is selected is dashed, what is pointed at is not
                let mut points = points;
                points.push(points[0]);
                self.painter
                    .extend(egui::Shape::dashed_line(&points, yellow, 6., 4.));
            } else {
                self.painter.add(egui::Shape::closed_line(points, yellow));
            }
        }
    }

    /// The box that selects, while it is dragged
    fn show_select_box(&self, state: &AnnotateUi) {
        if let Some(Drag::Select { from, .. }) = &state.drag
            && let Some(pos) = self.pointer
        {
            let accent = accent_color(self.ctx);
            let corner = self.to_screen(Point::from_xy(from[0], from[1]));
            self.painter.rect(
                egui::Rect::from_two_pos(corner, pos),
                0.,
                opacity(accent, Opacity::Faint),
                Stroke::new(1., accent),
                egui::StrokeKind::Inside,
            );
        }
    }
}

fn is_text(a: &Annotation) -> bool {
    matches!(a.shape, Shape::Text(_))
}

/// To the whole pixel
fn round(v: [f32; 2]) -> [i32; 2] {
    [v[0].round() as i32, v[1].round() as i32]
}

/// The list may have changed elsewhere: what was selected or typed and is gone is forgotten
fn forget_what_is_gone(annotations: &[Annotation], state: &mut AnnotateUi) {
    state.selected.retain(|i| *i < annotations.len());
    if state
        .typing
        .is_some_and(|i| !matches!(annotations.get(i).map(|a| &a.shape), Some(Shape::Text(_))))
    {
        state.typing = None;
    }
}

/// Delete removes what is selected, Esc lets go of it and then of the tool, the arrows move it.
/// Returns whether the image should be copied.
fn keys(
    ctx: &egui::Context,
    annotations: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    mapping: &Mapping,
    tool: Tool,
) -> bool {
    let original = mapping.original;
    let (delete, escape, shift) = ctx.input(|i| {
        (
            i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
            // that only closes a list
            i.key_pressed(Key::Escape) && !std::mem::take(&mut state.popup_open),
            i.modifiers.shift,
        )
    });
    let copy = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)));
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
    copy
}

/// A press on the image at `p`: it ends the typing, selects, starts a move, a box that selects,
/// a text or a drawing
fn press(
    canvas: &Canvas,
    annotations: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    p: [f32; 2],
    typed: bool,
) {
    let (tool, mapping, original) = (canvas.tool, canvas.mapping, canvas.mapping.original);
    let o = mapping.to_original(p);
    if state.typing.is_some() || typed {
        // the click only ends the typing
        state.finish_typing(annotations);
        return;
    }
    let under = canvas.hit(annotations, p);
    let text_under = under.filter(|i| is_text(&annotations[*i]));
    match under {
        // the text tool changes the text it clicks
        Some(i) if tool == Tool::Text && text_under.is_some() => state.start_typing(i),
        Some(i) if canvas.shift => {
            if let Some(at) = state.selected.iter().position(|s| *s == i) {
                state.selected.remove(at);
            } else {
                state.selected.push(i);
            }
        }
        // what is pointed at is selected and moved, with what else is selected
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
            let before = if canvas.shift {
                state.selected.clone()
            } else {
                vec![]
            };
            state.selected = before.clone();
            state.drag = Some(Drag::Select { from: p, before });
        }
        None if tool == Tool::Text => {
            if let Some(mut a) = state.style.new_annotation(tool) {
                a.offset = round(o);
                a.stand_upright(mapping);
                annotations.push(a);
                state.start_typing(annotations.len() - 1);
            }
        }
        None => {
            if let Some(mut a) = state.style.new_annotation(tool) {
                a.offset = round(o);
                if matches!(a.shape, Shape::Stamp(_)) {
                    a.stand_upright(mapping);
                    a.centre_at(o, original);
                }
                annotations.push(a);
                let index = annotations.len() - 1;
                state.selected = vec![index];
                state.drag = Some(Drag::Draw { index, from: o });
            }
        }
    }
}

/// The mouse at `p` while a drag goes on: what is drawn grows, what is moved follows, the box
/// selects what is in it
fn drag(canvas: &Canvas, annotations: &mut [Annotation], state: &mut AnnotateUi, p: [f32; 2]) {
    let (mapping, original) = (canvas.mapping, canvas.mapping.original);
    let o = mapping.to_original(p);
    match &state.drag {
        Some(Drag::Draw { index, from }) => {
            if let Some(a) = annotations.get_mut(*index) {
                let mut d = [o[0] - from[0], o[1] - from[1]];
                if canvas.shift && matches!(a.shape, Shape::Rect(_) | Shape::Ellipse(_)) {
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
                    // a square from where the drag started, a click places one as large as
                    // the last
                    Shape::Stamp(_) => {
                        let side = d[0].abs().max(d[1].abs());
                        let centre = if side * canvas.geo.scale < 4. {
                            a.width = state.style.stamp_size;
                            *from
                        } else {
                            a.width = side.round() as u32;
                            let half = |d: f32| (side / 2.).copysign(d);
                            [from[0] + half(d[0]), from[1] + half(d[1])]
                        };
                        a.centre_at(centre, original);
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

/// The cursor for what a click at `p` would do, and a thin line around what it would select
fn point(canvas: &Canvas, annotations: &[Annotation], state: &AnnotateUi, p: [f32; 2]) {
    let tool = canvas.tool;
    let pointed = canvas.hit(annotations, p).filter(|_| state.drag.is_none());
    canvas.ctx.set_cursor_icon(match pointed {
        Some(i) if tool == Tool::Text && is_text(&annotations[i]) => CursorIcon::Text,
        Some(_) => CursorIcon::Move,
        None if matches!(state.drag, Some(Drag::Move { .. })) => CursorIcon::Move,
        None if tool == Tool::Select => CursorIcon::Default,
        None if tool == Tool::Text => CursorIcon::Text,
        None => CursorIcon::Crosshair,
    });
    if let Some(i) = pointed.filter(|i| !state.selected.contains(i)) {
        canvas.show(&annotations[i], false);
    }
}

/// The end of a drag: what was drawn empty is removed, what was drawn or moved keeps to the
/// nearest anchor
fn release(canvas: &Canvas, annotations: &mut Vec<Annotation>, state: &mut AnnotateUi) {
    let original = canvas.mapping.original;
    let Some(drag) = state.drag.take() else {
        return;
    };
    match drag {
        Drag::Draw { index, .. } => {
            if annotations.get(index).is_some_and(|a| a.is_empty()) {
                state.remove(annotations, vec![index]);
            } else if let Some(a) = annotations.get_mut(index) {
                a.reanchor(original);
                if matches!(a.shape, Shape::Stamp(_)) {
                    state.style.stamp_size = a.width;
                }
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

/// A double click changes a text
fn double_click(canvas: &Canvas, annotations: &[Annotation], state: &mut AnnotateUi) {
    if canvas.double
        && !canvas.pointer_over_ui
        && state.typing.is_none()
        && let Some(pos) = canvas.pointer
        && let Some(i) = canvas
            .hit(annotations, canvas.to_result(pos))
            .filter(|i| is_text(&annotations[*i]))
    {
        state.start_typing(i);
    }
}

/// The lines around what is selected, not while it is typed or drawn, and around the one of the
/// card the mouse is over in the panel
fn show_selection(canvas: &Canvas, annotations: &[Annotation], state: &AnnotateUi) {
    let drawn = match state.drag {
        Some(Drag::Draw { index, .. }) => Some(index),
        _ => None,
    };
    for &i in &state.selected {
        if state.typing != Some(i) && drawn != Some(i) && !annotations[i].hidden {
            canvas.show(&annotations[i], true);
        }
    }
    if let Some(i) = state.pointed_card.filter(|i| !state.selected.contains(i))
        && let Some(a) = annotations.get(i).filter(|a| !a.hidden)
    {
        canvas.show(a, false);
    }
}
