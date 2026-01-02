use super::Modal;
use super::*;
use crate::appstate::OculanteState;
use crate::utils::*;
#[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
use notan::egui::*;

pub fn main_menu(ui: &mut Ui, state: &mut OculanteState, app: &mut App, gfx: &mut Graphics) {
    let window_x = state.window_size.x - ui.style().spacing.icon_spacing * 2. - 100.;

    ui.horizontal_centered(|ui| {
        use crate::shortcuts::InputEvent::*;

        // The Close button
        if state.persistent_settings.borderless && unframed_button(X, ui).clicked() {
            app.backend.exit();
        }

        let mut changed_channels = false;

        if key_pressed(app, state, RedChannel) {
            state.persistent_settings.current_channel = ColorChannel::Red;
            changed_channels = true;
        }
        if key_pressed(app, state, GreenChannel) {
            state.persistent_settings.current_channel = ColorChannel::Green;
            changed_channels = true;
        }
        if key_pressed(app, state, BlueChannel) {
            state.persistent_settings.current_channel = ColorChannel::Blue;
            changed_channels = true;
        }
        if key_pressed(app, state, AlphaChannel) {
            state.persistent_settings.current_channel = ColorChannel::Alpha;
            changed_channels = true;
        }
        if key_pressed(app, state, RGBChannel) {
            state.persistent_settings.current_channel = ColorChannel::Rgb;
            changed_channels = true;
        }
        if key_pressed(app, state, RGBAChannel) {
            state.persistent_settings.current_channel = ColorChannel::Rgba;
            changed_channels = true;
        }

        // Force rgba while edit mode is open.
        // TODO: display of channels should be done through a shader
        if state.persistent_settings.edit_enabled
            && state.persistent_settings.current_channel != ColorChannel::Rgba
        {
            state.persistent_settings.current_channel = ColorChannel::Rgba;
            changed_channels = true;
        }

        if window_x > ui.cursor().left() + 110. {
            ui.add_enabled_ui(!state.persistent_settings.edit_enabled, |ui| {
                ui.spacing_mut().button_padding = Vec2::new(10., 0.);
                ui.spacing_mut().interact_size.y = BUTTON_HEIGHT_SMALL;
                ui.spacing_mut().combo_width = 1.;
                ui.spacing_mut().icon_width = 0.;

                let color = if ui.style().visuals.dark_mode {
                    Color32::WHITE
                } else {
                    Color32::BLACK
                };
                ui.style_mut().visuals.widgets.inactive.fg_stroke = Stroke::new(1., color);

                if !ui.style().visuals.dark_mode {
                    ui.style_mut().visuals.override_text_color = Some(Color32::WHITE);
                    ui.style_mut().visuals.widgets.inactive.weak_bg_fill = Color32::BLACK;
                    ui.style_mut().visuals.widgets.hovered.weak_bg_fill = Color32::BLACK;
                }

                egui::ComboBox::from_id_salt("channels")
                    .icon(blank_icon)
                    .selected_text(RichText::new(
                        state
                            .persistent_settings
                            .current_channel
                            .to_string()
                            .to_uppercase(),
                    ))
                    .show_ui(ui, |ui| {
                        for channel in ColorChannel::iter() {
                            let r = ui.selectable_value(
                                &mut state.persistent_settings.current_channel,
                                channel,
                                RichText::new(channel.to_string().to_uppercase()),
                            );

                            if tooltip(
                                r,
                                &channel.to_string(),
                                &channel.hotkey(&state.persistent_settings.shortcuts),
                                ui,
                            )
                            .clicked()
                            {
                                changed_channels = true;
                            }
                        }
                    });
            });
        }

        if changed_channels && state.current_image.is_some() {
            state
                .current_texture
                .update_color_selection(gfx, &state.persistent_settings);
        }

        let label_rect = ui.ctx().available_rect().shrink(50.);

        // TODO Center toast to image viewing area (Shift to the left / Right if the info or edit panel gets opened)
        if state.persistent_settings.current_channel != ColorChannel::Rgba {
            let mut job = LayoutJob::simple(
                format!(
                    "Viewing {} channel. Press '{}' to revert.",
                    state.persistent_settings.current_channel,
                    ColorChannel::Rgba.hotkey(&state.persistent_settings.shortcuts)
                ),
                FontId::proportional(13.),
                ui.style().visuals.text_color(),
                1000.,
            );
            job.halign = Align::Center;
            let galley = ui.painter().layout_job(job);
            let tr = galley
                .rect
                .translate(label_rect.center_bottom().to_vec2())
                .expand(8.);
            ui.painter().rect_filled(
                tr,
                ui.get_rounding(BUTTON_HEIGHT_SMALL),
                ui.style().visuals.extreme_bg_color.gamma_multiply(0.7),
            );
            ui.painter()
                .galley(label_rect.center_bottom(), galley, Color32::RED);
        }

        if state.current_image.is_some() && window_x > ui.cursor().left() + 80. {
            if tooltip(
                // ui.checkbox(&mut state.info_enabled, "ℹ Info"),
                unframed_button_colored(INFO, state.persistent_settings.info_enabled, ui),
                "Show image info",
                &lookup(&state.persistent_settings.shortcuts, &InfoMode),
                ui,
            )
            .clicked()
            {
                state.persistent_settings.info_enabled = !state.persistent_settings.info_enabled;
            }
            if window_x > ui.cursor().left() + 80.
                && tooltip(
                    unframed_button_colored(
                        PENCIL_SIMPLE_LINE,
                        state.persistent_settings.edit_enabled,
                        ui,
                    ),
                    "Edit the image",
                    &lookup(&state.persistent_settings.shortcuts, &EditMode),
                    ui,
                )
                .clicked()
            {
                state.persistent_settings.edit_enabled = !state.persistent_settings.edit_enabled;
            }
        }

        if window_x > ui.cursor().left() + 80.
            && tooltip(
                unframed_button(ARROWS_OUT_SIMPLE, ui),
                "Toggle fullscreen",
                &lookup(&state.persistent_settings.shortcuts, &Fullscreen),
                ui,
            )
            .clicked()
        {
            toggle_fullscreen(app, state);
        }

        if window_x > ui.cursor().left() + 80.
            && tooltip(
                unframed_button_colored(ARROW_LINE_UP, state.always_on_top, ui),
                "Always on top",
                &lookup(&state.persistent_settings.shortcuts, &AlwaysOnTop),
                ui,
            )
            .clicked()
        {
            state.always_on_top = !state.always_on_top;
            app.window().set_always_on_top(state.always_on_top);
        }

        if state.current_path.is_some() && window_x > ui.cursor().left() + 80. {
            let delete_text = format!(
                "Are you sure you want to move {} to the trash?",
                state
                    .current_path
                    .clone()
                    .unwrap_or_default()
                    .file_name()
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default()
            );

            let modal = Modal::new("delete", ui.ctx());
            modal.show(delete_text, |_| {
                delete_file(state);
            });

            if tooltip(
                unframed_button(TRASH, ui),
                "Move file to trash",
                &lookup(&state.persistent_settings.shortcuts, &DeleteFile),
                ui,
            )
            .clicked()
            {
                modal.open();
            }
        }

        if state.current_texture.get().is_some()
            && window_x > ui.cursor().left() + 80.
            && tooltip(
                unframed_button(PLACEHOLDER, ui),
                "Clear image",
                &lookup(&state.persistent_settings.shortcuts, &ClearImage),
                ui,
            )
            .clicked()
        {
            clear_image(state);
        }

        if state.scrubber.len() > 1 && window_x > ui.cursor().left() {
            // TODO: Check if wrap is off and we are at first image
            if tooltip(
                unframed_button(CARET_LEFT, ui),
                "Previous image",
                &lookup(&state.persistent_settings.shortcuts, &PreviousImage),
                ui,
            )
            .clicked()
            {
                prev_image(state)
            }
            // TODO: Check if wrap is off and we are at last image
            if tooltip(
                unframed_button(CARET_RIGHT, ui),
                "Next image",
                &lookup(&state.persistent_settings.shortcuts, &NextImage),
                ui,
            )
            .clicked()
            {
                next_image(state)
            }
        }

        if state.current_path.is_some() && !state.is_loaded {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::default());
                ui.label(format!(
                    "Loading {}",
                    state
                        .current_path
                        .as_ref()
                        .map(|p| p.file_name().unwrap_or_default())
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default()
                ));
            });
            app.window().request_frame();
        }

        drag_area(ui, state, app);

        ui.add_space(ui.available_width() - ICON_SIZE * 2. - ICON_SIZE / 2.);

        if unframed_button(FOLDER, ui)
            .on_hover_text("Browse for an image")
            .clicked()
        {
            #[cfg(feature = "file_open")]
            browse_for_image_path(state);
            #[cfg(not(feature = "file_open"))]
            ui.ctx().memory_mut(|w| w.open_popup(Id::new("OPEN")));
        }

        draw_hamburger_menu(ui, state, app);
    });
}

