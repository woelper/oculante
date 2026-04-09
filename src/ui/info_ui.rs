use crate::appstate::OculanteState;
use crate::comparelist::CompareItem;
#[cfg(feature = "file_open")]
use crate::filebrowser::browse_for_image_path;
use crate::filebrowser::BrowserDir;
use crate::icons::*;
use crate::utils::*;
use egui_plot::{Line, Plot, PlotPoints};
use image::ColorType;

#[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
use egui::{self, *};

use super::*;
use std::time::Duration;

pub fn info_ui(
    ctx: &Context,
    state: &mut OculanteState,
    image_tiles: &[crate::app::ImageTile],
) -> (Pos2, Pos2) {
    let mut color_type = ColorType::Rgba8;
    let mut bbox_tl: Pos2 = Default::default();
    let mut bbox_br: Pos2 = Default::default();
    let mut uv_center: (f64, f64) = Default::default();
    let mut uv_size: (f64, f64) = Default::default();

    if let Some(img) = &state.current_image {
        color_type = img.color();

        // prefer edit result if present
        let img = if state.edit_state.result_pixel_op.width() > 0 {
            &state.edit_state.result_pixel_op
        } else {
            img
        };

        // don't do this every frame for performance reasons
        if ctx.cumulative_pass_nr() % 5 == 0 {
            if let Some(p) = get_pixel_checked(
                img,
                state.cursor_relative.x as u32,
                state.cursor_relative.y as u32,
            ) {
                state.sampled_color = [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32];
            }
        }
    }

    egui::SidePanel::left("info")
    .show_separator_line(false)
    .exact_width(PANEL_WIDTH)
    .resizable(false)
    .frame(egui::Frame::side_top_panel(&ctx.style()).corner_radius(0))
    .show(ctx, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false,true])
            .show(ui, |ui| {

            // Force-expand to prevent spacing issue with scroll bar
            // ui.allocate_space(egui::Vec2::new(1000., 0.));

            if state.current_image.is_some() {
                let img_w = state.image_geometry.dimensions.0 as f64;
                let img_h = state.image_geometry.dimensions.1 as f64;
                let desired_width = PANEL_WIDTH as f64 - PANEL_WIDGET_OFFSET as f64 - 20.;
                let scale = (desired_width / 8.) / img_w.max(1.0);
                uv_center = (
                    state.cursor_relative.x as f64 / state.image_geometry.dimensions.0 as f64,
                    (state.cursor_relative.y as f64 / state.image_geometry.dimensions.1 as f64),
                );

                egui::Grid::new("info")
                    .num_columns(2)
                    .show(ui, |ui| {
                    ui.label_i(format!("{ARROWS_OUT} Size",));
                    ui.label_right(
                        RichText::new(format!(
                            "{}x{}",
                            state.image_geometry.dimensions.0, state.image_geometry.dimensions.1
                        ))
                    );
                    ui.end_row();

                    if let Some(path) = &state.current_path {
                        // make sure we truncate filenames
                        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                        ui.label_i(format!("{} File", IMAGE));
                        let path_label = egui::Label::new(
                            RichText::new(file_name)
                        ).truncate();
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            ui.add(path_label)
                            .on_hover_text(format!("{}", path.display()));
                        });
                        ui.end_row();
                    }

                    ui.label_i(format!("{PALETTE} RGBA"));
                    ui.label_right(
                        RichText::new(disp_col(state.sampled_color))
                    );
                    ui.end_row();

                    ui.label_i(format!("{PALETTE} RGBA"));
                    ui.label_right(
                        RichText::new(disp_col_norm(state.sampled_color, 255.))
                    );
                    ui.end_row();

                    ui.label_i(format!("{PALETTE} HEX"));
                    let hex = Color32::from_rgba_unmultiplied(state.sampled_color[0] as u8, state.sampled_color[1] as u8, state.sampled_color[2] as u8, state.sampled_color[3] as u8).to_hex();
                    ui.label_right(
                        RichText::new(hex)
                    );
                    ui.end_row();

                    ui.label_i(format!("{PALETTE} Color"));
                    ui.label_right(
                        format!("{:?}", color_type)
                    );
                    ui.end_row();

                    ui.label_i(format!("{MOVE} Pos"));
                    ui.label_right(
                        RichText::new(format!(
                            "{:.0},{:.0}",
                            state.cursor_relative.x.floor(), state.cursor_relative.y.floor()
                        ))
                    );
                    ui.end_row();

                    ui.label_i(format!("{INTERSECT} UV"));
                    ui.label_right(
                        RichText::new(format!("{:.3},{:.3}", uv_center.0, 1.0 - uv_center.1))
                    );
                    ui.end_row();
                });

                ui.add_space(10.);
                let preview_size = desired_width as f32;
                let preview_rect = egui::Rect::from_min_size(
                    ui.cursor().left_top(),
                    egui::Vec2::splat(preview_size),
                );
                bbox_tl = preview_rect.left_top();
                bbox_br = preview_rect.right_bottom();

                // Draw magnified pixel preview
                zoom_preview(ui, state, preview_rect, image_tiles);

                ui.advance_cursor_after_rect(preview_rect);
            }
            ui.add_space(10.);
            ui.vertical_centered_justified(|ui| {
                ui.styled_collapsing("Compare", |ui| {

                    if state.persistent_settings.max_cache == 0 {
                        ui.label("Warning! Set your cache to more than 0 in settings for this to be fast.");
                    }
                    ui.vertical_centered_justified(|ui| {
                        dark_panel(ui, |ui| {
                            let browser_button = ui.button(format!("{FOLDER} Open another image..."));
                            if browser_button.clicked() {
                                state.filebrowser_last_dir = if ui.ctx().input(|i| i.modifiers.shift) {
                                    BrowserDir::CurrentImageDir
                                } else {
                                    BrowserDir::LastOpenDir
                                };

                                // TODO: Automatically insert image into compare list
                                #[cfg(feature = "file_open")]
                                browse_for_image_path(state);
                                #[cfg(not(feature = "file_open"))]
                                {
                                    use crate::filebrowser::BrowserState;


                                    let path_override = state.filebrowser_path();
                                    BrowserState::check_refresh_entries(
                                        ui,
                                        state.filebrowser_last_dir,
                                        Some(&path_override),
                                    );
                                    ui.ctx()
                                        .data_mut(|w| w.insert_temp(Id::new("FBPATH"), path_override));
                                    ui.ctx().memory_mut(|w| w.open_popup(Id::new("OPEN")));
                                }

                                state.is_loaded = false;
                                // tag to add new image
                                ui.ctx().data_mut(|w|w.insert_temp("compare".into(), true));
                            }

                            if ui.ctx().data(|r|r.get_temp::<bool>("compare".into())).is_some()
                                && state.is_loaded && !state.reset_image {
                                    if let Some(path) = &state.current_path {
                                        state.compare_list.insert(CompareItem::new(path, state.image_geometry));
                                        ui.ctx().data_mut(|w|w.remove_temp::<bool>("compare".into()));
                                    }
                                }

                            // let compare_list = state.compare_list.iter().cloned().collect();
                            let mut to_remove = None;
                            for CompareItem {path, geometry} in state.compare_list.iter() {
                                ui.horizontal(|ui|{
                                    if ui.button(X).clicked() {
                                        to_remove = Some(path.to_owned());
                                    }
                                    ui.vertical_centered_justified(|ui| {
                                        if ui.selectable_label(state.current_path.as_ref() == Some(path), path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default().to_string()).clicked(){
                                            state
                                                .player
                                                .load_advanced(path, Some(crate::utils::Frame::CompareResult(Default::default(), *geometry)));
                                            ui.ctx().request_repaint();
                                            ui.ctx().request_repaint_after(Duration::from_millis(500));
                                            state.current_path = Some(path.clone());
                                        }
                                    });
                                });
                            }
                            if let Some(remove) = to_remove {
                                state.compare_list.remove(remove);
                            }
                            if let Some(path) = &state.current_path {
                                if let Some(geo) = state.compare_list.get(path) {
                                    if state.image_geometry != geo
                                        && ui.button(RichText::new(format!("{LOCATION_PIN} Update position")).color(Color32::YELLOW)).clicked() {
                                                state.compare_list.insert(CompareItem::new(path, state.image_geometry));
                                        }
                                    } else if ui.button(format!("{PLUS} Add current image")).clicked() {
                                        state.compare_list.insert(CompareItem::new(path, state.image_geometry));
                                    }
                            }
                            if !state.compare_list.is_empty()
                                    && ui.button(format!("{TRASH} Clear all")).clicked() {
                                        state.compare_list.clear();
                            }
                        });
                    });
                });
            });

            if state.current_image.is_some() {
                ui.styled_collapsing("Alpha tools", |ui| {
                    ui.vertical_centered_justified(|ui| {
                        dark_panel(ui, |ui| {
                            if let Some(img) = &state.current_image {
                                if ui
                                    .button("Show alpha bleed")
                                    .on_hover_text("Highlight pixels with zero alpha and color information")
                                    .clicked()
                                {
                                    state.edit_state.result_pixel_op = highlight_bleed(img);
                                    state.send_frame(crate::utils::Frame::UpdateTexture);
                                    ui.ctx().request_repaint();
                                }
                                if ui
                                    .button("Show semi-transparent pixels")
                                    .on_hover_text(
                                        "Highlight pixels that are neither fully opaque nor fully transparent",
                                    )
                                    .clicked()
                                {
                                    state.edit_state.result_pixel_op = highlight_semitrans(img);
                                    state.send_frame(crate::utils::Frame::UpdateTexture);
                                    ui.ctx().request_repaint();
                                }
                                if ui.button("Reset image").clicked() {
                                    state.edit_state.result_pixel_op = Default::default();

                                    state.send_frame(crate::utils::Frame::UpdateTexture);
                                }
                            }
                        });
                    });
                });

                palette_ui(ui, state);

                if state.persistent_settings.experimental_features {
                    measure_ui(ui, state);
                }

                ui.horizontal(|ui| {
                    ui.label("Tiling");
                    ui.style_mut().spacing.slider_width = ui.available_width() - 16.;
                    ui.styled_slider(&mut state.tiling, 1..=10);
                });
            }

            advanced_ui(ui, state);

        });
    });
    (bbox_tl, bbox_br)
}

