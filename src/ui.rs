use egui::plot::{Line, Plot, Value, Values};
use image::{Rgba, imageops::FilterType::Gaussian};
use notan::{
    egui::{self, plot::Points, *},
    prelude::Graphics,
};

use crate::{
    update,
    utils::{
        color_to_pixel, disp_col, disp_col_norm, highlight_bleed, highlight_semitrans, paint_at,
        send_extended_info, ExtendedImageInfo, ImageExt, OculanteState,
    },
};

pub fn info_ui(ctx: &Context, state: &mut OculanteState, gfx: &mut Graphics) {
    if state.info_enabled {
        if let Some(img) = &state.current_image {
            if let Some(p) = img.get_pixel_checked(
                state.cursor_relative.x as u32,
                state.cursor_relative.y as u32,
            ) {
                state.sampled_color = [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32];
            }
        }

        egui::SidePanel::left("side_panel").show(&ctx, |ui| {
            if let Some(texture) = &state.current_texture {
                // texture.
                let tex_id = gfx.egui_register_texture(&texture);

                // width of image widget
                let desired_width = ui.available_width();

                let scale = (desired_width / 8.) / texture.size().0;
                let img_size = egui::Vec2::new(desired_width, desired_width);

                let uv_center = (
                    state.cursor_relative.x / state.image_dimension.0 as f32,
                    (state.cursor_relative.y / state.image_dimension.1 as f32),
                );

                egui::Grid::new("info").show(ui, |ui| {
                    ui.label("Size");

                    ui.label(
                        RichText::new(format!(
                            "{}x{}",
                            state.image_dimension.0, state.image_dimension.1
                        ))
                        .monospace(),
                    );
                    ui.end_row();

                    if let Some(path) = &state.current_path {
                        ui.label("File");
                        ui.label(
                            RichText::new(format!(
                                "{}",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            ))
                            .monospace(),
                        )
                        .on_hover_text(format!("{}", path.display()));
                        ui.end_row();
                    }

                    ui.label("🌗 RGBA");
                    ui.label(
                        RichText::new(format!("{}", disp_col(state.sampled_color)))
                            .monospace()
                            .background_color(Color32::from_rgba_unmultiplied(255, 255, 255, 6)),
                    );
                    ui.end_row();

                    ui.label("🌗 RGBA");
                    ui.label(
                        RichText::new(format!("{}", disp_col_norm(state.sampled_color, 255.)))
                            .monospace()
                            .background_color(Color32::from_rgba_unmultiplied(255, 255, 255, 6)),
                    );
                    ui.end_row();

                    ui.label("⊞ Pos");
                    ui.label(
                        RichText::new(format!(
                            "{:.0},{:.0}",
                            state.cursor_relative.x, state.cursor_relative.y
                        ))
                        .monospace()
                        .background_color(Color32::from_rgba_unmultiplied(255, 255, 255, 6)),
                    );
                    ui.end_row();

                    ui.label(" UV");
                    ui.label(
                        RichText::new(format!("{:.3},{:.3}", uv_center.0, 1.0 - uv_center.1))
                            .monospace()
                            .background_color(Color32::from_rgba_unmultiplied(255, 255, 255, 6)),
                    );
                    ui.end_row();
                });

                // make sure aspect ratio is compensated for the square preview
                let ratio = texture.size().0 / texture.size().1;
                let uv_size = (scale, scale * ratio);
                let x = ui
                    .add(
                        egui::Image::new(tex_id, img_size).uv(egui::Rect::from_x_y_ranges(
                            uv_center.0 - uv_size.0..=uv_center.0 + uv_size.0,
                            uv_center.1 - uv_size.1..=uv_center.1 + uv_size.1,
                        )), // .bg_fill(egui::Color32::RED),
                    )
                    .rect;

                let stroke_color = Color32::from_white_alpha(240);
                let bg_color = Color32::BLACK.linear_multiply(0.5);
                ui.painter_at(x).line_segment(
                    [x.center_bottom(), x.center_top()],
                    Stroke::new(4., bg_color),
                );
                ui.painter_at(x).line_segment(
                    [x.left_center(), x.right_center()],
                    Stroke::new(4., bg_color),
                );
                ui.painter_at(x).line_segment(
                    [x.center_bottom(), x.center_top()],
                    Stroke::new(1., stroke_color),
                );
                ui.painter_at(x).line_segment(
                    [x.left_center(), x.right_center()],
                    Stroke::new(1., stroke_color),
                );
                // ui.image(tex_id, img_size);
            }

            ui.vertical_centered_justified(|ui| {
                if let Some(img) = &state.current_image {
                    // if ui
                    //     .button("Calculate extended info")
                    //     .on_hover_text("Count unique colors in image")
                    //     .clicked()
                    // {
                    //     state.image_info = Some(ExtendedImageInfo::from_image(img));
                    // }
                    if ui
                        .button("Show alpha bleed")
                        .on_hover_text("Highlight pixels with zero alpha and color information")
                        .clicked()
                    {
                        state.current_texture = highlight_bleed(img).to_texture(gfx);
                    }
                    if ui
                        .button("Show semi-transparent pixels")
                        .on_hover_text(
                            "Highlight pixels that are neither fully opaque not fully transparent",
                        )
                        .clicked()
                    {
                        state.current_texture = highlight_semitrans(img).to_texture(gfx);
                    }
                    if ui.button("Reset image").clicked() {
                        state.current_texture = img.to_texture(gfx);
                    }

                    ui.add(egui::Slider::new(&mut state.tiling, 1..=10).text("Image tiling"));
                }
            });

            advanced_ui(ui, state);
        });
    }
}