pub fn draw_hamburger_menu(ui: &mut Ui, state: &mut OculanteState, app: &mut App) {
    use crate::shortcuts::InputEvent::*;

    ui.scope(|ui| {
        // maybe override font size?
        ui.style_mut().visuals.button_frame = false;
        ui.style_mut().visuals.widgets.inactive.expansion = 20.;
        ui.style_mut().override_text_style = Some(egui::TextStyle::Heading);

        ui.menu_button(RichText::new(LIST).size(ICON_SIZE), |ui| {
            if ui.styled_button(format!("{MOVE} Reset view")).clicked() {
                state.reset_image = true;
                ui.close_menu();
            }

            if ui.styled_button(format!("{FRAME} View 1:1")).clicked() {
                set_zoom(
                    1.0,
                    Some(nalgebra::Vector2::new(
                        app.window().width() as f32 / 2.,
                        app.window().height() as f32 / 2.,
                    )),
                    state,
                );
                ui.close_menu();
            }

            let copy_pressed = key_pressed(app, state, Copy);
            if let Some(img) = &state.current_image {
                if ui
                    .styled_button(format!("{COPY} Copy"))
                    .on_hover_text("Copy image to clipboard")
                    .clicked()
                    || copy_pressed
                {
                    clipboard_copy(img);
                    ui.close_menu();
                }
            }

            if ui
                .styled_button(format!("{CLIPBOARD} Paste"))
                .on_hover_text("Paste image from clipboard")
                .clicked()
                || key_pressed(app, state, Paste)
            {
                match clipboard_to_image() {
                    Ok(img) => {
                        state.current_path = None;
                        // Stop in the event that an animation is running
                        state.player.stop();
                        _ = state
                            .player
                            .image_sender
                            .send(crate::utils::Frame::new_still(img));
                        // Since pasted data has no path, make sure it's not set
                        state.send_message_info("Image pasted");
                    }
                    Err(e) => state.send_message_err(&e.to_string()),
                }
                ui.close_menu();
            }

            if ui.styled_button(format!("{GEAR} Preferences")).clicked() {
                state.settings_enabled = !state.settings_enabled;
                ui.close_menu();
            }

            if ui.styled_button(format!("{EXIT} Quit")).clicked() {
                app.backend.exit();
            }

            ui.styled_menu_button(format!("{CLOCK} Recent"), |ui| {
                let r = ui.max_rect();

                let recent_rect = Rect::from_two_pos(
                    Pos2::new(r.right_bottom().x + 100., r.left_top().y),
                    Pos2::new(r.left_bottom().x, r.left_top().y + 0.),
                );

                let panel_bg_color = match ui.style().visuals.dark_mode {
                    true => Color32::from_gray(31),
                    false => Color32::from_gray(247),
                };

                // FIXME: This overflows
                ui.allocate_new_ui(UiBuilder::new().max_rect(recent_rect), |ui| {
                    let mut max = 0;
                    for r in &state.volatile_settings.recent_images.clone() {
                        if let Some(filename) = r.file_stem() {
                            max = filename.len().max(max)
                        }
                    }

                    for r in &state.volatile_settings.recent_images.clone() {
                        let ext = r
                            .extension()
                            .map(|e| e.to_string_lossy().to_string())
                            .unwrap_or_default()
                            .to_uppercase();

                        ui.horizontal(|ui| {
                            egui::Frame::new()
                                .fill(panel_bg_color)
                                .corner_radius(ui.style().visuals.widgets.active.corner_radius)
                                .inner_margin(Margin::same(6))
                                .show(ui, |ui| {
                                    // ui.vertical_centered_justified(|ui| {

                                    let (_, icon_rect) = ui.allocate_space(Vec2::splat(28.));

                                    ui.painter().rect(
                                        icon_rect,
                                        ui.get_rounding(BUTTON_HEIGHT_SMALL),
                                        ui.style().visuals.selection.bg_fill.gamma_multiply(0.1),
                                        Stroke::NONE,
                                        StrokeKind::Inside,
                                    );

                                    ui.painter().text(
                                        icon_rect.center(),
                                        Align2::CENTER_CENTER,
                                        ext,
                                        FontId::proportional(10.),
                                        ui.style().visuals.selection.bg_fill.gamma_multiply(0.8),
                                    );

                                    if let Some(filename) = r.file_stem() {
                                        let res = ui.add(
                                            egui::Button::new(filename.to_string_lossy())
                                                .min_size(vec2(max as f32 * 10., 0.)),
                                        );

                                        // let res = ui.button(filename.to_string_lossy());
                                        if res.clicked() {
                                            load_image_from_path(r, state);
                                            ui.close_menu();
                                        }
                                    }
                                    // });
                                });
                        });
                    }
                });
            });
        });
    });
}