fn advanced_ui(ui: &mut Ui, state: &mut OculanteState) {
    if let Some(info) = &state.image_metadata {
        egui::Grid::new("extended").num_columns(2).show(ui, |ui| {
            ui.label("Number of colors");
            ui.label_right(format!("{}", info.num_colors));
            ui.end_row();

            ui.label("Fully transparent");
            ui.label_right(format!(
                "{:.2}%",
                (info.num_transparent_pixels as f32 / info.num_pixels as f32) * 100.
            ));
            ui.end_row();
            ui.label("Pixels");
            ui.label_right(format!("{}", info.num_pixels));
            ui.end_row();
        });

        if !info.exif.is_empty() {
            ui.styled_collapsing("EXIF", |ui| {
                dark_panel(ui, |ui| {
                    for (key, val) in &info.exif {
                        ui.scope(|ui| {
                            ui.style_mut().override_font_id =
                                Some(FontId::new(14., FontFamily::Name("bold".into())));
                            ui.colored_label(
                                if ui.style().visuals.dark_mode {
                                    Color32::from_gray(200)
                                } else {
                                    Color32::from_gray(20)
                                },
                                key,
                            );
                        });
                        ui.label(val);
                        ui.separator();
                    }
                });
            });
        }

        if let Some(dicom) = &info.dicom {
            ui.styled_collapsing("DICOM", |ui| {
                dark_panel(ui, |ui| {
                    for (key, val) in &dicom.dicom_data {
                        ui.scope(|ui| {
                            ui.style_mut().override_font_id =
                                Some(FontId::new(14., FontFamily::Name("bold".into())));
                            ui.colored_label(
                                if ui.style().visuals.dark_mode {
                                    Color32::from_gray(200)
                                } else {
                                    Color32::from_gray(20)
                                },
                                key,
                            );
                        });
                        ui.label(val);
                        ui.separator();
                    }
                });
            });
        }

        let red_vals = Line::new(
            "red",
            info.red_histogram
                .iter()
                .map(|(k, v)| [*k as f64, *v as f64])
                .collect::<PlotPoints>(),
        )
        .fill(0.)
        .color(Color32::RED);

        let green_vals = Line::new(
            "green",
            info.green_histogram
                .iter()
                .map(|(k, v)| [*k as f64, *v as f64])
                .collect::<PlotPoints>(),
        )
        .fill(0.)
        .color(Color32::GREEN);

        let blue_vals = Line::new(
            "blue",
            info.blue_histogram
                .iter()
                .map(|(k, v)| [*k as f64, *v as f64])
                .collect::<PlotPoints>(),
        )
        .fill(0.)
        .color(Color32::BLUE);

        Plot::new("histogram")
            .allow_zoom(false)
            .allow_drag(false)
            .show_axes(false)
            .show_grid(false)
            .width(PANEL_WIDTH - PANEL_WIDGET_OFFSET)
            .show(ui, |plot_ui| {
                plot_ui.line(red_vals);
                plot_ui.line(green_vals);
                plot_ui.line(blue_vals);
            });
    }
}

