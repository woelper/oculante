const ICON_SIZE: f32 = 24. * 0.8;
const ROUNDING: f32 = 8.;
pub const BUTTON_HEIGHT_LARGE: f32 = 35.;
pub const BUTTON_HEIGHT_SMALL: f32 = 24.;
pub const PANEL_WIDTH: f32 = 260.0;
const PANEL_WIDGET_OFFSET: f32 = 0.0;

mod info_ui;
pub use info_ui::info_ui;
mod palette_ui;
pub use palette_ui::palette_ui;
mod settings_ui;
pub use settings_ui::settings_ui;
mod top_bar;
pub use top_bar::*;
mod edit_ui;
pub use edit_ui::edit_ui;
mod theme;
pub use theme::*;
mod thumbnail_rendering;
pub use thumbnail_rendering::*;

#[cfg(feature = "file_open")]
use crate::filebrowser::browse_for_image_path;
use crate::icons::*;
use crate::utils::*;
use epaint::TextShape;
use image::DynamicImage;
use log::{debug, error, info};
#[cfg(not(any(target_os = "netbsd", target_os = "freebsd")))]
use mouse_position::mouse_position::Mouse;
use nalgebra::Vector2;
use notan::{
    egui::{self, *},
    prelude::{App, Graphics},
};
use std::{collections::BTreeSet, f32, ops::RangeInclusive, path::Path, time::Instant};
use strum::IntoEnumIterator;
use text::{LayoutJob, TextWrapping};

#[cfg(not(feature = "file_open"))]
use crate::filebrowser;

use crate::{
    appstate::{ImageGeometry, OculanteState},
    file_encoder::FileEncoder,
    image_editing::{
        process_pixels, Channel, ColorTypeExt, GradientStop, ImageOperation, ImgOpItem,
        MeasureShape, ScaleFilter,
    },
    paint::PaintStroke,
    settings::{set_system_theme, ColorTheme, PersistentSettings, VolatileSettings},
    shortcuts::{key_pressed, keypresses_as_string, lookup},
    thumbnails::{self, Thumbnails, THUMB_CAPTION_HEIGHT, THUMB_SIZE},
};

#[cfg(feature = "turbo")]
use crate::image_editing::{cropped_range, lossless_tx};

pub trait EguiExt {
    fn label_i(&mut self, _text: impl Into<WidgetText>) -> Response {
        unimplemented!()
    }

    fn label_unselectable(&mut self, _text: impl Into<WidgetText>) -> Response {
        unimplemented!()
    }

    fn label_right(&mut self, _text: impl Into<WidgetText>) -> Response {
        unimplemented!()
    }

    #[allow(unused)]
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

    fn get_rounding(&self, _height: f32) -> u8 {
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
    fn get_rounding(&self, height: f32) -> u8 {
        if height > 25. {
            self.style().visuals.widgets.inactive.corner_radius.ne * 2
        } else {
            self.style().visuals.widgets.inactive.corner_radius.ne
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
                false,
                galley.as_ref().map_or("", |x| x.text()),
            )
        });

