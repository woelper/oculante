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

        let grey_vals = Line::new(Values::from_values_iter(
            info.grey_histogram
                .iter()
                .map(|(k, v)| Value::new(*k as f64, *v as f64)),
        ))
        .color(Color32::GRAY);

        let red_vals = Line::new(Values::from_values_iter(
            info.red_histogram
                .iter()
                .map(|(k, v)| Value::new(*k as f64, *v as f64)),
        ))
        .fill(0.)
        .color(Color32::RED);

        let green_vals = Line::new(Values::from_values_iter(
            info.green_histogram
                .iter()
                .map(|(k, v)| Value::new(*k as f64, *v as f64)),
        ))
        .fill(0.)
        .color(Color32::GREEN);

        let blue_vals = Line::new(Values::from_values_iter(
            info.blue_histogram
                .iter()
                .map(|(k, v)| Value::new(*k as f64, *v as f64)),
        ))
        .fill(0.)
        .color(Color32::BLUE);

        ui.label("Histogram");
        Plot::new("my_plot")
            .allow_zoom(false)
            .allow_drag(false)
            .show(ui, |plot_ui| {
                plot_ui.line(grey_vals);
                plot_ui.line(red_vals);
                plot_ui.line(green_vals);
                plot_ui.line(blue_vals);
            });
    }
}

pub fn edit_advanced_ui(ui: &mut Ui, state: &mut OculanteState) {
//    ui.color_edit_button_rgb(rgb)
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
