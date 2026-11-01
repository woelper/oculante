use super::*;

/// The box to type a text in, returns whether typing ended.
pub(super) fn text_ui(
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
    let color = [r, g, b, alpha.max(80)].to_color32();
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
                Stroke::new(1., opacity(WHITE, Opacity::Half)),
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
