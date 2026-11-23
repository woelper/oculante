//! The annotations part of the edit panel. This includes the tools, and a card for each annotation with what can be configured for it.

use super::*;

/// The annotations part of the edit panel: a button to select and a tile for each
/// tool in a box that folds away, a row of three `stamps` below, then a card for each
/// annotation, the one in front first. Like the cards of the filters they stay when
/// it is folded, as they are still on the image. Returns whether the edits changed.
pub fn panel_ui(
    ui: &mut Ui,
    stack: &mut Vec<ImgOpItem>,
    state: &mut AnnotateUi,
    stamps: &mut [Stamp; 3],
) -> bool {
    let mut changed = false;
    state.pointed_card = None;
    // before the lists of this frame are shown, which close on Esc
    state.popup_open = egui::Popup::is_any_open(ui.ctx());
    let mut toggled = false;
    ui.scope(|ui| {
        ui.style_mut().visuals.collapsing_header_frame = true;
        ui.style_mut().visuals.indent_has_left_vline = false;
        let header = egui::CollapsingHeader::new("Annotations")
            .icon(crate::ui::caret_icon)
            .open(Some(state.open))
            .show_unindented(ui, |ui| {
                crate::ui::dark_panel(ui, |ui| {
                    let mut picked = None;
                    if select_button(ui, state.tool == Some(Tool::Select))
                        .on_hover_text(Tool::Select.hint())
                        .clicked()
                    {
                        picked = Some(Tool::Select);
                    }
                    let spacing = ui.spacing().item_spacing.x;
                    let width = ((ui.available_width() - spacing * 2.) / 3.).floor();
                    for row in Tool::ALL.chunks(3) {
                        ui.horizontal(|ui| {
                            for &tool in row {
                                let button = egui::Button::new("").min_size(egui::vec2(width, 40.));
                                let on = state.tool == Some(tool);
                                let response = lit(ui, on, button);
                                let color = lit_color(ui, on, &response);
                                paint_icon(ui.painter(), response.rect, tool.icon(), color, 1.5);
                                if response
                                    .on_hover_text(format!("{}\n{}", tool.name(), tool.hint()))
                                    .clicked()
                                {
                                    picked = Some(tool);
                                }
                            }
                        });
                    }
                    // the stamps, a right-click picks another symbol for a tile
                    ui.horizontal(|ui| {
                        for slot in stamps.iter_mut() {
                            let tool = Tool::Stamp(*slot);
                            let button = egui::Button::new("").min_size(egui::vec2(width, 40.));
                            let on = state.tool == Some(tool);
                            let response = lit(ui, on, button);
                            let color = lit_color(ui, on, &response);
                            paint_icon(ui.painter(), response.rect, tool.icon(), color, 1.5);
                            let response =
                                response.on_hover_text(format!("{}\n{}", tool.name(), tool.hint()));
                            if response.clicked() {
                                picked = Some(tool);
                            }
                            response.context_menu(|ui| {
                                for stamp in Stamp::ALL {
                                    let name = format!("{}  {}", stamp.glyph(), stamp.name());
                                    if ui.selectable_label(*slot == stamp, name).clicked() {
                                        if state.tool == Some(tool) {
                                            state.tool = Some(Tool::Stamp(stamp));
                                        }
                                        *slot = stamp;
                                        ui.close();
                                    }
                                }
                            });
                        }
                    });
                    if let Some(tool) = picked {
                        let (annotations, created) = annotations_or_new(stack);
                        changed |= created;
                        state.finish_typing(annotations);
                        state.tool = (state.tool != Some(tool)).then_some(tool);
                    }
                });
            });
        toggled = header.header_response.clicked();
    });
    if toggled {
        state.open = !state.open;
        ui.ctx().request_repaint();
        let any = annotations_mut(stack).is_some_and(|a| !a.is_empty());
        if !state.open {
            state.tool = None;
        } else if any {
            // there is something to select
            state.tool.get_or_insert(Tool::Select);
        }
    }
    if let Some(annotations) = annotations_mut(stack) {
        cards_ui(ui, annotations, state);
    }
    changed
}

/// As wide as the box of the tools, tinted while it is on
pub(crate) fn select_button(ui: &mut Ui, on: bool) -> egui::Response {
    let (selected_fill, selected_text) = selection_colors(ui);
    let mut text = egui::RichText::new(Tool::Select.name());
    if on {
        text = text.color(selected_text);
    }
    let mut button = egui::Button::new(text).min_size(egui::vec2(0., 32.));
    if on {
        button = button.fill(selected_fill);
    }
    // the text in the middle
    let response = ui.vertical_centered_justified(|ui| ui.add(button)).inner;
    let color = lit_color(ui, on, &response);
    let icon = egui::Rect::from_center_size(
        egui::pos2(response.rect.left() + 22., response.rect.center().y),
        egui::vec2(16., 16.),
    );
    paint_icon(ui.painter(), icon, Icon::Mouse, color, 1.2);
    response
}

