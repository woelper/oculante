use notan::egui::{self, *};


use crate::{utils::OculanteState, update};



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
                update::update(None);
            }

            });
    }

}
