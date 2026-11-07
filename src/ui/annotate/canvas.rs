//! Drawing, selecting and moving annotations on the image with the mouse, and the keys for them.

use std::cell::RefCell;

use super::*;

/// Drawing, selecting and moving annotations with the mouse, and the keys for them, unless a
/// window is open over the image. Returns whether the image should be copied.
pub fn canvas_ui(
    ctx: &egui::Context,
    annotations: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    mapping: &Mapping,
    geo: &ImageGeometry,
    pointer_over_ui: bool,
    window_open: bool,
) -> bool {
    // a field that has the keys keeps them, as a window open over the image does: Backspace in
    // the file manager does not remove what is selected
    let keys_free = !window_open && !ctx.egui_wants_keyboard_input();
    forget_what_is_gone(annotations, state);
    undo_and_redo(ctx, annotations, state, keys_free);
    let Some(tool) = state.tool else {
        state.finish_typing(annotations);
        state.selected.clear();
        state.drag = None;
        state.grab = false;
        state.rotate_corner = None;
        state.hovered = None;
        state.menu = None;
        state.overlay.clear();
        return false;
    };
    state.grab = true;
    let typed = text_ui(ctx, annotations, state, mapping, geo);
    let copy = keys_free && state.typing.is_none() && keys(ctx, annotations, state, mapping, tool);

    let canvas = Canvas::new(ctx, mapping, geo, tool, pointer_over_ui);
    let selection = lay_out_selection(&canvas, annotations, state);
    let mut dragged = Dragged::default();
    if let Some(mouse) = canvas.mouse {
        // a click besides the menu only closes it
        let closes_menu = canvas.pressed && !pointer_over_ui && state.menu.take().is_some();
        if canvas.pressed && !pointer_over_ui && !closes_menu {
            press(
                &canvas,
                annotations,
                state,
                selection.as_ref(),
                mouse,
                typed,
            );
        }
        right_click(
            &canvas,
            annotations,
            state,
            selection.as_ref(),
            mouse,
            typed,
        );
        if canvas.down || canvas.released {
            dragged = drag(&canvas, annotations, state, mouse.image);
        }
        point(
            &canvas,
            annotations,
            state,
            selection.as_ref(),
            &dragged,
            mouse,
        );
    }
    if canvas.down {
        canvas.show_select_box(state);
    }
    if canvas.released {
        release(&canvas, annotations, state, selection.as_ref());
        double_click(&canvas, annotations, state);
    }
    let target = selection.and_then(|s| s.target);
    show_selection(&canvas, annotations, state, target, &dragged);
    if state.selected.is_empty() || state.typing.is_some() {
        state.menu = None;
    }
    if let Some(at) = state.menu {
        menu_ui(ctx, annotations, state, mapping, at);
    }
    show_tooltip(&canvas, state, dragged.angle);
    state.overlay = canvas.shapes.take();
    copy
}

/// The selection's box at the start of the frame, and what the mouse is over.
struct Selection {
    bounds: Bounds,
    handles: Handles,
    target: Option<Target>,
}

/// Where the mouse is, on the screen and in the edited image
#[derive(Clone, Copy)]
struct Mouse {
    screen: Pos2,
    image: [f32; 2],
}

/// What a drag changed this frame: the new angle, and the handle it resizes by.
/// The handle flips to the opposite side once the drag passes it.
#[derive(Default)]
struct Dragged {
    angle: Option<String>,
    side: Option<[i8; 2]>,
}

/// What the canvas works with in a frame: where the image is on the screen, the mouse, and the
/// layer it draws on over the image
struct Canvas<'a> {
    ctx: &'a egui::Context,
    mapping: &'a Mapping,
    geo: &'a ImageGeometry,
    tool: Tool,
    pointer_over_ui: bool,
    /// What is drawn over the image, see `AnnotateUi::overlay`
    shapes: RefCell<Vec<egui::Shape>>,
    pressed: bool,
    down: bool,
    released: bool,
    right_pressed: bool,
    /// On the second release
    double: bool,
    shift: bool,
    /// A drag goes on over the panels too
    mouse: Option<Mouse>,
    /// The mouse may be this far off a line on the screen and still hit it, in pixels of the
    /// edited image
    slack: f32,
    /// The part of the screen that shows the image
    view: egui::Rect,
}

