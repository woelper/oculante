use std::sync::Arc;

use egui::text::{CCursor, CCursorRange};

use super::*;

/// Typing a text. The text is drawn into the image as it will look, and the keys go to a hidden
/// text box. The caret, selection and frame are drawn over the real letters, so they line up
/// even when the text is turned or in another font. Returns whether typing ended, and the
/// shapes to draw over the image.
pub(super) fn text_ui(
    ctx: &egui::Context,
    annotations: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    mapping: &Mapping,
    geo: &ImageGeometry,
    pointer_over_ui: bool,
) -> (bool, Vec<egui::Shape>) {
    let Some(i) = state.typing else {
        state.text_drag = None;
        state.over_text = false;
        return (false, vec![]);
    };
    let Some(a) = annotations.get_mut(i) else {
        state.typing = None;
        return (false, vec![]);
    };
    let Shape::Text(before) = &a.shape else {
        return (false, vec![]);
    };
    let lines = Lines::new(before, a, mapping, geo);

    // clicking the text moves the caret, dragging selects
    let (pressed, down, released, pointer) = ctx.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.pointer.hover_pos(),
        )
    });
    state.over_text = !pointer_over_ui && pointer.is_some_and(|p| lines.contains(p));
    let mut placed = None;
    let pressed_on_text = pressed && state.over_text;
    // egui drops focus on a press or release outside its box, so take it back when the click
    // was on our text
    let on_text = pressed_on_text || (released && state.text_drag.is_some());
    if pressed_on_text && let Some(p) = pointer {
        let at = lines.index_at(p);
        state.text_drag = Some(at);
        placed = Some((at, at));
    } else if down
        && let Some(anchor) = state.text_drag
        && let Some(p) = pointer
    {
        placed = Some((anchor, lines.index_at(p)));
    } else if !down {
        state.text_drag = None;
    }

    let focus = std::mem::take(&mut state.focus_text);
    let Shape::Text(text) = &mut a.shape else {
        return (false, vec![]);
    };
    let area = Id::new("annotation_text");
    // the area already uses `area`, the text box needs its own id
    let id = area.with("edit");
    let mut done = false;
    let mut focused = false;
    // Off screen: the text shows in the image, and clicks are handled above
    egui::Area::new(area)
        .fixed_pos(pos2(-100_000., -100_000.))
        .order(egui::Order::Foreground)
        .constrain(false)
        .show(ctx, |ui| {
            // no wrapping, to match the drawn text
            let mut unwrapped = |ui: &Ui, text: &dyn egui::TextBuffer, _wrap: f32| {
                ui.painter().layout_job(egui::text::LayoutJob::simple(
                    text.as_str().to_owned(),
                    egui::FontId::proportional(14.),
                    TRANSPARENT,
                    f32::INFINITY,
                ))
            };
            let edit = egui::TextEdit::multiline(text)
                .id(id)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .desired_rows(1)
                .layouter(&mut unwrapped)
                // Enter ends the typing, with Shift a new line starts
                .return_key(egui::KeyboardShortcut::new(
                    egui::Modifiers::SHIFT,
                    Key::Enter,
                ));
            let response = ui.add(edit);
            // a click on the text keeps typing
            if focus || on_text {
                response.request_focus();
            }
            let enter = response.has_focus()
                && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);
            if enter || (response.lost_focus() && !focus && !on_text) {
                done = true;
            }
            focused = response.has_focus();
        });

    // a click overrides the text box's own cursor
    let mut edit_state = egui::TextEdit::load_state(ctx, id);
    if let Some((anchor, at)) = placed
        && let Some(edit_state) = &mut edit_state
    {
        edit_state.cursor.set_char_range(Some(CCursorRange {
            primary: CCursor::new(at),
            secondary: CCursor::new(anchor),
            h_pos: None,
        }));
        egui::TextEdit::store_state(ctx, id, edit_state.clone());
    }
    let range = placed.or_else(|| {
        let range = edit_state?.cursor.char_range()?;
        Some((range.secondary.index.0, range.primary.index.0))
    });

    let mut shapes = vec![];
    if done {
        state.text_drag = None;
        state.finish_typing(annotations);
        return (true, shapes);
    }
    // the text after this frame's typing
    if let Some(a) = annotations.get(i)
        && let Shape::Text(text) = &a.shape
    {
        let lines = Lines::new(text, a, mapping, geo);
        let accent = accent_color(ctx);
        if let Some((anchor, at)) = range {
            lines.show_selection(&mut shapes, anchor, at, opacity(accent, Opacity::Faint));
        }
        lines.show_frame(&mut shapes);
        if focused && let Some((_, at)) = range {
            let caret = lines.show_caret(&mut shapes, at);
            // so an input method's popup appears by the caret
            ctx.output_mut(|o| {
                if let Some(ime) = &mut o.ime {
                    ime.rect = lines.screen_box();
                    ime.cursor_rect = caret;
                }
            });
        }
    }
    (false, shapes)
}

