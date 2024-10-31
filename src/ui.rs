use crate::{
    appstate::{ImageGeometry, Message, OculanteState},
    clear_image, clipboard_to_image, delete_file,
    file_encoder::FileEncoder,
    image_editing::{process_pixels, Channel, GradientStop, ImageOperation, ScaleFilter},
    paint::PaintStroke,
    set_zoom,
    settings::{set_system_theme, ColorTheme, PersistentSettings, VolatileSettings},
    shortcuts::{key_pressed, keypresses_as_string, lookup},
    utils::{
        clipboard_copy, disp_col, disp_col_norm, fix_exif, highlight_bleed, highlight_semitrans,
        load_image_from_path, next_image, prev_image, send_extended_info, set_title, solo_channel,
        toggle_fullscreen, unpremult, ColorChannel, ImageExt,
    },
};
#[cfg(not(feature = "file_open"))]
use crate::{filebrowser, SUPPORTED_EXTENSIONS};

const ICON_SIZE: f32 = 24. * 0.8;
const ROUNDING: f32 = 8.;
pub const BUTTON_HEIGHT_LARGE: f32 = 35.;
pub const BUTTON_HEIGHT_SMALL: f32 = 24.;

use crate::icons::*;
use egui_plot::{Line, Plot, PlotPoints};
use epaint::TextShape;
use image::{DynamicImage, RgbaImage};
use log::{debug, error, info};
#[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
use mouse_position::mouse_position::Mouse;
use notan::{
    egui::{self, *},
    prelude::{App, Graphics},
};
use std::{
    collections::BTreeSet,
    ops::RangeInclusive,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use strum::IntoEnumIterator;
const PANEL_WIDTH: f32 = 240.0;
const PANEL_WIDGET_OFFSET: f32 = 0.0;

#[cfg(feature = "turbo")]
use crate::image_editing::{cropped_range, lossless_tx};
pub trait EguiExt {
    fn label_i(&mut self, _text: impl Into<WidgetText>) -> Response {
        unimplemented!()
    }

    fn label_right(&mut self, _text: impl Into<WidgetText>) -> Response {
        unimplemented!()
    }

    fn label_i_selected(&mut self, _selected: bool, _text: impl Into<WidgetText>) -> Response {
        unimplemented!()
    }

    fn styled_slider<Num: emath::Numeric>(
        &mut self,
        _value: &mut Num,
        _range: RangeInclusive<Num>,
    ) -> Response {
        unimplemented!()
    }

    fn styled_checkbox(&mut self, _checked: &mut bool, _text: impl Into<WidgetText>) -> Response {
        unimplemented!()
    }

    fn styled_button(&mut self, _text: impl Into<WidgetText>) -> Response {
        unimplemented!()
    }

    fn styled_menu_button(
        &mut self,
        _title: impl Into<WidgetText>,
        _add_contents: impl FnOnce(&mut Ui),
    ) -> Response {
        unimplemented!()
    }

    fn styled_selectable_label(&mut self, _active: bool, _text: impl Into<WidgetText>) -> Response {
        unimplemented!()
    }

    fn _styled_label(&mut self, _text: impl Into<WidgetText>) -> Response {
        unimplemented!()
    }

    fn slider_timeline<Num: emath::Numeric>(
        &mut self,
        _value: &mut Num,
        _range: RangeInclusive<Num>,
    ) -> Response {
        unimplemented!()
    }

    fn get_rounding(&self, _height: f32) -> f32 {
        unimplemented!()
    }

    fn styled_collapsing<R>(
        &mut self,
        _heading: impl Into<WidgetText>,
        _add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> CollapsingResponse<R> {
        todo!()
    }
}

impl EguiExt for Ui {
    fn get_rounding(&self, height: f32) -> f32 {
        if height > 25. {
            self.style().visuals.widgets.inactive.rounding.ne * 2.
        } else {
            self.style().visuals.widgets.inactive.rounding.ne
        }
    }

    fn styled_checkbox(&mut self, checked: &mut bool, text: impl Into<WidgetText>) -> Response {
        let color = self.style().visuals.selection.bg_fill;

        let text = text.into();
        let spacing = &self.spacing();
        let icon_width = spacing.icon_width;
        let icon_spacing = spacing.icon_spacing;

        let (galley, mut desired_size) = if text.is_empty() {
            (None, vec2(icon_width, 0.0))
        } else {
            let total_extra = vec2(icon_width + icon_spacing, 0.0);

            let wrap_width = self.available_width() - total_extra.x;
            let galley = text.into_galley(self, None, wrap_width, TextStyle::Button);

            let mut desired_size = total_extra + galley.size();
            desired_size = desired_size.at_least(spacing.interact_size);

            (Some(galley), desired_size)
        };

        desired_size = desired_size.at_least(Vec2::splat(spacing.interact_size.y));
        desired_size.y = desired_size.y.max(icon_width);
        let (rect, mut response) = self.allocate_exact_size(desired_size, Sense::click());

        if response.clicked() {
            *checked = !*checked;
            response.mark_changed();
        }
        response.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::Checkbox,
                *checked,
                galley.as_ref().map_or("", |x| x.text()),
            )
        });

        if self.is_rect_visible(rect) {
            // let visuals = self.style().interact_selectable(&response, *checked); // too colorful
            let visuals = self.style().interact(&response);
            let (small_icon_rect, big_icon_rect) = self.spacing().icon_rectangles(rect);
            self.painter().add(epaint::RectShape::new(
                big_icon_rect.expand(visuals.expansion),
                visuals.rounding,
                if *checked {
                    color.gamma_multiply(0.3)
                } else {
                    visuals.bg_fill
                },
                visuals.bg_stroke,
            ));
            if *checked {
                // Check mark:

                let mut stroke = visuals.fg_stroke;
                stroke.color = color;
                self.painter().add(Shape::line(
                    vec![
                        pos2(small_icon_rect.left(), small_icon_rect.center().y),
                        pos2(
                            small_icon_rect.center().x - 1.,
                            small_icon_rect.bottom() - 1.,
                        ),
                        pos2(small_icon_rect.right(), small_icon_rect.top() + 1.),
                    ],
                    stroke,
                ));
            }
            if let Some(galley) = galley {
                let text_pos = pos2(
                    rect.min.x + icon_width + icon_spacing,
                    rect.center().y - 0.5 * galley.size().y,
                );
                self.painter()
                    .galley(text_pos, galley, visuals.text_color());
            }
        }

        response
    }

    /// Draw a justified icon from a string starting with an emoji
    fn label_i(&mut self, text: impl Into<WidgetText>) -> Response {
        let text: WidgetText = text.into();
        let text = text.text();

        let icon = text.chars().filter(|c| !c.is_ascii()).collect::<String>();
        let description = text.chars().filter(|c| c.is_ascii()).collect::<String>();

        self.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
            // self.horizontal(|ui| {
            ui.add_sized(
                egui::Vec2::new(8., ui.available_height()),
                egui::Label::new(RichText::new(icon).color(ui.style().visuals.selection.bg_fill)),
            );
            ui.label(
                RichText::new(description).color(ui.style().visuals.noninteractive().text_color()),
            );
        })
        .response
    }

    fn styled_menu_button(
        &mut self,
        title: impl Into<WidgetText>,
        add_contents: impl FnOnce(&mut Ui),
    ) -> Response {
        let text: WidgetText = title.into();
        let text = text.text();

        let icon = text.chars().filter(|c| !c.is_ascii()).collect::<String>();
        let description = text.chars().filter(|c| c.is_ascii()).collect::<String>();
        let spacing = if icon.len() == 0 { "" } else { "       " };
        self.spacing_mut().button_padding = Vec2::new(0., 10.);

        let r = self.menu_button(format!("{spacing}{description}"), add_contents);

        let mut icon_pos = r.response.rect.left_center();
        icon_pos.x += 16.;

        self.painter().text(
            icon_pos,
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(16.),
            self.style().visuals.selection.bg_fill,
        );

        r.response
    }

    /// Draw a justified icon from a string starting with an emoji
    fn styled_button(&mut self, text: impl Into<WidgetText>) -> Response {
        let text: WidgetText = text.into();
        let text = text.text();

        let icon = text.chars().filter(|c| !c.is_ascii()).collect::<String>();
        let description = text.chars().filter(|c| c.is_ascii()).collect::<String>();

        let spacing = if icon.len() == 0 { "" } else { "      " };
        let r = self.add(
            egui::Button::new(format!("{spacing}{description}"))
                .rounding(self.get_rounding(BUTTON_HEIGHT_LARGE))
                .min_size(vec2(140., BUTTON_HEIGHT_LARGE)), // .shortcut_text("sds")
        );

        let mut icon_pos = r.rect.left_center();
        icon_pos.x += 16.;

        self.painter().text(
            icon_pos,
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(16.),
            self.style().visuals.selection.bg_fill,
        );
        r
    }

    /// Draw a justified icon from a string starting with an emoji
    fn styled_selectable_label(&mut self, _active: bool, text: impl Into<WidgetText>) -> Response {
        let text: WidgetText = text.into();
        let text = text.text();

        let icon_size = 12.;
        let icon = text.chars().filter(|c| !c.is_ascii()).collect::<String>();
        let description = text.chars().filter(|c| c.is_ascii()).collect::<String>();
        self.spacing_mut().button_padding = Vec2::new(8., 0.);
        // self.style_mut().visuals.widgets.inactive.rounding = Rounding::same(6.);

        let spacing = if icon.len() == 0 { "" } else { "  " };
        let r = self.add(
            egui::Button::new(format!("{description}{spacing}"))
                .rounding(self.get_rounding(BUTTON_HEIGHT_LARGE))
                .min_size(vec2(0., BUTTON_HEIGHT_LARGE)), // .shortcut_text("sds")
        );

        let mut icon_pos = r.rect.right_center();
        icon_pos.x -= icon_size;

        self.painter().text(
            icon_pos,
            Align2::CENTER_CENTER,
            icon,
            FontId::proportional(icon_size),
            self.style().visuals.selection.bg_fill,
        );

        r

        // self.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
        //     // self.horizontal(|ui| {
        //     ui.add_sized(
        //         egui::Vec2::new(8., ui.available_height()),
        //         egui::Label::new(RichText::new(icon).color(ui.style().visuals.selection.bg_fill)),
        //     );
        //     ui.label(
        //         RichText::new(description).color(ui.style().visuals.noninteractive().text_color()),
        //     );
        // })
        // .response
    }

    /// Draw a justified icon from a string starting with an emoji
    fn label_right(&mut self, text: impl Into<WidgetText>) -> Response {
        self.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            // self.horizontal(|ui| {

            ui.label(text);
        })
        .response
    }

    fn styled_collapsing<R>(
        &mut self,
        heading: impl Into<WidgetText>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> CollapsingResponse<R> {
        self.style_mut().visuals.collapsing_header_frame = true;
        self.style_mut().visuals.indent_has_left_vline = false;

        CollapsingHeader::new(heading)
            // .show_background(true)
            .icon(caret_icon)
            .show_unindented(self, add_contents)
    }

    /// Draw a justified icon from a string starting with an emoji
    fn label_i_selected(&mut self, selected: bool, text: impl Into<WidgetText>) -> Response {
        let text: WidgetText = text.into();
        let text = text.text();

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
                r.clicked = true;
            }
            r
        })
        .inner
    }

    fn styled_slider<Num: emath::Numeric>(
        &mut self,
        value: &mut Num,
        range: RangeInclusive<Num>,
    ) -> Response {
        self.scope(|ui| {
            ui.style_mut().spacing.interact_size.y = 18.;

            let color = ui.style().visuals.selection.bg_fill;
            let style = ui.style_mut();

            style.visuals.widgets.inactive.fg_stroke.width = 7.0;
            style.visuals.widgets.inactive.fg_stroke.color = color;
            style.visuals.widgets.inactive.rounding =
                style.visuals.widgets.inactive.rounding.at_least(18.);
            style.visuals.widgets.inactive.expansion = -4.0;

            style.visuals.widgets.hovered.fg_stroke.width = 9.0;
            style.visuals.widgets.hovered.fg_stroke.color = color;
            style.visuals.widgets.hovered.rounding =
                style.visuals.widgets.hovered.rounding.at_least(18.);
            style.visuals.widgets.hovered.expansion = -4.0;

            style.visuals.widgets.active.fg_stroke.width = 9.0;
            style.visuals.widgets.active.fg_stroke.color = color;
            style.visuals.widgets.active.rounding =
                style.visuals.widgets.active.rounding.at_least(18.);
            style.visuals.widgets.active.expansion = -4.0;

            ui.horizontal(|ui| {
                let r = ui.add(
                    Slider::new(value, range)
                        .trailing_fill(true)
                        .handle_shape(style::HandleShape::Rect { aspect_ratio: 2.1 })
                        .show_value(false)
                        .integer(),
                );
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
            let available_width = ui.available_width() * 1. - 60.;
            let style = ui.style_mut();
            style.spacing.interact_size.y = 18.;

            style.visuals.widgets.hovered.bg_fill = color;
            style.visuals.widgets.hovered.fg_stroke.width = 0.;
            style.visuals.widgets.hovered.expansion = -1.5;

            style.visuals.widgets.active.bg_fill = color;
            style.visuals.widgets.active.fg_stroke.width = 0.;
            style.visuals.widgets.active.expansion = -2.5;

            style.visuals.widgets.inactive.fg_stroke.width = 5.0;
            style.visuals.widgets.inactive.fg_stroke.color = color;
            style.visuals.widgets.inactive.rounding =
                style.visuals.widgets.inactive.rounding.at_least(20.);
            style.visuals.widgets.inactive.expansion = -5.0;

            style.spacing.slider_width = available_width;

            ui.horizontal(|ui| {
                let r = ui.add(
                    Slider::new(value, range.clone())
                        .handle_shape(style::HandleShape::Rect { aspect_ratio: 2.1 })
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
    if let Some(texture) = &state.current_texture.get() {
        //let tex_id = gfx.egui_register_texture(&texture.texture_array[0]); //TODO: Adapt if needed

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

        /*egui::Painter::new(ctx.clone(), LayerId::background(), ctx.available_rect()).image(
            tex_id.id,
            image_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );*/
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

pub fn info_ui(ctx: &Context, state: &mut OculanteState, _gfx: &mut Graphics) {
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
    .show_separator_line(false)
    .exact_width(PANEL_WIDTH)
    .resizable(false)
    .show(ctx, |ui| {

        egui::ScrollArea::vertical().auto_shrink([false,true])
            .show(ui, |ui| {
            if let Some(texture) = &state.current_texture.get() {
                let desired_width = PANEL_WIDTH as f64 - PANEL_WIDGET_OFFSET as f64;
                let scale = (desired_width / 8.) / texture.size().0 as f64;
                let uv_center = (
                    state.cursor_relative.x as f64 / state.image_geometry.dimensions.0 as f64,
                    (state.cursor_relative.y as f64 / state.image_geometry.dimensions.1 as f64),
                );

                egui::Grid::new("info")
                    .num_columns(2)
                    .show(ui, |ui| {
                    ui.label_i(&format!("{ARROWS_OUT} Size",));
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
                        ui.label_i(&format!("{} File", IMAGE));
                        let path_label = egui::Label::new(
                            RichText::new(file_name)
                        ).truncate(true);
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            ui.add(path_label)
                            .on_hover_text(format!("{}", path.display()));
                        });
                        ui.end_row();
                    }

                    ui.label_i(&format!("{PALETTE} RGBA"));
                    ui.label_right(
                        RichText::new(disp_col(state.sampled_color))
                    );
                    ui.end_row();

                    ui.label_i(&format!("{PALETTE} RGBA"));
                    ui.label_right(
                        RichText::new(disp_col_norm(state.sampled_color, 255.))
                    );
                    ui.end_row();

                    ui.label_i("⊞ Pos");
                    ui.label_right(
                        RichText::new(format!(
                            "{:.0},{:.0}",
                            state.cursor_relative.x, state.cursor_relative.y
                        ))
                    );
                    ui.end_row();

                    ui.label_i(&format!("{INTERSECT} UV"));
                    ui.label_right(
                        RichText::new(format!("{:.3},{:.3}", uv_center.0, 1.0 - uv_center.1))
                    );
                    ui.end_row();
                });

                // make sure aspect ratio is compensated for the square preview
                let ratio = texture.size().0 as f64 / texture.size().1 as f64;
                let uv_size = (scale, scale * ratio);
                let bbox_tl: Pos2;
                let bbox_br: Pos2;
                if texture.texture_count == 1 {
                    let texture_resonse = texture.get_texture_at_xy(0,0);
                    ui.add_space(10.);
                    let preview_rect = ui
                        .add(
                            egui::Image::new(texture_resonse.texture.texture_egui)
                            .maintain_aspect_ratio(false)
                            .fit_to_exact_size(egui::Vec2::splat(desired_width as f32))
                            .rounding(ROUNDING)
                            .uv(egui::Rect::from_x_y_ranges(
                                (uv_center.0 - uv_size.0) as f32..=(uv_center.0 + uv_size.0) as f32,
                                (uv_center.1 - uv_size.1) as f32..=(uv_center.1 + uv_size.1) as f32,
                            )),
                        )
                        .rect;
                    bbox_tl = preview_rect.left_top();
                    bbox_br = preview_rect.right_bottom();
                }
                else {
                    (bbox_tl, bbox_br) = render_info_image_tiled(ui, uv_center,uv_size, desired_width, texture);
                }

                let bg_color = Color32::BLACK.linear_multiply(0.5);
                let preview_rect = egui::Rect::from_min_max(bbox_tl, bbox_br);
                ui.advance_cursor_after_rect(preview_rect);


                let stroke_color = Color32::from_white_alpha(240);

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
            ui.add_space(10.);
            ui.vertical_centered_justified(|ui| {
                ui.styled_collapsing("Compare", |ui| {

                    if state.persistent_settings.max_cache == 0 {
                        ui.label("Warning! Set your cache to more than 0 in settings for this to be fast.");
                    }
                    ui.vertical_centered_justified(|ui| {
                        dark_panel(ui, |ui| {
                            if ui.button(&format!("{FOLDER} Open another image...")).clicked() {
                                // TODO: Automatically insert image into compare list
                                #[cfg(feature = "file_open")]
                                crate::browse_for_image_path(state);
                                #[cfg(not(feature = "file_open"))]
                                ui.ctx().memory_mut(|w| w.open_popup(Id::new("OPEN")));
                            }
                            let mut compare_list: Vec<(PathBuf, ImageGeometry)> = state.compare_list.clone().into_iter().collect();
                            compare_list.sort_by(|a,b| a.0.cmp(&b.0));

                            for (path, geo) in compare_list {
                                ui.horizontal(|ui|{
                                    if ui.button(X).clicked() {
                                        state.compare_list.remove(&path);
                                    }
                                    ui.vertical_centered_justified(|ui| {
                                        if ui.selectable_label(state.current_path.as_ref() == Some(&path), path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default().to_string()).clicked(){
                                            state
                                                .player
                                                .load_advanced(&path, Some(crate::utils::Frame::CompareResult(Default::default(), geo.clone())), state.message_channel.0.clone());
                                            ui.ctx().request_repaint();
                                            ui.ctx().request_repaint_after(Duration::from_millis(500));
                                            state.current_path = Some(path);
                                            state.image_info = None;
                                        }
                                    });
                                });
                            }
                            if let Some(path) = &state.current_path {
                                if let Some(geo) = state.compare_list.get(path) {
                                    if state.image_geometry != *geo {
                                        if ui.button(RichText::new(format!("{LOCATION_PIN} Update position")).color(Color32::YELLOW)).clicked() {
                                            state.compare_list.insert(path.clone(), state.image_geometry.clone());
                                        }
                                    }
                                } else {
                                    if ui.button(format!("{PLUS} Add current image")).clicked() {
                                        state.compare_list.insert(path.clone(), state.image_geometry.clone());
                                    }
                                }
                            }
                            if !state.compare_list.is_empty() {
                                if ui.button(format!("{TRASH} Clear all")).clicked() {
                                    state.compare_list.clear();
                                }
                            }
                        });
                    });
                });
            });

            if state.current_texture.get().is_some() {
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
                                    state.send_frame(crate::Frame::UpdateTexture);
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
                                    state.send_frame(crate::Frame::UpdateTexture);
                                    ui.ctx().request_repaint();
                                }
                                if ui.button("Reset image").clicked() {
                                    state.edit_state.result_pixel_op = Default::default();

                                    state.send_frame(crate::Frame::UpdateTexture);
                                }
                            }
                        });
                    });
                });
            }

            if state.current_texture.get().is_some() {
                ui.horizontal(|ui| {
                    ui.label("Tiling");
                    ui.style_mut().spacing.slider_width = ui.available_width() - 16.;
                    ui.styled_slider(&mut state.tiling, 1..=10);
                });
            }
            advanced_ui(ui, state);
        });
    });
}