/// The card of what comes next while nothing is selected, then one for each
/// annotation, the one in front first
fn cards_ui(ui: &mut Ui, annotations: &mut Vec<Annotation>, state: &mut AnnotateUi) {
    // what comes next, while nothing is selected
    if let Some(tool) = state.tool
        && state.selected.is_empty()
        && state.typing.is_none()
        && let Some(mut next) = state.style.new_annotation(tool)
    {
        card(ui, false, |ui| {
            ui.label(format!("Next {}", tool.name().to_lowercase()));
            if options_ui(ui, &mut next, usize::MAX) {
                state.style.remember(&next);
            }
        });
    }

    let names = names(annotations);
    let count = annotations.len();
    let mut swap = None;
    let mut remove = None;
    // the spotlights dim together, a changed shade is that of all of them
    let mut shade = None;
    for i in (0..count).rev() {
        let selected = state.selected.contains(&i);
        let response = card(ui, selected, |ui| {
            ui.horizontal(|ui| {
                let size = 10.;
                let small = |text: &str| {
                    egui::Button::new(egui::RichText::new(text).size(size)).frame(false)
                };
                // the list goes from the front to the back
                ui.add_enabled_ui(i + 1 < count, |ui| {
                    if ui
                        .add(small(crate::icons::CHEVRON_UP))
                        .on_hover_text("Bring forward")
                        .clicked()
                    {
                        swap = Some((i, i + 1));
                    }
                });
                ui.add_enabled_ui(i > 0, |ui| {
                    if ui
                        .add(small(crate::icons::CHEVRON_DOWN))
                        .on_hover_text("Send backward")
                        .clicked()
                    {
                        swap = Some((i - 1, i));
                    }
                });
                let a = &mut annotations[i];
                let eye = if a.hidden {
                    crate::icons::EYEOFF
                } else {
                    crate::icons::EYE
                };
                if ui
                    .add(small(eye))
                    .on_hover_text(if a.hidden { "Show" } else { "Hide" })
                    .clicked()
                {
                    state.toggle_hidden(a, i);
                }
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new(crate::icons::X).size(size * 1.5))
                            .frame(false),
                    )
                    .on_hover_text("Remove")
                    .clicked()
                {
                    remove = Some(i);
                }
                ui.add_space(size / 2.);
                let mut name = egui::RichText::new(&names[i]);
                if selected {
                    name = name.color(selection_colors(ui).1);
                } else if a.hidden {
                    name = name.weak();
                }
                if ui
                    .add(
                        egui::Label::new(name)
                            .truncate()
                            .sense(egui::Sense::click()),
                    )
                    .on_hover_text("Select")
                    .clicked()
                {
                    state.finish_typing(annotations);
                    state.selected = vec![i];
                    // the selection lasts while annotating
                    state.tool.get_or_insert(Tool::Select);
                    state.open = true;
                }
            });
            if selected
                && let Some(a) = annotations.get_mut(i)
                && options_ui(ui, a, i)
            {
                state.style.remember(a);
                if a.is_spotlight() {
                    shade = Some(a.color);
                }
            }
        });
        if response.hovered() {
            state.pointed_card = Some(i);
        }
    }
    if let Some((a, b)) = swap {
        annotations.swap(a, b);
        let other = |i: usize| match i {
            i if i == a => b,
            i if i == b => a,
            i => i,
        };
        for s in &mut state.selected {
            *s = other(*s);
        }
        state.typing = state.typing.map(other);
    }
    if let Some(color) = shade {
        for a in annotations.iter_mut().filter(|a| a.is_spotlight()) {
            a.color = color;
        }
    }
    if let Some(i) = remove {
        state.remove(annotations, vec![i]);
    }
}

/// The annotations of the edits, which get them if they have none
fn annotations_or_new(stack: &mut Vec<ImgOpItem>) -> (&mut Vec<Annotation>, bool) {
    let found = stack
        .iter()
        .position(|op| matches!(op.operation, ImageOperation::Annotate(_)));
    let created = found.is_none();
    let i = found.unwrap_or_else(|| {
        stack.push(ImgOpItem::new(ImageOperation::Annotate(vec![])));
        stack.len() - 1
    });
    let op = &mut stack[i];
    op.active = true;
    match &mut op.operation {
        ImageOperation::Annotate(annotations) => (annotations, created),
        _ => unreachable!("found as one"),
    }
}