/// The lines of a text being typed, as drawn on screen
struct Lines<'a> {
    text: &'a str,
    /// See `line_positions`
    positions: Vec<Arc<Vec<f32>>>,
    /// From the text's own coordinates to the screen
    screen: Transform,
    size: f32,
    /// 3 screen pixels, in the text's own coordinates
    pad: f32,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str, a: &Annotation, mapping: &Mapping, geo: &ImageGeometry) -> Self {
        let screen = a
            .transform(mapping)
            .post_scale(geo.scale, geo.scale)
            .post_translate(geo.offset.x, geo.offset.y);
        Self {
            text,
            positions: text.split('\n').map(|l| line_positions(l, a)).collect(),
            screen,
            size: a.width.max(1) as f32,
            pad: 3. / scale_of(screen).max(1e-6),
        }
    }

    /// Top and bottom of a line's letters, in the text's own coordinates
    fn line_span(&self, line: usize) -> (f32, f32) {
        let top = line as f32 * self.size * LINE_HEIGHT;
        (top + self.size * 0.05, top + self.size * 1.25)
    }

    fn line_of(&self, index: usize) -> (usize, usize) {
        let mut rest = index;
        for (line, text) in self.text.split('\n').enumerate() {
            let count = text.chars().count();
            if rest <= count {
                return (line, rest);
            }
            rest -= count + 1;
        }
        let last = self.positions.len().saturating_sub(1);
        (last, self.positions[last].len().saturating_sub(1))
    }

    fn x_at(&self, line: usize, column: usize) -> f32 {
        let positions = &self.positions[line];
        positions[column.min(positions.len() - 1)]
    }

    fn to_screen(&self, x: f32, y: f32) -> Pos2 {
        let mut p = Point::from_xy(x, y);
        self.screen.map_point(&mut p);
        pos2(p.x, p.y)
    }

    fn to_own(&self, p: Pos2) -> Option<Point> {
        let mut q = Point::from_xy(p.x, p.y);
        self.screen.invert()?.map_point(&mut q);
        Some(q)
    }

    /// The frame around the text in its own coordinates: left, top, right and bottom
    fn frame(&self) -> [f32; 4] {
        let widest = self
            .positions
            .iter()
            .filter_map(|p| p.last().copied())
            .fold(self.size * 0.5, f32::max);
        let (top, _) = self.line_span(0);
        let (_, bottom) = self.line_span(self.positions.len() - 1);
        [
            -self.pad,
            top - self.pad,
            widest + self.pad,
            bottom + self.pad,
        ]
    }

    fn contains(&self, p: Pos2) -> bool {
        let f = self.frame();
        self.to_own(p)
            .is_some_and(|q| q.x >= f[0] && q.x <= f[2] && q.y >= f[1] && q.y <= f[3])
    }

    /// The character index nearest to a screen point
    fn index_at(&self, p: Pos2) -> usize {
        let Some(q) = self.to_own(p) else {
            return 0;
        };
        let lines = self.positions.len();
        let line = ((q.y / (self.size * LINE_HEIGHT)).floor().max(0.) as usize).min(lines - 1);
        let column = self.positions[line]
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (*a - q.x).abs().total_cmp(&(*b - q.x).abs()))
            .map_or(0, |(column, _)| column);
        let before: usize = self.positions[..line].iter().map(|p| p.len()).sum();
        before + column
    }

    /// A box in the text's own coordinates, as corners on screen
    fn quad(&self, [left, top, right, bottom]: [f32; 4]) -> Vec<Pos2> {
        vec![
            self.to_screen(left, top),
            self.to_screen(right, top),
            self.to_screen(right, bottom),
            self.to_screen(left, bottom),
        ]
    }

    fn screen_box(&self) -> egui::Rect {
        egui::Rect::from_points(&self.quad(self.frame()))
    }

    fn show_frame(&self, shapes: &mut Vec<egui::Shape>) {
        shapes.push(egui::Shape::closed_line(
            self.quad(self.frame()),
            Stroke::new(1., opacity(WHITE, Opacity::Half)),
        ));
    }

    /// Draws the caret before character `index` and returns where it is on screen
    fn show_caret(&self, shapes: &mut Vec<egui::Shape>, index: usize) -> egui::Rect {
        let (line, column) = self.line_of(index);
        let x = self.x_at(line, column);
        let (top, bottom) = self.line_span(line);
        let ends = [self.to_screen(x, top), self.to_screen(x, bottom)];
        // a dark edge so it shows on light images too
        shapes.push(egui::Shape::line_segment(
            ends,
            Stroke::new(3., opacity(BLACK, Opacity::Half)),
        ));
        shapes.push(egui::Shape::line_segment(ends, Stroke::new(1.5, WHITE)));
        egui::Rect::from_two_pos(ends[0], ends[1])
    }

    fn show_selection(
        &self,
        shapes: &mut Vec<egui::Shape>,
        anchor: usize,
        at: usize,
        color: Color32,
    ) {
        if anchor == at {
            return;
        }
        let (from, to) = (self.line_of(anchor.min(at)), self.line_of(anchor.max(at)));
        for line in from.0..=to.0 {
            let start = if line == from.0 { from.1 } else { 0 };
            let left = self.x_at(line, start);
            let mut right = if line == to.0 {
                self.x_at(line, to.1)
            } else {
                // a selected line break gets a little extra width
                self.x_at(line, usize::MAX) + self.size * 0.3
            };
            right = right.max(left);
            let (top, bottom) = self.line_span(line);
            shapes.push(egui::Shape::convex_polygon(
                self.quad([left, top, right, bottom]),
                color,
                Stroke::NONE,
            ));
        }
    }
}