fn render_info_image_tiled(
    ui: &mut Ui,
    uv_center: (f64, f64),
    uv_size: (f64, f64),
    desired_width: f64,
    texture: &crate::texture_wrapper::TexWrap,
) -> (Pos2, Pos2) {
    let xy_size = (
        (texture.width() as f64 * uv_size.0) as i32,
        (texture.height() as f64 * uv_size.1) as i32,
    );

    let xy_center = (
        (texture.width() as f64 * uv_center.0) as i32,
        (texture.height() as f64 * uv_center.1) as i32,
    ); //(16384,1024);//
    let sc1 = (
        2.0 * xy_size.0 as f64 / desired_width,
        2.0 * xy_size.1 as f64 / desired_width,
    );

    //coordinates of the image-view
    let mut bbox_tl = egui::pos2(f32::MAX, f32::MAX);
    let mut bbox_br = egui::pos2(f32::MIN, f32::MIN);

    //Ui position to start at
    let base_ui_curs = nalgebra::Vector2::new(ui.cursor().min.x as f64, ui.cursor().min.y as f64);
    let mut curr_ui_curs = base_ui_curs;
    //our start position

    //Loop control variables, start end end coordinates of interest
    let x_coordinate_end = (xy_center.0 + xy_size.0) as i32;
    let mut y_coordinate = xy_center.1 - xy_size.1;
    let y_coordinate_end = (xy_center.1 + xy_size.1) as i32;

    while y_coordinate <= y_coordinate_end {
        let mut y_coordinate_increment = 0; //increment for y coordinate after x loop
        let mut x_coordinate = xy_center.0 - xy_size.0;
        curr_ui_curs.x = base_ui_curs.x;
        let mut last_display_size_y: f64 = 0.0;
        while x_coordinate <= x_coordinate_end {
            //get texture tile
            let curr_tex_response =
                texture.get_texture_at_xy(x_coordinate as i32, y_coordinate as i32);

            //increment coordinates by usable width/height
            y_coordinate_increment = curr_tex_response.offset_height;
            x_coordinate += curr_tex_response.offset_width;

            //Handling last texture in a row or col
            let mut curr_tex_end = nalgebra::Vector2::new(
                i32::min(curr_tex_response.x_tex_right_global, x_coordinate_end),
                i32::min(curr_tex_response.y_tex_bottom_global, y_coordinate_end),
            );

            //Handling positive coordinate overflow
            if curr_tex_response.x_tex_right_global as f32 >= texture.width() - 1.0f32 {
                x_coordinate = x_coordinate_end + 1;
                curr_tex_end.x += (x_coordinate_end - curr_tex_response.x_tex_right_global).max(0);
            }

            if curr_tex_response.y_tex_bottom_global as f32 >= texture.height() - 1.0f32 {
                y_coordinate_increment = y_coordinate_end - y_coordinate + 1;
                curr_tex_end.y += (y_coordinate_end - curr_tex_response.y_tex_bottom_global).max(0);
            }

            //Usable tile size, depending on offsets
            let tile_size = nalgebra::Vector2::new(
                curr_tex_end.x
                    - curr_tex_response.x_offset_texture
                    - curr_tex_response.x_tex_left_global
                    + 1,
                curr_tex_end.y
                    - curr_tex_response.y_offset_texture
                    - curr_tex_response.y_tex_top_global
                    + 1,
            );

            //Display size - tile size scaled
            let display_size =
                nalgebra::Vector2::new(tile_size.x as f64 / sc1.0, tile_size.y as f64 / sc1.1);

            //Texture display range
            let uv_start = nalgebra::Vector2::new(
                curr_tex_response.x_offset_texture as f64 / curr_tex_response.texture_width as f64,
                curr_tex_response.y_offset_texture as f64 / curr_tex_response.texture_height as f64,
            );

            let uv_end = nalgebra::Vector2::new(
                (curr_tex_end.x - curr_tex_response.x_tex_left_global + 1) as f64
                    / curr_tex_response.texture_width as f64,
                (curr_tex_end.y - curr_tex_response.y_tex_top_global + 1) as f64
                    / curr_tex_response.texture_height as f64,
            );

            //let tex_id2 = gfx.egui_register_texture(curr_tex_response.texture);
            let draw_tl_32 = Pos2::new(curr_ui_curs.x as f32, curr_ui_curs.y as f32);
            let draw_br_32 = Pos2::new(
                (curr_ui_curs.x + display_size.x) as f32,
                (curr_ui_curs.y + display_size.y) as f32,
            );
            let r_ret = egui::Rect::from_min_max(draw_tl_32, draw_br_32);

            egui::Image::new(curr_tex_response.texture.texture_egui)
                .maintain_aspect_ratio(false)
                .fit_to_exact_size(egui::Vec2::new(
                    display_size.x as f32,
                    display_size.y as f32,
                ))
                .uv(egui::Rect::from_x_y_ranges(
                    uv_start.x as f32..=uv_end.x as f32,
                    uv_start.y as f32..=uv_end.y as f32,
                ))
                .paint_at(ui, r_ret);

            //Update display cursor
            curr_ui_curs.x += display_size.x;
            last_display_size_y = display_size.y;

            //Update coordinates for preview rectangle
            bbox_tl.x = bbox_tl.x.min(r_ret.left());
            bbox_tl.y = bbox_tl.y.min(r_ret.top());
            bbox_br.x = bbox_br.x.max(r_ret.right());
            bbox_br.y = bbox_br.y.max(r_ret.bottom());
        }
        //Update y coordinates
        y_coordinate += y_coordinate_increment;
        curr_ui_curs.y += last_display_size_y;
    }
    (bbox_tl, bbox_br)
}