pub fn settings_ui(ctx: &Context, state: &mut OculanteState) {
    if state.settings_enabled {
        egui::Window::new("Settings")
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .resizable(false)
            .default_width(400.)
            // .title_bar(false)
            .show(&ctx, |ui| {
                ui.vertical_centered_justified(|ui| {
                    if ui.button("Check for updates").clicked() {
                        state.message = Some("Checking for updates...".into());
                        update::update(Some(state.message_channel.0.clone()));
                        state.settings_enabled = false;
                    }

                    if ui.button("Close").clicked() {
                        state.settings_enabled = false;
                    }
                });
            });
    }
}

pub fn advanced_ui(ui: &mut Ui, state: &mut OculanteState) {
    if let Some(info) = &state.image_info {
        egui::Grid::new("extended").show(ui, |ui| {
            ui.label(format!("Number of colors"));
            ui.label(format!("{}", info.num_colors));
            ui.end_row();

            ui.label("Fully transparent");
            ui.label(format!(
                "{:.2}%",
                (info.num_transparent_pixels as f32 / info.num_pixels as f32) * 100.
            ));
            ui.end_row();

            ui.label("Pixels");
            ui.label(format!("{}", info.num_pixels));
            ui.end_row();
        });

        let red_vals = Points::new(Values::from_values_iter(
            info.red_histogram
                .iter()
                .map(|(k, v)| Value::new(*k as f64, *v as f64)),
        ))
        // .fill(0.)
        .stems(0.0)
        .color(Color32::RED);

        let green_vals = Points::new(Values::from_values_iter(
            info.green_histogram
                .iter()
                .map(|(k, v)| Value::new(*k as f64, *v as f64)),
        ))
        // .fill(0.)
        .stems(0.0)
        .color(Color32::GREEN);

        let blue_vals = Points::new(Values::from_values_iter(
            info.blue_histogram
                .iter()
                .map(|(k, v)| Value::new(*k as f64, *v as f64)),
        ))
        // .fill(0.)
        .stems(0.0)
        .color(Color32::BLUE);

        Plot::new("histogram")
            .allow_zoom(false)
            .allow_drag(false)
            .show(ui, |plot_ui| {
                // plot_ui.line(grey_vals);
                plot_ui.points(red_vals);
                plot_ui.points(green_vals);
                plot_ui.points(blue_vals);
            });
    }
}

