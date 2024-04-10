use crate::{
    appstate::{ImageGeometry, Message, OculanteState},
    clipboard_to_image,
    image_editing::{process_pixels, Channel, GradientStop, ImageOperation, ScaleFilter},
    paint::PaintStroke,
    set_zoom,
    settings::{set_system_theme, ColorTheme},
    shortcuts::{key_pressed, keypresses_as_string, lookup, MouseWheelDirection, MouseWheelEvent},
    utils::{
        clipboard_copy, disp_col, disp_col_norm, fix_exif, highlight_bleed, highlight_semitrans,
        load_image_from_path, next_image, prev_image, send_extended_info, set_title, solo_channel,
        toggle_fullscreen, unpremult, ColorChannel, ImageExt,
    },
    FrameSource,
};
#[cfg(not(feature = "file_open"))]
use crate::{filebrowser, SUPPORTED_EXTENSIONS};

const ICON_SIZE: f32 = 24.;

use egui_phosphor::regular::*;
use egui_plot::{Plot, PlotPoints, Points};
use image::RgbaImage;
use log::{debug, error, info};
#[cfg(not(target_os = "netbsd"))]
use mouse_position::mouse_position::Mouse;
use notan::{
    egui::{self, *},
    prelude::{App, Graphics},
};
use std::{collections::BTreeSet, ops::RangeInclusive, path::PathBuf, time::Instant};
use strum::IntoEnumIterator;
const PANEL_WIDTH: f32 = 240.0;
const PANEL_WIDGET_OFFSET: f32 = 10.0;

#[cfg(feature = "turbo")]
use crate::image_editing::{cropped_range, lossless_tx};
pub trait EguiExt {
    fn label_i(&mut self, _text: &str) -> Response {
        unimplemented!()
    }

    fn label_i_selected(&mut self, _selected: bool, _text: &str) -> Response {
        unimplemented!()
    }

    fn slider_styled<Num: emath::Numeric>(
        &mut self,
        _value: &mut Num,
        _range: RangeInclusive<Num>,
    ) -> Response {
        unimplemented!()
    }

    fn slider_timeline<Num: emath::Numeric>(
        &mut self,
        _value: &mut Num,
        _range: RangeInclusive<Num>,
    ) -> Response {
        unimplemented!()
    }
}

impl EguiExt for Ui {
    /// Draw a justified icon from a string starting with an emoji
    fn label_i(&mut self, text: &str) -> Response {
        let icon = text.chars().filter(|c| !c.is_ascii()).collect::<String>();
        let description = text.chars().filter(|c| c.is_ascii()).collect::<String>();
        self.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            // self.horizontal(|ui| {
            ui.add_sized(
                egui::Vec2::new(28., ui.available_height()),
                egui::Label::new(RichText::new(icon).color(ui.style().visuals.selection.bg_fill)),
            );
            ui.label(
                RichText::new(description).color(ui.style().visuals.noninteractive().text_color()),
            );
        })
        .response
    }

    /// Draw a justified icon from a string starting with an emoji
    fn label_i_selected(&mut self, selected: bool, text: &str) -> Response {
        let icon = text.chars().filter(|c| !c.is_ascii()).collect::<String>();
        let description = text.chars().filter(|c| c.is_ascii()).collect::<String>();
        self.horizontal(|ui| {
            let mut r = ui.add_sized(
                egui::Vec2::new(30., ui.available_height()),
                egui::SelectableLabel::new(selected, RichText::new(icon)),
            );
            if ui
                .add_sized(
                    egui::Vec2::new(ui.available_width(), ui.available_height()),
                    egui::SelectableLabel::new(selected, RichText::new(description)),
                )
                .clicked()
            {
                r.clicked = [true, true, true, true, true];
            }
            r
        })
        .inner
    }

    fn slider_styled<Num: emath::Numeric>(
        &mut self,
        value: &mut Num,
        range: RangeInclusive<Num>,
    ) -> Response {
        self.scope(|ui| {
            let color = ui.style().visuals.selection.bg_fill;
            // let color = Color32::RED;
            let available_width = ui.available_width() * 0.6;
            let style = ui.style_mut();
            style.visuals.widgets.hovered.bg_fill = color;
            style.visuals.widgets.hovered.fg_stroke.width = 0.;

            style.visuals.widgets.active.bg_fill = color;
            style.visuals.widgets.active.fg_stroke.width = 0.;

            style.visuals.widgets.inactive.fg_stroke.width = 5.0;
            style.visuals.widgets.inactive.fg_stroke.color = color;
            style.visuals.widgets.inactive.rounding =
                style.visuals.widgets.inactive.rounding.at_least(20.);
            style.visuals.widgets.inactive.expansion = -5.0;

            style.spacing.slider_width = available_width;

            ui.horizontal(|ui| {
                let r = ui.add(Slider::new(value, range).show_value(false).integer());
                ui.monospace(format!("{:.0}", value.to_f64()));
                r
            })
            .inner
        })
        .inner
    }

    fn slider_timeline<Num: emath::Numeric>(
        &mut self,
        value: &mut Num,
        range: RangeInclusive<Num>,
    ) -> Response {
        self.scope(|ui| {
            let color = ui.style().visuals.selection.bg_fill;
            // let color = Color32::RED;
            let available_width = ui.available_width() * 1. - 60.;
            let style = ui.style_mut();
            style.visuals.widgets.hovered.bg_fill = color;
            style.visuals.widgets.hovered.fg_stroke.width = 0.;

            style.visuals.widgets.active.bg_fill = color;
            style.visuals.widgets.active.fg_stroke.width = 0.;

            style.visuals.widgets.inactive.fg_stroke.width = 5.0;
            style.visuals.widgets.inactive.fg_stroke.color = color;
            style.visuals.widgets.inactive.rounding =
                style.visuals.widgets.inactive.rounding.at_least(20.);
            style.visuals.widgets.inactive.expansion = -5.0;

            style.spacing.slider_width = available_width;

            ui.horizontal(|ui| {
                let r = ui.add(
                    Slider::new(value, range.clone())
                        .show_value(false)
                        .integer(),
                );
                ui.monospace(format!(
                    "{:.0}/{:.0}",
                    value.to_f64() + 1.,
                    range.end().to_f64() + 1.
                ));
                r
            })
            .inner
        })
        .inner
    }
}