impl<'a> Canvas<'a> {
    fn new(
        ctx: &'a egui::Context,
        mapping: &'a Mapping,
        geo: &'a ImageGeometry,
        tool: Tool,
        pointer_over_ui: bool,
    ) -> Self {
        let (pressed, down, released, right_pressed, double, shift) = ctx.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.primary_released(),
                i.pointer.secondary_pressed(),
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary),
                i.modifiers.shift,
            )
        });
        let mut canvas = Self {
            ctx,
            mapping,
            geo,
            tool,
            pointer_over_ui,
            shapes: RefCell::default(),
            pressed,
            down,
            released,
            right_pressed,
            double,
            shift,
            mouse: None,
            slack: 6. / geo.scale.max(0.01),
            view: canvas_rect(ctx),
        };
        canvas.mouse = ctx.input(|i| i.pointer.hover_pos()).map(|screen| Mouse {
            screen,
            image: canvas.to_result(screen),
        });
        canvas
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

    /// The annotation in front at `p` in the edited image. A spotlight only when nothing else
    /// is there: what it shows can still be clicked.
    fn hit(&self, annotations: &[Annotation], p: [f32; 2]) -> Option<usize> {
        let find = |spotlight: bool| {
            (0..annotations.len()).rev().find(|i| {
                let a = &annotations[*i];
                !a.hidden && a.is_spotlight() == spotlight && hits(a, self.mapping, p, self.slack)
            })
        };
        find(false).or_else(|| find(true))
    }

    /// The box around what is drawn of the shown ones of `indices`, a little around it, on the
    /// screen
    fn screen_box(
        &self,
        annotations: &[Annotation],
        indices: &[usize],
    ) -> Option<(Bounds, egui::Rect)> {
        let bounds = selection_bounds(annotations, indices, self.mapping)?;
        let d = bounds.drawn;
        let rect = egui::Rect::from_min_max(
            self.to_screen(Point::from_xy(d[0], d[1])),
            self.to_screen(Point::from_xy(d[2], d[3])),
        )
        .expand(FRAME_GAP);
        Some((bounds, rect))
    }

    /// The box around what is selected, not while it is typed or drawn
    fn selected_box(
        &self,
        annotations: &[Annotation],
        state: &AnnotateUi,
    ) -> Option<(Bounds, egui::Rect)> {
        if state.typing.is_some() || matches!(state.drag, Some(Drag::Draw { .. })) {
            return None;
        }
        self.screen_box(annotations, &state.selected)
    }

    /// A thin box around the one the mouse points at, or whose card it is over
    fn show_pointed(&self, annotations: &[Annotation], i: usize) {
        if let Some((_, rect)) = self.screen_box(annotations, &[i]) {
            frame_line(&mut self.shapes.borrow_mut(), rect, 1.);
        }
    }

    /// The box that selects, while it is dragged
    fn show_select_box(&self, state: &AnnotateUi) {
        if let Some(Drag::Select { from, .. }) = &state.drag
            && let Some(mouse) = self.mouse
        {
            let accent = accent_color(self.ctx);
            let corner = self.to_screen(Point::from_xy(from[0], from[1]));
            let rect = egui::Rect::from_two_pos(corner, mouse.screen);
            let mut shapes = self.shapes.borrow_mut();
            shapes.push(egui::Shape::rect_filled(
                rect,
                0.,
                opacity(accent, Opacity::Faint),
            ));
            shapes.push(egui::Shape::rect_stroke(
                rect,
                0.,
                Stroke::new(1., accent),
                egui::StrokeKind::Inside,
            ));
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

/// What changed, here or on the cards, can be undone once it is done. Ctrl+Z (Cmd+Z on a Mac)
/// undoes, with Shift or Ctrl+Y it redoes, while a tool is picked and the keys are free; a box to
/// type in has its own.
fn undo_and_redo(
    ctx: &egui::Context,
    annotations: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    keys_free: bool,
) {
    let busy =
        state.drag.is_some() || state.typing.is_some() || ctx.input(|i| i.pointer.any_down());
    state.history.record(annotations, busy);
    if state.tool.is_none() || busy || !keys_free {
        return;
    }
    let (undo, redo) = ctx.input(|i| {
        let z = i.modifiers.command && i.key_pressed(Key::Z);
        let y = i.modifiers.command && i.key_pressed(Key::Y);
        (z && !i.modifiers.shift, (z && i.modifiers.shift) || y)
    });
    if (undo || redo) && state.history.step(annotations, undo) {
        state.selected.retain(|i| *i < annotations.len());
        state.drag = None;
        state.typing = None;
        state.menu = None;
    }
}

/// Delete removes what is selected, Esc closes the menu, stops a drag, lets go of what is
/// selected and then of the tool, Ctrl+A (Cmd+A on a Mac) selects everything shown, the arrows
/// move what is selected. Returns whether the image should be copied.
fn keys(
    ctx: &egui::Context,
    annotations: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    mapping: &Mapping,
    tool: Tool,
) -> bool {
    let original = mapping.original;
    let (delete, mut escape, shift, select_all) = ctx.input(|i| {
        (
            i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace),
            // that only closes a list
            i.key_pressed(Key::Escape) && !std::mem::take(&mut state.popup_open),
            i.modifiers.shift,
            i.modifiers.command && i.key_pressed(Key::A),
        )
    });
    let copy = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)));
    // that only closes the menu, what is selected stays
    if escape && state.menu.take().is_some() {
        escape = false;
    }
    if delete && !state.selected.is_empty() {
        let selected = state.selected.clone();
        state.remove(annotations, selected);
    }
    if escape {
        match state.drag.take() {
            // a drag stops, what it changed is as it was
            Some(Drag::Move { starts, .. }) => {
                for (i, start) in state.selected.iter().zip(starts) {
                    annotations[*i].set_start(start, original);
                }
            }
            Some(Drag::Resize { before, .. } | Drag::Rotate { before, .. }) => {
                for (i, a) in before {
                    if let Some(slot) = annotations.get_mut(i) {
                        *slot = a;
                    }
                }
            }
            Some(Drag::Draw { index, .. }) => state.remove(annotations, vec![index]),
            Some(Drag::Select { before, .. }) => state.selected = before,
            Some(Drag::Button(_)) | None => {
                if state.selected.is_empty() {
                    // from drawing back to selecting, then no more annotating
                    state.tool = (tool != Tool::Select).then_some(Tool::Select);
                }
                state.selected.clear();
            }
        }
    }
    if select_all && state.drag.is_none() {
        state.select_all(annotations);
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

/// The box around what is selected as the frame begins: which corner shows the button to turn,
/// what of it the mouse is over, and since when, for its tooltip
fn lay_out_selection(
    canvas: &Canvas,
    annotations: &[Annotation],
    state: &mut AnnotateUi,
) -> Option<Selection> {
    let Some((bounds, rect)) = canvas.selected_box(annotations, state) else {
        state.rotate_corner = None;
        state.hovered = None;
        return None;
    };
    let pointer = canvas
        .mouse
        .map(|mouse| mouse.screen)
        .filter(|_| !canvas.pointer_over_ui);
    if state.drag.is_none() {
        state.rotate_corner = rotate_corner(rect, state.rotate_corner, pointer);
    }
    let idle = matches!(state.drag, None | Some(Drag::Button(_)));
    let handles = Handles::new(rect, state.rotate_corner.filter(|_| idle), canvas.view);
    let target = pointer.and_then(|pos| handles.target(pos));
    let now = canvas.ctx.input(|i| i.time);
    state.hovered = match (state.hovered, target.filter(|_| state.drag.is_none())) {
        (Some((was, since)), Some(t)) if was == t => Some((t, since)),
        (_, Some(t)) => Some((t, now)),
        _ => None,
    };
    Some(Selection {
        bounds,
        handles,
        target,
    })
}

/// A move of what is selected, from `o` in the image as it was loaded
fn start_move(
    annotations: &[Annotation],
    state: &mut AnnotateUi,
    o: [f32; 2],
    original: (u32, u32),
) {
    state.drag = Some(Drag::Move {
        from: o,
        starts: state
            .selected
            .iter()
            .map(|i| annotations[*i].start(original))
            .collect(),
    });
}

/// A press on the image where the mouse is: it ends the typing, grabs a handle or a button of the
/// box around what is selected, selects, starts a move, a box that selects, a text or a drawing
fn press(
    canvas: &Canvas,
    annotations: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    selection: Option<&Selection>,
    mouse: Mouse,
    typed: bool,
) {
    let (pos, p) = (mouse.screen, mouse.image);
    let (tool, mapping, original) = (canvas.tool, canvas.mapping, canvas.mapping.original);
    let o = mapping.to_original(p);
    if state.typing.is_some() || typed {
        // the click only ends the typing
        state.finish_typing(annotations);
        return;
    }
    if let Some(selection) = selection
        && let Some(target) = selection.target
    {
        let before: Vec<(usize, Annotation)> = state
            .selected
            .iter()
            .filter(|i| !annotations[**i].hidden)
            .map(|&i| (i, annotations[i].clone()))
            .collect();
        let shape = selection.bounds.shape;
        state.drag = Some(match target {
            Target::Button(action) => Drag::Button(action),
            Target::Handle(side) => Drag::Resize {
                side,
                from: p,
                shape,
                before,
            },
            Target::Rotate(_) => {
                let pivot = middle_of(shape);
                // one is shown at its angle, several by how much they turn
                let base = match before.as_slice() {
                    [(_, a)] => shown_angle(a, mapping),
                    _ => 0.,
                };
                Drag::Rotate {
                    pivot,
                    from: angle_of(p, pivot),
                    base,
                    before,
                }
            }
        });
        return;
    }
    let under = canvas.hit(annotations, p);
    let text_under = under.filter(|i| is_text(&annotations[*i]));
    // anywhere in the box around what is selected it is moved, Shift selects more with a box as
    // besides it
    let inside = tool == Tool::Select
        && !canvas.shift
        && selection.is_some_and(|s| s.handles.rect.contains(pos));
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
            start_move(annotations, state, o, original);
        }
        None if inside => start_move(annotations, state, o, original),
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
                if let Shape::Counter(number) = &mut a.shape {
                    *number = next_counter(annotations);
                }
                if matches!(a.shape, Shape::Stamp(_) | Shape::Counter(_)) {
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

/// A right-click on what is selected, or in its box, opens its menu; on another one it selects
/// that first, besides everything it closes the menu
fn right_click(
    canvas: &Canvas,
    annotations: &[Annotation],
    state: &mut AnnotateUi,
    selection: Option<&Selection>,
    mouse: Mouse,
    typed: bool,
) {
    let (pos, p) = (mouse.screen, mouse.image);
    if !canvas.right_pressed
        || canvas.pointer_over_ui
        || state.drag.is_some()
        || state.typing.is_some()
        || typed
    {
        return;
    }
    let in_box = selection.is_some_and(|s| s.handles.rect.contains(pos));
    state.menu = match canvas.hit(annotations, p) {
        _ if in_box => Some(pos),
        Some(i) => {
            if !state.selected.contains(&i) {
                state.selected = vec![i];
            }
            Some(pos)
        }
        None => None,
    };
}

/// The mouse at `p` while a drag goes on: what is drawn grows, what is moved follows, the box
/// selects what is in it, what is resized or turned follows the handle
fn drag(
    canvas: &Canvas,
    annotations: &mut [Annotation],
    state: &mut AnnotateUi,
    p: [f32; 2],
) -> Dragged {
    let (mapping, original) = (canvas.mapping, canvas.mapping.original);
    let o = mapping.to_original(p);
    let mut dragged = Dragged::default();
    match &state.drag {
        Some(Drag::Draw { index, from }) => {
            if let Some(a) = annotations.get_mut(*index) {
                let mut d = [o[0] - from[0], o[1] - from[1]];
                let squared = matches!(
                    a.shape,
                    Shape::Rect(_) | Shape::Ellipse(_) | Shape::Spotlight { .. }
                );
                if canvas.shift && squared {
                    // as wide as high
                    let side = d[0].abs().max(d[1].abs());
                    d = [side.copysign(d[0]), side.copysign(d[1])];
                }
                let corner = [from[0] + d[0].min(0.), from[1] + d[1].min(0.)];
                let size = round([d[0].abs(), d[1].abs()]);
                match &mut a.shape {
                    Shape::Arrow(to) => *to = round(d),
                    Shape::Rect(s)
                    | Shape::Ellipse(s)
                    | Shape::Redact { size: s, .. }
                    | Shape::Spotlight { size: s, .. } => {
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
                    Shape::Stamp(_) | Shape::Counter(_) => {
                        let side = d[0].abs().max(d[1].abs());
                        let centre = if side * canvas.geo.scale < 4. {
                            a.width = if matches!(a.shape, Shape::Counter(_)) {
                                state.style.counter_size
                            } else {
                                state.style.stamp_size
                            };
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
        Some(Drag::Resize {
            side,
            from,
            shape,
            before,
        }) => {
            let by = [p[0] - from[0], p[1] - from[1]];
            let (scale, fixed) = stretch(*side, *shape, by, canvas.shift);
            for (i, a) in before {
                if let Some(slot) = annotations.get_mut(*i) {
                    *slot = resized(a, mapping, fixed, scale);
                }
            }
            let indices: Vec<usize> = before.iter().map(|(i, _)| *i).collect();
            keep_in_place(annotations, &indices, mapping, *side, fixed, scale);
            let mirrored = |axis: usize| {
                if scale[axis] < 0. {
                    -side[axis]
                } else {
                    side[axis]
                }
            };
            dragged.side = Some([mirrored(0), mirrored(1)]);
        }
        Some(Drag::Rotate {
            pivot,
            from,
            base,
            before,
        }) => {
            let mut shown = wrap(base + angle_of(p, *pivot) - from);
            if canvas.shift {
                shown = wrap((shown / 15.).round() * 15.);
            }
            let turn = wrap(shown - base).round() as i32;
            for (i, a) in before {
                if let Some(slot) = annotations.get_mut(*i) {
                    *slot = rotated(a, mapping, *pivot, turn);
                }
            }
            let shown = wrap(base + turn as f32).round() as i32;
            dragged.angle = Some(if canvas.shift {
                format!("{shown}° (snapped)")
            } else {
                format!("{shown}°")
            });
        }
        Some(Drag::Button(_)) | None => {}
    }
    dragged
}

/// The cursor for what is dragged, or for what a click where the mouse is would do, and a thin
/// box around what it would select
fn point(
    canvas: &Canvas,
    annotations: &[Annotation],
    state: &AnnotateUi,
    selection: Option<&Selection>,
    dragged: &Dragged,
    mouse: Mouse,
) {
    let (pos, p) = (mouse.screen, mouse.image);
    // the handles and buttons of the box go before what is below them, a drag goes on over the
    // panels too
    let box_cursor = match &state.drag {
        Some(Drag::Resize { side, .. }) => Some(resize_cursor(dragged.side.unwrap_or(*side))),
        Some(Drag::Rotate { .. }) => Some(CursorIcon::Grabbing),
        Some(Drag::Button(_)) => Some(CursorIcon::PointingHand),
        None => selection.and_then(|s| s.target).map(Target::cursor),
        _ => None,
    };
    if let Some(cursor) = box_cursor {
        canvas.ctx.set_cursor_icon(cursor);
        return;
    }
    if canvas.pointer_over_ui {
        return;
    }
    let tool = canvas.tool;
    let pointed = canvas.hit(annotations, p).filter(|_| state.drag.is_none());
    let inside = tool == Tool::Select
        && !canvas.shift
        && state.drag.is_none()
        && selection.is_some_and(|s| s.handles.rect.contains(pos));
    canvas.ctx.set_cursor_icon(match pointed {
        Some(i) if tool == Tool::Text && is_text(&annotations[i]) => CursorIcon::Text,
        Some(_) => CursorIcon::Move,
        None if matches!(state.drag, Some(Drag::Move { .. })) => CursorIcon::Move,
        None if inside => CursorIcon::Move,
        None if tool == Tool::Select => CursorIcon::Default,
        None if tool == Tool::Text => CursorIcon::Text,
        None => CursorIcon::Crosshair,
    });
    if let Some(i) = pointed.filter(|i| !state.selected.contains(i)) {
        canvas.show_pointed(annotations, i);
    }
}

/// The end of a drag: what was drawn empty is removed, what was drawn, moved, resized or turned
/// keeps to the nearest anchor, a button acts when let go over it
fn release(
    canvas: &Canvas,
    annotations: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    selection: Option<&Selection>,
) {
    let (mapping, original) = (canvas.mapping, canvas.mapping.original);
    let Some(drag) = state.drag.take() else {
        return;
    };
    match drag {
        Drag::Draw { index, .. } => {
            if annotations.get(index).is_some_and(|a| a.is_empty()) {
                state.remove(annotations, vec![index]);
            } else if let Some(a) = annotations.get_mut(index) {
                a.reanchor(original);
                match a.shape {
                    Shape::Stamp(_) => state.style.stamp_size = a.width,
                    Shape::Counter(_) => state.style.counter_size = a.width,
                    _ => {}
                }
            }
        }
        Drag::Move { .. } => {
            for &i in &state.selected {
                annotations[i].reanchor(original);
            }
        }
        Drag::Resize { before, .. } | Drag::Rotate { before, .. } => {
            for (i, _) in &before {
                if let Some(a) = annotations.get_mut(*i) {
                    a.reanchor(original);
                }
            }
            // new stamps and counters are as large as one made larger
            if let [(i, _)] = before.as_slice()
                && let Some(a) = annotations.get(*i)
            {
                match a.shape {
                    Shape::Stamp(_) => state.style.stamp_size = a.width,
                    Shape::Counter(_) => state.style.counter_size = a.width,
                    _ => {}
                }
            }
        }
        Drag::Button(action) => {
            if selection.and_then(|s| s.target) == Some(Target::Button(action)) {
                let selected = state.selected.clone();
                act(annotations, &selected, mapping, action);
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
        && let Some(mouse) = canvas.mouse
        && let Some(i) = canvas
            .hit(annotations, mouse.image)
            .filter(|i| is_text(&annotations[*i]))
    {
        state.start_typing(i);
    }
}

/// The selection box after this frame, with its handles, and a thin box around the card under the mouse.
/// `target` is what the mouse was over at the frame's start.
fn show_selection(
    canvas: &Canvas,
    annotations: &[Annotation],
    state: &AnnotateUi,
    target: Option<Target>,
    dragged: &Dragged,
) {
    if let Some((_, rect)) = canvas.selected_box(annotations, state) {
        let idle = matches!(state.drag, None | Some(Drag::Button(_)));
        let handles = Handles::new(rect, state.rotate_corner.filter(|_| idle), canvas.view);
        let hot = match &state.drag {
            Some(Drag::Resize { side, .. }) => Some(Target::Handle(dragged.side.unwrap_or(*side))),
            Some(Drag::Button(action)) => Some(Target::Button(*action)),
            None => target,
            _ => None,
        };
        handles.paint(canvas.ctx, &mut canvas.shapes.borrow_mut(), hot, idle);
    }
    if let Some(i) = state.pointed_card.filter(|i| !state.selected.contains(i))
        && annotations.get(i).is_some_and(|a| !a.hidden)
    {
        canvas.show_pointed(annotations, i);
    }
}

/// The angle while what is selected is turned, or what a handle or a button does once the mouse rests on it
fn show_tooltip(canvas: &Canvas, state: &AnnotateUi, angle: Option<String>) {
    let Some(pos) = canvas.mouse.map(|mouse| mouse.screen) else {
        return;
    };
    if let Some(angle) = angle {
        tooltip(canvas.ctx, pos, &angle);
    } else if let Some((target, since)) = state.hovered {
        let waited = canvas.ctx.input(|i| i.time) - since;
        if waited >= TOOLTIP_DELAY {
            tooltip(canvas.ctx, pos, target.hint());
        } else {
            canvas
                .ctx
                .request_repaint_after(std::time::Duration::from_secs_f64(TOOLTIP_DELAY - waited));
        }
    }
}