pub fn settings_ui(app: &mut App, ctx: &Context, state: &mut OculanteState, _gfx: &mut Graphics) {
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
                        apply_theme(state, ctx);
                    }
                }
                );


                egui::Grid::new("settings").num_columns(2).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .color_edit_button_srgb(&mut state.persistent_settings.accent_color)
                            .changed()
                        {
                          apply_theme(state, ctx);
                        }
                        ui.label("Accent color");
                    });

                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgb(&mut state.persistent_settings.background_color);
                        ui.label("Background color");
                    });

                    ui.end_row();

                    ui
                    .styled_checkbox(&mut state.persistent_settings.vsync, "Enable vsync")
                    .on_hover_text(
                        "Vsync reduces tearing and saves CPU. Toggling it off will make some operations such as panning/zooming more snappy. This needs a restart to take effect.",
                    );
                ui
                .styled_checkbox(&mut state.persistent_settings.show_scrub_bar, "Show index slider")
                .on_hover_text(
                    "Enable an index slider to quickly scrub through lots of images",
                );
                    ui.end_row();

                    if ui
                    .styled_checkbox(&mut state.persistent_settings.wrap_folder, "Wrap images at folder boundaries")
                    .on_hover_text(
                        "When you move past the first or last image in a folder, should oculante continue or stop?",
                    )
                    .changed()
                {
                    state.scrubber.wrap = state.persistent_settings.wrap_folder;
                }
                ui.horizontal(|ui| {
                    ui.label("Number of images to cache");
                    if ui
                    .add(egui::DragValue::new(&mut state.persistent_settings.max_cache).clamp_range(0..=10000))

                    .on_hover_text(
                        "Keeps this many images in memory for faster opening.",
                    )
                    .changed()
                {
                    state.player.cache.cache_size = state.persistent_settings.max_cache;
                    state.player.cache.clear();
                }
                });

                ui.end_row();
                ui
                    .styled_checkbox(&mut state.persistent_settings.keep_view, "Do not reset image view")
                    .on_hover_text(
                        "When a new image is loaded, keep current zoom and offset",
                    );

                ui
                    .styled_checkbox(&mut state.persistent_settings.keep_edits, "Keep image edits")
                    .on_hover_text(
                        "When a new image is loaded, keep current edits",
                    );
                ui.end_row();
                ui
                    .styled_checkbox(&mut state.persistent_settings.show_checker_background, "Show checker background where transparent")
                    .on_hover_text(
                        "Show checker pattern as backdrop.",
                    );

                ui
                    .styled_checkbox(&mut state.persistent_settings.show_frame, "Draw frame around image")
                    .on_hover_text(
                        "Draw a small frame around the image. It is centered on the outmost pixel. This can be helpful on images with lots of transparency.",
                    );
                    ui.end_row();
                if ui.styled_checkbox(&mut state.persistent_settings.zen_mode, "Turn on Zen mode").on_hover_text("Zen mode hides all UI and fits the image to the frame.").changed(){
                    set_title(app, state);
                }
                if ui.styled_checkbox(&mut state.persistent_settings.force_redraw, "Redraw every frame").on_hover_text("Requires a restart. Turns off optimisations and redraws everything each frame. This will consume more CPU but gives you instant feedback, for example if new images come in or if modifications are made.").changed(){
                    app.window().set_lazy_loop(!state.persistent_settings.force_redraw);
                }

                // ui.label(format!("lazy {}", app.window().lazy_loop()));
                ui.end_row();
                if ui.styled_checkbox(&mut state.persistent_settings.linear_mag_filter, "Interpolate when zooming in").on_hover_text("When zooming in, do you prefer to see individual pixels or an interpolation?").changed(){
                    state.send_frame(crate::Frame::UpdateTexture);
                }
                if ui.styled_checkbox(&mut state.persistent_settings.linear_min_filter, "Interpolate when zooming out").on_hover_text("When zooming out, do you prefer crisper or smoother pixels?").changed(){
                    state.send_frame(crate::Frame::UpdateTexture);
                }
                ui.end_row();

                if ui.styled_checkbox(&mut state.persistent_settings.use_mipmaps, "Use mipmaps").on_hover_text("When zooming out, less memory will be used. Faster performance, but blurry.").changed(){
                    state.send_frame(crate::Frame::UpdateTexture);
                }

                ui.styled_checkbox(&mut state.persistent_settings.fit_image_on_window_resize, "Fit image on window resize").on_hover_text("When you resize the main window, do you want to fit the image with it?");
                ui.end_row();

                ui.add(egui::DragValue::new(&mut state.persistent_settings.zoom_multiplier).clamp_range(0.05..=10.0).prefix("Zoom multiplier: ").speed(0.01)).on_hover_text("Adjust how much you zoom when you use the mouse wheel or the trackpad.");
                #[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
                ui.styled_checkbox(&mut state.persistent_settings.borderless, "Borderless mode").on_hover_text("Don't draw OS window decorations. Needs restart.");
                ui.end_row();

                ui.label("Minimum window size");
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut state.persistent_settings.min_window_size.0).clamp_range(1..=2000).prefix("x : ").speed(0.01));
                    ui.add(egui::DragValue::new(&mut state.persistent_settings.min_window_size.1).clamp_range(1..=2000).prefix("y : ").speed(0.01));
                });
                ui.end_row();


            });

                // TODO: add more options here
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
                        apply_theme(state, ctx);
                    }
                });

                ui.styled_collapsing("Keybindings",|ui| {
                    keybinding_ui(app, state, ui);
                });

            });
    state.settings_enabled = settings_enabled;
}

