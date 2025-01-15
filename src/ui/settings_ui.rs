use super::*;
use crate::appstate::OculanteState;
use crate::utils::*;
#[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
use notan::egui::*;

pub fn settings_ui(app: &mut App, ctx: &Context, state: &mut OculanteState, _gfx: &mut Graphics) {
    #[derive(Debug, PartialEq)]
    enum SettingsState {
        General,
        Visual,
        Input,
        Debug,
        None,
    }

    fn configuration_item_ui<R>(
        title: &str,
        description: &str,
        add_contents: impl FnOnce(&mut Ui) -> R,
        ui: &mut Ui,
    ) {
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(RichText::new(title)).selectable(false));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.scope(add_contents);
            });
        });
        ui.add(
            egui::Label::new(
                RichText::new(description)
                    .small()
                    .color(ui.style().visuals.weak_text_color()),
            )
            .selectable(false),
        );
        ui.add_space(14.);
    }

    let mut settings_enabled = state.settings_enabled;
    egui::Window::new("Preferences")
            .collapsible(false)
            .open(&mut settings_enabled)
            .resizable(true)
            .default_width(600.)
            .show(ctx, |ui| {

                let mut scroll_to = SettingsState::None;

                ui.horizontal(|ui|{
                    ui.vertical(|ui| {
                        if ui.styled_button(format!("{OPTIONS} General")).clicked() {
                            scroll_to = SettingsState::General;
                        }
                        if ui.styled_button(format!("{DISPLAY} Visual")).clicked() {
                            scroll_to = SettingsState::Visual;
                        }
                        if ui.styled_button(format!("{MOUSE} Input")).clicked() {
                            scroll_to = SettingsState::Input;
                        }
                        if ui.styled_button(format!("{DEBUG} Debug")).clicked() {
                            scroll_to = SettingsState::Debug;
                        }
                    });

                    dark_panel(ui, |ui| {
                        // ui.add_space(ui.available_width());
                        egui::ScrollArea::vertical().auto_shrink([false,false]).min_scrolled_height(400.).min_scrolled_width(400.).show(ui, |ui| {

                            ui.vertical(|ui| {

                                let general = ui.heading("General");
                                if SettingsState::General == scroll_to {
                                    general.scroll_to_me(Some(Align::TOP));
                                }
                                light_panel(ui, |ui| {

                                    configuration_item_ui("Vsync", "VSync eliminates tearing and saves CPU usage. Toggling VSync off will make some operations such as panning and zooming snappier. A restart is required to take effect.", |ui| {
                                        ui.styled_checkbox(&mut state.persistent_settings.vsync, "");
                                    }, ui);

                                    configuration_item_ui("Show index slider", "Enables an index slider to quickly scrub through lots of images.", |ui| {
                                        ui.styled_checkbox(&mut state.persistent_settings.show_scrub_bar, "")
                                        ;
                                    }, ui);

                                    configuration_item_ui("Wrap images at folder boundaries", "Repeats the current directory when you move past the first or last file in the current directory.", |ui| {
                                        if ui
                                        .styled_checkbox(&mut state.persistent_settings.wrap_folder, "")
                                        .changed()
                                        {
                                            state.scrubber.wrap = state.persistent_settings.wrap_folder;
                                        }
                                    }, ui);

                                    configuration_item_ui("Number of images to cache", "Keeps this many images in memory for faster opening.", |ui| {
                                        if ui
                                        .add(egui::DragValue::new(&mut state.persistent_settings.max_cache).clamp_range(0..=10000))
                                        .changed()
                                        {
                                            state.player.cache.cache_size = state.persistent_settings.max_cache;
                                            state.player.cache.clear();
                                        }
                                    }, ui);

                                    configuration_item_ui("Do not reset image view", "When a new image is loaded, keep the current zoom and offset.", |ui| {
                                        ui.styled_checkbox(&mut state.persistent_settings.keep_view, "");
                                    }, ui);

                                    configuration_item_ui("Keep image edits", "When a new image is loaded, keep current edits on the previously edited image.", |ui| {
                                        ui.styled_checkbox(&mut state.persistent_settings.keep_edits, "");
                                    }, ui);

                                    configuration_item_ui("Redraw every frame", "Turns off optimisations and redraws everything each frame. This will consume more CPU but gives you instant feedback if new images come in or if modifications are made. A restart is required to take effect.", |ui| {
                                        if ui.styled_checkbox(&mut state.persistent_settings.force_redraw, "").changed(){
                                            app.window().set_lazy_loop(!state.persistent_settings.force_redraw);
                                        }
                                    }, ui);

                                    configuration_item_ui("Use mipmaps", "When zooming out, less memory will be used. Faster performance, but blurry.", |ui| {
                                        if ui.styled_checkbox(&mut state.persistent_settings.use_mipmaps, "").changed(){
                                            state.send_frame(crate::utils::Frame::UpdateTexture);
                                        }
                                    }, ui);

                                    configuration_item_ui("Fit image on window resize", "When you resize the main window, do you want to fit the image with it?", |ui| {
                                        ui.styled_checkbox(&mut state.persistent_settings.fit_image_on_window_resize, "");
                                    }, ui);

                                    configuration_item_ui("Zoom multiplier", "Multiplier of zoom when you use the mouse wheel or the trackpad.", |ui| {
                                        ui.add(egui::DragValue::new(&mut state.persistent_settings.zoom_multiplier).clamp_range(0.05..=10.0).speed(0.01));
                                    }, ui);

                                    #[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
                                    configuration_item_ui("Borderless mode", "Don't draw OS window decorations. A restart is required to take effect.", |ui| {
                                        ui.styled_checkbox(&mut state.persistent_settings.borderless, "");
                                    }, ui);

                                    configuration_item_ui("Minimum window size", "Set the minimum size of the main window.", |ui| {
                                        ui.horizontal(|ui| {
                                            ui.add(egui::DragValue::new(&mut state.persistent_settings.min_window_size.0).clamp_range(1..=2000).prefix("x : ").speed(0.01));
                                            ui.add(egui::DragValue::new(&mut state.persistent_settings.min_window_size.1).clamp_range(1..=2000).prefix("y : ").speed(0.01));
                                        });

                                    }, ui);

                                    #[cfg(feature = "update")]
                                    configuration_item_ui("Check for updates", "Check and install the latest update if available. A restart is required to use a newly installed version.", |ui| {
                                        if ui.button("Check").clicked() {
                                            state.send_message_info("Checking for updates...");
                                            crate::update::update(Some(state.message_channel.0.clone()));
                                            state.settings_enabled = false;
                                        }
                                    }, ui);

                                    configuration_item_ui("Visit GitHub Repository", "Check out the source code, request a feature, submit a bug, or leave a star if you like it!", |ui| {
                                        if ui.link("Check it out!").on_hover_text("https://github.com/woelper/oculante").clicked() {
                                            _ = webbrowser::open("https://github.com/woelper/oculante");
                                        }
                                    }, ui);

                                    configuration_item_ui("Reset all settings", "Reset Oculante to default", |ui| {
                                        if ui.button("Reset").clicked() {
                                            state.persistent_settings = Default::default();
                                            apply_theme(state, ctx);
                                        }
                                    }, ui);


                                });

                                let visual = ui.heading("Visual");
                                if SettingsState::Visual == scroll_to {
                                    visual.scroll_to_me(Some(Align::TOP));
                                }
                                light_panel(ui, |ui| {
                                    configuration_item_ui("Color theme", "Customize look and feel", |ui| {
                                        egui::ComboBox::from_id_source("Color theme")
                                        .selected_text(format!("{:?}", state.persistent_settings.theme))
                                        .show_ui(ui, |ui| {
                                            let mut r = ui.selectable_value(&mut state.persistent_settings.theme, ColorTheme::Dark, "Dark");
                                            if ui.selectable_value(&mut state.persistent_settings.theme, ColorTheme::Light, "Light").changed() {
                                                r.mark_changed();
                                            }
                                            if ui.selectable_value(&mut state.persistent_settings.theme, ColorTheme::System, "Same as system").clicked() {
                                                r.mark_changed();
                                            }

                                            if r.changed() {
                                                apply_theme(state, ctx);
                                            }
                                        });
                                    }, ui);

                                   configuration_item_ui("Accent color", "Customize the primary color used in the UI", |ui| {
                                        if ui
                                        .color_edit_button_srgb(&mut state.persistent_settings.accent_color)
                                        .changed()
                                        {
                                            apply_theme(state, ctx);
                                        }
                                    }, ui);

                                    // This does not work, it just scales the texture in Notan.
                                    // https://docs.rs/egui/latest/egui/struct.Context.html#method.set_zoom_factor
                                    // configuration_item_ui("Zoom", "UI global zoom factor", |ui| {
                                    //     let mut z = ui.ctx().zoom_factor();
                                    //     if ui.add(egui::DragValue::new(&mut z).clamp_range(0.1..=4.0).speed(0.01)).changed() {
                                    //         ui.ctx().set_zoom_factor(z);
                                    //     }
                                    // }, ui);

                                    configuration_item_ui("Background color", "The color used as a background for images", |ui| {
                                        ui.color_edit_button_srgb(&mut state.persistent_settings.background_color);
                                    }, ui);

                                    configuration_item_ui("Transparency Grid", "Replaces image transparency with a checker background.", |ui| {
                                        ui.styled_checkbox(&mut state.persistent_settings.show_checker_background, "");
                                    }, ui);

                                    configuration_item_ui("Draw frame around image", "Draw a small frame around the image. It is centered on the outmost pixel. This can be helpful on images with lots of transparency.", |ui| {
                                        ui
                                        .styled_checkbox(&mut state.persistent_settings.show_frame, "");
                                    }, ui);

                                    configuration_item_ui("Interpolate when zooming in", "When zooming in, do you prefer to see individual pixels or an interpolation?", |ui| {
                                        if ui.styled_checkbox(&mut state.persistent_settings.linear_mag_filter, "").changed(){
                                            state.send_frame(crate::utils::Frame::UpdateTexture);
                                        }
                                    }, ui);

                                    configuration_item_ui("Interpolate when zooming out", "When zooming out, do you prefer crisper or smoother pixels?", |ui| {
                                        if ui.styled_checkbox(&mut state.persistent_settings.linear_min_filter, "").changed() {
                                            state.send_frame(crate::utils::Frame::UpdateTexture);
                                        }
                                    }, ui);

                                    configuration_item_ui("Zen mode", "Hides all UI and fits images to the frame.", |ui| {
                                        if ui.styled_checkbox(&mut state.persistent_settings.zen_mode, "").changed(){
                                            set_title(app, state);
                                        }
                                    }, ui);


                                    // TODO: add more options here
                                    ui.horizontal(|ui| {
                                        ui.label("Window title");
                                        if ui
                                        .text_edit_singleline(&mut state.persistent_settings.title_format)
                                        .on_hover_text(
                                            "Configures the window title. Valid options are: {APP}, {VERSION}, {FULLPATH}, {FILENAME}, and {RES}",
                                        )
                                        .changed()
                                        {
                                            set_title(app, state);
                                        }
                                    });

                                });


                                let input = ui.heading("Input");
                                if SettingsState::Input == scroll_to {
                                    input.scroll_to_me(Some(Align::TOP));
                                }
                                light_panel(ui, |ui| {
                                    keybinding_ui(app, state, ui);
                                });

                                let debug = ui.heading("Debug");
                                if SettingsState::Debug == scroll_to {
                                    debug.scroll_to_me(Some(Align::TOP));
                                }
                                light_panel(ui, |ui| {
                                    #[cfg(debug_assertions)]
                                    configuration_item_ui("Send test message", "Send some messages", |ui| {
                                        if ui.button("Info").clicked() {
                                            state.send_message_info("Test");
                                        }
                                        if ui.button("Warn").clicked() {
                                            state.send_message_warn("Test");
                                        }
                                        if ui.button("Err").clicked() {
                                            state.send_message_err("Test");
                                        }
                                    }, ui);

                                    configuration_item_ui("Enable experimental features", "Turn on features that are not yet finished", |ui| {
                                        ui.styled_checkbox(&mut state.persistent_settings.experimental_features, "");
                                    }, ui);
                                });
                            });
                        });
                    });
                });




            });
    state.settings_enabled = settings_enabled;
}

