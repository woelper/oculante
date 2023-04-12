#[cfg(feature = "file_open")]
use crate::browse_for_image_path;
use crate::{
    appstate::{ImageGeometry, OculanteState},
    image_editing::{process_pixels, Channel, ImageOperation, ScaleFilter},
    paint::PaintStroke,
    set_zoom,
    shortcuts::{key_pressed, keypresses_as_string, lookup},
    utils::{
        clipboard_copy, disp_col, disp_col_norm, highlight_bleed, highlight_semitrans,
        load_image_from_path, next_image, prev_image, send_extended_info, set_title, solo_channel,
        toggle_fullscreen, unpremult, ColorChannel, ImageExt,
    },
};

use arboard::Clipboard;
use egui::plot::Plot;
use image::RgbaImage;
use log::{debug, error, info};
use notan::{
    egui::{
        self,
        plot::{PlotPoints, Points},
        *,
    },
    prelude::{App, Graphics},
};
use std::{collections::HashSet, ops::RangeInclusive, path::PathBuf, time::Instant};
use strum::IntoEnumIterator;

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
            let mut style = ui.style_mut();
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
            let mut style = ui.style_mut();
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

pub fn info_ui(ctx: &Context, state: &mut OculanteState, gfx: &mut Graphics) {
    if let Some(img) = &state.current_image {
        if let Some(p) = img.get_pixel_checked(
            state.cursor_relative.x as u32,
            state.cursor_relative.y as u32,
        ) {
            state.sampled_color = [p[0] as f32, p[1] as f32, p[2] as f32, p[3] as f32];
        }
    }

    egui::SidePanel::left("side_panel").show(ctx, |ui| {


        egui::ScrollArea::vertical().auto_shrink([false,true]).always_show_scroll(true).show(ui, |ui| {
            if let Some(texture) = &state.current_texture {
                // texture.
                let tex_id = gfx.egui_register_texture(texture);

                // width of image widget
                let desired_width = ui.available_width() - ui.spacing().button_padding.x*4.;

                let scale = (desired_width / 8.) / texture.size().0;
                let img_size = egui::Vec2::new(desired_width, desired_width);

                let uv_center = (
                    state.cursor_relative.x / state.image_dimension.0 as f32,
                    (state.cursor_relative.y / state.image_dimension.1 as f32),
                );

                egui::Grid::new("info").show(ui, |ui| {
                    ui.label_i("⬜ Size");

                    ui.label(
                        RichText::new(format!(
                            "{}x{}",
                            state.image_dimension.0, state.image_dimension.1
                        ))
                        .monospace(),
                    );
                    ui.end_row();


                    if let Some(path) = &state.current_path {
                        // make sure we truncate filenames
                        let max_chars = 20;
                        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
                        let skip_symbol = if file_name.chars().count() > max_chars {".."} else {""};

                        ui.label_i("🖻 File");
                        ui.label(
                            RichText::new(format!(
                                "{skip_symbol}{}",
                                file_name.chars().rev().take(max_chars).collect::<String>().chars().rev().collect::<String>()
                            ))
                            // .monospace(),
                        )
                        .on_hover_text(format!("{}", path.display()));
                        ui.end_row();
                    }

                    ui.label_i("🌗 RGBA");
                    ui.label(
                        RichText::new(disp_col(state.sampled_color))
                            .monospace()
                            .background_color(Color32::from_rgba_unmultiplied(255, 255, 255, 6)),
                    );
                    ui.end_row();

                    ui.label_i("🌗 RGBA");
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
                        egui::Image::new(tex_id, img_size).uv(egui::Rect::from_x_y_ranges(
                            uv_center.0 - uv_size.0..=uv_center.0 + uv_size.0,
                            uv_center.1 - uv_size.1..=uv_center.1 + uv_size.1,
                        )), // .bg_fill(egui::Color32::RED),
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
                // ui.image(tex_id, img_size);
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
                            state.is_loaded = false;
                            state.current_image = None;
                            state
                                .player
                                .load(&path, state.message_channel.0.clone());
                            state.current_path = Some(path);
                            state.persistent_settings.keep_view = true;
                        }
                    }
                    if ui.button("Clear").clicked() {
                        state.compare_list.clear();
                    }
                }
                if state.is_loaded {
                    state.persistent_settings.keep_view = false;
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
                            state.current_texture = highlight_bleed(img).to_texture(gfx);
                        }
                        if ui
                            .button("Show semi-transparent pixels")
                            .on_hover_text(
                                "Highlight pixels that are neither fully opaque nor fully transparent",
                            )
                            .clicked()
                        {
                            state.current_texture = highlight_semitrans(img).to_texture(gfx);
                        }
                        if ui.button("Reset image").clicked() {
                            state.current_texture = img.to_texture(gfx);
                        }

                    }
                });
            });
            // ui.add(egui::Slider::new(&mut state.tiling, 1..=10).text("Image tiling"));

            ui.horizontal(|ui| {
                ui.slider_styled(&mut state.tiling, 1..=10);
                ui.label("Image tiling");
            });
            advanced_ui(ui, state);

        });





    });
}