pub fn advanced_ui(ui: &mut Ui, state: &mut OculanteState) {
    if let Some(info) = &state.image_info {
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

        let red_vals = Line::new(
            info.red_histogram
                .iter()
                .map(|(k, v)| [*k as f64, *v as f64])
                .collect::<PlotPoints>(),
        )
        .fill(0.)
        .color(Color32::RED);

        let green_vals = Line::new(
            info.green_histogram
                .iter()
                .map(|(k, v)| [*k as f64, *v as f64])
                .collect::<PlotPoints>(),
        )
        .fill(0.)
        .color(Color32::GREEN);

        let blue_vals = Line::new(
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

/// Everything related to image editing
#[allow(unused_variables)]
pub fn edit_ui(app: &mut App, ctx: &Context, state: &mut OculanteState, gfx: &mut Graphics) {
    egui::SidePanel::right("editing")
        .min_width(100.)
        .show_separator_line(false)
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

                        // General Image Adjustments
                        ImageOperation::Brightness(0),
                        ImageOperation::Contrast(0),
                        ImageOperation::Exposure(20),
                        ImageOperation::Desaturate(0),
                        ImageOperation::Invert,

                        // Colour and Hue
                        ImageOperation::ChannelSwap((Channel::Red, Channel::Red)),
                        ImageOperation::Equalize((0, 255)),
                        ImageOperation::HSV((0, 100, 100)),
                        ImageOperation::Add([0, 0, 0]),
                        ImageOperation::Mult([255, 255, 255]),
                        ImageOperation::Fill([255, 255, 255, 255]),

                        // Colour Mapping and Conversion
                        ImageOperation::LUT("Lomography Redscale 100".into()),
                        ImageOperation::GradientMap(vec![GradientStop::new(0, [155,33,180]), GradientStop::new(128, [255,83,0]),GradientStop::new(255, [224,255,0])]),
                        ImageOperation::Posterize(8),
                        ImageOperation::Filter3x3([0,-100, 0, -100, 500, -100, 0, -100, 0]),

                        // Mathematical
                        ImageOperation::MMult,
                        ImageOperation::MDiv,
                        ImageOperation::Expression("r = 1.0".into()),
                        ImageOperation::ScaleImageMinMax,

                        // Effects
                        ImageOperation::Blur(0),
                        ImageOperation::Noise {
                            amt: 50,
                            mono: false,
                        },
                        ImageOperation::ChromaticAberration(15),

                        // Geometry and Transformations
                        ImageOperation::Flip(false),
                        ImageOperation::Rotate(90),
                        ImageOperation::Resize {
                            dimensions: state.image_geometry.dimensions,
                            aspect: true,
                            filter: ScaleFilter::Hamming,
                        },
                        ImageOperation::Crop([0, 0, 0, 0]),
                        ImageOperation::CropPerspective{points: [
                            (0,0),
                            (state.image_geometry.dimensions.0, 0),
                            (0, state.image_geometry.dimensions.1),
                            (state.image_geometry.dimensions.0, state.image_geometry.dimensions.1),
                            ]
                        , original_size : state.image_geometry.dimensions
                        },
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

                    modifier_stack_ui(&mut state.edit_state.image_op_stack, &mut image_changed, ui, &state.image_geometry, &mut state.edit_state.block_panning, &mut state.volatile_settings);

                    if !state.edit_state.image_op_stack.is_empty() && !state.edit_state.pixel_op_stack.is_empty() {
                        ui.separator();
                        ui.separator();
                        ui.end_row();
                    }

                    modifier_stack_ui(
                        &mut state.edit_state.pixel_op_stack,
                        &mut pixels_changed,
                        ui, &state.image_geometry, &mut state.edit_state.block_panning, &mut state.volatile_settings
                    );

                    ui.label_i(&format!("Reset"));
                    ui.centered_and_justified(|ui| {
                        if ui.button("Reset all edits").clicked() {
                            state.edit_state = Default::default();
                            pixels_changed = true
                        }
                    });
                    ui.end_row();

                    ui.label_i(&format!("Compare"));
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
                                state.current_texture.set(img.to_texture_with_texwrap(gfx, &state.persistent_settings), gfx);
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
                } else if ui.button(format!("Paint mode")).clicked() {
                    state.edit_state.painting = true;
                }
            });

            if state.edit_state.painting {
                egui::Grid::new("paint").show(ui, |ui| {
                    ui.label("📜 Keep history");
                    ui.styled_checkbox(&mut state.edit_state.non_destructive_painting, "")
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
                    .button(format!("Apply all edits"))
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
                if let Some(tex) = &mut state.current_texture.get() {
                    if let Some(img) = &state.current_image {
                        if tex.width() as u32 == state.edit_state.result_pixel_op.width()
                            && state.edit_state.result_pixel_op.height() == img.height()
                        {
                            state.edit_state.result_pixel_op.update_texture_with_texwrap(gfx, tex);
                        } else {
                            state.current_texture.set(state.edit_state.result_pixel_op.to_texture_with_texwrap(gfx, &state.persistent_settings), gfx);
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
                        .button(format!("Restore original"))
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
                    if ui.button(format!("Save as...")).clicked() {
                        ui.ctx().memory_mut(|w| w.open_popup(Id::new("SAVE")));
                    }

                    let encoding_options = state.volatile_settings.encoding_options.clone();

                    if ctx.memory(|w| w.is_popup_open(Id::new("SAVE"))) {
                        let msg_sender = state.message_channel.0.clone();

                        // let keys = state.encoding_options.keys().map(|k|k.as_str()).collect::<Vec<&str>>();
                        let keys = &state.volatile_settings.encoding_options.iter().map(|e|e.ext()).collect::<Vec<_>>();
                        let key_slice = keys.iter().map(|k|k.as_str()).collect::<Vec<_>>();

                        filebrowser::browse_modal(
                            true,
                            key_slice.as_slice(),
                            &mut state.volatile_settings,
                            |p| {


                                let dynimage = DynamicImage::ImageRgba8(state.edit_state.result_pixel_op.clone());
                                let encoding_options = FileEncoder::matching_variant(p, &encoding_options);
                                    match encoding_options.save(&dynimage, p) {


                                    // match state.edit_state.result_pixel_op.save(&p) {
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
                        format!("Overwrite")
                    } else {
                        format!("Save")
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

                    if ui.button(format!("Save edits")).on_hover_text("Saves an .oculante metafile in the same directory as the image. This file will contain all edits and will be restored automatically if you open the image again. This leaves the original image unmodified and allows you to continue editing later.").clicked() {
                        if let Ok(f) = std::fs::File::create(p.with_extension("oculante")) {
                            _ = serde_json::to_writer_pretty(&f, &state.edit_state);
                        }
                    }
                    if ui.button(format!("Save directory edits")).on_hover_text("Saves an .oculante metafile in the same directory as the image. This file will contain all edits and will be restored automatically if you open the image again. This leaves the original image unmodified and allows you to continue editing later.").clicked() {
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
        .styled_checkbox(&mut stroke.fade, "")
        .on_hover_text("Fade out the stroke over its path");
    if r.changed() {
        combined_response.changed = true;
    }
    if r.hovered() {
        combined_response.hovered = true;
    }

    let r = ui
        .styled_checkbox(&mut stroke.flip_random, "")
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
    settings: &mut VolatileSettings,
) {
    let mut delete: Option<usize> = None;
    let mut swap: Option<(usize, usize)> = None;

    let stack_len = stack.len();

    for (i, operation) in stack.iter_mut().enumerate() {
        ui.label_i(&format!("{operation}"));

        // let op draw itself and check for response

        ui.push_id(i, |ui| {
            // draw the image operator
            ui.style_mut().spacing.slider_width = ui.available_width() * 0.6;
            if operation.ui(ui, geo, mouse_grab, settings).changed() {
                *image_changed = true;
            }

            ui.add_space(ui.style().spacing.icon_spacing);

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.style_mut().spacing.icon_spacing = 0.;
                ui.style_mut().spacing.button_padding = Vec2::ZERO;
                ui.style_mut().spacing.interact_size = Vec2::ZERO;
                ui.style_mut().spacing.indent = 0.0;
                ui.style_mut().spacing.item_spacing = Vec2::ZERO;
                if egui::Button::new("")
                    .small()
                    .frame(false)
                    .ui(ui)
                    .on_hover_text("Remove operator")
                    .clicked()
                {
                    delete = Some(i);
                    *image_changed = true;
                }

                let up = i != 0;
                let down = i != stack_len - 1;

                ui.add_enabled_ui(up, |ui| {
                    if egui::Button::new("")
                        .small()
                        .frame(false)
                        .ui(ui)
                        .on_hover_text("Move up")
                        .clicked()
                    {
                        swap = Some(((i as i32 - 1).max(0) as usize, i));
                        *image_changed = true;
                    }
                });

                ui.add_enabled_ui(down, |ui| {
                    if egui::Button::new("")
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
            });

            ui.end_row();
        });

        ui.end_row();
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

        ui.styled_collapsing("Lossless Jpeg transforms", |ui| {
            ui.label("These operations will immediately write changes to disk.");
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
    let window_x = state.window_size.x - ui.style().spacing.icon_spacing * 2. - 100.;

    ui.horizontal_centered(|ui| {
        use crate::shortcuts::InputEvent::*;

        // The Close button
        if state.persistent_settings.borderless {
            if unframed_button(X, ui).clicked() {
                app.backend.exit();
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

        if window_x > ui.cursor().left() + 110. {
            ui.add_enabled_ui(!state.persistent_settings.edit_enabled, |ui| {
                ui.spacing_mut().button_padding = Vec2::new(10., 0.);
                // ui.spacing_mut().interact_size.y = ui.available_height() * 0.7;
                ui.spacing_mut().interact_size.y = BUTTON_HEIGHT_SMALL;
                ui.spacing_mut().combo_width = 1.;
                ui.spacing_mut().icon_width = 0.;

                // style.visuals.widgets.inactive.fg_stroke = Stroke::new(1., Color32::WHITE);
                let color = if ui.style().visuals.dark_mode {
                    Color32::WHITE
                } else {
                    Color32::BLACK
                };
                ui.style_mut().visuals.widgets.inactive.fg_stroke = Stroke::new(1., color);

                egui::ComboBox::from_id_source("channels")
                    .icon(blank_icon)
                    .selected_text(
                        RichText::new(
                            state
                                .persistent_settings
                                .current_channel
                                .to_string()
                                .to_uppercase(),
                        ), // .size(combobox_text_size),
                    )
                    .show_ui(ui, |ui| {
                        for channel in ColorChannel::iter() {
                            let r = ui.selectable_value(
                                &mut state.persistent_settings.current_channel,
                                channel,
                                RichText::new(channel.to_string().to_uppercase()), // .size(combobox_text_size),
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

        // TODO: remove redundancy
        if changed_channels {
            if let Some(img) = &state.current_image {
                match &state.persistent_settings.current_channel {
                    ColorChannel::Rgb => {
                        state.current_texture.set(
                            unpremult(img).to_texture_with_texwrap(gfx, &state.persistent_settings),
                            gfx,
                        );
                    }
                    ColorChannel::Rgba => {
                        state.current_texture.set(
                            img.to_texture_with_texwrap(gfx, &state.persistent_settings),
                            gfx,
                        );
                    }
                    _ => {
                        state.current_texture.set(
                            solo_channel(img, state.persistent_settings.current_channel as usize)
                                .to_texture_with_texwrap(gfx, &state.persistent_settings),
                            gfx,
                        );
                    }
                }
            }
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
                send_extended_info(
                    &state.current_image,
                    &state.current_path,
                    &state.extended_info_channel,
                );
            }
            if window_x > ui.cursor().left() + 80. {
                if tooltip(
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
                    state.persistent_settings.edit_enabled =
                        !state.persistent_settings.edit_enabled;
                }
            }
        }

        if window_x > ui.cursor().left() + 80. {
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
        }

        if window_x > ui.cursor().left() + 80. {
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
        }

        if state.current_path.is_some() && window_x > ui.cursor().left() + 80. {
            let modal = egui_modal::Modal::new(ui.ctx(), "delete");
            modal.show(|ui| {
                ui.horizontal(|ui| {
                    ui.vertical_centered_justified(|ui| {
                        ui.add_space(10.);

                        ui.label(
                            RichText::new(WARNING_CIRCLE)
                                .size(100.)
                                .color(ui.style().visuals.warn_fg_color),
                        );
                        ui.add_space(20.);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!(
                                "Are you sure you want to move {} to the trash?",
                                state
                                    .current_path
                                    .clone()
                                    .unwrap_or_default()
                                    .file_name()
                                    .map(|s| s.to_string_lossy())
                                    .unwrap_or_default()
                            ));
                        });
                        ui.add_space(20.);
                        ui.scope(|ui| {
                            let warn_color = Color32::from_rgb(255, 77, 77);
                            ui.style_mut().visuals.widgets.inactive.weak_bg_fill = warn_color;
                            ui.style_mut().visuals.widgets.inactive.fg_stroke =
                                Stroke::new(1., Color32::WHITE);
                            ui.style_mut().visuals.widgets.hovered.weak_bg_fill =
                                warn_color.linear_multiply(0.8);

                            if ui.styled_button("Yes").clicked() {
                                delete_file(state);
                                modal.close();
                            }
                        });

                        if ui.styled_button("Cancel").clicked() {
                            modal.close();
                        }
                    });
                });
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

        if state.current_texture.get().is_some() && window_x > ui.cursor().left() + 80. {
            if tooltip(
                unframed_button(PLACEHOLDER, ui),
                "Clear image",
                &lookup(&state.persistent_settings.shortcuts, &ClearImage),
                ui,
            )
            .clicked()
            {
                clear_image(state);
            }
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

        if state.current_path.is_some() {
            if !state.is_loaded {
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
        }

        drag_area(ui, state, app);

        ui.add_space(ui.available_width() - ICON_SIZE * 2. - ICON_SIZE / 2.);

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
                    &mut state.volatile_settings,
                    |p| {
                        let _ = state.load_channel.0.clone().send(p.to_path_buf());
                        ui.ctx().memory_mut(|w| w.close_popup());
                    },
                    ui.ctx(),
                );
            }
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
                    Pos2::new(r.right_bottom().x + 200., r.left_top().y),
                    Pos2::new(
                        r.left_bottom().x - ui.available_width(),
                        r.left_top().y + 300.,
                    ),
                );

                ui.allocate_ui_at_rect(recent_rect, |ui| {
                    for r in &state.volatile_settings.recent_images.clone() {
                        ui.horizontal(|ui| {
                            render_file_icon(&r, ui);
                            // ui.add(egui::Image::from_uri(format!("file://{}", r.display())).max_width(120.));
                            ui.vertical_centered_justified(|ui| {
                                if let Some(filename) = r.file_name() {
                                    if ui.button(filename.to_string_lossy()).clicked() {
                                        load_image_from_path(r, state);
                                        ui.close_menu();
                                    }
                                }
                            });
                        });
                    }
                });
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
}

pub fn drag_area(ui: &mut Ui, state: &mut OculanteState, app: &mut App) {
    #[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
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
        if r.drag_stopped() {
            ui.ctx()
                .memory_mut(|w| w.data.remove::<(i32, i32)>("offset".into()))
        }
    }
}
pub fn render_file_icon(icon_path: &Path, ui: &mut Ui) -> Response {
    let size = 30.;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    ui.painter()
        .rect_filled(rect, Rounding::same(size / 4.), Color32::GRAY);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon_path
            .extension()
            .map(|e| e.to_string_lossy().to_string().to_uppercase())
            .unwrap_or_default(),
        FontId::proportional(10.),
        Color32::BLACK,
    );
    // ui.add(egui::Label::new(""))
    response
}

pub fn blank_icon(
    _ui: &egui::Ui,
    _rect: egui::Rect,
    _visuals: &egui::style::WidgetVisuals,
    _is_open: bool,
    _above_or_below: egui::AboveOrBelow,
) {
}

pub fn apply_theme(state: &mut OculanteState, ctx: &Context) {
    let mut button_color = Color32::from_gray(38);
    let mut panel_color = Color32::from_gray(25);

    match state.persistent_settings.theme {
        ColorTheme::Light => ctx.set_visuals(Visuals::light()),
        ColorTheme::Dark => ctx.set_visuals(Visuals::dark()),
        ColorTheme::System => set_system_theme(ctx),
    }

    // Switching theme resets accent color, set it again
    let mut style: egui::Style = (*ctx.style()).clone();

    if style.visuals.dark_mode {
        style.visuals.extreme_bg_color = Color32::from_hex("#0D0D0D").unwrap_or_default();
        if state.persistent_settings.background_color == [200, 200, 200] {
            state.persistent_settings.background_color =
                PersistentSettings::default().background_color;
        }
        if state.persistent_settings.accent_color == [0, 170, 255] {
            state.persistent_settings.accent_color = PersistentSettings::default().accent_color;
        }
    } else {
        style.visuals.extreme_bg_color = Color32::from_hex("#D9D9D9").unwrap_or_default();

        button_color = Color32::from_gray(255);
        panel_color = Color32::from_gray(230);
        if state.persistent_settings.background_color
            == PersistentSettings::default().background_color
        {
            state.persistent_settings.background_color = [200, 200, 200];
        }
        if state.persistent_settings.accent_color == PersistentSettings::default().accent_color {
            state.persistent_settings.accent_color = [0, 170, 255];
        }
        style.visuals.widgets.inactive.bg_fill = Color32::WHITE;
    }

    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style.interaction.tooltip_delay = 0.0;
    style.spacing.icon_width = 20.;
    style.spacing.window_margin = 5.0.into();
    style.spacing.item_spacing = vec2(8., 6.);
    style.spacing.icon_width_inner = style.spacing.icon_width / 1.5;
    style.spacing.interact_size.y = BUTTON_HEIGHT_SMALL;
    style.visuals.window_fill = panel_color;

    // button color
    style.visuals.widgets.inactive.weak_bg_fill = button_color;
    // style.visuals.widgets.inactive.bg_fill = button_color;
    // style.visuals.widgets.inactive.bg_fill = button_color;

    // button rounding
    style.visuals.widgets.inactive.rounding = Rounding::same(4.);
    style.visuals.widgets.active.rounding = Rounding::same(4.);
    style.visuals.widgets.hovered.rounding = Rounding::same(4.);

    // No stroke on buttons
    style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;

    style.visuals.warn_fg_color = Color32::from_rgb(255, 204, 0);

    style.visuals.panel_fill = panel_color;

    style.text_styles.get_mut(&TextStyle::Body).unwrap().size = 15.;
    style.text_styles.get_mut(&TextStyle::Button).unwrap().size = 15.;
    style.text_styles.get_mut(&TextStyle::Small).unwrap().size = 12.;
    style.text_styles.get_mut(&TextStyle::Heading).unwrap().size = 18.;
    // accent color
    style.visuals.selection.bg_fill = Color32::from_rgb(
        state.persistent_settings.accent_color[0],
        state.persistent_settings.accent_color[1],
        state.persistent_settings.accent_color[2],
    );

    let accent_color = style.visuals.selection.bg_fill.to_array();

    let accent_color_luma = (accent_color[0] as f32 * 0.299
        + accent_color[1] as f32 * 0.587
        + accent_color[2] as f32 * 0.114)
        .max(0.)
        .min(255.) as u8;
    let accent_color_luma = if accent_color_luma < 80 { 220 } else { 80 };
    // Set text on highlighted elements
    style.visuals.selection.stroke = Stroke::new(2.0, Color32::from_gray(accent_color_luma));
    ctx.set_style(style);
}

fn caret_icon(ui: &mut egui::Ui, openness: f32, response: &egui::Response) {
    let galley = ui.ctx().fonts(|fonts| {
        fonts.layout(
            CARET_RIGHT.to_string(),
            FontId::proportional(12.),
            ui.style().visuals.selection.bg_fill,
            10.,
        )
    });
    let mut text_shape = TextShape::new(response.rect.left_top(), galley, Color32::RED);
    text_shape.angle = egui::lerp(0.0..=3.141 / 2., openness);
    let mut text = egui::Shape::Text(text_shape);
    let r = text.visual_bounding_rect();
    let x_offset = 5.0;
    let y_offset = 4.0;

    text.translate(vec2(
        egui::lerp(
            -ui.style().spacing.icon_spacing + x_offset
                ..=r.size().x + ui.style().spacing.icon_spacing - 4.0 + x_offset,
            openness,
        ),
        egui::lerp(
            -ui.style().spacing.icon_spacing + y_offset
                ..=-ui.style().spacing.icon_spacing + y_offset,
            openness,
        ),
    ));

    ui.painter().add(text);
}

fn dark_panel<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) {
    let panel_bg_color = match ui.style().visuals.dark_mode {
        true => Color32::from_gray(13),
        false => Color32::from_gray(217),
    };

    let button_color = match ui.style().visuals.dark_mode {
        true => Color32::from_gray(25),
        false => Color32::from_gray(230),
    };

    egui::Frame::none()
        .fill(panel_bg_color)
        .rounding(ui.style().visuals.widgets.active.rounding)
        .inner_margin(Margin::same(6.))
        .show(ui, |ui| {
            ui.style_mut().visuals.widgets.inactive.weak_bg_fill = button_color;

            ui.scope(add_contents);
            // let mut prepared = ui.begin(ui);
            // let ret = add_contents(&mut prepared.content_ui);
            // let response = prepared.end(ui);
            // InnerResponse::new(ret, response)
        });
}
