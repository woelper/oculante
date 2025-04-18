use super::*;
use crate::appstate::OculanteState;
use crate::utils::*;
use image::{ColorType, GenericImageView, RgbaImage};
#[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
use notan::egui::*;

/// Everything related to image editing
#[allow(unused_variables)]
pub fn edit_ui(app: &mut App, ctx: &Context, state: &mut OculanteState, gfx: &mut Graphics) {
    // A flag to indicate that the image needs to be rebuilt
    let mut image_changed = false;
    let mut pixels_changed = false;

    if let Some(img) = &state.current_image {
        // Ensure that edit result image is always filled
        if state.edit_state.result_pixel_op.width() == 0 {
            debug!("Edit state pixel comp buffer is default, cloning from image");
            // FIXME This needs to go, and we need to implement operators for DynamicImage
            state.edit_state.result_pixel_op = img.clone();
            pixels_changed = true;
        }
        if state.edit_state.result_image_op.width() == 0 {
            debug!("Edit state image comp buffer is default, cloning from image");
            // FIXME This needs to go, and we need to implement operators for DynamicImage
            state.edit_state.result_image_op = img.clone();
            image_changed = true;
        }
    }

    // TODO: Move these to image_editing
    let mut ops = [
        // General Image Adjustments
        ImgOpItem::new(ImageOperation::Brightness(0)),
        ImgOpItem::new(ImageOperation::Contrast(0)),
        ImgOpItem::new(ImageOperation::Exposure(0)),
        ImgOpItem::new(ImageOperation::Desaturate(0)),
        ImgOpItem::new(ImageOperation::Invert),
        // Colour and Hue
        ImgOpItem::new(ImageOperation::ChannelSwap((Channel::Red, Channel::Red))),
        ImgOpItem::new(ImageOperation::Equalize((0, 255))),
        ImgOpItem::new(ImageOperation::HSV((0, 100, 100))),
        ImgOpItem::new(ImageOperation::Add([0, 0, 0])),
        ImgOpItem::new(ImageOperation::Mult([255, 255, 255])),
        ImgOpItem::new(ImageOperation::Fill([255, 255, 255, 255])),
        ImgOpItem::new(ImageOperation::Slice(128, 20, false)),
        // Colour Mapping and Conversion
        ImgOpItem::new(ImageOperation::LUT("Lomography Redscale 100".into())),
        ImgOpItem::new(ImageOperation::GradientMap(vec![
            GradientStop::new(0, [155, 33, 180]),
            GradientStop::new(128, [255, 83, 0]),
            GradientStop::new(255, [224, 255, 0]),
        ])),
        ImgOpItem::new(ImageOperation::Posterize(8)),
        ImgOpItem::new(ImageOperation::Filter3x3([
            0, -100, 0, -100, 500, -100, 0, -100, 0,
        ])),
        ImgOpItem::new(ImageOperation::ColorConverter(
            crate::image_editing::ColorTypeExt::Rgba8,
        )),
        // Mathematical
        ImgOpItem::new(ImageOperation::MMult),
        ImgOpItem::new(ImageOperation::MDiv),
        ImgOpItem::new(ImageOperation::Expression("r = 1.0".into())),
        ImgOpItem::new(ImageOperation::ScaleImageMinMax),
        // Effects
        ImgOpItem::new(ImageOperation::Blur(0)),
        ImgOpItem::new(ImageOperation::Noise {
            amt: 50,
            mono: false,
        }),
        ImgOpItem::new(ImageOperation::ChromaticAberration(15)),
        // Geometry and Transformations
        ImgOpItem::new(ImageOperation::Flip(false)),
        ImgOpItem::new(ImageOperation::Rotate(90)),
        ImgOpItem::new(ImageOperation::Resize {
            dimensions: state.image_geometry.dimensions,
            aspect: true,
            filter: ScaleFilter::Hamming,
        }),
        ImgOpItem::new(ImageOperation::Crop([0, 0, 0, 0])),
        ImgOpItem::new(ImageOperation::CropPerspective {
            points: [
                (0, 0),
                (state.image_geometry.dimensions.0, 0),
                (0, state.image_geometry.dimensions.1),
                (
                    state.image_geometry.dimensions.0,
                    state.image_geometry.dimensions.1,
                ),
            ],
            original_size: state.image_geometry.dimensions,
        }),
    ];

    egui::SidePanel::right("editing")
        .min_width(100.)
        // safeguard to not expand too much
        .max_width(500.)
        .show_separator_line(false)
        .show(ctx, |ui| {


            let open = ui.ctx().data(|r|r.get_temp::<bool>("filter_open".into()));

            ui.scope(|ui| {
                ui.style_mut().visuals.collapsing_header_frame = true;
                ui.style_mut().visuals.indent_has_left_vline = false;
                CollapsingHeader::new("Filters")
                    .icon(caret_icon)
                    .open(open)
                    .show_unindented(ui, |ui| {
                        dark_panel(ui, |ui| {
                            egui::ScrollArea::vertical().max_height(300.).show(ui, |ui| {

                                ui.vertical_centered_justified(|ui|{
                                    for op in &mut ops {
                                        if ui.button( format!("{op}")).clicked() {
                                            if op.operation.is_per_pixel() {
                                                state.edit_state.pixel_op_stack.push(op.clone());
                                            } else {
                                                state.edit_state.image_op_stack.push(op.clone());
                                            }
                                            image_changed = true;
                                            ui.ctx().data_mut(|w|w.insert_temp("filter_open".into(), false));
                                        }
                                    }
                                });
                            });
                        });
                    });
            });

            if open.is_some() {
                ui.ctx().data_mut(|w|w.remove_temp::<bool>("filter_open".into()));
            }


            egui::ScrollArea::vertical().show(ui, |ui| {

                ui.vertical_centered_justified(|ui| {
                    modifier_stack_ui(&mut state.edit_state.image_op_stack, &mut image_changed, ui, &state.image_geometry, &mut state.edit_state.block_panning, &mut state.volatile_settings);

                    // draw a line between different operator types
                    if !state.edit_state.image_op_stack.is_empty() && !state.edit_state.pixel_op_stack.is_empty() {
                        ui.separator();
                    }
                    modifier_stack_ui(
                        &mut state.edit_state.pixel_op_stack,
                        &mut pixels_changed,
                        ui, &state.image_geometry, &mut state.edit_state.block_panning, &mut state.volatile_settings
                    );
                });


            ui.vertical_centered_justified(|ui| {
                if state.edit_state.painting {

                    if ctx.input(|i|i.pointer.secondary_down()) {
                        if let Some(stroke) = state.edit_state.paint_strokes.last_mut() {
                            if let Some(p) = get_pixel_checked(&state.edit_state.result_pixel_op, state.cursor_relative.x as u32, state.cursor_relative.y as u32) {
                                stroke.color = [
                                    p[0] as f32 / 255.,
                                    p[1] as f32 / 255.,
                                    p[2] as f32 / 255.,
                                    p[3] as f32 / 255.,
                                ];
                                // state.sampled_color = [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32];
                            }
                        }
                    }

                    if ui
                        .add(
                            egui::Button::new("Stop painting")
                                .fill(ui.style().visuals.selection.bg_fill),
                        )
                        .clicked()
                    {
                        state.edit_state.painting = false;
                    }
                } else if ui.button("Paint mode").clicked() {
                    state.edit_state.painting = true;
                }
            });

            if state.edit_state.painting {
                egui::Grid::new("paint").show(ui, |ui| {
                    ui.label("📜 Keep history");
                    ui.styled_checkbox(&mut state.edit_state.non_destructive_painting, "")
                        .on_hover_text("Keeps all paint history and allows edits to it. Slower.");
                    ui.end_row();

                    if let Some(stroke) = state.edit_state.paint_strokes.last_mut() {
                        if stroke.is_empty() {
                            ui.label("Color");
                            ui.label("Fade");
                            ui.label("Flip");
                            ui.label("Width");
                            ui.label("Brush");
                            ui.end_row();

                            stroke_ui(stroke, &state.edit_state.brushes, ui, gfx);
                        }
                    }
                });

                if state
                    .edit_state
                    .paint_strokes
                    .iter()
                    .filter(|s| !s.is_empty())
                    .count()
                    != 0
                {
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label("Strokes");
                        if ui.button("↩").clicked() {
                            let _ = state.edit_state.paint_strokes.pop();
                            let _ = state.edit_state.paint_strokes.pop();
                            pixels_changed = true;
                        }
                        if ui.button("Clear all").clicked() {
                            state.edit_state.paint_strokes.clear();
                            pixels_changed = true;
                        }
                    });

                    let mut delete_stroke: Option<usize> = None;

                    egui::ScrollArea::vertical()
                        .min_scrolled_height(64.)
                        .show(ui, |ui| {
                            let mut stroke_lost_highlight = false;
                            if ui
                                .vertical(|ui| {
                                    egui::Grid::new("stroke").show(ui, |ui| {
                                        ui.label("Color");
                                        ui.label("Fade");
                                        ui.label("Flip");
                                        ui.label("Width");
                                        ui.label("Brush");
                                        ui.label("Del");
                                        ui.end_row();

                                        for (i, stroke) in
                                            state.edit_state.paint_strokes.iter_mut().enumerate()
                                        {
                                            if stroke.is_empty() || stroke.committed {
                                                continue;
                                            }

                                            let r = stroke_ui(
                                                stroke,
                                                &state.edit_state.brushes,
                                                ui,
                                                gfx,
                                            );
                                            if r.changed() {
                                                pixels_changed = true;
                                            }

                                            if r.hovered() {
                                                pixels_changed = true;
                                                stroke.highlight = true;
                                            } else {
                                                stroke.highlight = false;
                                                stroke_lost_highlight = true;
                                            }

                                            // safety catch to update brush highlights
                                            if r.clicked_elsewhere() {
                                                pixels_changed = true;
                                            }

                                            if ui.button("⊗").clicked() {
                                                delete_stroke = Some(i);
                                            }
                                            ui.end_row();
                                        }
                                    });
                                })
                                .response
                                .hovered()
                            {
                                // only update if this outer response is triggered, so we don't trigger it all the time
                                if stroke_lost_highlight {
                                    pixels_changed = true;
                                }
                            }
                        });
                    if let Some(stroke_to_delete) = delete_stroke {
                        state.edit_state.paint_strokes.remove(stroke_to_delete);
                        pixels_changed = true;
                    }
                }


                // If we have no lines, create an empty one
                if state.edit_state.paint_strokes.is_empty() {
                    state.edit_state.paint_strokes.push(PaintStroke::new());
                }

                if let Some(current_stroke) = state.edit_state.paint_strokes.last_mut() {
                    // if state.mouse_delta.x > 0.0 {
                    if ctx.input(|i|i.pointer.primary_down()) && !state.pointer_over_ui {
                        debug!("PAINT");
                        // get pos in image
                        // let p = state.cursor_relative;
                        let uv = (
                            state.cursor_relative.x / state.image_geometry.dimensions.0 as f32,
                            (state.cursor_relative.y / state.image_geometry.dimensions.1 as f32),
                        );
                        current_stroke.points.push(uv);
                        pixels_changed = true;
                    } else if !current_stroke.is_empty() {
                        // clone last stroke to inherit settings
                        if let Some(last_stroke) = state.edit_state.paint_strokes.clone().last() {
                            state
                                .edit_state
                                .paint_strokes
                                .push(last_stroke.without_points());
                        }
                    }
                }
            }

            ui.horizontal(|ui|{
                if ui.add(egui::Button::new("Original").min_size(vec2(ui.available_width()/2., 0.))).clicked() {
                    if let Some(img) = &state.current_image {
                        state.image_geometry.dimensions = img.dimensions();
                        if let Err(error) = state.current_texture.set_image(img, gfx, &state.persistent_settings){
                            state.send_message_warn(&format!("Error while displaying image: {error}"));
                        }
                    }
                }
                if ui.add(egui::Button::new("Modified").min_size(vec2(ui.available_width(), 0.))).clicked() {
                    pixels_changed = true;

                }
            });

            ui.vertical_centered_justified(|ui| {
                if ui
                    .button("Apply all edits")
                    .on_hover_text("Apply all edits to the image and reset edit controls")
                    .clicked()
                {
                    if let Some(img) = &mut state.current_image {
                        *img = state.edit_state.result_pixel_op.clone();
                        state.edit_state = Default::default();
                        // state.dimensions = img.dimensions();
                        pixels_changed = true;
                        image_changed = true;
                    }
                }

                if ui.button("Remove all edits").clicked() {
                    state.edit_state = Default::default();
                    pixels_changed = true
                }
            });


            ui.vertical_centered_justified(|ui| {
                if let Some(path) = &state.current_path {
                    if ui
                        .button("Reload & Restore")
                        .on_hover_text("Completely reloads the current image, destroying all edits.")
                        .clicked()
                    {
                        state.is_loaded = false;
                        state.player.cache.clear();
                        state.player.load(path);
                    }
                }


                #[cfg(feature = "turbo")]
                jpg_lossless_ui(state, ui);


                if state.current_path.is_none() && state.current_image.is_some() {
                    #[cfg(not(feature = "file_open"))]
                    {
                        if ui.button("Create output file").on_hover_text("This image does not have any file associated with it. Click to create a default one.").clicked() {
                            let dest = state.volatile_settings.last_open_directory.clone().join("untitled").with_extension(&state.edit_state.export_extension);
                            state.current_path = Some(dest);
                            set_title(app, state);
                        }
                    }
                }

                #[cfg(feature = "file_open")]
                if state.current_image.is_some() {
                    if ui.button(format!("Save as...")).clicked() {

                        let start_directory = state.volatile_settings.last_open_directory.clone();

                        let image_to_save = state.edit_state.result_pixel_op.clone();
                        let msg_sender = state.message_channel.0.clone();
                        let err_sender = state.message_channel.0.clone();
                        let image_info = state.image_info.clone();

                        std::thread::spawn(move || {
                            let file_dialog_result = rfd::FileDialog::new()
                                .set_directory(start_directory)
                                .save_file();

                                if let Some(file_path) = file_dialog_result {
                                    debug!("Selected File Path = {:?}", file_path);
                                    match image_to_save
                                        .save(&file_path) {
                                            Ok(_) => {
                                                _ = msg_sender.send(crate::appstate::Message::Saved(file_path.clone()));
                                                debug!("Saved to {}", file_path.display());
                                                // Re-apply exif
                                                if let Some(info) = &image_info {
                                                    debug!("Extended image info present");

                                                    // before doing anything, make sure we have raw exif data
                                                    if info.raw_exif.is_some() {
                                                        if let Err(e) = fix_exif(&file_path, info.raw_exif.clone()) {
                                                            error!("{e}");
                                                        } else {
                                                            info!("Saved EXIF.")
                                                        }
                                                    } else {
                                                        debug!("No raw exif");
                                                    }
                                                }
                                            }
                                            Err(e) => {
                                                _ = err_sender.send(crate::appstate::Message::err(&format!("Error: Could not save: {e}")));
                                            }
                                        }
                                        // state.toast_cooldown = 0.0;
                                }

                        });
                        ui.ctx().request_repaint();
                    }
                }

                #[cfg(not(feature = "file_open"))]
                if state.current_image.is_some() {
                    if ui.button("Save as...").clicked() {
                        ui.ctx().memory_mut(|w| w.open_popup(Id::new("SAVE")));
                    }

                    let encoding_options = state.volatile_settings.encoding_options.clone();

                    if ctx.memory(|w| w.is_popup_open(Id::new("SAVE"))) {
                        let msg_sender = state.message_channel.0.clone();
                        let keys = &state.volatile_settings.encoding_options.iter().map(|e|e.ext()).collect::<Vec<_>>();
                        let key_slice = keys.iter().map(|k|k.as_str()).collect::<Vec<_>>();
                        let encoders = state.volatile_settings.encoding_options.clone();
                        filebrowser::browse_modal(
                            true,
                            key_slice.as_slice(),
                            &mut state.volatile_settings,
                            |p| {
                                _ = save_with_encoding(&state.edit_state.result_pixel_op, p, &state.image_metadata, &encoders);
                            },
                            ctx,
                        );
                    }
                }

                let modal = super::Modal::new("modal", ctx);

                if let Some(p) = &state.current_path {
                    let text = if p.exists() { "Overwrite" } else { "Save"};

                    modal.show( "Overwrite?", |_|{
                        _ = save_with_encoding(&state.edit_state.result_pixel_op, p, &state.image_metadata, &state.volatile_settings.encoding_options).map(|_| state.send_message_info("Saved")).map_err(|e| state.send_message_err(&format!("Error: {e}")));
                    });


                    if ui.button(text).on_hover_text("Saves the image. This will create a new file or overwrite an existing one.").clicked() {
                        if p.exists() {
                            modal.open();
                        } else {
                            _ = save_with_encoding(&state.edit_state.result_pixel_op, p, &state.image_metadata, &state.volatile_settings.encoding_options).map(|_| state.send_message_info("Saved")).map_err(|e| state.send_message_err(&format!("Error: {e}")));
                        }
                    }

                    if ui.button("Save edits").on_hover_text("Saves an .oculante metafile in the same directory as the image. This file will contain all edits and will be restored automatically if you open the image again. This leaves the original image unmodified and allows you to continue editing later.").clicked() {
                        if let Ok(f) = std::fs::File::create(p.with_extension("oculante")) {
                            _ = serde_json::to_writer_pretty(&f, &state.edit_state);
                        }
                    }
                    if ui.button("Save directory edits").on_hover_text("Saves an .oculante metafile in the same directory as all applicable images. This file will contain all edits and will be restored automatically if you open the image(s) again. This leaves the original image(s) unmodified and allows you to continue editing later.").clicked() {
                        if let Some(parent) = p.parent() {
                            if let Ok(f) = std::fs::File::create(parent.join(".oculante")) {
                                _ = serde_json::to_writer_pretty(&f, &state.edit_state);
                            }
                        }
                    }
                }
            });
        });

        if state.edit_state.result_image_op.color() != ColorType::Rgba8 {
            let op_present = state.edit_state.image_op_stack.first().map(|op| if let ImageOperation::ColorConverter(_) = op.operation {true} else {false}).unwrap_or_default();
            if !op_present {
                state.edit_state.image_op_stack.insert(0, ImgOpItem::new(ImageOperation::ColorConverter(ColorTypeExt::Rgba8)));
                image_changed = true;
                pixels_changed = true;
                state.send_message_info("Color conversion operator added.");
            }
        }

        if let Some(img) = &state.current_image {
            if img.color() != ColorType::Rgba8 {
                ui.add_space(10.);
                ui.small(format!("{INFO} Your image is not 8 bit RGBA. For full editing support a conversion operator was added."));
            }
        }

        #[cfg(debug_assertions)]
        {
            ui.colored_label(Color32::LIGHT_BLUE, "Debug info");
            ui.label(format!("image op: {:?}", state.edit_state.result_image_op.color()));
            ui.label(format!("pixel op: {:?}", state.edit_state.result_pixel_op.color()));

        }


            // Do the processing

            // If expensive operations happened (modifying image geometry), process them here
            let message: Option<String> = None;
            if image_changed {
                if let Some(img) = &mut state.current_image {
                    let stamp = Instant::now();
                    // start with a fresh copy of the unmodified image
                    // FIXME This needs to go, and we need to implement operators for DynamicImage
                    state.edit_state.result_image_op = img.clone();
                    for operation in &state.edit_state.image_op_stack {
                        if !operation.active {
                            continue;
                        }
                        if let Err(e) = operation.operation.process_image(&mut state.edit_state.result_image_op) {
                            error!("{e}");
                            state.send_message_warn(&format!("{e}"));
                        }
                    }
                    debug!(
                        "Image changed. Finished evaluating in {}s",
                        stamp.elapsed().as_secs_f32()
                    );

                    // tag strokes as uncommitted as they need to be rendered again
                    for stroke in &mut state.edit_state.paint_strokes {
                        stroke.committed = false;
                    }
                }
                pixels_changed = true;
            }

            if pixels_changed {
                // init result as a clean copy of image operation result
                let stamp = Instant::now();

                // start from the result of the image operations
                state.edit_state.result_pixel_op = state.edit_state.result_image_op.clone();

                // only process pixel stack if it is empty so we don't run through pixels without need
                if !state.edit_state.pixel_op_stack.is_empty() {
                    let ops = &state.edit_state.pixel_op_stack.iter().filter(|op|op.active).map(|op| op.operation.clone()).collect();
                    if let Err(e) = process_pixels(&mut state.edit_state.result_pixel_op, ops) {
                        state.send_message_warn(&format!("{e}"));
                    }
                }

                    debug!(
                    "Finished Pixel op stack in {} s",
                    stamp.elapsed().as_secs_f32()
                );

                // draw paint lines
                for stroke in &state.edit_state.paint_strokes {
                    if !stroke.committed {
                        if let Some(compatible_buffer) = state.edit_state.result_pixel_op.as_mut_rgba8() {
                            stroke.render(
                                compatible_buffer,
                                &state.edit_state.brushes,
                            );
                        }

                    }
                }

                state.send_frame(crate::utils::Frame::UpdateTexture);
                debug!(
                    "Done updating tex after pixel; ops in {} s",
                    stamp.elapsed().as_secs_f32()
                );
            }

            // render uncommitted strokes if destructive to speed up painting
            if state.edit_state.painting {
                // render previous strokes
                if state
                    .edit_state
                    .paint_strokes
                    .iter()
                    .filter(|l| !l.points.is_empty())
                    .count()
                    > 1
                    && !state.edit_state.non_destructive_painting
                {
                    let stroke_count = state.edit_state.paint_strokes.len();

                    for (i, stroke) in state.edit_state.paint_strokes.iter_mut().enumerate() {
                        if i < stroke_count - 1 && !stroke.committed && !stroke.is_empty() {

                            if let Some(compatible_buffer) = state.edit_state.result_pixel_op.as_mut_rgba8() {
                                stroke.render(
                                    compatible_buffer,
                                    &state.edit_state.brushes,
                                );
                            }


                            stroke.committed = true;
                            debug!("Committed stroke {}", i);
                        }
                    }
                }
            }

            state.image_geometry.dimensions = state.edit_state.result_pixel_op.dimensions();


        });
}