/// Proof-of-concept funtion to draw texture completely with egui
#[allow(unused)]
pub fn image_ui(ctx: &Context, state: &mut OculanteState, gfx: &mut Graphics) {
    if let Some(texture) = &state.current_texture {
        let tex_id = gfx.egui_register_texture(texture);

        let image_rect = Rect::from_center_size(
            Pos2::new(
                state.image_geometry.offset.x
                    + state.image_geometry.dimensions.0 as f32 / 2. * state.image_geometry.scale,
                state.image_geometry.offset.y
                    + state.image_geometry.dimensions.1 as f32 / 2. * state.image_geometry.scale,
            ),
            vec2(
                state.image_geometry.dimensions.0 as f32,
                state.image_geometry.dimensions.1 as f32,
            ) * state.image_geometry.scale,
        );

        egui::Painter::new(ctx.clone(), LayerId::background(), ctx.available_rect()).image(
            tex_id.id,
            image_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    // state.image_geometry.scale;

    // let preview_rect = ui
    // .add(
    //     egui::Image::new(tex_id)
    //     .maintain_aspect_ratio(false)
    //     .fit_to_exact_size(egui::Vec2::splat(desired_width))
    //     .uv(egui::Rect::from_x_y_ranges(
    //         uv_center.0 - uv_size.0..=uv_center.0 + uv_size.0,
    //         uv_center.1 - uv_size.1..=uv_center.1 + uv_size.1,
    //     )),
    // )
    // .rect;
}

pub fn info_ui(ctx: &Context, state: &mut OculanteState, gfx: &mut Graphics) {
    if let Some(img) = &state.current_image {
        let mut img = img;

        // prefer edit result if present
        if state.edit_state.result_pixel_op.width() > 0 {
            img = &state.edit_state.result_pixel_op;
        }

        if let Some(p) = img.get_pixel_checked(
            state.cursor_relative.x as u32,
            state.cursor_relative.y as u32,
        ) {
            state.sampled_color = [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32];
        }
    }

    egui::SidePanel::left("side_panel")
    .max_width(PANEL_WIDTH)
    .min_width(PANEL_WIDTH/2.)
    .show(ctx, |ui| {


        egui::ScrollArea::vertical().auto_shrink([false,true])
            .show(ui, |ui| {
            if let Some(texture) = &state.current_texture {
                // texture.
                let tex_id = gfx.egui_register_texture(texture);

                // width of image widget
                // let desired_width = ui.available_width() - ui.spacing().indent;
                let desired_width = PANEL_WIDTH - PANEL_WIDGET_OFFSET;

                let scale = (desired_width / 8.) / texture.size().0;

                let uv_center = (
                    state.cursor_relative.x / state.image_geometry.dimensions.0 as f32,
                    (state.cursor_relative.y / state.image_geometry.dimensions.1 as f32),
                );

                egui::Grid::new("info").show(ui, |ui| {
                    ui.label_i(&format!("{ARROWS_OUT} Size",));

                    ui.label(
                        RichText::new(format!(
                            "{}x{}",
                            state.image_geometry.dimensions.0, state.image_geometry.dimensions.1
                        ))
                        .monospace(),
                    );
                    ui.end_row();


                    if let Some(path) = &state.current_path {
                        // make sure we truncate filenames
                        let max_chars = 18;
                        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                        let skip_symbol = if file_name.chars().count() > max_chars {".."} else {""};

                        ui.label_i(&format!("{} File", IMAGE_SQUARE));
                        let path_label = egui::Label::new(
                            RichText::new(format!(
                                "{skip_symbol}{}",
                                file_name.chars().rev().take(max_chars).collect::<String>().chars().rev().collect::<String>()
                            ))
                        ).truncate(true);

                        ui.add(path_label)
                        .on_hover_text(format!("{}", path.display()));
                        ui.end_row();
                    }

                    ui.label_i(&format!("{PALETTE} RGBA"));
                    ui.label(
                        RichText::new(disp_col(state.sampled_color))
                            .monospace()
                            .background_color(Color32::from_rgba_unmultiplied(255, 255, 255, 6)),
                    );
                    ui.end_row();

                    ui.label_i(&format!("{PALETTE} RGBA"));
                    ui.label(
                        RichText::new(disp_col_norm(state.sampled_color, 255.))
                            .monospace()
                            .background_color(Color32::from_rgba_unmultiplied(255, 255, 255, 6)),
                    );
                    ui.end_row();

                    ui.label_i("⊞ Pos");
                    ui.label(
                        RichText::new(format!(
                            "{:.0},{:.0}",
                            state.cursor_relative.x, state.cursor_relative.y
                        ))
                        .monospace()
                        .background_color(Color32::from_rgba_unmultiplied(255, 255, 255, 6)),
                    );
                    ui.end_row();

                    ui.label_i(" UV");
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


                let preview_rect = ui
                    .add(
                        egui::Image::new(tex_id)
                        .maintain_aspect_ratio(false)
                        .fit_to_exact_size(egui::Vec2::splat(desired_width))
                        .uv(egui::Rect::from_x_y_ranges(
                            uv_center.0 - uv_size.0..=uv_center.0 + uv_size.0,
                            uv_center.1 - uv_size.1..=uv_center.1 + uv_size.1,
                        )),
                    )
                    .rect;



                let stroke_color = Color32::from_white_alpha(240);
                let bg_color = Color32::BLACK.linear_multiply(0.5);
                ui.painter_at(preview_rect).line_segment(
                    [preview_rect.center_bottom(), preview_rect.center_top()],
                    Stroke::new(4., bg_color),
                );
                ui.painter_at(preview_rect).line_segment(
                    [preview_rect.left_center(), preview_rect.right_center()],
                    Stroke::new(4., bg_color),
                );
                ui.painter_at(preview_rect).line_segment(
                    [preview_rect.center_bottom(), preview_rect.center_top()],
                    Stroke::new(1., stroke_color),
                );
                ui.painter_at(preview_rect).line_segment(
                    [preview_rect.left_center(), preview_rect.right_center()],
                    Stroke::new(1., stroke_color),
                );
            }
            ui.collapsing("Compare", |ui| {
                ui.vertical_centered_justified(|ui| {
                if let Some(p) = &(state.current_path).clone() {
                    if ui.button("Add/update current image").clicked() {
                        state.compare_list.insert(p.clone(), state.image_geometry.clone());
                    }


        let mut compare_list: Vec<(PathBuf, ImageGeometry)> = state.compare_list.clone().into_iter().collect();
        compare_list.sort_by(|a,b| a.0.cmp(&b.0));
                    for (path, geo) in compare_list {
                        if ui.selectable_label(p==&path, path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default().to_string()).clicked(){
                            state.image_geometry = geo.clone();
                            state
                                .player
                                .load_advanced(&path, Some(FrameSource::CompareResult), state.message_channel.0.clone());
                            state.current_path = Some(path);
                        }
                    }
                    if ui.button("Clear").clicked() {
                        state.compare_list.clear();
                    }
                }

            });
            });

            ui.collapsing("Alpha tools", |ui| {
                ui.vertical_centered_justified(|ui| {
                    if let Some(img) = &state.current_image {
                        if ui
                            .button("Show alpha bleed")
                            .on_hover_text("Highlight pixels with zero alpha and color information")
                            .clicked()
                        {
                            state.current_texture = highlight_bleed(img).to_texture(gfx, state.persistent_settings.linear_mag_filter);
                        }
                        if ui
                            .button("Show semi-transparent pixels")
                            .on_hover_text(
                                "Highlight pixels that are neither fully opaque nor fully transparent",
                            )
                            .clicked()
                        {
                            state.current_texture = highlight_semitrans(img).to_texture(gfx, state.persistent_settings.linear_mag_filter);
                        }
                        if ui.button("Reset image").clicked() {
                            state.current_texture = img.to_texture(gfx, state.persistent_settings.linear_mag_filter);
                        }

                    }
                });
            });
            // ui.add(egui::Slider::new(&mut state.tiling, 1..=10).text("Image tiling"));

            ui.horizontal(|ui| {
                ui.label("Tiling");
                ui.slider_styled(&mut state.tiling, 1..=10);
            });
            advanced_ui(ui, state);

        });





    });
}

pub fn settings_ui(app: &mut App, ctx: &Context, state: &mut OculanteState, gfx: &mut Graphics) {
    let mut settings_enabled = state.settings_enabled;
    egui::Window::new("Preferences")
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .open(&mut settings_enabled)
            .resizable(true)
            .default_width(600.)
            .show(ctx, |ui| {

                #[cfg(debug_assertions)]
                if ui.button("send test msg").clicked() {
                    state.send_message_info("Test");
                }

                egui::ComboBox::from_label("Color theme")
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
                        match state.persistent_settings.theme {
                            ColorTheme::Light =>
                                ctx.set_visuals(Visuals::light()),
                            ColorTheme::Dark =>
                                ctx.set_visuals(Visuals::dark()),
                            ColorTheme::System =>
                                set_system_theme(ctx),
                        }
                    }
                }
                );




                egui::Grid::new("settings").num_columns(2).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .color_edit_button_srgb(&mut state.persistent_settings.accent_color)
                            .changed()
                        {
                            let mut style: egui::Style = (*ctx.style()).clone();
                            style.visuals.selection.bg_fill = Color32::from_rgb(
                                state.persistent_settings.accent_color[0],
                                state.persistent_settings.accent_color[1],
                                state.persistent_settings.accent_color[2],
                            );
                            ctx.set_style(style);
                        }
                        ui.label("Accent color");
                    });

                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgb(&mut state.persistent_settings.background_color);
                        ui.label("Background color");
                    });

                    ui.end_row();

                    ui
                    .checkbox(&mut state.persistent_settings.vsync, "Enable vsync")
                    .on_hover_text(
                        "Vsync reduces tearing and saves CPU. Toggling it off will make some operations such as panning/zooming more snappy. This needs a restart to take effect.",
                    );
                ui
                .checkbox(&mut state.persistent_settings.show_scrub_bar, "Show index slider")
                .on_hover_text(
                    "Enable an index slider to quickly scrub through lots of images",
                );
                    ui.end_row();

                    if ui
                    .checkbox(&mut state.persistent_settings.wrap_folder, "Wrap images at folder boundary")
                    .on_hover_text(
                        "When you move past the first or last image in a folder, should oculante continue or stop?",
                    )
                    .changed()
                {
                    state.scrubber.wrap = state.persistent_settings.wrap_folder;
                }
                ui.horizontal(|ui| {
                    ui.label("Number of image to cache");
                    if ui
                    .add(egui::DragValue::new(&mut state.persistent_settings.max_cache).clamp_range(0..=10000))

                    .on_hover_text(
                        "Keep this many images in memory for faster opening.",
                    )
                    .changed()
                {
                    state.player.cache.cache_size = state.persistent_settings.max_cache;
                    state.player.cache.clear();
                }
                });

                ui.end_row();
                ui
                    .checkbox(&mut state.persistent_settings.keep_view, "Do not reset image view")
                    .on_hover_text(
                        "When a new image is loaded, keep current zoom and offset",
                    );

                ui
                    .checkbox(&mut state.persistent_settings.keep_edits, "Keep image edits")
                    .on_hover_text(
                        "When a new image is loaded, keep current edits",
                    );
                ui.end_row();
                ui
                    .checkbox(&mut state.persistent_settings.show_checker_background, "Show checker background")
                    .on_hover_text(
                        "Show a checker pattern as backdrop.",
                    );

                ui
                    .checkbox(&mut state.persistent_settings.show_frame, "Draw frame around image")
                    .on_hover_text(
                        "Draw a small frame around the image. It is centered on the outmost pixel. This can be helpful on images with lots of transparency.",
                    );
                    ui.end_row();
                if ui.checkbox(&mut state.persistent_settings.zen_mode, "Turn on Zen mode").on_hover_text("Zen mode hides all UI and fits the image to the frame.").changed(){
                    set_title(app, state);
                }
                if ui.checkbox(&mut state.persistent_settings.force_redraw, "Redraw every frame").on_hover_text("Requires restart. Turn off optimisations and redraw everything each frame. This will consume more CPU but give you instant feedback, for example if new images come in or modifications are made.").changed(){
                    app.window().set_lazy_loop(!state.persistent_settings.force_redraw);
                }

                // ui.label(format!("lazy {}", app.window().lazy_loop()));
                ui.end_row();
                if ui.checkbox(&mut state.persistent_settings.linear_mag_filter, "Interpolate pixels on zoom").on_hover_text("When zooming in, do you prefer to see individual pixels or an interpolation?").changed(){
                    if let Some(img) = &state.current_image {
                        if state.edit_state.result_image_op.is_empty() {
                            state.current_texture = img.to_texture(gfx, state.persistent_settings.linear_mag_filter);
                        } else {
                            state.current_texture =  state.edit_state.result_pixel_op.to_texture(gfx, state.persistent_settings.linear_mag_filter);
                        }
                    }
                }

                ui.checkbox(&mut state.persistent_settings.fit_image_on_window_resize, "Fit image on window resize").on_hover_text("When you resize the main window, fir the image with it?");
                ui.end_row();

                ui.add(egui::DragValue::new(&mut state.persistent_settings.zoom_multiplier).clamp_range(0.05..=10.0).prefix("Zoom multiplier: ").speed(0.01)).on_hover_text("Adjust how much you zoom when you use the mouse wheel or the trackpad.");
                #[cfg(not(target_os = "netbsd"))]
                ui.checkbox(&mut state.persistent_settings.borderless, "Borderless mode").on_hover_text("Don't draw OS window decorations. Needs restart.");
                ui.end_row();


            });

                ui.horizontal(|ui| {
                    ui.label("Configure window title");
                    if ui
                    .text_edit_singleline(&mut state.persistent_settings.title_format)
                    .on_hover_text(
                        "Configure the title. Use {APP}, {VERSION}, {FULLPATH}, {FILENAME} and {RES} as placeholders.",
                    )
                    .changed()
                    {
                        set_title(app, state);
                    }
                });

                if ui.link("Visit github repo").on_hover_text("Check out the source code, request a feature, submit a bug or leave a star if you like it!").clicked() {
                    _ = webbrowser::open("https://github.com/woelper/oculante");
                }


                ui.vertical_centered_justified(|ui| {

                    #[cfg(feature = "update")]
                    if ui.button("Check for updates").on_hover_text("Check and install update if available. You will need to restart the app to use the new version.").clicked() {
                        state.send_message_info("Checking for updates...");
                        crate::update::update(Some(state.message_channel.0.clone()));
                        state.settings_enabled = false;
                    }

                    if ui.button("Reset all settings").clicked() {
                        state.persistent_settings = Default::default();
                    }
                });

                ui.collapsing("Keybindings",|ui| {
                    keybinding_ui(app, state, ui);
                });

                ui.collapsing("Mouse wheel", |ui| {
                    egui::Grid::new("mouse_settings").show(ui, |ui| {
                        ui.label("Action");
                        ui.label("Up/Down");
                        ui.label("Modifiers");
                        ui.end_row();

                        let mut enabled_triggers: std::collections::HashSet<MouseWheelEvent> = Default::default();

                        for (action, opt_trigger) in state.persistent_settings.mouse_wheel_settings.iter_mut() {
                            if let Some(trigger) = opt_trigger {
                                if enabled_triggers.insert(*trigger) {
                                    ui.label(format!("{action:?}"));
                                    enabled_triggers.insert(*trigger);
                                } else {
                                    ui.label(RichText::new(format!("{action:?}")).strikethrough())
                                        .on_hover_text("Invalid setting for the action. Change the setting or only the first action with the same setting will be performed.");
                                }

                                egui::ComboBox::from_id_source(format!("Up/Down {action:?}"))
                                    .selected_text(format!("{:?}", trigger.direction))
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut trigger.direction, MouseWheelDirection::Up, "Up");
                                        ui.selectable_value(&mut trigger.direction, MouseWheelDirection::Down, "Down");
                                    }
                                );
                                ui.horizontal(|ui| { ui.checkbox(&mut trigger.ctrl, "Ctrl");
                                ui.checkbox(&mut trigger.shift, "Shift"); });
                                if ui.button("Disable").on_hover_text("Click to disable this binding").clicked() {
                                    *opt_trigger = None;
                                }
                                

                            } else {
                                ui.label(format!("{action:?}"));
                                if ui.button("Enable").on_hover_text("Click to enable binding").clicked() {
                                    *opt_trigger = Some(Default::default());
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
            });
    state.settings_enabled = settings_enabled;
}

pub fn advanced_ui(ui: &mut Ui, state: &mut OculanteState) {
    if let Some(info) = &state.image_info {
        egui::Grid::new("extended").show(ui, |ui| {
            ui.label("Number of colors");
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

        if !info.exif.is_empty() {
            ui.collapsing("EXIF", |ui| {
                egui::ScrollArea::new([true, false]).show(ui, |ui| {
                    egui::Grid::new("extended_exif")
                        .striped(true)
                        .show(ui, |ui| {
                            for (key, val) in &info.exif {
                                ui.label(key);
                                ui.label(val);
                                ui.end_row();
                            }
                        });
                });
            });
        }

        let red_vals = Points::new(
            info.red_histogram
                .iter()
                .map(|(k, v)| [*k as f64, *v as f64])
                .collect::<PlotPoints>(),
        )
        .stems(0.0)
        .color(Color32::RED);

        let green_vals = Points::new(
            info.green_histogram
                .iter()
                .map(|(k, v)| [*k as f64, *v as f64])
                .collect::<PlotPoints>(),
        )
        .stems(0.0)
        .color(Color32::GREEN);

        let blue_vals = Points::new(
            info.blue_histogram
                .iter()
                .map(|(k, v)| [*k as f64, *v as f64])
                .collect::<PlotPoints>(),
        )
        .stems(0.0)
        .color(Color32::BLUE);

        Plot::new("histogram")
            .allow_zoom(false)
            .allow_drag(false)
            .width(PANEL_WIDTH - PANEL_WIDGET_OFFSET)
            .show(ui, |plot_ui| {
                // plot_ui.line(grey_vals);
                plot_ui.points(red_vals);
                plot_ui.points(green_vals);
                plot_ui.points(blue_vals);
            });
    }
}

/// Everything related to image editing
#[allow(unused_variables)]
pub fn edit_ui(app: &mut App, ctx: &Context, state: &mut OculanteState, gfx: &mut Graphics) {
    egui::SidePanel::right("editing")
        .min_width(100.)
        .show(ctx, |ui| {
            // A flag to indicate that the image needs to be rebuilt
            let mut image_changed = false;
            let mut pixels_changed = false;

            if let Some(img) = &state.current_image {
                // Ensure that edit result image is always filled
                if state.edit_state.result_pixel_op.width() == 0 {
                    debug!("Edit state pixel comp buffer is default, cloning from image");
                    state.edit_state.result_pixel_op = img.clone();
                    pixels_changed = true;
                }
                if state.edit_state.result_image_op.width() == 0 {
                    debug!("Edit state image comp buffer is default, cloning from image");
                    state.edit_state.result_image_op = img.clone();
                    image_changed = true;
                }
            }

            egui::Grid::new("editing")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    let mut ops = [
                        ImageOperation::Brightness(0),
                        ImageOperation::Contrast(0),
                        ImageOperation::Exposure(20),
                        ImageOperation::Desaturate(0),
                        ImageOperation::LUT("Lomography Redscale 100".into()),
                        ImageOperation::Equalize((0, 255)),
                        ImageOperation::ScaleImageMinMax,
                        ImageOperation::Posterize(8),
                        ImageOperation::ChannelSwap((Channel::Red, Channel::Red)),
                        ImageOperation::Rotate(90),
                        ImageOperation::HSV((0, 100, 100)),
                        ImageOperation::Crop([0, 0, 0, 0]),
                        ImageOperation::CropPerspective{points: [
                            (0,0),
                            (state.image_geometry.dimensions.0, 0),
                            (0, state.image_geometry.dimensions.1),
                            (state.image_geometry.dimensions.0, state.image_geometry.dimensions.1),
                            ]
                        , original_size : state.image_geometry.dimensions
                        },
                        ImageOperation::Mult([255, 255, 255]),
                        ImageOperation::Fill([255, 255, 255, 255]),
                        ImageOperation::Blur(0),
                        ImageOperation::Filter3x3([0,-100, 0, -100, 500, -100, 0, -100, 0]),
                        ImageOperation::GradientMap(vec![GradientStop::new(0, [155,33,180]), GradientStop::new(128, [255,83,0]),GradientStop::new(255, [224,255,0])]),
                        ImageOperation::MMult,
                        ImageOperation::MDiv,
                        ImageOperation::Expression("r = 1.0".into()),
                        ImageOperation::Noise {
                            amt: 50,
                            mono: false,
                        },
                        ImageOperation::Add([0, 0, 0]),
                        ImageOperation::Resize {
                            dimensions: state.image_geometry.dimensions,
                            aspect: true,
                            filter: ScaleFilter::Hamming,
                        },
                        ImageOperation::Invert,
                        ImageOperation::Flip(false),
                        ImageOperation::ChromaticAberration(15),
                    ];

                    ui.label_i("➕ Filter");
                    let available_w_single_spacing =
                        ui.available_width();
                        //  - ui.style().spacing.item_spacing.x;

                    egui::ComboBox::from_id_source("Imageops")
                        .selected_text("Select a filter to add...")
                        .width(available_w_single_spacing)
                        .show_ui(ui, |ui| {
                            for op in &mut ops {
                                if ui.label_i_selected(false, &format!("{op}")).clicked() {
                                    if op.is_per_pixel() {
                                        state.edit_state.pixel_op_stack.push(op.clone());
                                        pixels_changed = true;
                                    } else {
                                        state.edit_state.image_op_stack.push(op.clone());
                                        image_changed = true;
                                    }
                                }
                            }
                        });
                    ui.end_row();

                    modifier_stack_ui(&mut state.edit_state.image_op_stack, &mut image_changed, ui, &state.image_geometry, &mut state.edit_state.block_panning);
                    modifier_stack_ui(
                        &mut state.edit_state.pixel_op_stack,
                        &mut pixels_changed,
                        ui, &state.image_geometry, &mut state.edit_state.block_panning
                    );

                    ui.label_i(&format!("{RECYCLE} Reset"));
                    ui.centered_and_justified(|ui| {
                        if ui.button("Reset all edits").clicked() {
                            state.edit_state = Default::default();
                            pixels_changed = true
                        }
                    });
                    ui.end_row();

                    ui.label_i(&format!("{GIT_DIFF} Compare"));
                    let available_w_single_spacing =
                        ui.available_width() - ui.style().spacing.item_spacing.x;
                    ui.horizontal(|ui| {
                        if ui
                            .add_sized(
                                egui::vec2(available_w_single_spacing / 2., ui.available_height()),
                                egui::Button::new("Original"),
                            )
                            .clicked()
                        {
                            if let Some(img) = &state.current_image {
                                state.image_geometry.dimensions = img.dimensions();
                                state.current_texture = img.to_texture(gfx, state.persistent_settings.linear_mag_filter);
                            }
                        }
                        if ui
                            .add_sized(
                                egui::vec2(available_w_single_spacing / 2., ui.available_height()),
                                egui::Button::new("Modified"),
                            )
                            .clicked()
                        {
                            pixels_changed = true;
                        }
                    });
                    ui.end_row();
                });

            ui.vertical_centered_justified(|ui| {
                if state.edit_state.painting {

                    if ctx.input(|i|i.pointer.secondary_down()) {
                        if let Some(stroke) = state.edit_state.paint_strokes.last_mut() {
                            if let Some(p) = state.edit_state.result_pixel_op.get_pixel_checked(
                                state.cursor_relative.x as u32,
                                state.cursor_relative.y as u32,
                            ) {
                                debug!("{:?}", p);
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
                } else if ui.button(format!("{PAINT_BRUSH_HOUSEHOLD} Paint mode")).clicked() {
                    state.edit_state.painting = true;
                }
            });

            if state.edit_state.painting {
                egui::Grid::new("paint").show(ui, |ui| {
                    ui.label("📜 Keep history");
                    ui.checkbox(&mut state.edit_state.non_destructive_painting, "")
                        .on_hover_text("Keep all paint history and edit it. Slower.");
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

                ui.end_row();

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
            ui.end_row();

            ui.vertical_centered_justified(|ui| {
                if ui
                    .button(format!("{STACK} Apply all edits"))
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
            });

            // Do the processing

            // If expensive operations happened (modifying image geometry), process them here
            if image_changed {
                if let Some(img) = &mut state.current_image {
                    let stamp = Instant::now();
                    // start with a fresh copy of the unmodified image
                    state.edit_state.result_image_op = img.clone();
                    for operation in &mut state.edit_state.image_op_stack {
                        if let Err(e) = operation.process_image(&mut state.edit_state.result_image_op) {
                            error!("{e}")
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
                    let ops = &state.edit_state.pixel_op_stack;
                    process_pixels(&mut state.edit_state.result_pixel_op, ops);

                }

                                debug!(
                    "Finished Pixel op stack in {} s",
                    stamp.elapsed().as_secs_f32()
                );

                // draw paint lines
                for stroke in &state.edit_state.paint_strokes {
                    if !stroke.committed {
                        stroke.render(
                            &mut state.edit_state.result_pixel_op,
                            &state.edit_state.brushes,
                        );
                    }
                }

                // Update the texture
                if let Some(tex) = &mut state.current_texture {
                    if let Some(img) = &state.current_image {
                        if tex.width() as u32 == state.edit_state.result_pixel_op.width()
                            && state.edit_state.result_pixel_op.height() == img.height()
                        {
                            state.edit_state.result_pixel_op.update_texture(gfx, tex);
                        } else {
                            state.current_texture =
                                state.edit_state.result_pixel_op.to_texture(gfx, state.persistent_settings.linear_mag_filter);
                        }
                    }
                }

                debug!(
                    "Done updating tex after pixel; ops in {} s",
                    stamp.elapsed().as_secs_f32()
                );

    //             let sender = state.texture_channel.0.clone();

    //             let f = Frame::new_edit(state.edit_state.result_pixel_op.clone());
                    //  _ = sender.send(f);


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
                            stroke.render(
                                &mut state.edit_state.result_image_op,
                                &state.edit_state.brushes,
                            );
                            stroke.committed = true;
                            debug!("Committed stroke {}", i);
                        }
                    }
                }
            }

            state.image_geometry.dimensions = state.edit_state.result_pixel_op.dimensions();

            ui.vertical_centered_justified(|ui| {
                if let Some(path) = &state.current_path {
                    if ui
                        .button(format!("{RECYCLE} Restore original"))
                        .on_hover_text("Completely reload image, destroying all edits.")
                        .clicked()
                    {
                        state.is_loaded = false;
                        state.player.cache.clear();
                        state.player.load(&path, state.message_channel.0.clone());
                    }
                }


                #[cfg(feature = "turbo")]
                jpg_lossless_ui(state, ui);


                if state.current_path.is_none() && state.current_image.is_some() {
                    #[cfg(not(feature = "file_open"))]
                    {
                        if ui.button("Create output file").on_hover_text("This image does not have any file associated with it. Click to create a default one.").clicked() {
                            let dest = state.persistent_settings.last_open_directory.clone().join("untitled").with_extension(&state.edit_state.export_extension);
                            state.current_path = Some(dest);
                            set_title(app, state);
                        }
                    }
                }

                #[cfg(feature = "file_open")]
                if state.current_image.is_some() {
                    if ui.button(format!("{FLOPPY_DISK} Save as...")).clicked() {

                        let start_directory = state.persistent_settings.last_open_directory.clone();

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
                                                _ = msg_sender.send(Message::Saved(file_path.clone()));
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
                                                _ = err_sender.send(Message::err(&format!("Error: Could not save: {e}")));
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
                    if ui.button(format!("{FLOPPY_DISK} Save as...")).clicked() {
                        ui.ctx().memory_mut(|w| w.open_popup(Id::new("SAVE")));

                    }


                    if ctx.memory(|w| w.is_popup_open(Id::new("SAVE"))) {


                        let msg_sender = state.message_channel.0.clone();

                        filebrowser::browse_modal(
                            true,
                            &["png", "jpg", "bmp", "webp", "tif", "tga"],
                            |p| {
                                    match state.edit_state.result_pixel_op
                                    .save(&p) {
                                        Ok(_) => {
                                            _ = msg_sender.send(Message::Saved(p.clone()));
                                            debug!("Saved to {}", p.display());
                                            // Re-apply exif
                                            if let Some(info) = &state.image_info {
                                                debug!("Extended image info present");

                                                // before doing anything, make sure we have raw exif data
                                                if info.raw_exif.is_some() {
                                                    if let Err(e) = fix_exif(&p, info.raw_exif.clone()) {
                                                        error!("{e}");
                                                    } else {
                                                        info!("Saved EXIF.");
                                                        _ = msg_sender.send(Message::Info("Exif metadata was saved to file".into()));
                                                    }
                                                } else {
                                                    debug!("No raw exif");
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            _ = msg_sender.send(Message::err(&format!("Error: Could not save: {e}")));
                                        }
                                }
                            },
                            ctx,
                        );
                    }
                }

                if let Some(p) = &state.current_path {
                    let text = if p
                        // .with_extension(&state.edit_state.export_extension)
                        .exists()
                    {
                        format!("{FLOPPY_DISK} Overwrite")
                    } else {
                        format!("{FLOPPY_DISK} Save")
                    };

                    if ui.button(text).on_hover_text("Save the image. This will create a new file or overwrite.").clicked() {
                        match state
                        .edit_state
                        .result_pixel_op
                        .save(p) {
                            Ok(_) => {
                                debug!("Saved to {}", p.display());
                                state.send_message_info(&format!("Saved to {}", p.display()));
                                // Re-apply exif
                                if let Some(info) = &state.image_info {
                                    debug!("Extended image info present");
                                    // before doing anything, make sure we have raw exif data
                                    if info.raw_exif.is_some() {
                                        if let Err(e) = fix_exif(&p, info.raw_exif.clone()) {
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
                                state.send_message_err(&format!("Could not save: {e}"));
                            }
                        }
                    }

                    if ui.button(format!("{ARCHIVE_TRAY} Save edits")).on_hover_text("Saves an .oculante metafile in the same directory as the image. This file will contain all edits and will be restored automatically if you open the image again. This leaves the original image unmodified and allows you to continue editing later.").clicked() {
                        if let Ok(f) = std::fs::File::create(p.with_extension("oculante")) {
                            _ = serde_json::to_writer_pretty(&f, &state.edit_state);
                        }
                    }
                    if ui.button(format!("{ARCHIVE_TRAY} Save directory edits")).on_hover_text("Saves an .oculante metafile in the same directory as the image. This file will contain all edits and will be restored automatically if you open the image again. This leaves the original image unmodified and allows you to continue editing later.").clicked() {
                        if let Some(parent) = p.parent() {
                            if let Ok(f) = std::fs::File::create(parent.join(".oculante")) {
                                _ = serde_json::to_writer_pretty(&f, &state.edit_state);
                            }

                        }

                    }
                }
            });

            if pixels_changed && state.persistent_settings.info_enabled {
                state.image_info = None;
                send_extended_info(
                    &Some(state.edit_state.result_pixel_op.clone()),
                    &state.current_path,
                    &state.extended_info_channel,
                );
            }
        });
}

// TODO redo as impl UI
pub fn tooltip(r: Response, tooltip: &str, hotkey: &str, _ui: &mut Ui) -> Response {
    r.on_hover_ui(|ui| {
        let avg = (ui.style().visuals.selection.bg_fill.r() as i32
            + ui.style().visuals.selection.bg_fill.g() as i32
            + ui.style().visuals.selection.bg_fill.b() as i32)
            / 3;
        let contrast_color: u8 = if avg > 128 { 0 } else { 255 };
        ui.horizontal(|ui| {
            ui.label(tooltip);
            ui.label(
                RichText::new(hotkey)
                    .monospace()
                    .color(Color32::from_gray(contrast_color))
                    .background_color(ui.style().visuals.selection.bg_fill),
            );
        });
    })
}

// TODO redo as impl UI
pub fn unframed_button(text: impl Into<String>, ui: &mut Ui) -> Response {
    ui.add(egui::Button::new(RichText::new(text).size(ICON_SIZE)).frame(false))
}

pub fn unframed_button_colored(text: impl Into<String>, is_colored: bool, ui: &mut Ui) -> Response {
    if is_colored {
        ui.add(
            egui::Button::new(
                RichText::new(text)
                    .size(ICON_SIZE)
                    // .heading()
                    .color(ui.style().visuals.selection.bg_fill),
            )
            .frame(false),
        )
    } else {
        ui.add(
            egui::Button::new(
                RichText::new(text).size(ICON_SIZE), // .heading()
            )
            .frame(false),
        )
    }
}

pub fn stroke_ui(
    stroke: &mut PaintStroke,
    brushes: &[RgbaImage],
    ui: &mut Ui,
    gfx: &mut Graphics,
) -> Response {
    let mut combined_response = ui.color_edit_button_rgba_unmultiplied(&mut stroke.color);

    let r = ui
        .checkbox(&mut stroke.fade, "")
        .on_hover_text("Fade out the stroke over its path");
    if r.changed() {
        combined_response.changed = true;
    }
    if r.hovered() {
        combined_response.hovered = true;
    }

    let r = ui
        .checkbox(&mut stroke.flip_random, "")
        .on_hover_text("Flip brush in X any Y randomly to make stroke less uniform");
    if r.changed() {
        combined_response.changed = true;
    }
    if r.hovered() {
        combined_response.hovered = true;
    }

    let r = ui.add(
        egui::DragValue::new(&mut stroke.width)
            .clamp_range(0.0..=0.3)
            .speed(0.001),
    );
    if r.changed() {
        combined_response.changed = true;
    }
    if r.hovered() {
        combined_response.hovered = true;
    }

    ui.horizontal(|ui| {
        if let Some(notan_texture) = brushes[stroke.brush_index].to_texture_premult(gfx) {
            let texture_id = gfx.egui_register_texture(&notan_texture);
            ui.add(
                egui::Image::new(texture_id)
                    .fit_to_exact_size(egui::Vec2::splat(ui.available_height())),
            );
        }

        let r = egui::ComboBox::from_id_source(format!("s {:?}", stroke.points))
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
                            combined_response.changed = true
                        }
                    });
                }
            })
            .response;

        if r.hovered() {
            combined_response.hovered = true;
        }
    });

    if combined_response.hovered() {
        stroke.highlight = true;
    } else {
        stroke.highlight = false;
    }
    if combined_response.changed() {
        stroke.highlight = false;
    }
    combined_response
}

fn modifier_stack_ui(
    stack: &mut Vec<ImageOperation>,
    image_changed: &mut bool,
    ui: &mut Ui,
    geo: &ImageGeometry,
    mouse_grab: &mut bool,
) {
    let mut delete: Option<usize> = None;
    let mut swap: Option<(usize, usize)> = None;

    // egui::Grid::new("dfdfd").num_columns(2).show(ui, |ui| {
    for (i, operation) in stack.iter_mut().enumerate() {
        ui.label_i(&format!("{operation}"));

        // let op draw itself and check for response

        ui.push_id(i, |ui| {
            // ui.end_row();

            // draw the image operator
            if operation.ui(ui, geo, mouse_grab).changed() {
                *image_changed = true;
            }

            // now draw the ordering/delete ui
            ui.add_space(45.);

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                // ui.add_space(size);

                // ui.vertical(|ui| {
                ui.style_mut().spacing.icon_spacing = 0.;
                ui.style_mut().spacing.button_padding = Vec2::ZERO;
                ui.style_mut().spacing.interact_size = Vec2::ZERO;
                ui.style_mut().spacing.indent = 0.0;
                ui.style_mut().spacing.item_spacing = Vec2::ZERO;

                if egui::Button::new("❌")
                    .small()
                    .frame(false)
                    .ui(ui)
                    .on_hover_text("Remove operator")
                    .clicked()
                {
                    delete = Some(i);
                    *image_changed = true;
                }

                if egui::Button::new("⏶")
                    .small()
                    .frame(false)
                    .ui(ui)
                    .on_hover_text("Move up")
                    .clicked()
                {
                    swap = Some(((i as i32 - 1).max(0) as usize, i));
                    *image_changed = true;
                }

                if egui::Button::new("⏷")
                    .small()
                    .frame(false)
                    .ui(ui)
                    .on_hover_text("Move down")
                    .clicked()
                {
                    swap = Some((i, i + 1));
                    *image_changed = true;
                }
            });

            ui.end_row();
        });

        ui.end_row();
    }
    // });

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

        ui.collapsing("Lossless Jpeg transforms", |ui| {
            ui.label("These operations will immediately write changes to disk.");
            let mut reload = false;

            ui.columns(3, |col| {
                if col[0].button("➡ Rotate 90°").clicked() {
                    if lossless_tx(
                        p,
                        turbojpeg::Transform {
                            op: turbojpeg::TransformOp::Rot90,
                            ..turbojpeg::Transform::default()
                        },
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
                        turbojpeg::Transform {
                            op: turbojpeg::TransformOp::Rot270,
                            ..turbojpeg::Transform::default()
                        },
                    )
                    .is_ok()
                    {
                        reload = true;
                    }
                }

                if col[2].button("⬇ Rotate 180°").clicked() {
                    if lossless_tx(
                        p,
                        turbojpeg::Transform {
                            op: turbojpeg::TransformOp::Rot180,
                            ..turbojpeg::Transform::default()
                        },
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
                        turbojpeg::Transform {
                            op: turbojpeg::TransformOp::Hflip,
                            ..turbojpeg::Transform::default()
                        },
                    )
                    .is_ok()
                    {
                        reload = true;
                    }
                }

                if col[1].button("Flip V").clicked() {
                    if lossless_tx(
                        p,
                        turbojpeg::Transform {
                            op: turbojpeg::TransformOp::Vflip,
                            ..turbojpeg::Transform::default()
                        },
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
                    .filter(|op| matches!(op, ImageOperation::Crop(_)))
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
                        .push(ImageOperation::Crop([0, 0, 0, 0]))
                }

                ui.add_enabled_ui(crop != ImageOperation::Crop([0, 0, 0, 0]), |ui| {

                    if ui
                        .button("Crop")
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

                                match lossless_tx(
                                    p,
                                    turbojpeg::Transform {
                                        op: turbojpeg::TransformOp::None,
                                        crop: Some(turbojpeg::TransformCrop {
                                            x: crop_range[0] as usize,
                                            y: crop_range[1] as usize,
                                            width: Some(crop_range[2] as usize),
                                            height: Some(crop_range[3] as usize),
                                        }),
                                        ..turbojpeg::Transform::default()
                                    },
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
                state.player.load(&p, state.message_channel.0.clone());
            }
        });
    }
}

pub fn scrubber_ui(state: &mut OculanteState, ui: &mut Ui) {
    let len = state.scrubber.len().saturating_sub(1);

    if ui
        .slider_timeline(&mut state.scrubber.index, 0..=len)
        .changed()
    {
        let p = state.scrubber.set(state.scrubber.index);
        state.current_path = Some(p.clone());
        state.player.load(&p, state.message_channel.0.clone());
    }
}

fn keybinding_ui(app: &mut App, state: &mut OculanteState, ui: &mut Ui) {
    // Make sure no shortcuts are received by the application
    state.key_grab = true;

    let no_keys_pressed = app.keyboard.down.is_empty();

    ui.horizontal(|ui| {
        ui.label("While this is open, regular shortcuts will not work.");
        if no_keys_pressed {
            ui.label(egui::RichText::new("Please press & hold a key").color(Color32::RED));
        }
    });

    let k = app
        .keyboard
        .down
        .iter()
        .map(|k| format!("{:?}", k.0))
        .collect::<BTreeSet<String>>();

    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let s = state.persistent_settings.shortcuts.clone();
            let mut ordered_shortcuts = state
                .persistent_settings
                .shortcuts
                .iter_mut()
                .collect::<Vec<_>>();
            ordered_shortcuts
                .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

            egui::Grid::new("info").num_columns(2).show(ui, |ui| {
                for (event, keys) in ordered_shortcuts {
                    ui.label(format!("{event:?}"));

                    ui.label(lookup(&s, event));
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
        });
}

// fn keystrokes(ui: &mut Ui) {
//     ui.add(Button::new(format!("{:?}", k.0)).fill(Color32::DARK_BLUE));
// }

pub fn main_menu(ui: &mut Ui, state: &mut OculanteState, app: &mut App, gfx: &mut Graphics) {
    ui.horizontal_centered(|ui| {
        use crate::shortcuts::InputEvent::*;

        // ui.label("Channels");
        if state.persistent_settings.borderless {
            if unframed_button(X, ui).clicked() {
                app.backend.exit();
            }
        }

        if unframed_button(FOLDER, ui)
            .on_hover_text("Browse for image")
            .clicked()
        {
            #[cfg(feature = "file_open")]
            crate::browse_for_image_path(state);
            #[cfg(not(feature = "file_open"))]
            ui.ctx().memory_mut(|w| w.open_popup(Id::new("OPEN")));
        }

        #[cfg(not(feature = "file_open"))]
        {
            if ui.ctx().memory(|w| w.is_popup_open(Id::new("OPEN"))) {
                filebrowser::browse_modal(
                    false,
                    SUPPORTED_EXTENSIONS,
                    |p| {
                        let _ = state.load_channel.0.clone().send(p.to_path_buf());
                        ui.ctx().memory_mut(|w| w.close_popup());
                    },
                    ui.ctx(),
                );
            }
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

        ui.add_enabled_ui(!state.persistent_settings.edit_enabled, |ui| {
            // hack to center combo box in Y

            ui.spacing_mut().button_padding = Vec2::new(10., 0.);
            let combobox_text_size = 16.;
            egui::ComboBox::from_id_source("channels")
                .selected_text(
                    RichText::new(state.persistent_settings.current_channel.to_string())
                        .size(combobox_text_size),
                )
                .show_ui(ui, |ui| {
                    for channel in ColorChannel::iter() {
                        let r = ui.selectable_value(
                            &mut state.persistent_settings.current_channel,
                            channel,
                            RichText::new(channel.to_string()).size(combobox_text_size),
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

        // TODO: remove redundancy
        if changed_channels {
            if let Some(img) = &state.current_image {
                match &state.persistent_settings.current_channel {
                    ColorChannel::Rgb => {
                        state.current_texture = unpremult(img)
                            .to_texture(gfx, state.persistent_settings.linear_mag_filter)
                    }
                    ColorChannel::Rgba => {
                        state.current_texture =
                            img.to_texture(gfx, state.persistent_settings.linear_mag_filter)
                    }
                    _ => {
                        state.current_texture =
                            solo_channel(img, state.persistent_settings.current_channel as usize)
                                .to_texture(gfx, state.persistent_settings.linear_mag_filter)
                    }
                }
            }
        }

        if state.current_path.is_some() {
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

        if state.current_image.is_some() {
            if tooltip(
                // ui.checkbox(&mut state.info_enabled, "ℹ Info"),
                ui.selectable_label(
                    state.persistent_settings.info_enabled,
                    RichText::new(format!("{}", INFO)).size(ICON_SIZE * 0.8),
                ),
                "Show image info",
                &lookup(&state.persistent_settings.shortcuts, &InfoMode),
                ui,
            )
            .clicked()
            {
                state.persistent_settings.info_enabled = !state.persistent_settings.info_enabled;
                send_extended_info(
                    &state.current_image,
                    &state.current_path,
                    &state.extended_info_channel,
                );
            }

            if tooltip(
                ui.selectable_label(
                    state.persistent_settings.edit_enabled,
                    RichText::new(format!("{}", PENCIL_SIMPLE_LINE)).size(ICON_SIZE * 0.8),
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

        // FIXME This crashes/freezes!
        // if tooltip(
        //     unframed_button("⛶", ui),
        //     "Full Screen",
        //     &lookup(&state.persistent_settings.shortcuts, &Fullscreen),
        //     ui,
        // )
        // .clicked()
        // {
        //     toggle_fullscreen(app, state);
        // }

        if tooltip(
            unframed_button(ARROWS_OUT_SIMPLE, ui),
            "Toggle fullscreen",
            &lookup(&state.persistent_settings.shortcuts, &Fullscreen),
            ui,
        )
        .clicked()
        {
            toggle_fullscreen(app, state);
        }

        if tooltip(
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

        if let Some(p) = &state.current_path {
            if tooltip(
                unframed_button(TRASH, ui),
                "Move file to trash",
                &lookup(&state.persistent_settings.shortcuts, &DeleteFile),
                ui,
            )
            .clicked()
            {
                _ = trash::delete(p);
                state.send_message_info(&format!(
                    "Deleted {}",
                    p.file_name()
                        .map(|f| f.to_string_lossy().to_string())
                        .unwrap_or_default()
                ));
            }

            if !state.is_loaded {
                ui.horizontal(|ui| {
                    ui.add(egui::Spinner::default());
                    ui.label(format!("Loading {}", p.display()));
                });
                app.window().request_frame();
            }
        }

        drag_area(ui, state, app);

        ui.add_space(ui.available_width() - 32.);
        draw_hamburger_menu(ui, state, app);
    });
}

pub fn draw_hamburger_menu(ui: &mut Ui, state: &mut OculanteState, app: &mut App) {
    use crate::shortcuts::InputEvent::*;

    ui.scope(|ui| {
        // ui.style_mut().override_text_style = Some(egui::TextStyle::Heading);
        // maybe override font size?
        ui.style_mut().visuals.button_frame = false;
        ui.style_mut().visuals.widgets.inactive.expansion = 20.;

        ui.style_mut().override_text_style = Some(egui::TextStyle::Heading);

        ui.menu_button(RichText::new(LIST).size(ICON_SIZE), |ui| {
            if ui.button("Reset view").clicked() {
                state.reset_image = true;
                ui.close_menu();
            }
            if ui.button("View 1:1").clicked() {
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
                    .button("🗐 Copy")
                    .on_hover_text("Copy image to clipboard")
                    .clicked()
                    || copy_pressed
                {
                    clipboard_copy(img);
                    ui.close_menu();
                }
            }

            if ui
                .button("📋 Paste")
                .on_hover_text("Paste image from clipboard")
                .clicked()
                || key_pressed(app, state, Paste)
            {
                match clipboard_to_image() {
                    Ok(img) => {
                        state.current_path = None;
                        // Stop in the even that an animation is running
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

            if ui.button("⛭ Preferences").clicked() {
                state.settings_enabled = !state.settings_enabled;
                ui.close_menu();
            }

            ui.menu_button("Recent", |ui| {
                for r in &state.persistent_settings.recent_images.clone() {
                    if let Some(filename) = r.file_name() {
                        if ui.button(filename.to_string_lossy()).clicked() {
                            load_image_from_path(r, state);
                            ui.close_menu();
                        }
                    }
                }
            });

            if ui.button("Quit").clicked() {
                app.backend.exit();
            }

            // TODO: expose favourites with a tool button
            // ui.menu_button("Favourites", |ui| {
            //     for r in &state.persistent_settings.favourite_images.clone() {
            //         if let Some(filename) = r.file_name() {
            //             if ui.button(filename.to_string_lossy()).clicked() {
            //ui.close_menu();

            //             }
            //         }
            //     }

            // });
        });

        // });
    });
}

pub fn drag_area(ui: &mut Ui, state: &mut OculanteState, app: &mut App) {
    #[cfg(not(target_os = "netbsd"))]
    if state.persistent_settings.borderless {
        let r = ui.interact(
            ui.available_rect_before_wrap(),
            Id::new("drag"),
            Sense::click_and_drag(),
        );

        if r.dragged() {
            // improve responsiveness
            app.window().request_frame();
            let position = Mouse::get_mouse_position();
            match position {
                Mouse::Position { mut x, mut y } => {
                    // translate mouse pos into real pixels
                    let dpi = app.backend.window().dpi();
                    x = (x as f64 * dpi) as i32;
                    y = (y as f64 * dpi) as i32;

                    let offset = match ui
                        .ctx()
                        .memory(|r| r.data.get_temp::<(i32, i32)>("offset".into()))
                    {
                        Some(o) => o,
                        None => {
                            let window_pos = app.window().position();
                            let offset = (window_pos.0 - x, window_pos.1 - y);
                            ui.ctx()
                                .memory_mut(|w| w.data.insert_temp(Id::new("offset"), offset));
                            offset
                        }
                    };
                    app.window().set_position(x + offset.0, y + offset.1);
                }
                Mouse::Error => error!("Error getting mouse position"),
            }
        }
        if r.drag_released() {
            ui.ctx()
                .memory_mut(|w| w.data.remove::<(i32, i32)>("offset".into()))
        }
    }
}
