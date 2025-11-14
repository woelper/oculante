use egui::{self, DragValue};
use egui_modal::{Icon, Modal, ModalStyle};

struct ExampleApp {
    modal_style: ModalStyle,
    modal_title: String,
    modal_body: String,
    nested_modal_text: String,

    include_title: bool,
    include_body: bool,
    include_buttons: bool,
    close_on_outside_click: bool,

    dialog_icon: Option<Icon>,
}

impl Default for ExampleApp {
    fn default() -> Self {
        Self {
            modal_style: ModalStyle::default(),
            modal_title: "a modal".to_string(),
            modal_body: "here is the modal body".to_string(),

            nested_modal_text: String::new(),
            include_title: true,
            include_body: true,
            include_buttons: true,
            close_on_outside_click: false,

            dialog_icon: Some(Icon::Info),
        }
    }
}

impl eframe::App for ExampleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::Window::new("egui-modal").show(ctx, |ui| {
            // you can put the modal creation and show logic wherever you want
            // (though of course it needs to be created before it can be used)
            let nested_modal = Modal::new(ctx, "nested_modal");
            let modal = Modal::new(ctx, "modal")
                .with_style(&self.modal_style)
                .with_close_on_outside_click(self.close_on_outside_click || !self.include_buttons);

            // the show function defines what is shown in the modal, but the modal
            // won't actually show until you do modal.open()
            modal.show(|ui| {
                // these helper functions are NOT mandatory to use, they just
                // help implement some styling with margins and separators
                // you can put whatever you like in here
                if self.include_title {
                    modal.title(ui, &mut self.modal_title);
                }
                // the "frame" of the modal refers to the container of the icon and body.
                // this helper just applies a margin specified by the ModalStyle
                modal.frame(ui, |ui| {
                    if self.include_body {
                        modal.body(ui, &self.modal_body);
                    }
                });
                if self.include_buttons {
                    modal.buttons(ui, |ui| {
                        if modal.button(ui, "close").clicked()
                            || (self.close_on_outside_click && modal.was_outside_clicked())
                        {
                            // all buttons created with the helper functions automatically
                            // close the modal on click, but you can close it yourself with
                            // ['modal.close()']
                            println!("hello world!")
                        }

                        modal.caution_button(ui, "button, but caution");
                        if modal.suggested_button(ui, "open another modal").clicked() {
                            // always close your previous modal before opening a new one otherwise weird
                            // layering things will happen. again, the helper functions for the buttons automatically
                            // close the modal on click, so we don't have to manually do that here
                            nested_modal.open();
                        }
                    })
                }
            });

            // a dialog is useful when you have a one-time occurance and you want to relay information to the user
            let mut dialog_modal = Modal::new(ctx, "dialog_modal").with_style(&self.modal_style);
            // make sure you don't forget to show the dialog!
            dialog_modal.show_dialog();

            ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                if ui.button("open modal").clicked() {
                    modal.open();
                }

                if ui.button("open dialog").clicked() {
                    // [`.dialog()`] can be used to both set the visual info for the dialog
                    // and open it at the same time
                    let mut dialog_builder = dialog_modal
                        .dialog()
                        .with_title("this is a dialog")
                        .with_body("this helps for showing information about one-time events");
                    if let Some(dialog_icon) = self.dialog_icon.clone() {
                        dialog_builder = dialog_builder.with_icon(dialog_icon);
                    }
                    dialog_builder.open();
                }

                ui.separator();
                // to prevent locking the example window without any way to close the modal :)
                // remember to implement this yourself if you don't use buttons in your modal
                let mut cooc_enabled = self.close_on_outside_click || !self.include_buttons;
                ui.add_enabled_ui(self.include_buttons, |ui| {
                    if ui
                        .checkbox(&mut cooc_enabled, "close if click outside modal")
                        .clicked()
                    {
                        self.close_on_outside_click = !self.close_on_outside_click
                    };
                });
                ui.checkbox(&mut self.include_title, "include title");
                ui.checkbox(&mut self.include_body, "include body");
                ui.checkbox(&mut self.include_buttons, "include buttons");
                ui.separator();
                egui::Grid::new("options_grid")
                    .min_col_width(200.)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("title");
                        ui.text_edit_singleline(&mut self.modal_title);
                        ui.end_row();

                        ui.label("body");
                        ui.text_edit_singleline(&mut self.modal_body);
                        ui.end_row();

                        let mut has_height = self.modal_style.default_height.is_some();
                        let mut has_width = self.modal_style.default_width.is_some();
                        if ui.checkbox(&mut has_height, "default height").changed() {
                            if has_height {
                                self.modal_style.default_height = Some(100.)
                            } else {
                                self.modal_style.default_height = None
                            }
                        }
                        if let Some(modal_height) = self.modal_style.default_height.as_mut() {
                            let modal_height = DragValue::new(modal_height).range(0..=1000);
                            ui.add_sized(ui.available_rect_before_wrap().size(), modal_height);
                        }
                        ui.end_row();

                        if ui.checkbox(&mut has_width, "default width").changed() {
                            if has_width {
                                self.modal_style.default_width = Some(100.)
                            } else {
                                self.modal_style.default_width = None
                            }
                        }
                        if let Some(modal_width) = self.modal_style.default_width.as_mut() {
                            let modal_width = DragValue::new(modal_width).range(0..=1000);
                            ui.add_sized(ui.available_rect_before_wrap().size(), modal_width);
                        }
                        ui.end_row();

                        ui.label("body margin");
                        let body_margin =
                            DragValue::new(&mut self.modal_style.body_margin).range(0..=20);
                        ui.add_sized(ui.available_rect_before_wrap().size(), body_margin);
                        ui.end_row();

                        ui.label("frame margin");
                        let frame_margin =
                            DragValue::new(&mut self.modal_style.frame_margin).range(0..=20);
                        ui.add_sized(ui.available_rect_before_wrap().size(), frame_margin);
                        ui.end_row();

                        ui.label("icon margin");
                        let icon_margin =
                            DragValue::new(&mut self.modal_style.icon_margin).range(0..=20);
                        ui.add_sized(ui.available_rect_before_wrap().size(), icon_margin);
                        ui.end_row();

                        ui.label("icon size");
                        let icon_size =
                            DragValue::new(&mut self.modal_style.icon_size).range(8..=48);
                        ui.add_sized(ui.available_rect_before_wrap().size(), icon_size);
                        ui.end_row();

                        ui.label("dialog icon");
                        let mut use_icon = self.dialog_icon.is_some();
                        ui.horizontal(|ui| {
                            if ui.checkbox(&mut use_icon, "use a dialog icon").clicked() {
                                if use_icon {
                                    self.dialog_icon = Some(Icon::Info);
                                } else {
                                    self.dialog_icon = None;
                                }
                            }
                            if let Some(icon) = self.dialog_icon.as_mut() {
                                ui.selectable_value(icon, Icon::Info, "info");
                                ui.selectable_value(icon, Icon::Warning, "warning");
                                ui.selectable_value(icon, Icon::Success, "success");
                                ui.selectable_value(icon, Icon::Error, "error");
                            }
                        });
                        ui.end_row();

                        ui.label("body alignment");
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut self.modal_style.body_alignment,
                                egui::Align::Min,
                                "min",
                            );
                            ui.selectable_value(
                                &mut self.modal_style.body_alignment,
                                egui::Align::Center,
                                "center",
                            );
                            ui.selectable_value(
                                &mut self.modal_style.body_alignment,
                                egui::Align::Max,
                                "max",
                            );
                        });
                        ui.end_row();

                        ui.label("overlay color");
                        ui.color_edit_button_srgba(&mut self.modal_style.overlay_color);
                        ui.end_row();

                        ui.label("caution button (fill, text)");
                        ui.horizontal(|ui| {
                            ui.color_edit_button_srgba(&mut self.modal_style.caution_button_fill);
                            ui.color_edit_button_srgba(
                                &mut self.modal_style.caution_button_text_color,
                            );
                        });
                        ui.end_row();

                        ui.label("suggested button (fill, text)");
                        ui.horizontal(|ui| {
                            ui.color_edit_button_srgba(&mut self.modal_style.suggested_button_fill);
                            ui.color_edit_button_srgba(
                                &mut self.modal_style.suggested_button_text_color,
                            );
                        });
                        ui.end_row();

                        ui.label("icon colors (info, warning, success, error)");
                        ui.horizontal(|ui| {
                            ui.color_edit_button_srgba(&mut self.modal_style.info_icon_color);
                            ui.color_edit_button_srgba(&mut self.modal_style.warning_icon_color);
                            ui.color_edit_button_srgba(&mut self.modal_style.success_icon_color);
                            ui.color_edit_button_srgba(&mut self.modal_style.error_icon_color);
                        });
                        ui.end_row();
                    });
            });

            // why is this down here?? just wanted to show that you can put
            // the modal's [`.show()`] anywhere but we could have put this above
            // modal if we wanted
            nested_modal.show(|ui| {
                nested_modal.frame(ui, |ui| {
                    nested_modal.body(ui, "hello there!");
                    // you can put textboxes in here.
                    ui.text_edit_singleline(&mut self.nested_modal_text);
                });
                nested_modal.buttons(ui, |ui| {
                    nested_modal.button(ui, "close");
                })
            });
        });
    }
}
fn main() {
    let _ = eframe::run_native(
        "egui-modal example",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(ExampleApp::default()))),
    );
}
