use super::*;
use crate::appstate::OculanteState;
use crate::utils::*;
#[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
use notan::egui::*;
use quantette::{ColorSpace, PalettePipeline};

pub fn palette_ui(ui: &mut Ui, state: &mut OculanteState) {
    ui.styled_collapsing("Palette", |ui| {
        ui.vertical_centered_justified(|ui| {
            dark_panel(ui, |ui| {
                ui.allocate_space(vec2(ui.available_width(), 0.));
                if let Some(sampled_colors) = ui
                    .ctx()
                    .memory(|r| r.data.get_temp::<Vec<[u8; 4]>>("picker".into()))
                {
                    if !sampled_colors.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::splat(6.);
                            for color in &sampled_colors {
                                let (rect, resp) =
                                    ui.allocate_exact_size(Vec2::splat(32.), Sense::click());

                                let egui_color = Color32::from_rgba_premultiplied(
                                    (color[0]) as u8,
                                    (color[1]) as u8,
                                    (color[2]) as u8,
                                    (color[3]) as u8,
                                );

                                let sampled_color = [
                                    state.sampled_color[0] as u8,
                                    state.sampled_color[1] as u8,
                                    state.sampled_color[2] as u8,
                                    state.sampled_color[3] as u8,
                                ];

                                ui.painter().rect_filled(rect, 1., egui_color);
                                if color == &sampled_color {
                                    ui.painter().rect_stroke(
                                        rect,
                                        1.,
                                        Stroke::new(2., ui.style().visuals.selection.bg_fill),
                                    );
                                }
                                if resp.hovered() {
                                    if ui.ctx().input(|r| r.pointer.secondary_clicked()) {
                                        ui.ctx().memory_mut(|w| {
                                            let cols =
                                                w.data.get_temp_mut_or_default::<Vec<[u8; 4]>>(
                                                    "picker".into(),
                                                );
                                            if let Some(i) = cols.iter().position(|c| c == color) {
                                                cols.remove(i);
                                            }
                                        });
                                    }
                                    if ui.ctx().input(|r| r.pointer.primary_clicked()) {
                                        ui.ctx()
                                            .output_mut(|w| w.copied_text = egui_color.to_hex());
                                        state.send_message_info(&format!(
                                            "Copied color: {}",
                                            egui_color.to_hex()
                                        ));
                                    }
                                }
                                resp.on_hover_ui(|ui| {
                                    ui.label(format!("HEX: {}", egui_color.to_hex()));
                                    ui.label(format!(
                                        "RGBA: {}",
                                        disp_col([
                                            color[0] as f32,
                                            color[1] as f32,
                                            color[2] as f32,
                                            color[3] as f32,
                                        ])
                                    ));
                                    ui.label("Left click to copy Hex, right click to remove.");
                                });
                            }
                        });
                        if ui.button("Clear").clicked() {
                            ui.ctx().memory_mut(|w| {
                                w.data.remove_temp::<Vec<[u8; 4]>>("picker".into())
                            });
                        }
                        if ui.button("Sort").clicked() {
                            ui.ctx().memory_mut(|w| {
                                let cols = w
                                    .data
                                    .get_temp_mut_or_default::<Vec<[u8; 4]>>("picker".into());
                                cols.sort();
                            });
                        }
                        #[cfg(not(feature = "file_open"))]
                        if ui.button("Save ASE").clicked() {
                            ui.ctx().memory_mut(|w| w.open_popup(Id::new("SAVEASE")));
                        }

                        #[cfg(feature = "file_open")]
                        if ui.button(format!("Save ASE")).clicked() {
                            let start_directory =
                                state.volatile_settings.last_open_directory.clone();
                            std::thread::spawn(move || {
                                let file_dialog_result = rfd::FileDialog::new()
                                    .set_directory(start_directory)
                                    .save_file();
                                if let Some(p) = file_dialog_result {
                                    let swatches = sampled_colors
                                        .iter()
                                        .map(|c| ase_swatch::types::ObjectColor {
                                            name: "".into(),
                                            object_type: ase_swatch::types::ObjectColorType::Global,
                                            data: ase_swatch::types::Color {
                                                mode: ase_swatch::types::ColorMode::Rgb,
                                                values: [
                                                    c[0] as f32 / 255.,
                                                    c[1] as f32 / 255.,
                                                    c[2] as f32 / 255.,
                                                ]
                                                .to_vec(),
                                            },
                                        })
                                        .collect::<Vec<_>>();
                                    let s = ase_swatch::create_ase(&vec![], &swatches);
                                    if let Ok(mut f) = std::fs::File::create(p) {
                                        _ = std::io::Write::write_all(&mut f, &s);
                                    }
                                }
                            });
                            ui.ctx().request_repaint();
                        }

                        #[cfg(not(feature = "file_open"))]
                        if ui.ctx().memory(|w| w.is_popup_open(Id::new("SAVEASE"))) {
                            filebrowser::browse_modal(
                                true,
                                &["ase"],
                                &mut state.volatile_settings,
                                |p| {
                                    let swatches = sampled_colors
                                        .iter()
                                        .map(|c| ase_swatch::types::ObjectColor {
                                            name: "".into(),
                                            object_type: ase_swatch::types::ObjectColorType::Global,
                                            data: ase_swatch::types::Color {
                                                mode: ase_swatch::types::ColorMode::Rgb,
                                                values: [
                                                    c[0] as f32 / 255.,
                                                    c[1] as f32 / 255.,
                                                    c[2] as f32 / 255.,
                                                ]
                                                .to_vec(),
                                            },
                                        })
                                        .collect::<Vec<_>>();

                                    let s = ase_swatch::create_ase(&vec![], &swatches);
                                    if let Ok(mut f) = std::fs::File::create(p) {
                                        _ = std::io::Write::write_all(&mut f, &s);
                                    }
                                },
                                ui.ctx(),
                            );
                        }
                    }
                } else {
                    ui.label("Right click to sample color");
                }
                if let Some(img) = &state.current_image {
                    if ui.button("From image").clicked() {
                        ui.ctx()
                            .memory_mut(|w| w.data.remove_temp::<Vec<[u8; 4]>>("picker".into()));

                        if let Ok(mut pipeline) =
                            PalettePipeline::try_from(&img.clone().into_rgb8())
                        {
                            let palette = pipeline
                                .palette_size(32)
                                .colorspace(ColorSpace::Oklab)
                                .quantize_method(quantette::KmeansOptions::new())
                                .palette_par();

                            for col in palette {
                                ui.ctx().memory_mut(|w| {
                                    let cols = w
                                        .data
                                        .get_temp_mut_or_default::<Vec<[u8; 4]>>("picker".into());
                                    cols.push([col.red, col.green, col.blue, 255]);
                                });
                            }
                        }
                    }
                }
                if ui.ctx().input(|r| r.pointer.secondary_clicked()) {
                    if !state.pointer_over_ui {
                        ui.ctx().memory_mut(|w| {
                            let cols = w
                                .data
                                .get_temp_mut_or_default::<Vec<[u8; 4]>>("picker".into());

                            let sampled_color = [
                                state.sampled_color[0] as u8,
                                state.sampled_color[1] as u8,
                                state.sampled_color[2] as u8,
                                state.sampled_color[3] as u8,
                            ];

                            if !cols.contains(&sampled_color) {
                                cols.push(sampled_color);
                            } else {
                                state.send_message_info("Color already in palette");
                            }
                        });
                    }
                }
            });
        });
    });
}