/// Everything related to image editing
pub fn edit_ui(ctx: &Context, state: &mut OculanteState, gfx: &mut Graphics) {
    //    ui.color_edit_button_rgb(rgb)
    if !state.edit_enabled {
        return;
    }

    egui::SidePanel::right("edit_panel")
        .min_width(360.)
        .show(&ctx, |ui| {
            // A flag to indicate that the image needs to be rebuilt
            let mut changed = false;

            egui::Grid::new("editing").show(ui, |ui| {
                ui.label("🔃 Rotation");
                ui.horizontal(|ui| {
                    if let Some(img) = &mut state.current_image {
                        if ui
                            .button("⟳")
                            .on_hover_text("Rotate 90 deg right")
                            .clicked()
                        {
                            *img = image::imageops::rotate90(img);
                            changed = true;
                        }
                        if ui.button("⟲").on_hover_text("Rotate 90 deg left").clicked() {
                            *img = image::imageops::rotate270(img);
                            changed = true;
                        }
                    }
                });
                ui.end_row();

                // Blur
                ui.label("💧 Blur");
                if ui
                    .add(egui::Slider::new(&mut state.edit_state.blur, 0.0..=10.))
                    .changed()
                {
                    changed = true;
                }
                ui.end_row();

                // Contrast
                ui.label("◑ Contrast");
                if ui
                    .add(egui::Slider::new(
                        &mut state.edit_state.contrast,
                        -100.0..=100.,
                    ))
                    .changed()
                {
                    changed = true;
                }
                ui.end_row();

                // Brightness
                ui.label("☀ Brightness");
                if ui
                    .add(egui::Slider::new(
                        &mut state.edit_state.brightness,
                        -255..=255,
                    ))
                    .changed()
                {
                    changed = true;
                }
                ui.end_row();

                ui.label("✖ Mult color");
                ui.horizontal(|ui| {
                    if ui
                        .color_edit_button_rgb(&mut state.edit_state.color_mult)
                        .changed()
                    {
                        changed = true;
                    }
                });
                ui.end_row();

                ui.label("➕ Add  color");
                ui.horizontal(|ui| {
                    if ui
                        .color_edit_button_rgb(&mut state.edit_state.color_add)
                        .changed()
                    {
                        changed = true;
                    }
                });
                ui.end_row();

                ui.label("！ Invert colors");
                ui.horizontal(|ui| {
                    if let Some(img) = &mut state.current_image {
                        if ui.button("Invert").clicked() {
                            image::imageops::invert(img);
                            changed = true;
                        }
                    }
                });
                ui.end_row();

                ui.label("⬌ Flipping");
                ui.horizontal(|ui| {
                    if let Some(img) = &mut state.current_image {
                        if ui.button("Horizontal").clicked() {
                            *img = image::imageops::flip_horizontal(img);
                            changed = true;
                        }
                        if ui.button("Vertical").clicked() {
                            *img = image::imageops::flip_vertical(img);
                            changed = true;
                        }
                    }
                });
                ui.end_row();

                ui.label("✂ Crop");
                ui.horizontal(|ui| {
                    let r1 = ui.add(
                        egui::DragValue::new(&mut state.edit_state.crop[0])
                            .speed(4.)
                            .clamp_range(0..=10000)
                            .prefix("left "),
                    );
                    let r2 = ui.add(
                        egui::DragValue::new(&mut state.edit_state.crop[2])
                            .speed(4.)
                            .clamp_range(0..=10000)
                            .prefix("right "),
                    );
                    let r3 = ui.add(
                        egui::DragValue::new(&mut state.edit_state.crop[1])
                            .speed(4.)
                            .clamp_range(0..=10000)
                            .prefix("top "),
                    );
                    let r4 = ui.add(
                        egui::DragValue::new(&mut state.edit_state.crop[3])
                            .speed(4.)
                            .clamp_range(0..=10000)
                            .prefix("bottom "),
                    );
                    // TODO rewrite with any
                    if r1.changed() || r2.changed() || r3.changed() || r4.changed() {
                        changed = true;
                    }
                });
                ui.end_row();

                ui.label("🔁 Reset");
                if ui.button("Reset all edits").clicked() {
                    state.edit_state = Default::default();
                    changed = true
                }
                ui.end_row();

                ui.label("❓ Compare");
                ui.horizontal(|ui| {
                    if ui.button("Unmodified").clicked() {
                        if let Some(img) = &state.current_image {
                            state.current_texture = img.to_texture(gfx);
                        }
                    }
                    if ui.button("Modified").clicked() {
                        changed = true;
                    }
                });
                ui.end_row();

                ui.label("🖊 Paint");
                ui.checkbox(&mut state.edit_state.painting, "Enable painting");
                
                if state.edit_state.painting {
                    ui.end_row();
                    ui.label("Color");
                    ui.color_edit_button_rgba_unmultiplied(&mut state.edit_state.color_paint);
                    ui.end_row();
                
                    ui.label("Strokes");
                    ui.horizontal(|ui| {


                        if ui.button("↩").clicked() {
                            let _ = state.edit_state.paint_lines.pop();
                            let _ = state.edit_state.paint_lines.pop();
                            changed = true;
                        }
    
                        if ui.button("⊗").clicked() {
                            let _ = state.edit_state.paint_lines.clear();
                            changed = true;
                        }
    
                        // If we have no lines, create an empty one
                        if state.edit_state.paint_lines.is_empty() {
                            state.edit_state.paint_lines.push(vec![]);
                        }
    
                        if state.edit_state.painting {
                            if let Some(current_line) = state.edit_state.paint_lines.last_mut() {
                                if ctx.input().pointer.primary_down() {
                                    // get pos in image
                                    let p = state.cursor_relative;
                                    // ui.label(format!("{}/{}", p.x, p.y));
                                    current_line.push(Pos2::new(p.x, p.y));
                                    changed = true;
                                } else if !current_line.is_empty() {
                                    state.edit_state.paint_lines.push(vec![]);
                                }
                            }
                        }
                    });
                
                
                }
                
              
                ui.end_row();


                ui.label("Apply edits");
                if ui.button("Apply").on_hover_text("Apply all edits to the image and reset edit controls").clicked() {
                    if let Some(img) = &mut state.current_image {
                        *img = state.edit_state.result.clone();
                        state.edit_state = Default::default();
                        changed = true;
                    }
                }
                ui.end_row();

                // Do the processing
                if changed {
                    if let Some(img) = &state.current_image {
                        // test if there is cropping
                        if state.edit_state.crop != [0, 0, 0, 0] {
                            let sub_img = image::imageops::crop_imm(
                                img,
                                state.edit_state.crop[0].max(0) as u32,
                                state.edit_state.crop[1].max(0) as u32,
                                (img.width() as i32 - state.edit_state.crop[2]).max(0) as u32,
                                (img.height() as i32 - state.edit_state.crop[3]).max(0) as u32,
                            );
                            state.edit_state.result = sub_img.to_image();
                        } else {
                            state.edit_state.result = img.clone();
                        }

                        // test if blur is changed
                        if state.edit_state.blur != 0.0 {
                            state.edit_state.result = image::imageops::blur(
                                &state.edit_state.result,
                                state.edit_state.blur,
                            );
                        }

                        // test if mult or add is modified
                        if state.edit_state.color_mult != [1., 1., 1.]
                            || state.edit_state.color_add != [0., 0., 0.]
                        {
                            for p in state.edit_state.result.pixels_mut() {
                                // mult
                                p[0] = (p[0] as f32 * state.edit_state.color_mult[0]) as u8;
                                p[1] = (p[1] as f32 * state.edit_state.color_mult[1]) as u8;
                                p[2] = (p[2] as f32 * state.edit_state.color_mult[2]) as u8;
                                // add
                                p[0] = (p[0] as f32 + state.edit_state.color_add[0] * 255.) as u8;
                                p[1] = (p[1] as f32 + state.edit_state.color_add[1] * 255.) as u8;
                                p[2] = (p[2] as f32 + state.edit_state.color_add[2] * 255.) as u8;
                            }
                        }

                        if state.edit_state.brightness != 0 {
                            state.edit_state.result = image::imageops::brighten(
                                &state.edit_state.result,
                                state.edit_state.brightness,
                            );
                        }
                        if state.edit_state.contrast != 0.0 {
                            state.edit_state.result = image::imageops::contrast(
                                &state.edit_state.result,
                                state.edit_state.contrast,
                            );
                        }

                        // draw paint lines
                        let active_brush = &state.edit_state.brushes[0];
                        let fac = 0.5;
                        let active_brush = image::imageops::resize(
                            active_brush,
                            (active_brush.width() as f32 * fac) as u32,
                            (active_brush.height() as f32 * fac) as u32,
                            Gaussian,
                        );
                        for line in &state.edit_state.paint_lines {
                            for p in egui::Shape::dotted_line(
                                line,
                                Color32::DARK_RED,
                                active_brush.width() as f32 / 4.,
                                0.,
                            ) {
                                let pos_on_line = p.visual_bounding_rect().center();

                                paint_at(
                                    &mut state.edit_state.result,
                                    &active_brush,
                                    &pos_on_line,
                                    state.edit_state.color_paint,
                                );
                            }
                        }
                    }

                    state.current_texture = state.edit_state.result.to_texture(gfx);
                }

            

                ui.label("💾 Save");
                let compatible_extensions = ["png", "jpg"];
                if let Some(path) = &state.current_path {
                    if let Some(ext) = path.extension() {
                        if compatible_extensions
                            .contains(&ext.to_string_lossy().to_string().as_str())
                        {
                            if ui.button("Overwrite").clicked() {
                                let _ = state.edit_state.result.save(path);
                            }
                        } else {
                            if ui.button("Save as png").clicked() {
                                let _ = state.edit_state.result.save(path.with_extension("png"));
                            }
                        }
                    }
                }
            });

            // // Unsharp
            // if let Some(img) = &mut state.current_image {
            //     let response_amt = ui.add(
            //         egui::Slider::new(&mut state.edit_state.unsharpen, 0.0..=20.)
            //             .text("💧 unsharpen amt"),
            //     );
            //     let response_thresh = ui.add(
            //         egui::Slider::new(&mut state.edit_state.unsharpen_threshold, 0..=20)
            //             .text("💧 unsharpen threshold"),
            //     );
            //     if response_amt.changed() {
            //         let img_blurred = image::imageops::unsharpen(
            //             img,
            //             state.edit_state.unsharpen,
            //             state.edit_state.unsharpen_threshold,
            //         );
            //         state.current_texture = img_blurred.to_texture(gfx);
            //         state.edit_state.result = img_blurred;
            //     }
            //     if response_amt.drag_released() {
            //         *img = state.edit_state.result.clone();
            //     }

            //     if response_thresh.changed() {
            //         let img_blurred = image::imageops::unsharpen(
            //             img,
            //             state.edit_state.unsharpen,
            //             state.edit_state.unsharpen_threshold,
            //         );
            //         state.current_texture = img_blurred.to_texture(gfx);
            //         state.edit_state.result = img_blurred;
            //     }
            //     if response_thresh.drag_released() {
            //         *img = state.edit_state.result.clone();
            //     }
            // }

            if changed && state.info_enabled {
                state.image_info = None;
                send_extended_info(&state.current_image, &state.extended_info_channel);
            }
        });
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