        if self.is_rect_visible(rect) {
            let visuals = self.style().interact(&response);
            let (small_icon_rect, big_icon_rect) = self.spacing().icon_rectangles(rect);
            self.painter().add(epaint::RectShape::new(
                big_icon_rect.expand(visuals.expansion),
                visuals.corner_radius,
                if *checked {
                    color.gamma_multiply(0.3)
                } else {
                    visuals.weak_bg_fill
                },
                visuals.bg_stroke,
                StrokeKind::Inside,
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

    /// Draw a justified icon from a string containing an emoji
    fn label_i(&mut self, text: impl Into<WidgetText>) -> Response {
        let text: WidgetText = text.into();
        let text = text.text();

        let (icon, description) = parse_icon_plus_text(text);
        let icon = icon.unwrap_or_default();

        self.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
            ui.add(egui::Label::new(
                RichText::new(icon).color(ui.style().visuals.selection.bg_fill),
            ));
            ui.label(
                RichText::new(description).color(ui.style().visuals.noninteractive().text_color()),
            );
        })
        .response
    }

    /// Unselectable label
    fn label_unselectable(&mut self, text: impl Into<WidgetText>) -> Response {
        self.add(egui::Label::new(text).selectable(false))
    }

    fn styled_menu_button(
        &mut self,
        title: impl Into<WidgetText>,
        add_contents: impl FnOnce(&mut Ui),
    ) -> Response {
        let text: WidgetText = title.into();
        let text = text.text();

        let (icon, description) = parse_icon_plus_text(text);
        let icon = icon.unwrap_or_default();

        let spacing = if icon.is_empty() { "" } else { "       " };
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

        let (icon, description) = parse_icon_plus_text(text);
        let icon = icon.unwrap_or_default();

        let spacing = if icon.is_empty() { "" } else { "       " };
        let r = self.add(
            egui::Button::new(format!("{spacing}{description}"))
                .corner_radius(self.get_rounding(BUTTON_HEIGHT_LARGE))
                .min_size(vec2(140., BUTTON_HEIGHT_LARGE)),
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

        let (icon, description) = parse_icon_plus_text(text);
        let icon = icon.unwrap_or_default();

        self.spacing_mut().button_padding = Vec2::new(8., 0.);
        let spacing = if icon.is_empty() { "" } else { "  " };
        let r = self.add(
            egui::Button::new(format!("{description}{spacing}"))
                .corner_radius(self.get_rounding(BUTTON_HEIGHT_LARGE))
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
    }

    /// Draw a right justified label
    fn label_right(&mut self, text: impl Into<WidgetText>) -> Response {
        self.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
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
            .icon(caret_icon)
            .show_unindented(self, add_contents)
    }

    /// Draw a justified icon from a string starting with an emoji
    fn label_i_selected(&mut self, selected: bool, text: impl Into<WidgetText>) -> Response {
        let text: WidgetText = text.into();
        let text = text.text();

        let (icon, description) = parse_icon_plus_text(text);
        let icon = icon.unwrap_or_default();

        // let description = text.chars().filter(|c| c.is_ascii()).collect::<String>();
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
                // r.flags.insert(egui::response::Flags::CLICKED);
                r.flags.insert(egui::response::Flags::CLICKED);
                // r.clicked = true;
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
            // style.visuals.widgets.inactive.rounding =
            //     style.visuals.widgets.inactive.rounding.at_least(18.);

            style.visuals.widgets.inactive.corner_radius =
                style.visuals.widgets.inactive.corner_radius.at_least(18);
            style.visuals.widgets.inactive.expansion = -4.0;

            style.visuals.widgets.hovered.fg_stroke.width = 9.0;
            style.visuals.widgets.hovered.fg_stroke.color = color;
            // style.visuals.widgets.hovered.rounding =
            //     style.visuals.widgets.hovered.rounding.at_least(18.);
            style.visuals.widgets.hovered.corner_radius =
                style.visuals.widgets.hovered.corner_radius.at_least(18);

            style.visuals.widgets.hovered.expansion = -4.0;

            style.visuals.widgets.active.fg_stroke.width = 9.0;
            style.visuals.widgets.active.fg_stroke.color = color;
            // style.visuals.widgets.active.rounding =
            //     style.visuals.widgets.active.rounding.at_least(18.);
            style.visuals.widgets.active.corner_radius =
                style.visuals.widgets.active.corner_radius.at_least(18);

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
            // style.visuals.widgets.inactive.rounding =
            //     style.visuals.widgets.inactive.rounding.at_least(20.);
            style.visuals.widgets.inactive.corner_radius =
                style.visuals.widgets.inactive.corner_radius.at_least(18);

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

fn parse_icon_plus_text(line: &str) -> (Option<String>, String) {
    use unicode_segmentation::UnicodeSegmentation;

    let trimmed = line.trim();

    // Helper to check if a grapheme is an "icon" (e.g., Private Use Area: U+E000 to U+F8FF)
    fn is_icon(grapheme: &str) -> bool {
        grapheme.chars().next().is_some_and(|c| {
            let code = c as u32;
            (0xE000..=0xF8FF).contains(&code) // Adjust range based on your icons
        })
    }

    // 1) Check for icon at the front
    if let Some((candidate, remainder)) = trimmed.split_once(' ') {
        if candidate.graphemes(true).count() == 1 && is_icon(candidate) {
            return (Some(candidate.to_owned()), remainder.to_owned());
        }
    }

    // 2) Check for icon at the end
    if let Some((remainder, candidate)) = trimmed.rsplit_once(' ') {
        if candidate.graphemes(true).count() == 1 && is_icon(candidate) {
            return (Some(candidate.to_owned()), remainder.to_owned());
        }
    }

    // 3) No icon found
    (None, trimmed.to_owned())
}

/// Proof-of-concept funtion to draw texture completely with egui
#[allow(unused)]
pub fn image_ui(ctx: &Context, state: &mut OculanteState, gfx: &mut Graphics) {
    if let Some(texture) = &state.current_texture.get() {
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

fn measure_ui(ui: &mut Ui, state: &mut OculanteState) {
    ui.styled_collapsing("Measure", |ui| {
        ui.vertical_centered_justified(|ui| {
            dark_panel(ui, |ui| {
                ui.allocate_space(vec2(ui.available_width(), 0.));
                // draw a line using egui with the mouse

                let cursor_abs = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();

                let cursor_relative = pos_from_coord(
                    state.image_geometry.offset,
                    Vector2::new(cursor_abs.x, cursor_abs.y),
                    Vector2::new(
                        state.image_geometry.dimensions.0 as f32,
                        state.image_geometry.dimensions.1 as f32,
                    ),
                    state.image_geometry.scale,
                );

                let x = state
                    .edit_state
                    .image_op_stack
                    .iter()
                    .filter(|op| matches!(op.operation, ImageOperation::Measure { .. }))
                    .collect::<Vec<_>>();
                if x.len() != 1 {
                    state
                        .edit_state
                        .image_op_stack
                        .push(ImgOpItem::new(ImageOperation::Measure {
                            shapes: vec![MeasureShape::new_rect(vec![(0, 0), (0, 0)])],
                        }));
                }

                if ui.ctx().input(|r| r.pointer.secondary_pressed()) {
                    for op in &mut state.edit_state.image_op_stack {
                        if !op.active {
                            continue;
                        }
                        if let ImageOperation::Measure { shapes } = &mut op.operation {
                            for shape in shapes {
                                if let MeasureShape::Rect { points, .. } = shape {
                                    points[0].0 = cursor_relative.x as u32;
                                    points[0].1 = cursor_relative.y as u32;
                                }
                            }
                        }
                    }
                }

                if ui.ctx().input(|r| r.pointer.secondary_down()) {
                    for op in &mut state.edit_state.image_op_stack {
                        if !op.active {
                            continue;
                        }
                        if let ImageOperation::Measure { shapes } = &mut op.operation {
                            for shape in shapes {
                                if let MeasureShape::Rect { points, .. } = shape {
                                    points[1].0 = cursor_relative.x as u32;
                                    points[1].1 = cursor_relative.y as u32;
                                }
                            }
                        }
                    }
                }
                if ui.ctx().input(|r| r.pointer.secondary_released()) {
                    for op in &mut state.edit_state.image_op_stack {
                        if !op.active {
                            continue;
                        }
                        if let ImageOperation::Measure { shapes } = &mut op.operation {
                            for shape in shapes {
                                if let MeasureShape::Rect { points, .. } = shape {
                                    points[1].0 = cursor_relative.x as u32;
                                    points[1].1 = cursor_relative.y as u32;
                                }
                            }
                        }
                    }
                }
            });
        });

        for op in &mut state.edit_state.image_op_stack {
            op.operation.ui(
                ui,
                &state.image_geometry,
                &mut state.mouse_grab,
                &mut state.volatile_settings,
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
                    .color(ui.style().visuals.selection.bg_fill),
            )
            .frame(false),
        )
    } else {
        ui.add(egui::Button::new(RichText::new(text).size(ICON_SIZE)).frame(false))
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
        state.player.load(&p);
    }
}

/// An area that can be dragged by a user to move the window
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

pub fn blank_icon(
    _ui: &egui::Ui,
    _rect: egui::Rect,
    _visuals: &egui::style::WidgetVisuals,
    _is_open: bool,
    _above_or_below: egui::AboveOrBelow,
) {
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
    text_shape.angle = egui::lerp(0.0..=f32::consts::PI / 2., openness);
    let mut text = egui::Shape::Text(text_shape);
    let r = text.visual_bounding_rect();
    let x_offset = 5.0;
    let y_offset = 4.0;

    text.translate(vec2(
        egui::lerp(
            -ui.style().spacing.icon_spacing + x_offset
                ..=r.size().x + ui.style().spacing.icon_spacing - 3.0 + x_offset,
            openness,
        ),
        egui::lerp(
            -ui.style().spacing.icon_spacing + y_offset
                ..=-ui.style().spacing.icon_spacing + y_offset + 1.,
            openness,
        ),
    ));

    ui.painter().add(text);
}

fn light_panel<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) {
    let panel_bg_color = match ui.style().visuals.dark_mode {
        true => Color32::from_gray(25),
        false => Color32::from_gray(230),
    };

    egui::Frame::new()
        .fill(panel_bg_color)
        .corner_radius(ui.style().visuals.widgets.active.corner_radius)
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.scope(add_contents);
        });
}

fn dark_panel<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) {
    let panel_bg_color = match ui.style().visuals.dark_mode {
        true => Color32::from_gray(13),
        false => Color32::from_gray(217),
    };

    egui::Frame::new()
        .fill(panel_bg_color)
        .corner_radius(ui.style().visuals.widgets.active.corner_radius)
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.scope(add_contents);
        });
}

fn show_modal<R>(
    ctx: &Context,
    warning_text: impl Into<WidgetText>,
    add_contents: impl FnOnce(&mut Ui) -> R,
    id_source: impl std::fmt::Display,
) -> egui_modal::Modal {
    let modal = egui_modal::Modal::new(ctx, id_source);
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
                    ui.label(warning_text);
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
                        ui.scope(add_contents);
                        modal.close();
                    }
                });

                if ui.styled_button("Cancel").clicked() {
                    modal.close();
                }
            });
        });
    });
    modal
}

