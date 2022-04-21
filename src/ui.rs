use egui::plot::{Line, Plot, Value, Values};
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

pub fn advanced_ui(ui: &mut Ui, state: &mut OculanteState) {
    if let Some(info) = &state.image_info {
        ui.label(format!("Number of colors: {}", info.num_colors));
        ui.label(format!(
            "Fully transparent: {:.2}%",
            (info.num_transparent_pixels as f32 / info.num_pixels as f32) * 100.
        ));
        ui.label(format!("Pixels: {}", info.num_pixels));

        let hist_vals = info
            .histogram
            .iter()
            .map(|(k, v)| Value::new(*k as f64, *v as f64));
        let line = Line::new(Values::from_values_iter(hist_vals));
        ui.label("Histogram");
        Plot::new("my_plot")
            // .data_aspect(2.0)
            .show(ui, |plot_ui| plot_ui.line(line));
    }
}

pub fn tooltip(r: Response, tooltip: &str, hotkey: &str, ui: &mut Ui) -> Response {
    r.on_hover_ui(|ui| {
        ui.horizontal(|ui| {
            ui.label(tooltip);
            ui.label(
                RichText::new(hotkey)
                    .monospace()
                    .color(Color32::WHITE)
                    .background_color(ui.style().visuals.selection.bg_fill),
            );
        });
    })
}

pub fn unframed_button(text: impl Into<WidgetText>, ui: &mut Ui) -> Response {
    ui.add(egui::Button::new(text).frame(false))
}