/// Renders a magnified, pixel-perfect view by reusing existing image tiles.
/// No image conversion or texture upload — just draws UV-cropped quads from
/// the tiles already on the GPU.
fn zoom_preview(
    ui: &mut Ui,
    state: &OculanteState,
    rect: egui::Rect,
    tiles: &[crate::app::ImageTile],
) {
    if tiles.is_empty() {
        return;
    }

    let img_w = state.image_geometry.dimensions.0 as f32;
    let img_h = state.image_geometry.dimensions.1 as f32;
    if img_w < 1.0 || img_h < 1.0 {
        return;
    }

    // Snap to integer pixel coordinates so the preview locks to pixel grid
    let cx = state.cursor_relative.x.floor();
    let cy = state.cursor_relative.y.floor();

    // How many source pixels the preview spans in each direction
    let radius: f32 = 16.0;
    let src_size = radius * 2.0 + 1.0;

    // Source region in image pixel coordinates
    let src_left = cx - radius;
    let src_top = cy - radius;
    let src_right = cx + radius + 1.0;
    let src_bottom = cy + radius + 1.0;

    // How many screen pixels per source pixel
    let px_per_src = rect.width() / src_size;

    // Fill background for out-of-bounds areas
    ui.painter()
        .rect_filled(rect, 0.0, Color32::from_gray(30));

    // Clip to the preview rect
    let clip = rect;

    // Draw each tile that overlaps the source region
    for tile in tiles {
        let tx = tile.x as f32;
        let ty = tile.y as f32;
        let tw = tile.w as f32;
        let th = tile.h as f32;
        let tile_right = tx + tw;
        let tile_bottom = ty + th;

        // Check overlap between source region and this tile
        let overlap_left = src_left.max(tx);
        let overlap_top = src_top.max(ty);
        let overlap_right = src_right.min(tile_right);
        let overlap_bottom = src_bottom.min(tile_bottom);

        if overlap_left >= overlap_right || overlap_top >= overlap_bottom {
            continue; // No overlap
        }

        // UV coordinates within this tile for the overlapping region
        let uv_left = (overlap_left - tx) / tw;
        let uv_top = (overlap_top - ty) / th;
        let uv_right = (overlap_right - tx) / tw;
        let uv_bottom = (overlap_bottom - ty) / th;
        let uv = egui::Rect::from_min_max(
            egui::pos2(uv_left, uv_top),
            egui::pos2(uv_right, uv_bottom),
        );

        // Screen position for this overlap region within the preview rect
        let screen_left = rect.left() + (overlap_left - src_left) * px_per_src;
        let screen_top = rect.top() + (overlap_top - src_top) * px_per_src;
        let screen_right = rect.left() + (overlap_right - src_left) * px_per_src;
        let screen_bottom = rect.top() + (overlap_bottom - src_top) * px_per_src;
        let draw_rect = egui::Rect::from_min_max(
            egui::pos2(screen_left, screen_top),
            egui::pos2(screen_right, screen_bottom),
        );

        ui.painter().with_clip_rect(clip).image(
            tile.texture.id(),
            draw_rect,
            uv,
            Color32::WHITE,
        );
    }

    // Crosshair
    let center_x = rect.left() + radius * px_per_src;
    let center_y = rect.top() + radius * px_per_src;
    let stroke = egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(128, 128, 128, 128));
    ui.painter().line_segment(
        [
            egui::pos2(center_x + px_per_src / 2.0, rect.top()),
            egui::pos2(center_x + px_per_src / 2.0, rect.bottom()),
        ],
        stroke,
    );
    ui.painter().line_segment(
        [
            egui::pos2(rect.left(), center_y + px_per_src / 2.0),
            egui::pos2(rect.right(), center_y + px_per_src / 2.0),
        ],
        stroke,
    );
    let center_rect = egui::Rect::from_min_size(
        egui::pos2(center_x, center_y),
        egui::vec2(px_per_src, px_per_src),
    );
    ui.painter().rect_stroke(
        center_rect,
        0.0,
        egui::Stroke::new(1.5, Color32::from_gray(40)),
        egui::StrokeKind::Middle,
    );
}