fn keybinding_ui(app: &mut App, state: &mut OculanteState, ui: &mut Ui) {
    // Make sure no shortcuts are received by the application
    state.key_grab = true;

    let no_keys_pressed = app.keyboard.down.is_empty();

    ui.horizontal(|ui| {
        ui.label_unselectable("While this is open, regular shortcuts will not work.");
        if no_keys_pressed {
            ui.label_unselectable(
                egui::RichText::new("Please press & hold a key").color(Color32::RED),
            );
        }
    });

    let k = app
        .keyboard
        .down
        .iter()
        .map(|k| format!("{:?}", k.0))
        .collect::<BTreeSet<String>>();

    let s = state.persistent_settings.shortcuts.clone();
    let mut ordered_shortcuts = state
        .persistent_settings
        .shortcuts
        .iter_mut()
        .collect::<Vec<_>>();
    ordered_shortcuts.sort_by(|a, b| a.0.partial_cmp(b.0).unwrap_or(std::cmp::Ordering::Equal));

    egui::Grid::new("info").num_columns(4).show(ui, |ui| {
        for (event, keys) in ordered_shortcuts {
            ui.label_unselectable(format!("{event:?}"));
            ui.label_unselectable(lookup(&s, event));
            ui.add_space(200.);
            if !no_keys_pressed {
                if ui
                    .button(format!("Assign {}", keypresses_as_string(&k)))
                    .clicked()
                {
                    *keys = app
                        .keyboard
                        .down
                        .iter()
                        .map(|(k, _)| format!("{k:?}"))
                        .collect();
                }
            } else {
                ui.add_enabled(false, egui::Button::new("Press key(s)..."));
            }
            ui.end_row();
        }
    });
}