pub fn settings_ui(app: &mut App, ctx: &Context, state: &mut OculanteState) {
    let mut settings_enabled = state.settings_enabled;

    egui::Window::new("Preferences")
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .collapsible(false)
            .open(&mut settings_enabled)
            .resizable(false)
            .default_width(600.)
            .show(ctx, |ui| {

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

                            state.persistent_settings.save()
                        }
                        ui.label("Accent color");
                    });

                    ui.horizontal(|ui| {
                        if ui
                            .color_edit_button_srgb(&mut state.persistent_settings.background_color)
                            .changed()
                        {
                            state.persistent_settings.save()
                        }
                        ui.label("Background color");
                    });

                    ui.end_row();

                    if ui
                    .checkbox(&mut state.persistent_settings.vsync, "Enable vsync")
                    .on_hover_text(
                        "Vsync reduces tearing and saves CPU. Toggling it off will make some operations such as panning/zooming more snappy. This needs a restart to take effect.",
                    )
                    .changed()
                {
                    state.persistent_settings.save()
                }
                if ui
                .checkbox(&mut state.persistent_settings.show_scrub_bar, "Show index slider")
                .on_hover_text(
                    "Enable an index slider to quickly scrub through lots of images",
                )
                .changed()
                {
                    state.persistent_settings.save()
                }
                    ui.end_row();

                    if ui
                    .checkbox(&mut state.persistent_settings.wrap_folder, "Wrap images at folder boundary")
                    .on_hover_text(
                        "When you move past the first or last image in a folder, should oculante continue or stop?",
                    )
                    .changed()
                {
                    state.persistent_settings.save();
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
                    state.persistent_settings.save()
                }
                });

                ui.end_row();
                if ui
                    .checkbox(&mut state.persistent_settings.keep_view, "Do not reset image view")
                    .on_hover_text(
                        "When a new image is loaded, keep current zoom and offset",
                    )
                    .changed()
                    {
                        state.persistent_settings.save()
                    }

                if ui
                    .checkbox(&mut state.persistent_settings.keep_edits, "Keep image edits")
                    .on_hover_text(
                        "When a new image is loaded, keep current edits",
                    )
                    .changed()
                    {
                        state.persistent_settings.save()
                    }
                ui.end_row();
                if ui
                    .checkbox(&mut state.persistent_settings.show_checker_background, "Show checker background.")
                    .on_hover_text(
                        "Show a checker pattern as backdrop.",
                    )
                    .changed()
                    {
                        state.persistent_settings.save();
                    }
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
                        state.persistent_settings.save()
                    }
                });

                if ui.link("Visit github repo").on_hover_text("Check out the source code, request a feature, submit a bug or leave a star if you like it!").clicked() {
                    _ = webbrowser::open("https://github.com/woelper/oculante");
                }


                ui.vertical_centered_justified(|ui| {

                    #[cfg(feature = "update")]
                    if ui.button("Check for updates").on_hover_text("Check and install update if available. You will need to restart the app to use the new version.").clicked() {
                        state.message = Some("Checking for updates...".into());
                        crate::update::update(Some(state.message_channel.0.clone()));
                        state.settings_enabled = false;
                    }

                    if ui.button("Reset all settings").clicked() {
                        state.persistent_settings = Default::default();
                    state.persistent_settings.save();
                    }
                });

                ui.collapsing("Keybindings",|ui| {
                    keybinding_ui(app, state, ui);
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
                        ImageOperation::Equalize((0, 255)),
                        ImageOperation::Posterize(8),
                        ImageOperation::ChannelSwap((Channel::Red, Channel::Red)),
                        ImageOperation::Rotate(90),
                        ImageOperation::HSV((0, 100, 100)),
                        ImageOperation::Crop([0, 0, 0, 0]),
                        ImageOperation::Mult([255, 255, 255]),
                        ImageOperation::Fill([255, 255, 255, 255]),
                        ImageOperation::Blur(0),
                        ImageOperation::MMult,
                        ImageOperation::MDiv,
                        ImageOperation::Expression("r = 1.0".into()),
                        ImageOperation::Noise {
                            amt: 50,
                            mono: false,
                        },
                        ImageOperation::Add([0, 0, 0]),
                        ImageOperation::Resize {
                            dimensions: state.image_dimension,
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

                    modifier_stack_ui(&mut state.edit_state.image_op_stack, &mut image_changed, ui);
                    modifier_stack_ui(
                        &mut state.edit_state.pixel_op_stack,
                        &mut pixels_changed,
                        ui,
                    );

                    ui.label_i("🔁 Reset");
                    ui.centered_and_justified(|ui| {
                        if ui.button("Reset all edits").clicked() {
                            state.edit_state = Default::default();
                            pixels_changed = true
                        }
                    });
                    ui.end_row();

                    ui.label_i("❓ Compare");
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
                                state.image_dimension = img.dimensions();
                                state.current_texture = img.to_texture(gfx);
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
                    if ctx.input().pointer.secondary_down() {
                        if let Some(stroke) = state.edit_state.paint_strokes.last_mut() {
                            if let Some(p) = state.edit_state.result_pixel_op.get_pixel_checked(
                                state.cursor_relative.x as u32,
                                state.cursor_relative.y as u32,
                            ) {
                                info!("{:?}", p);
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
                } else if ui.button("🖊 Paint mode").clicked() {
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
                    if ctx.input().pointer.primary_down() && !state.pointer_over_ui {
                        debug!("PAINT");
                        // get pos in image
                        // let p = state.cursor_relative;
                        let uv = (
                            state.cursor_relative.x / state.image_dimension.0 as f32,
                            (state.cursor_relative.y / state.image_dimension.1 as f32),
                        );
                        // info!("pnt @ {:?}", uv);
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
                    .button("⤵ Apply all edits")
                    .on_hover_text("Apply all edits to the image and reset edit controls")
                    .clicked()
                {
                    if let Some(img) = &mut state.current_image {
                        *img = state.edit_state.result_pixel_op.clone();
                        state.edit_state = Default::default();
                        // state.image_dimension = img.dimensions();
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
                    info!(
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

                                info!(
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
                                state.edit_state.result_pixel_op.to_texture(gfx);
                        }
                    }
                }

                info!(
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
                            info!("Committed stroke {}", i);
                        }
                    }
                }
            }

            state.image_dimension = state.edit_state.result_pixel_op.dimensions();

            ui.vertical_centered_justified(|ui| {
                if let Some(path) = &state.current_path {
                    if ui
                        .button("⟳ Reload image")
                        .on_hover_text("Completely reload image, destroying all edits.")
                        .clicked()
                    {
                        state.is_loaded = false;
                        state.player.load(&path, state.message_channel.0.clone());
                    }
                }

                ui.horizontal(|ui| {
                    ui.label("File:");

                    if let Some(p) = &mut state.current_path {
                        if let Some(pstem) = p.file_stem() {
                            let mut stem = pstem.to_string_lossy().to_string();
                            if ui.text_edit_singleline(&mut stem).changed() {
                                if let Some(parent) = p.parent() {
                                    *p = parent
                                        .join(stem)
                                        .with_extension(&state.edit_state.export_extension);
                                }
                            }
                        }
                    }
                    egui::ComboBox::from_id_source("ext")
                        .selected_text(&state.edit_state.export_extension)
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for f in ["png", "jpg", "bmp", "webp", "tif", "tga"] {
                                ui.selectable_value(
                                    &mut state.edit_state.export_extension,
                                    f.to_string(),
                                    f,
                                );
                            }
                        });
                });

                #[cfg(feature = "turbo")]
                jpg_lossless_ui(state, ui);

                if state.current_path.is_none() {
                    if ui.button("Save as...").clicked() {
                        let start_directory = &state.persistent_settings.last_open_directory;

                        let file_dialog_result = rfd::FileDialog::new()
                            .set_directory(start_directory)
                            .save_file();
                        if let Some(file_path) = file_dialog_result {
                            debug!("Selected File Path = {:?}", file_path);
                            _ = state
                                                .edit_state
                                                .result_pixel_op
                                                .save(file_path.with_extension(&state.edit_state.export_extension));
                            state.current_path = Some(file_path);
                        }
                    }
                }

                if let Some(p) = &state.current_path {
                    let text = if p
                        .with_extension(&state.edit_state.export_extension)
                        .exists()
                    {
                        "💾 Overwrite"
                    } else {
                        "💾 Save"
                    };

                    if ui.button(text).on_hover_text("Save the image. This will create a new file or overwrite.").clicked() {
                        _ = state
                            .edit_state
                            .result_pixel_op
                            .save(p.with_extension(&state.edit_state.export_extension));
                    }

                    if ui.button("💾 Save edits").on_hover_text("Saves an .oculante metafile in the same directory as the image. This file will contain all edits and will be restored automatically if you open the image again. This leaves the original image unmodified and allows you to continue editing later.").clicked() {
                        if let Ok(f) = std::fs::File::create(p.with_extension("oculante")) {
                            _ = serde_json::to_writer_pretty(&f, &state.edit_state);
                        }
                    }
                    if ui.button("💾 Save edits for directory").on_hover_text("Saves an .oculante metafile in the same directory as the image. This file will contain all edits and will be restored automatically if you open the image again. This leaves the original image unmodified and allows you to continue editing later.").clicked() {
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
    ui.add(egui::Button::new(RichText::new(text)).frame(false))
}

pub fn unframed_button_colored(text: impl Into<String>, is_colored: bool, ui: &mut Ui) -> Response {
    if is_colored {
        ui.add(
            egui::Button::new(
                RichText::new(text)
                    .heading()
                    .color(ui.style().visuals.selection.bg_fill),
            )
            .frame(false),
        )
    } else {
        ui.add(egui::Button::new(RichText::new(text).heading()).frame(false))
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
            ui.image(texture_id, egui::Vec2::splat(ui.available_height()));
        }

        let r = egui::ComboBox::from_id_source(format!("s {:?}", stroke.points))
            .selected_text(format!("Brush {}", stroke.brush_index))
            .show_ui(ui, |ui| {
                for (b_i, b) in brushes.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if let Some(notan_texture) = b.to_texture_premult(gfx) {
                            let texture_id = gfx.egui_register_texture(&notan_texture);
                            ui.image(texture_id, egui::Vec2::splat(32.));
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

fn modifier_stack_ui(stack: &mut Vec<ImageOperation>, image_changed: &mut bool, ui: &mut Ui) {
    let mut delete: Option<usize> = None;
    let mut swap: Option<(usize, usize)> = None;

    // egui::Grid::new("dfdfd").num_columns(2).show(ui, |ui| {
    for (i, operation) in stack.iter_mut().enumerate() {
        ui.label_i(&format!("{operation}"));

        // let op draw itself and check for response

        ui.push_id(i, |ui| {
            // ui.end_row();

            // draw the image operator
            if operation.ui(ui).changed() {
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
        .collect::<HashSet<String>>();

    let mut changed = false;

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
                            changed = true;
                        }
                    } else {
                        ui.add_enabled(false, egui::Button::new("Press key(s)..."));
                    }
                    ui.end_row();
                }
            });
        });
    if changed {
        state.persistent_settings.save();
    }
}

// fn keystrokes(ui: &mut Ui) {
//     ui.add(Button::new(format!("{:?}", k.0)).fill(Color32::DARK_BLUE));
// }

pub fn main_menu(ui: &mut Ui, state: &mut OculanteState, app: &mut App, gfx: &mut Graphics) {
    ui.horizontal_centered(|ui| {
        use crate::shortcuts::InputEvent::*;

        ui.label("Channels");

        let mut changed_channels = false;

        if key_pressed(app, state, RedChannel) {
            state.current_channel = ColorChannel::Red;
            changed_channels = true;
        }
        if key_pressed(app, state, GreenChannel) {
            state.current_channel = ColorChannel::Green;
            changed_channels = true;
        }
        if key_pressed(app, state, BlueChannel) {
            state.current_channel = ColorChannel::Blue;
            changed_channels = true;
        }
        if key_pressed(app, state, AlphaChannel) {
            state.current_channel = ColorChannel::Alpha;
            changed_channels = true;
        }

        if key_pressed(app, state, RGBChannel) {
            state.current_channel = ColorChannel::Rgb;
            changed_channels = true;
        }
        if key_pressed(app, state, RGBAChannel) {
            state.current_channel = ColorChannel::Rgba;
            changed_channels = true;
        }

        ui.add_enabled_ui(!state.persistent_settings.edit_enabled, |ui| {
            // hack to center combo box in Y

            ui.spacing_mut().button_padding = Vec2::new(10., 0.);
            egui::ComboBox::from_id_source("channels")
                .selected_text(format!("{:?}", state.current_channel))
                .show_ui(ui, |ui| {
                    for channel in ColorChannel::iter() {
                        let r = ui.selectable_value(
                            &mut state.current_channel,
                            channel,
                            channel.to_string(),
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
                match &state.current_channel {
                    ColorChannel::Rgb => state.current_texture = unpremult(img).to_texture(gfx),
                    ColorChannel::Rgba => state.current_texture = img.to_texture(gfx),
                    _ => {
                        state.current_texture =
                            solo_channel(img, state.current_channel as usize).to_texture(gfx)
                    }
                }
            }
        }

        if state.current_image.is_some() {
            if state.current_path.is_some() {
                if tooltip(
                    unframed_button("◀", ui),
                    "Previous image",
                    &lookup(&state.persistent_settings.shortcuts, &PreviousImage),
                    ui,
                )
                .clicked()
                {
                    prev_image(state)
                }
                if tooltip(
                    unframed_button("▶", ui),
                    "Next image",
                    &lookup(&state.persistent_settings.shortcuts, &NextImage),
                    ui,
                )
                .clicked()
                {
                    next_image(state)
                }
            }

            if tooltip(
                // ui.checkbox(&mut state.info_enabled, "ℹ Info"),
                ui.selectable_label(state.persistent_settings.info_enabled, "ℹ Info"),
                "Show image info",
                &lookup(&state.persistent_settings.shortcuts, &InfoMode),
                ui,
            )
            .clicked()
            {
                state.persistent_settings.info_enabled = !state.persistent_settings.info_enabled;
                // TODO: Remove if save on exit
                state.persistent_settings.save();
                send_extended_info(
                    &state.current_image,
                    &state.current_path,
                    &state.extended_info_channel,
                );
            }

            if tooltip(
                ui.selectable_label(state.persistent_settings.edit_enabled, "✏ Edit"),
                "Edit the image",
                &lookup(&state.persistent_settings.shortcuts, &EditMode),
                ui,
            )
            .clicked()
            {
                state.persistent_settings.edit_enabled = !state.persistent_settings.edit_enabled;
                // TODO: Remove if save on exit
                state.persistent_settings.save();
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

        if unframed_button("⛶", ui).clicked() {
            toggle_fullscreen(app, state);
        }

        if tooltip(
            unframed_button_colored("📌", state.always_on_top, ui),
            "Always on top",
            &lookup(&state.persistent_settings.shortcuts, &AlwaysOnTop),
            ui,
        )
        .clicked()
        {
            state.always_on_top = !state.always_on_top;
            app.window().set_always_on_top(state.always_on_top);
        }

        #[cfg(feature = "file_open")]
        if unframed_button("🗁", ui)
            .on_hover_text("Browse for image")
            .clicked()
        {
            browse_for_image_path(state)
        }

        ui.scope(|ui| {
            // ui.style_mut().override_text_style = Some(egui::TextStyle::Heading);
            // maybe override font size?
            ui.style_mut().visuals.button_frame = false;
            ui.style_mut().visuals.widgets.inactive.expansion = 20.;

            // FIXME: Needs submenu not to be out of bounds
            // ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {

            ui.style_mut().override_text_style = Some(egui::TextStyle::Heading);

            ui.menu_button("☰", |ui| {
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
                    if let Ok(clipboard) = &mut Clipboard::new() {
                        if let Ok(imagedata) = clipboard.get_image() {
                            if let Some(image) = image::RgbaImage::from_raw(
                                imagedata.width as u32,
                                imagedata.height as u32,
                                (imagedata.bytes).to_vec(),
                            ) {
                                // Stop in the even that an animation is running
                                state.player.stop();
                                _ = state
                                    .player
                                    .image_sender
                                    .send(crate::utils::Frame::new_still(image));
                                // Since pasted data has no path, make sure it's not set
                                state.current_path = None;
                            }
                        }
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
    });
}