/// Save an image to a path using encoding options and generate a thumbnail
fn save_with_encoding(
    image: &DynamicImage,
    path: &Path,
    image_info: &Option<ExtendedImageInfo>,
    encoders: &Vec<FileEncoder>,
) -> anyhow::Result<()> {
    let encoding_options = FileEncoder::matching_variant(path, encoders);
    encoding_options.save(image, path)?;
    debug!("Saved to {}", path.display());
    // Re-apply exif
    if let Some(info) = &image_info {
        debug!("Extended image info present");
        // before doing anything, make sure we have raw exif data
        if info.raw_exif.is_some() {
            fix_exif(path, info.raw_exif.clone())?;
        } else {
            debug!("No raw exif");
        }
    }
    thumbnails::generate(path)?;
    Ok(())
}

pub struct Modal {
    id: String,
    ctx: egui::Context,
}

impl Modal {
    pub fn new(id: &str, ctx: &egui::Context) -> Self {
        Self {
            id: id.to_string(),
            ctx: ctx.clone(),
        }
    }

    pub fn show<R>(
        &self,
        warning_text: impl Into<WidgetText>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) {
        if !self.ctx.memory(|w| w.is_popup_open(self.id.clone().into())) {
            return;
        }
        egui::Modal::new("m".into()).show(&self.ctx, |ui| {
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
                        ui.label(warning_text);
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
                            ui.scope(add_contents);
                            self.ctx.memory_mut(|w| w.close_popup());
                        }
                    });
                    if ui.styled_button("Cancel").clicked() {
                        self.ctx.memory_mut(|w| w.close_popup());
                    }
                });
            });
        });
    }
    pub fn open(&self) {
        self.ctx
            .memory_mut(|w| w.open_popup(self.id.clone().into()));
    }
}
