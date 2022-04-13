use notan::egui::{self, *};

use crate::{update, utils::OculanteState};

pub fn settings_ui(ctx: &Context, state: &mut OculanteState) {
    if state.settings_enabled {
        egui::Window::new("Settings")
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .default_width(400.)
            // .title_bar(false)
            .show(&ctx, |ui| {
                if ui.button("Check for updates").clicked() {
                    state.message = Some("Checking for updates...".into());
                    update::update(Some(state.message_channel.0.clone()));
                    state.settings_enabled = false;

                }

                if ui.button("Close").clicked() {
                    state.settings_enabled = false;
                }
            });
    }
}


pub fn tooltip(r: Response, tooltip: &str, hotkey: &str, ui: &mut Ui, ) -> Response {
    r.on_hover_ui(|ui| {
        ui.horizontal(|ui| {
            ui.label(tooltip);
            ui.label(
                RichText::new(hotkey)
                    .monospace()
                    .color(Color32::WHITE)
                    .background_color(
                        ui.style().visuals.selection.bg_fill,
                    ),
            );
        });
    })
}


pub fn unframed_button(text: impl Into<WidgetText>, ui: &mut Ui, ) -> Response {
    ui.add(egui::Button::new(text).frame(false))
}