fn stroke_ui(
    stroke: &mut PaintStroke,
    brushes: &[RgbaImage],
    ui: &mut Ui,
    gfx: &mut Graphics,
) -> Response {
    let mut combined_response = ui.color_edit_button_rgba_unmultiplied(&mut stroke.color);

    let r = ui
        .styled_checkbox(&mut stroke.fade, "")
        .on_hover_text("Fade out the stroke over its path");
    if r.changed() {
        combined_response.mark_changed();
    }
    if r.hovered() {
        // combined_response.chacha
        // combined_response.flags.insert(egui::response::Flags::CLICKED);
        combined_response
            .flags
            .set(egui::response::Flags::CLICKED, true);

        // combined_response
        //     .flags
        //     .set(egui::Response::Flags::HOVERED, true);
    }

    let r = ui
        .styled_checkbox(&mut stroke.flip_random, "")
        .on_hover_text("Flip brush X and Y randomly to make stroke less uniform");
    if r.changed() {
        combined_response.mark_changed();
    }
    if r.hovered() {
        combined_response.mark_changed();
    }

    let r = ui.add(
        egui::DragValue::new(&mut stroke.width)
            .range(0.0..=0.3)
            .speed(0.001),
    );
    if r.changed() {
        combined_response.mark_changed();
    }
    if r.hovered() {
        combined_response.mark_changed();
    }

    ui.horizontal(|ui| {
        if let Some(notan_texture) = brushes[stroke.brush_index].to_texture_premult(gfx) {
            let texture_id = gfx.egui_register_texture(&notan_texture);
            ui.add(
                egui::Image::new(texture_id)
                    .fit_to_exact_size(egui::Vec2::splat(ui.available_height())),
            );
        }

        let r = egui::ComboBox::from_id_salt(format!("s {:?}", stroke.points))
            .selected_text(format!("Brush {}", stroke.brush_index))
            .show_ui(ui, |ui| {
                for (b_i, b) in brushes.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if let Some(notan_texture) = b.to_texture_premult(gfx) {
                            let texture_id = gfx.egui_register_texture(&notan_texture);
                            ui.add(
                                egui::Image::new(texture_id)
                                    .fit_to_exact_size(egui::Vec2::splat(ui.available_height())),
                            );
                        }

                        if ui
                            .selectable_value(&mut stroke.brush_index, b_i, format!("Brush {b_i}"))
                            .clicked()
                        {
                            combined_response.mark_changed();
                        }
                    });
                }
            })
            .response;

        if r.hovered() {
            combined_response
                .flags
                .insert(egui::response::Flags::HOVERED);
        }
    });

    stroke.highlight = combined_response.hovered();

    if combined_response.changed() {
        stroke.highlight = false;
    }
    combined_response
}