/// The names in the list, numbered for each kind
pub(super) fn names(annotations: &[Annotation]) -> Vec<String> {
    let mut counts = std::collections::HashMap::new();
    annotations
        .iter()
        .map(|a| match &a.shape {
            // a counter has its number already
            Shape::Text(_) | Shape::Counter(_) => a.label(),
            _ => {
                let label = a.label();
                let n = counts.entry(label.clone()).or_insert(0);
                *n += 1;
                format!("{label} {n}")
            }
        })
        .collect()
}

/// A card as those of the other edits
/// Tinted for what is selected
fn card(ui: &mut Ui, tinted: bool, add_contents: impl FnOnce(&mut Ui)) -> egui::Response {
    egui::Frame::new()
        // the select button while it is on has this as well
        .fill(if tinted {
            selection_colors(ui).0
        } else {
            card_fill(ui)
        })
        .corner_radius(ui.visuals().widgets.active.corner_radius)
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            ui.allocate_space(egui::vec2(ui.available_width(), 0.));
            add_contents(ui);
        })
        .response
}

fn card_fill(ui: &Ui) -> Color32 {
    CARD.of(ui)
}

/// What can be set for an annotation: what only its kind has, a line, then its
/// colors and outline. `id` keeps the lists of fonts of the cards apart. Returns
/// whether it changed.
fn options_ui(ui: &mut Ui, a: &mut Annotation, id: usize) -> bool {
    use crate::icons::{ARROWS_OUT, ARROWS_OUT_SIMPLE, PALETTE, PENCIL_SIMPLE_LINE};
    let before = a.clone();
    match &mut a.shape {
        Shape::Arrow(_) | Shape::Line(_) | Shape::Ellipse(_) | Shape::Pen(_) => {
            slider_row(
                ui,
                format!("{PENCIL_SIMPLE_LINE} Width"),
                &mut a.width,
                1..=100,
            );
        }
        Shape::Rect(_) => {
            slider_row(
                ui,
                format!("{PENCIL_SIMPLE_LINE} Width"),
                &mut a.width,
                1..=100,
            );
            slider_row(
                ui,
                format!("{ARROWS_OUT_SIMPLE} Radius"),
                &mut a.radius,
                0..=100,
            );
        }
        Shape::Text(_) => {
            slider_row(
                ui,
                format!("{PENCIL_SIMPLE_LINE} Size"),
                &mut a.width,
                4..=400,
            );
            font_ui(ui, &mut a.font, id);
        }
        Shape::Counter(number) => {
            // the next one gets its number when it is placed
            if id != usize::MAX {
                slider_row(ui, format!("{} Number", crate::icons::LIST), number, 1..=99);
            }
            slider_row(
                ui,
                format!("{PENCIL_SIMPLE_LINE} Size"),
                &mut a.width,
                8..=1000,
            );
            color_row(ui, format!("{PALETTE} Number color"), &mut a.number_color);
        }
        Shape::Stamp(stamp) => {
            // the tiles pick the next one
            if id != usize::MAX {
                ui.horizontal_wrapped(|ui| {
                    for other in Stamp::ALL {
                        if icon_button(ui, *stamp == other, Icon::Stamp(other))
                            .on_hover_text(other.name())
                            .clicked()
                        {
                            *stamp = other;
                        }
                    }
                });
            }
            slider_row(
                ui,
                format!("{PENCIL_SIMPLE_LINE} Size"),
                &mut a.width,
                8..=1000,
            );
        }
        Shape::Redact { style, .. } => {
            ui.horizontal(|ui| {
                for (way, icon, name) in [
                    (Redaction::Pixelate, Icon::Pixelate, "Pixelate"),
                    (Redaction::Fill, Icon::Fill, "Fill with the color"),
                ] {
                    if icon_button(ui, *style == way, icon)
                        .on_hover_text(name)
                        .clicked()
                    {
                        *style = way;
                    }
                }
            });
            if *style == Redaction::Pixelate {
                slider_row(
                    ui,
                    format!("{PENCIL_SIMPLE_LINE} Blocks"),
                    &mut a.width,
                    2..=100,
                );
            } else {
                color_row(ui, format!("{PALETTE} Color"), &mut a.color);
            }
            return *a != before;
        }
        Shape::Spotlight { oval, .. } => {
            ui.horizontal(|ui| {
                let shapes = [(false, Icon::Frame, "Box"), (true, Icon::Circle, "Oval")];
                for (round, icon, name) in shapes {
                    if icon_button(ui, *oval == round, icon)
                        .on_hover_text(name)
                        .clicked()
                    {
                        *oval = round;
                    }
                }
            });
            if !*oval {
                slider_row(
                    ui,
                    format!("{ARROWS_OUT_SIMPLE} Radius"),
                    &mut a.radius,
                    0..=200,
                );
            }
            // how much the shade covers, its alpha in percent
            let percent = (a.color[3] as u32 * 100 + 127) / 255;
            let mut dim = percent;
            slider_row(ui, format!("{PALETTE} Dim"), &mut dim, 0..=100);
            if dim != percent {
                a.color[3] = ((dim * 255 + 50) / 100).min(255) as u8;
            }
            color_row(ui, format!("{PALETTE} Shade"), &mut a.color);
            return *a != before;
        }
    }
    ui.separator();
    slider_row(ui, format!("{ARROWS_OUT} Outline"), &mut a.outline, 0..=40);
    color_row(ui, format!("{PALETTE} Color"), &mut a.color);
    ui.add_enabled_ui(a.outline > 0, |ui| {
        color_row(ui, format!("{PALETTE} Outline color"), &mut a.outline_color);
    });
    slider_row(ui, format!("{ARROWS_OUT} Shadow"), &mut a.shadow, 0..=100);
    ui.add_enabled_ui(a.shadow > 0, |ui| {
        slider_row(
            ui,
            format!("{ARROWS_OUT} Shadow size"),
            &mut a.shadow_size,
            0..=100,
        );
    });
    *a != before
}

