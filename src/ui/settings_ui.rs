use std::fs::remove_dir_all;
use std::sync::{Arc, Mutex};

use super::*;
use crate::appstate::OculanteState;
use crate::thumbnails::get_disk_cache_path;
use crate::{settings, utils::*};
#[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
use notan::egui::*;

pub fn settings_ui(app: &mut App, ctx: &Context, state: &mut OculanteState, _gfx: &mut Graphics) {
    #[derive(Debug, PartialEq)]
    enum SettingsState {
        General,
        Visual,
        Input,
        Debug,
        Decoders,
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

    let config_state = ctx.data_mut(|w| {
        let ui_state = w.get_temp_mut_or_insert_with::<Arc<Mutex<SettingsUiState>>>(
            Id::new("SETTINGSUISTATE"),
            || {
                let mut config_state = SettingsUiState::default();
                let settings::HeifLimits {
                    image_size_pixels,
                    number_of_tiles,
                    bayer_pattern_pixels,
                    items,
                    color_profile_size,
                    memory_block_size,
                    components,
                    iloc_extents_per_item,
                    size_entity_group,
                    children_per_box,
                    ..
                } = state.persistent_settings.decoders.heif;

                config_state.heif_image_size = image_size_pixels.to_string();
                config_state.heif_tiles = number_of_tiles.to_string();
                config_state.heif_bayer_pat = bayer_pattern_pixels.to_string();
                config_state.heif_items = items.to_string();
                config_state.heif_color_prof = color_profile_size.to_string();
                config_state.heif_mem_block = memory_block_size.to_string();
                config_state.heif_components = components.to_string();
                config_state.heif_iloc_extents = iloc_extents_per_item.to_string();
                config_state.heif_size_entity = size_entity_group.to_string();
                config_state.heif_child_per_box = children_per_box.to_string();

                Mutex::new(config_state).into()
            },
        );
        Arc::clone(ui_state)
    });

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
                        if ui.styled_button(format!("{DECODER} Decoders")).clicked() {
                            scroll_to = SettingsState::Decoders;
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

                                    configuration_item_ui("VSync", "VSync eliminates tearing and saves CPU usage. Toggling VSync off will make some operations such as panning and zooming snappier. A restart is required to take effect.", |ui| {
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
                                        .add(egui::DragValue::new(&mut state.persistent_settings.max_cache).range(0..=10000))
                                        .changed()
                                        {
                                            state.player.cache.cache_size = state.persistent_settings.max_cache;
                                            state.player.cache.clear();
                                        }
                                    }, ui);

                                    configuration_item_ui(
                                        "Number of recent images",
                                        "Remember this many recently opened images.",
                                        |ui| {
                                            if ui.add(
                                                egui::DragValue::new(&mut state.persistent_settings.max_recents)
                                                    .range(0..=12),
                                            )
                                            .changed() 
                                            {
                                                state
                                                    .volatile_settings
                                                    .recent_images
                                                    .truncate(state.persistent_settings.max_recents.into());
                                            }
                                        },
                                        ui,
                                    );
                                    
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

                                    configuration_item_ui("Fit image on window resize", "Fits the image to the window while resizing.", |ui| {
                                        ui.styled_checkbox(&mut state.persistent_settings.fit_image_on_window_resize, "");
                                    }, ui);

                                    configuration_item_ui("Zoom multiplier", "Multiplier of how fast the image will change size when using your mouse wheel or trackpad.", |ui| {
                                        ui.add(egui::DragValue::new(&mut state.persistent_settings.zoom_multiplier).range(0.05..=10.0).speed(0.01));
                                    }, ui);

                                    configuration_item_ui(
                                        "Auto scale to fit",
                                        "Automatically scale images up to fit the window.",
                                        |ui| {
                                            ui.styled_checkbox(&mut state.persistent_settings.auto_scale, "");
                                        },
                                        ui,
                                    );

                                    #[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
                                    configuration_item_ui("Borderless mode", "Prevents drawing OS window decorations. A restart is required to take effect.", |ui| {
                                        ui.styled_checkbox(&mut state.persistent_settings.borderless, "");
                                    }, ui);

                                    configuration_item_ui("Minimum window size", "Set the minimum size of the main window.", |ui| {
                                        ui.horizontal(|ui| {
                                            ui.add(egui::DragValue::new(&mut state.persistent_settings.min_window_size.0).range(1..=2000).prefix("x : ").speed(0.01));
                                            ui.add(egui::DragValue::new(&mut state.persistent_settings.min_window_size.1).range(1..=2000).prefix("y : ").speed(0.01));
                                        });

                                    }, ui);

                                    #[cfg(feature = "update")]
                                    configuration_item_ui("Check for updates", "Check for updates and install the latest update if available. A restart is required to use a newly installed version.", |ui| {
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

                                    configuration_item_ui("Reset all settings", "Reset Oculante to default settings.", |ui| {
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
                                    configuration_item_ui("Color theme", "Customize the look and feel.", |ui| {
                                        egui::ComboBox::from_id_salt("Color theme")
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

                                   configuration_item_ui("Accent color", "Customize the primary color used in the UI.", |ui| {
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

                                    configuration_item_ui("Background color", "The color used as a background for images.", |ui| {
                                        ui.color_edit_button_srgb(&mut state.persistent_settings.background_color);
                                    }, ui);

                                    configuration_item_ui("Transparency Grid", "Replaces image transparency with a checker background.", |ui| {
                                        ui.styled_checkbox(&mut state.persistent_settings.show_checker_background, "");
                                    }, ui);

                                    configuration_item_ui("Draw frame around image", "Draws a frame around images which can help see its edges when there are many transparent areas. It is centered on the outmost pixel.", |ui| {
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
                                            "Configures the window title. Valid options are: {APP}, {VERSION}, {FULLPATH}, {FILENAME}, {NUM}, and {RES}.",
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

                                let decoders = ui.heading("Decoders");
                                if SettingsState::Decoders == scroll_to {
                                    decoders.scroll_to_me(Some(Align::TOP));
                                }
                                light_panel(ui, |ui| {
                                    configuration_item_ui(
                                        "HEIF security override",
                                        "Disable all HEIF security limits. A restart is required to take effect.",
                                        |ui| {
                                            ui.styled_checkbox(
                                                &mut state.persistent_settings.decoders.heif.override_all,
                                                // Keeping this commented for now to not break design consistency. This is useful in the future if we need to warn users this option can use a lot of ram.
                                                //"Disable security limits"
                                                ""
                                            );
                                        },
                                        ui
                                    );

                                    configuration_item_ui(
                                        "HEIF max image size",
                                        "Sets the maximum image size in pixels that libheif will decode (0 = unlimited). A restart is required to take effect.",
                                        |ui| {
                                            let mut config_state = config_state.lock().unwrap();

                                            let response = ui.add_enabled(
                                                !state.persistent_settings.decoders.heif.override_all,
                                                TextEdit::singleline(&mut config_state.heif_image_size)
                                                    .min_size(vec2(0., BUTTON_HEIGHT_SMALL)),
                                            );
                                            if response.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                                if config_state.heif_image_size.is_empty() {
                                                    state.persistent_settings.decoders.heif.image_size_pixels =
                                                        settings::Limit::Default;
                                                } else {
                                                    let limit = config_state
                                                        .heif_image_size
                                                        .parse()
                                                        .map(settings::Limit::U64)
                                                        .unwrap_or_default();
                                                    state.persistent_settings.decoders.heif.image_size_pixels = limit;
                                                }
                                            }
                                        },
                                        ui,
                                    );

                                    configuration_item_ui(
                                        "HEIF memory block size",
                                        "Sets the max memory block size per image (0 = unlimited). A restart is required to take effect.",
                                        |ui| {
                                            let mut config_state = config_state.lock().unwrap();

                                            let response = ui.add_enabled(
                                                !state.persistent_settings.decoders.heif.override_all,
                                                TextEdit::singleline(&mut config_state.heif_mem_block)
                                                    .min_size(vec2(0., BUTTON_HEIGHT_SMALL)),
                                            );
                                            if response.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                                if config_state.heif_mem_block.is_empty() {
                                                    state.persistent_settings.decoders.heif.memory_block_size =
                                                        settings::Limit::default();
                                                } else {
                                                    let limit = config_state
                                                        .heif_mem_block
                                                        .parse()
                                                        .map(settings::Limit::U64)
                                                        .unwrap_or_default();
                                                    state.persistent_settings.decoders.heif.memory_block_size = limit;
                                                }
                                            }
                                        },
                                        ui,
                                    );

                                    configuration_item_ui(
                                        "HEIF number of tiles",
                                        "Sets the max number of tiles to attempt decoding (0 = unlimited). A restart is required to take effect.",
                                        |ui| {
                                            let mut config_state = config_state.lock().unwrap();
                                            let response = ui.add_enabled(
                                                !state.persistent_settings.decoders.heif.override_all,
                                                TextEdit::singleline(&mut config_state.heif_tiles)
                                                    .min_size(vec2(0., BUTTON_HEIGHT_SMALL)),
                                            );
                                            if response.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                                if config_state.heif_tiles.is_empty() {
                                                    state.persistent_settings.decoders.heif.number_of_tiles =
                                                        settings::Limit::default();
                                                    } else {
                                                        let limit = config_state
                                                            .heif_tiles
                                                            .parse()
                                                            .map(settings::Limit::U32)
                                                            .unwrap_or_default();
                                                        state.persistent_settings.decoders.heif.number_of_tiles = limit;
                                                    }
                                            }
                                        },
                                        ui,
                                    );

                                    configuration_item_ui(
                                        "HEIF bayer pattern pixels",
                                        "Sets the max number of bayer pattern pixels (0 = unlimited). A restart is required to take effect.",
                                        |ui| {
                                            let mut config_state = config_state.lock().unwrap();
                                            let response = ui.add_enabled(
                                                !state.persistent_settings.decoders.heif.override_all,
                                                TextEdit::singleline(&mut config_state.heif_bayer_pat)
                                                    .min_size(vec2(0., BUTTON_HEIGHT_SMALL)),
                                            );
                                            if response.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                                if config_state.heif_bayer_pat.is_empty() {
                                                state.persistent_settings.decoders.heif.bayer_pattern_pixels =
                                                    settings::Limit::default();
                                                } else {
                                                    let limit = config_state
                                                        .heif_bayer_pat
                                                        .parse()
                                                        .map(settings::Limit::U32)
                                                        .unwrap_or_default();
                                                    state.persistent_settings.decoders.heif.bayer_pattern_pixels = limit;
                                                }
                                            }
                                        },
                                        ui,
                                    );

                                    configuration_item_ui(
                                        "HEIF items",
                                        "Set the max number of image items (0 = unlimited). A restart is required to take effect.",
                                        |ui| {
                                            let mut config_state = config_state.lock().unwrap();
                                            let response = ui.add_enabled(
                                                !state.persistent_settings.decoders.heif.override_all,
                                                TextEdit::singleline(&mut config_state.heif_items)
                                                    .min_size(vec2(0., BUTTON_HEIGHT_SMALL)),
                                            );
                                            if response.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                                if config_state.heif_items.is_empty() {
                                                    state.persistent_settings.decoders.heif.items = settings::Limit::default();
                                                } else {
                                                    let limit = config_state
                                                        .heif_items
                                                        .parse()
                                                        .map(settings::Limit::U32)
                                                        .unwrap_or_default();
                                                    state.persistent_settings.decoders.heif.items = limit;
                                                }
                                            }
                                        },
                                        ui,
                                    );

                                    configuration_item_ui(
                                        "HEIF color profile size",
                                        "Set the max color profile size (0 = unlimited). A restart is required to take effect.",
                                        |ui| {
                                            let mut config_state = config_state.lock().unwrap();
                                            let response = ui.add_enabled(
                                                !state.persistent_settings.decoders.heif.override_all,
                                                TextEdit::singleline(&mut config_state.heif_color_prof)
                                                    .min_size(vec2(0., BUTTON_HEIGHT_SMALL)),
                                            );
                                            if response.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                                if config_state.heif_color_prof.is_empty() {
                                                    state.persistent_settings.decoders.heif.color_profile_size =
                                                        settings::Limit::default();
                                                } else {
                                                    let limit = config_state
                                                        .heif_color_prof
                                                        .parse()
                                                        .map(settings::Limit::U32)
                                                        .unwrap_or_default();
                                                    state.persistent_settings.decoders.heif.color_profile_size = limit;
                                                }
                                            }
                                        },
                                        ui,
                                    );

                                configuration_item_ui(
                                    "HEIF components",
                                    "Set the max number of components to decode (0 = unlimited). A restart is required to take effect.",
                                    |ui| {
                                        let mut config_state = config_state.lock().unwrap();
                                        let response = ui.add_enabled(
                                            !state.persistent_settings.decoders.heif.override_all,
                                            TextEdit::singleline(&mut config_state.heif_components)
                                                .min_size(vec2(0., BUTTON_HEIGHT_SMALL)),
                                        );
                                        if response.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                            if config_state.heif_components.is_empty() {
                                                state.persistent_settings.decoders.heif.components =
                                                    settings::Limit::default();
                                            } else {
                                                let limit = config_state
                                                    .heif_components
                                                    .parse()
                                                    .map(settings::Limit::U32)
                                                    .unwrap_or_default();
                                                state.persistent_settings.decoders.heif.components = limit;
                                            }
                                        }
                                    },
                                    ui,
                                );

                                configuration_item_ui(
                                    "HEIF iloc extents",
                                    "Set the max number of image locations (0 = unlimited). A restart is required to take effect.",
                                    |ui| {
                                        let mut config_state = config_state.lock().unwrap();
                                        let response = ui.add_enabled(
                                            !state.persistent_settings.decoders.heif.override_all,
                                            TextEdit::singleline(&mut config_state.heif_iloc_extents)
                                                .min_size(vec2(0., BUTTON_HEIGHT_SMALL)),
                                        );
                                        if response.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                            if config_state.heif_iloc_extents.is_empty() {
                                                state
                                                    .persistent_settings
                                                    .decoders
                                                    .heif
                                                    .iloc_extents_per_item = settings::Limit::default();
                                            } else {
                                                let limit = config_state
                                                    .heif_iloc_extents
                                                    .parse()
                                                    .map(settings::Limit::U32)
                                                    .unwrap_or_default();
                                                state
                                                    .persistent_settings
                                                    .decoders
                                                    .heif
                                                    .iloc_extents_per_item = limit;
                                            }
                                        }
                                    },
                                    ui,
                                );

                                configuration_item_ui(
                                    "HEIF entity group size",
                                    "Set the max entity group size (0 = unlimited). A restart is required to take effect.",
                                    |ui| {
                                        let mut config_state = config_state.lock().unwrap();
                                        let response = ui.add_enabled(
                                            !state.persistent_settings.decoders.heif.override_all,
                                            TextEdit::singleline(&mut config_state.heif_size_entity)
                                                .min_size(vec2(0., BUTTON_HEIGHT_SMALL)),
                                        );
                                        if response.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                            if config_state.heif_size_entity.is_empty() {
                                                state.persistent_settings.decoders.heif.size_entity_group =
                                                    settings::Limit::default();
                                            } else {
                                                let limit = config_state
                                                    .heif_size_entity
                                                    .parse()
                                                    .map(settings::Limit::U32)
                                                    .unwrap_or_default();
                                                state.persistent_settings.decoders.heif.size_entity_group = limit;
                                            }
                                        }
                                    },
                                    ui,
                                );

                                configuration_item_ui(
                                    "HEIF children per box",
                                    "Set the max number of metadata per box (0 = unlimited). A restart is required to take effect.",
                                    |ui| {
                                        let mut config_state = config_state.lock().unwrap();
                                        let response = ui.add_enabled(
                                            !state.persistent_settings.decoders.heif.override_all,
                                            TextEdit::singleline(&mut config_state.heif_child_per_box)
                                                .min_size(vec2(0., BUTTON_HEIGHT_SMALL)),
                                        );
                                        if response.lost_focus() || ui.input(|i| i.key_pressed(Key::Enter)) {
                                            if config_state.heif_child_per_box.is_empty() {
                                                state.persistent_settings.decoders.heif.children_per_box =
                                                    settings::Limit::default();
                                            } else {
                                                let limit = config_state
                                                    .heif_child_per_box
                                                    .parse()
                                                    .map(settings::Limit::U32)
                                                    .unwrap_or_default();
                                                state.persistent_settings.decoders.heif.children_per_box = limit;
                                            }
                                        }
                                    },
                                    ui,
                                );
                                });

                                let debug = ui.heading("Debug");
                                if SettingsState::Debug == scroll_to {
                                    debug.scroll_to_me(Some(Align::TOP));
                                }
                                light_panel(ui, |ui| {
                                    #[cfg(debug_assertions)]
                                    configuration_item_ui("Send test message", "Send some messages.", |ui| {
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

                                    configuration_item_ui("Enable experimental features", "Turn on features that are not yet finished.", |ui| {
                                        ui.styled_checkbox(&mut state.persistent_settings.experimental_features, "");
                                    }, ui);

                                    configuration_item_ui("Thumbnails", "Functionality to test thumbnail generation.", |ui| {
                                        if ui.button("Delete thumbnails").clicked() {
                                            _ = get_disk_cache_path().map(remove_dir_all);
                                        }
                                        if ui.button("Open thumbnails directory").clicked() {
                                            std::thread::spawn(||{
                                                _ = get_disk_cache_path().map(open::that);
                                            });
                                        }

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

    egui::Grid::new("info")
        .num_columns(4)
        .spacing([100.0, 10.0])
        .show(ui, |ui| {
            for (event, keys) in ordered_shortcuts {
                ui.label_unselectable(format!("{event:?}"));
                ui.label_unselectable(lookup(&s, event));
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

#[derive(Default)]
struct SettingsUiState {
    pub heif_image_size: String,
    pub heif_tiles: String,
    pub heif_bayer_pat: String,
    pub heif_items: String,
    pub heif_color_prof: String,
    pub heif_mem_block: String,
    pub heif_components: String,
    pub heif_iloc_extents: String,
    pub heif_size_entity: String,
    pub heif_child_per_box: String,
}
