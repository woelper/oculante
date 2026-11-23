//! Right click menu for selected annotations

use super::*;

/// The right-click menu for the selection, at `at` on screen. It acts on the whole selection.
pub(super) fn menu_ui(
    ctx: &egui::Context,
    annotations: &mut Vec<Annotation>,
    state: &mut AnnotateUi,
    mapping: &Mapping,
    at: Pos2,
) {
    let any_turned = state
        .shown(annotations)
        .iter()
        .any(|i| annotations.get(*i).is_some_and(|a| turned(a, mapping)));
    let (mut reset, mut delete) = (false, false);
    egui::Area::new(Id::new("annotation_menu"))
        .fixed_pos(at)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.visuals_mut().button_frame = false;
                ui.set_min_width(150.);
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    reset = ui
                        .add_enabled(any_turned, egui::Button::new("Reset rotation"))
                        .clicked();
                    delete = ui.button("Delete").clicked();
                });
            });
        });
    // hidden annotations are left as they are
    if reset {
        let shown = state.shown(annotations);
        reset_rotation(annotations, &shown, mapping);
        state.menu = None;
    }
    if delete {
        let shown = state.shown(annotations);
        state.remove(annotations, shown);
        state.menu = None;
    }
}

/// Turned as it is seen, by a degree at least
fn turned(a: &Annotation, mapping: &Mapping) -> bool {
    shown_angle(a, mapping).round() as i32 != 0
}

/// Each of `selected` upright as it is seen, turned around its own middle. A flip stays.
pub(super) fn reset_rotation(
    annotations: &mut [Annotation],
    selected: &[usize],
    mapping: &Mapping,
) {
    for &i in selected {
        let Some(a) = annotations.get(i).filter(|a| turned(a, mapping)) else {
            continue;
        };
        let turn = -(shown_angle(a, mapping).round() as i32);
        if let Some(middle) = middle_in(a, mapping) {
            let mut upright = rotated(a, mapping, middle, turn);
            upright.reanchor(mapping.original);
            annotations[i] = upright;
        }
    }
}