/// The labels of the sliders take this much room, so the sliders line up
const LABEL_WIDTH: f32 = 96.;

fn slider_row(ui: &mut Ui, label: String, value: &mut u32, range: RangeInclusive<u32>) {
    use crate::ui::EguiExt;
    // a value set elsewhere stays in reach
    let range = *range.start()..=(*range.end()).max(*value);
    ui.horizontal(|ui| {
        let height = ui.spacing().interact_size.y;
        ui.allocate_ui_with_layout(
            egui::vec2(LABEL_WIDTH, height),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| ui.label_i(label),
        );
        // the value takes 40 points at the right
        let spacing = ui.spacing().item_spacing.x;
        ui.spacing_mut().slider_width = (ui.available_width() - 40. - 2. * spacing).max(40.);
        ui.styled_slider(value, range);
    });
}

fn color_row(ui: &mut Ui, label: String, color: &mut [u8; 4]) {
    use crate::ui::EguiExt;
    ui.horizontal(|ui| {
        ui.label_i(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.color_edit_button_srgba_unmultiplied(color);
        });
    });
}

/// The font and its weight
fn font_ui(ui: &mut Ui, font: &mut Font, id: usize) {
    let families = font_families();
    let shown = |family: &str| {
        if family.is_empty() {
            "Inter".to_string()
        } else {
            family.to_string()
        }
    };
    ui.label("Font");
    egui::ComboBox::from_id_salt(("annotation font", id))
        .selected_text(shown(&font.family))
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for (family, _) in families.iter() {
                // Inter is the one without a name
                let value = if family == "Inter" {
                    String::new()
                } else {
                    family.clone()
                };
                ui.selectable_value(&mut font.family, value, family);
            }
        });
    let weights = families
        .iter()
        .find(|(family, _)| *family == shown(&font.family))
        .map(|(_, weights)| weights.clone())
        .unwrap_or_else(|| vec![400]);
    ui.label("Weight");
    egui::ComboBox::from_id_salt(("annotation weight", id))
        .selected_text(weight_name(font.weight))
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for weight in weights {
                ui.selectable_value(&mut font.weight, weight, weight_name(weight));
            }
        });
}

fn weight_name(weight: u16) -> String {
    match weight {
        100 => "Thin".into(),
        200 => "Extra light".into(),
        300 => "Light".into(),
        400 => "Regular".into(),
        500 => "Medium".into(),
        600 => "Semibold".into(),
        700 => "Bold".into(),
        800 => "Extra bold".into(),
        900 => "Black".into(),
        other => other.to_string(),
    }
}

fn icon_button(ui: &mut Ui, on: bool, icon: Icon) -> egui::Response {
    let response = lit(ui, on, egui::Button::new("").min_size(egui::vec2(28., 24.)));
    let color = lit_color(ui, on, &response);
    paint_icon(ui.painter(), response.rect, icon, color, 1.);
    response
}

/// A button that is on has the colours of what is selected
fn lit<'a>(ui: &mut Ui, on: bool, button: egui::Button<'a>) -> egui::Response {
    let (fill, _) = selection_colors(ui);
    ui.add(if on { button.fill(fill) } else { button })
}

/// What is drawn on a button of `lit`: the accent on the shade of it while it is on
fn lit_color(ui: &Ui, on: bool, response: &egui::Response) -> Color32 {
    if on {
        selection_colors(ui).1
    } else {
        state_color(ui, response)
    }
}