fn modifier_stack_ui(
    stack: &mut Vec<ImgOpItem>,
    image_changed: &mut bool,
    ui: &mut Ui,
    geo: &ImageGeometry,
    mouse_grab: &mut bool,
    settings: &mut VolatileSettings,
) {
    let mut delete: Option<usize> = None;
    let mut swap: Option<(usize, usize)> = None;

    let stack_len = stack.len();

    for (i, operation) in stack.iter_mut().enumerate() {
        let frame_color = if ui.style().visuals.dark_mode {
            Color32::from_hex("#212121").unwrap()
        } else {
            Color32::from_hex("#F2F2F2").unwrap()
        };
        egui::Frame::new()
            .fill(frame_color)
            .corner_radius(ui.style().visuals.widgets.active.corner_radius)
            .inner_margin(Margin::same(6))
            .show(ui, |ui| {
                ui.allocate_space(vec2(ui.available_width(), 0.0));
                ui.horizontal(|ui| {
                    let up = i != 0;
                    let down = i != stack_len - 1;
                    let caret_size = 10.;

                    ui.add_enabled_ui(up, |ui| {
                        let ur = ui.add(
                            egui::Button::new(RichText::new("").size(caret_size)).frame(false),
                        );
                        if ur.on_hover_text("Move up").clicked() {
                            swap = Some(((i as i32 - 1).max(0) as usize, i));
                            *image_changed = true;
                        }
                    });

                    ui.add_enabled_ui(down, |ui| {
                        let dr = ui.add(
                            egui::Button::new(RichText::new("").size(caret_size)).frame(false),
                        );
                        if dr.on_hover_text("Move down").clicked() {
                            swap = Some((i, i + 1));
                            *image_changed = true;
                        }
                    });

                    let icon = if operation.active { EYE } else { EYEOFF };
                    if egui::Button::new(RichText::new(icon).size(caret_size))
                        .frame(false)
                        .ui(ui)
                        .on_hover_text("Bypass")
                        .clicked()
                    {
                        operation.active = !operation.active;
                        *image_changed = true;
                    }

                    if egui::Button::new(RichText::new("").size(caret_size * 1.5))
                        .frame(false)
                        .ui(ui)
                        .on_hover_text("Remove operator")
                        .clicked()
                    {
                        delete = Some(i);
                        *image_changed = true;
                    }
                    ui.add_space(caret_size / 2.);
                    ui.add_enabled_ui(operation.active, |ui| {
                        ui.label(format!("{operation}"));
                    });
                });

                ui.push_id(i, |ui| {
                    // draw the image operator
                    ui.style_mut().spacing.slider_width = ui.available_width() * 0.8;

                    ui.add_enabled_ui(operation.active, |ui| {
                        if operation
                            .operation
                            .ui(ui, geo, mouse_grab, settings)
                            .changed()
                        {
                            *image_changed = true;
                        }
                    });

                    ui.style_mut().spacing.icon_spacing = 0.;
                    ui.style_mut().spacing.button_padding = Vec2::ZERO;
                    ui.style_mut().spacing.interact_size = Vec2::ZERO;
                    ui.style_mut().spacing.indent = 0.0;
                    ui.style_mut().spacing.item_spacing = Vec2::ZERO;

                    // ui.add_space(80.);
                });
            });
    }

    if let Some(delete) = delete {
        stack.remove(delete);
    }

    if let Some(swap) = swap {
        if swap.1 < stack.len() {
            stack.swap(swap.0, swap.1);
        }
    }
}

/// A ui for lossless JPEG editing
#[cfg(feature = "turbo")]
fn jpg_lossless_ui(state: &mut OculanteState, ui: &mut Ui) {
    if let Some(p) = &state.current_path.clone() {
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_string().to_lowercase());
        if ext != Some("jpg".to_string()) && ext != Some("jpeg".to_string()) {
            return;
        }

        ui.styled_collapsing("Lossless JPEG edits", |ui| {
            ui.label(format!("{WARNING_CIRCLE} These operations will immediately write changes to disk."));
            let mut reload = false;

            ui.columns(3, |col| {
                if col[0].button("➡ Rotate 90°").clicked() {
                    if lossless_tx(
                        p,
                        turbojpeg::Transform::op(turbojpeg::TransformOp::Rot90)
                    )
                    .is_ok()
                    {
                        reload = true;
                    }
                }
                //◑
                if col[1].button("⬅ Rotate -90°").clicked() {
                    if lossless_tx(
                        p,
                        turbojpeg::Transform::op(turbojpeg::TransformOp::Rot270)
                    )
                    .is_ok()
                    {
                        reload = true;
                    }
                }

                if col[2].button("⬇ Rotate 180°").clicked() {
                    if lossless_tx(
                        p,
                        turbojpeg::Transform::op(turbojpeg::TransformOp::Rot180)
                    )
                    .is_ok()
                    {
                        reload = true;
                    }
                }
            });

            ui.columns(2,|col| {
                if col[0].button("Flip H").clicked() {
                    if lossless_tx(
                        p,
                        turbojpeg::Transform::op(turbojpeg::TransformOp::Hflip)
                    )
                    .is_ok()
                    {
                        reload = true;
                    }
                }

                if col[1].button("Flip V").clicked() {
                    if lossless_tx(
                        p,
                        turbojpeg::Transform::op(turbojpeg::TransformOp::Vflip)
                    )
                    .is_ok()
                    {
                        reload = true;
                    }
                }
            });

            ui.vertical_centered_justified(|ui| {
                let crop_ops = state
                    .edit_state
                    .image_op_stack
                    .iter()
                    .filter(|op|op.active)
                    .filter(|op| matches!(op.operation, ImageOperation::Crop(_)))
                    .map(|op|&op.operation)
                    .collect::<Vec<_>>();

                let crop = crop_ops
                    .first()
                    .cloned()
                    .cloned()
                    .unwrap_or(ImageOperation::Crop([0, 0, 0, 0]));

                if crop_ops.is_empty() {
                    info!("A missing crop operator was added.");
                    state
                        .edit_state
                        .image_op_stack
                        .push(ImgOpItem::new(ImageOperation::Crop([0, 0, 0, 0])))
                }

                ui.add_enabled_ui(crop != ImageOperation::Crop([0, 0, 0, 0]), |ui| {

                    if ui
                        .button("Crop Losslessly")
                        .on_hover_text("Crop according to values defined in the operator stack above")
                        .on_disabled_hover_text("Please modify crop values above before cropping. You would be cropping nothing right now.")
                        .clicked()
                    {
                        match crop {
                            ImageOperation::Crop(amt) => {
                                debug!("CROP {:?}", amt);

                                let dim = state
                                    .current_image
                                    .as_ref()
                                    .map(|i| i.dimensions())
                                    .unwrap_or_default();

                                let crop_range = cropped_range(&amt, &dim);

                                let mut crop = turbojpeg::Transform::default();
                                crop.crop = Some(turbojpeg::TransformCrop {
                                    x: crop_range[0] as usize,
                                    y: crop_range[1] as usize,
                                    width: Some(crop_range[2] as usize),
                                    height: Some(crop_range[3] as usize),
                                });

                                match lossless_tx(
                                    p,
                                    crop
                                ) {
                                    Ok(_) => reload = true,
                                    Err(e) => log::warn!("{e}"),
                                };
                            }
                            _ => (),
                        };
                    }
                });
                });

            if reload {
                state.is_loaded = false;
                state.player.cache.clear();
                state.player.load(p);
            }
        });
    }
}
