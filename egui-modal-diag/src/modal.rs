use egui::{
    emath::{Align, Align2},
    epaint::{Color32, Pos2, Rounding},
    Area, Button, Context, Id, Layout, Response, RichText, Sense, Ui, WidgetText, Window,
};

const ERROR_ICON_COLOR: Color32 = Color32::from_rgb(200, 90, 90);
const INFO_ICON_COLOR: Color32 = Color32::from_rgb(150, 200, 210);
const WARNING_ICON_COLOR: Color32 = Color32::from_rgb(230, 220, 140);
const SUCCESS_ICON_COLOR: Color32 = Color32::from_rgb(140, 230, 140);

const CAUTION_BUTTON_FILL: Color32 = Color32::from_rgb(87, 38, 34);
const SUGGESTED_BUTTON_FILL: Color32 = Color32::from_rgb(33, 54, 84);
const CAUTION_BUTTON_TEXT_COLOR: Color32 = Color32::from_rgb(242, 148, 148);
const SUGGESTED_BUTTON_TEXT_COLOR: Color32 = Color32::from_rgb(141, 182, 242);

const OVERLAY_COLOR: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 200);

/// The different styles a modal button can take.
pub enum ModalButtonStyle {
    /// A normal [`egui`] button
    None,
    /// A button highlighted blue
    Suggested,
    /// A button highlighted red
    Caution,
}

/// An icon. If used, it will be shown next to the body of
/// the modal.
#[derive(Clone, Default, PartialEq)]
pub enum Icon {
    #[default]
    /// An info icon
    Info,
    /// A warning icon
    Warning,
    /// A success icon
    Success,
    /// An error icon
    Error,
    /// A custom icon. The first field in the tuple is
    /// the text of the icon, and the second field is the
    /// color.
    Custom((String, Color32)),
}

impl std::fmt::Display for Icon {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Icon::Info => write!(f, "ℹ"),
            Icon::Warning => write!(f, "⚠"),
            Icon::Success => write!(f, "✔"),
            Icon::Error => write!(f, "❗"),
            Icon::Custom((icon_text, _)) => write!(f, "{icon_text}"),
        }
    }
}

#[derive(Clone, Default)]
struct DialogData {
    title: Option<String>,
    body: Option<String>,
    icon: Option<Icon>,
}

/// Used for constructing and opening a modal dialog. This can be used
/// to both set the title/body/icon of the modal and open it as a one-time call
/// (as opposed to a continous call in the update loop) at the same time.
/// Make sure to call `DialogBuilder::open` to actually open the dialog.
#[must_use = "use `DialogBuilder::open`"]
pub struct DialogBuilder {
    data: DialogData,
    modal_id: Id,
    ctx: Context,
}

#[derive(Clone)]
enum ModalType {
    Modal,
    Dialog(DialogData),
}

#[derive(Clone)]
/// Information about the current state of the modal. (Pretty empty
/// right now but may be expanded upon in the future.)
struct ModalState {
    is_open: bool,
    was_outside_clicked: bool,
    modal_type: ModalType,
    last_frame_height: Option<f32>,
}

#[derive(Clone, Debug)]
/// Contains styling parameters for the modal, like body margin
/// and button colors.
pub struct ModalStyle {
    /// The margin around the modal body. Only applies if using
    /// [`.body()`]
    pub body_margin: f32,
    /// The margin around the container of the icon and body. Only
    /// applies if using [`.frame()`]
    pub frame_margin: f32,
    /// The margin around the container of the icon. Only applies
    /// if using [`.icon()`].
    pub icon_margin: f32,
    /// The size of any icons used in the modal
    pub icon_size: f32,
    /// The color of the overlay that dims the background
    pub overlay_color: Color32,

    /// The fill color for the caution button style
    pub caution_button_fill: Color32,
    /// The fill color for the suggested button style
    pub suggested_button_fill: Color32,

    /// The text color for the caution button style
    pub caution_button_text_color: Color32,
    /// The text color for the suggested button style
    pub suggested_button_text_color: Color32,

    /// The text of the acknowledgement button for dialogs
    pub dialog_ok_text: String,

    /// The color of the info icon
    pub info_icon_color: Color32,
    /// The color of the warning icon
    pub warning_icon_color: Color32,
    /// The color of the success icon
    pub success_icon_color: Color32,
    /// The color of the error icon
    pub error_icon_color: Color32,

    /// The default width of the modal
    pub default_width: Option<f32>,
    /// The default height of the modal
    pub default_height: Option<f32>,

    /// The alignment of text inside the body
    pub body_alignment: Align,
}

impl ModalState {
    fn load(ctx: &Context, id: Id) -> Self {
        ctx.data_mut(|d| d.get_temp(id).unwrap_or_default())
    }
    fn save(self, ctx: &Context, id: Id) {
        ctx.data_mut(|d| d.insert_temp(id, self))
    }
}

impl Default for ModalState {
    fn default() -> Self {
        Self {
            was_outside_clicked: false,
            is_open: false,
            modal_type: ModalType::Modal,
            last_frame_height: None,
        }
    }
}

impl Default for ModalStyle {
    fn default() -> Self {
        Self {
            body_margin: 5.,
            icon_margin: 7.,
            frame_margin: 2.,
            icon_size: 30.,
            overlay_color: OVERLAY_COLOR,

            caution_button_fill: CAUTION_BUTTON_FILL,
            suggested_button_fill: SUGGESTED_BUTTON_FILL,

            caution_button_text_color: CAUTION_BUTTON_TEXT_COLOR,
            suggested_button_text_color: SUGGESTED_BUTTON_TEXT_COLOR,

            dialog_ok_text: "ok".to_string(),

            info_icon_color: INFO_ICON_COLOR,
            warning_icon_color: WARNING_ICON_COLOR,
            success_icon_color: SUCCESS_ICON_COLOR,
            error_icon_color: ERROR_ICON_COLOR,

            default_height: None,
            default_width: None,

            body_alignment: Align::Min,
        }
    }
}
/// A [`Modal`] is created using [`Modal::new()`]. Make sure to use a `let` binding when
/// using [`Modal::new()`] to ensure you can call things like [`Modal::open()`] later on.
/// ```
/// let modal = Modal::new(ctx, "my_modal");
/// modal.show(|ui| {
///     ui.label("Hello world!")
/// });
/// if ui.button("modal").clicked() {
///     modal.open();
/// }
/// ```
/// Helper functions are also available to use that help apply margins based on the modal's
/// [`ModalStyle`]. They are not necessary to use, but may help reduce boilerplate.
/// ```
/// let other_modal = Modal::new(ctx, "another_modal");
/// other_modal.show(|ui| {
///     other_modal.frame(ui, |ui| {
///         other_modal.body(ui, "Hello again, world!");
///     });
///     other_modal.buttons(ui, |ui| {
///         other_modal.button(ui, "Close");
///     });
/// });
/// if ui.button("open the other modal").clicked() {
///     other_modal.open();
/// }
/// ```
pub struct Modal {
    close_on_outside_click: bool,
    style: ModalStyle,
    ctx: Context,
    id: Id,
    window_id: Id,
}

fn ui_with_margin<R>(ui: &mut Ui, margin: f32, add_contents: impl FnOnce(&mut Ui) -> R) {
    egui::Frame::none()
        .inner_margin(margin)
        .show(ui, |ui| add_contents(ui));
}

impl Modal {
    /// Creates a new [`Modal`]. Can use constructor functions like [`Modal::with_style`]
    /// to modify upon creation.
    pub fn new(ctx: &Context, id_source: impl std::fmt::Display) -> Self {
        let self_id = Id::new(id_source.to_string());
        Self {
            window_id: self_id.with("window"),
            id: self_id,
            style: ModalStyle::default(),
            ctx: ctx.clone(),
            close_on_outside_click: false,
        }
    }

    fn set_open_state(&self, is_open: bool) {
        let mut modal_state = ModalState::load(&self.ctx, self.id);
        modal_state.is_open = is_open;
        modal_state.save(&self.ctx, self.id)
    }

    fn set_outside_clicked(&self, was_clicked: bool) {
        let mut modal_state = ModalState::load(&self.ctx, self.id);
        modal_state.was_outside_clicked = was_clicked;
        modal_state.save(&self.ctx, self.id)
    }

    /// Was the outer overlay clicked this frame?
    pub fn was_outside_clicked(&self) -> bool {
        let modal_state = ModalState::load(&self.ctx, self.id);
        modal_state.was_outside_clicked
    }

    /// Is the modal currently open?
    pub fn is_open(&self) -> bool {
        let modal_state = ModalState::load(&self.ctx, self.id);
        modal_state.is_open
    }

    /// Open the modal; make it visible. The modal prevents user input to other parts of the
    /// application.
    ///
    /// ⚠️ WARNING ⚠️: This function requires a write lock to the [`egui::Context`]. Using it within
    /// closures within functions like [`egui::Ui::input_mut`] will result in a deadlock. [Tracking issue](https://github.com/n00kii/egui-modal/issues/15)
    pub fn open(&self) {
        self.set_open_state(true)
    }

    /// Close the modal so that it is no longer visible, allowing input to flow back into
    /// the application.
    ///
    /// ⚠️ WARNING ⚠️: This function requires a write lock to the [`egui::Context`]. Using it within
    /// closures within functions like [`egui::Ui::input_mut`] will result in a deadlock. [Tracking issue](https://github.com/n00kii/egui-modal/issues/15)
    pub fn close(&self) {
        self.set_open_state(false)
    }

    /// If set to `true`, the modal will close itself if the user clicks outside on the modal window
    /// (onto the overlay).
    pub fn with_close_on_outside_click(mut self, do_close_on_click_ouside: bool) -> Self {
        self.close_on_outside_click = do_close_on_click_ouside;
        self
    }

    /// Change the [`ModalStyle`] of the modal upon creation.
    pub fn with_style(mut self, style: &ModalStyle) -> Self {
        self.style = style.clone();
        self
    }

    /// Helper function for styling the title of the modal.
    /// ```
    /// let modal = Modal::new(ctx, "modal");
    /// modal.show(|ui| {
    ///     modal.title(ui, "my title");
    /// });
    /// ```
    pub fn title(&self, ui: &mut Ui, text: impl Into<RichText>) {
        let text: RichText = text.into();
        ui.vertical_centered(|ui| {
            ui.heading(text);
        });
        ui.separator();
    }

    /// Helper function for styling the icon of the modal.
    /// ```
    /// let modal = Modal::new(ctx, "modal");
    /// modal.show(|ui| {
    ///     modal.frame(ui, |ui| {
    ///         modal.icon(ui, Icon::Info);
    ///     });
    /// });
    /// ```
    pub fn icon(&self, ui: &mut Ui, icon: Icon) {
        let color = match icon {
            Icon::Info => self.style.info_icon_color,
            Icon::Warning => self.style.warning_icon_color,
            Icon::Success => self.style.success_icon_color,
            Icon::Error => self.style.error_icon_color,
            Icon::Custom((_, color)) => color,
        };
        let text = RichText::new(icon.to_string())
            .color(color)
            .size(self.style.icon_size);
        ui_with_margin(ui, self.style.icon_margin, |ui| {
            ui.add(egui::Label::new(text));
        });
    }

    /// Helper function for styling the container the of body and icon.
    /// ```
    /// let modal = Modal::new(ctx, "modal");
    /// modal.show(|ui| {
    ///     modal.title(ui, "my title");
    ///     modal.frame(ui, |ui| {
    ///         // inner modal contents go here
    ///     });
    ///     modal.buttons(ui, |ui| {
    ///         // button contents go here
    ///     });
    /// });
    /// ```
    pub fn frame<R>(&self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) {
        let last_frame_height = ModalState::load(&self.ctx, self.id)
            .last_frame_height
            .unwrap_or_default();
        let default_height = self.style.default_height.unwrap_or_default();
        let space_height = ((default_height - last_frame_height) * 0.5).max(0.);
        ui.with_layout(
            Layout::top_down(Align::Center).with_cross_align(Align::Center),
            |ui| {
                ui_with_margin(ui, self.style.frame_margin, |ui| {
                    if space_height > 0. {
                        ui.add_space(space_height);
                        add_contents(ui);
                        ui.add_space(space_height);
                    } else {
                        add_contents(ui);
                    }
                })
            },
        );
    }

    /// Helper function that should be used when using a body and icon together.
    /// ```
    /// let modal = Modal::new(ctx, "modal");
    /// modal.show(|ui| {
    ///     modal.frame(ui, |ui| {
    ///         modal.body_and_icon(ui, "my modal body", Icon::Warning);
    ///     });
    /// });
    /// ```
    pub fn body_and_icon(&self, ui: &mut Ui, text: impl Into<WidgetText>, icon: Icon) {
        egui::Grid::new(self.id).num_columns(2).show(ui, |ui| {
            self.icon(ui, icon);
            self.body(ui, text);
        });
    }

    /// Helper function for styling the body of the modal.
    /// ```
    /// let modal = Modal::new(ctx, "modal");
    /// modal.show(|ui| {
    ///     modal.frame(ui, |ui| {
    ///         modal.body(ui, "my modal body");
    ///     });
    /// });
    /// ```
    pub fn body(&self, ui: &mut Ui, text: impl Into<WidgetText>) {
        let text: WidgetText = text.into();
        ui.with_layout(Layout::top_down(self.style.body_alignment), |ui| {
            ui_with_margin(ui, self.style.body_margin, |ui| {
                ui.label(text);
            })
        });
    }

    /// Helper function for styling the button container of the modal.
    /// ```
    /// let modal = Modal::new(ctx, "modal");
    /// modal.show(|ui| {
    ///     modal.buttons(ui, |ui| {
    ///         modal.button(ui, "my modal button");
    ///     });
    /// });
    /// ```
    pub fn buttons<R>(&self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) {
        ui.separator();
        ui.with_layout(Layout::right_to_left(Align::Min), add_contents);
    }

    /// Helper function for creating a normal button for the modal.
    /// Automatically closes the modal on click.
    pub fn button(&self, ui: &mut Ui, text: impl Into<WidgetText>) -> Response {
        self.styled_button(ui, text, ModalButtonStyle::None)
    }

    /// Helper function for creating a "cautioned" button for the modal.
    /// Automatically closes the modal on click.
    pub fn caution_button(&self, ui: &mut Ui, text: impl Into<WidgetText>) -> Response {
        self.styled_button(ui, text, ModalButtonStyle::Caution)
    }

    /// Helper function for creating a "suggested" button for the modal.
    /// Automatically closes the modal on click.
    pub fn suggested_button(&self, ui: &mut Ui, text: impl Into<WidgetText>) -> Response {
        self.styled_button(ui, text, ModalButtonStyle::Suggested)
    }

    fn styled_button(
        &self,
        ui: &mut Ui,
        text: impl Into<WidgetText>,
        button_style: ModalButtonStyle,
    ) -> Response {
        let button = match button_style {
            ModalButtonStyle::Suggested => {
                let text: WidgetText = text.into().color(self.style.suggested_button_text_color);
                Button::new(text).fill(self.style.suggested_button_fill)
            }
            ModalButtonStyle::Caution => {
                let text: WidgetText = text.into().color(self.style.caution_button_text_color);
                Button::new(text).fill(self.style.caution_button_fill)
            }
            ModalButtonStyle::None => Button::new(text.into()),
        };

        let response = ui.add(button);
        if response.clicked() {
            self.close()
        }
        response
    }

    /// The ui contained in this function will be shown within the modal window. The modal will only actually show
    /// when [`Modal::open`] is used.
    pub fn show<R>(&self, add_contents: impl FnOnce(&mut Ui) -> R) {
        let mut modal_state = ModalState::load(&self.ctx, self.id);
        self.set_outside_clicked(false);
        if modal_state.is_open {
            let ctx_clone = self.ctx.clone();
            let area_resp = Area::new(self.id)
                .interactable(true)
                .fixed_pos(Pos2::ZERO)
                .show(&self.ctx, |ui: &mut Ui| {
                    let screen_rect = ui.ctx().input(|i| i.screen_rect);
                    let area_response = ui.allocate_response(screen_rect.size(), Sense::click());
                    // let current_focus = area_response.ctx.memory().focus().clone();
                    // let top_layer = area_response.ctx.memory().layer_ids().last();
                    // if let Some(focus) = current_focus {
                    //     area_response.ctx.memory().surrender_focus(focus)
                    // }
                    if area_response.clicked() {
                        self.set_outside_clicked(true);
                        if self.close_on_outside_click {
                            self.close();
                        }
                    }
                    ui.painter()
                        .rect_filled(screen_rect, Rounding::ZERO, self.style.overlay_color);
                });

            ctx_clone.move_to_top(area_resp.response.layer_id);

            // the below lines of code addresses a weird problem where if the default_height changes, egui doesnt respond unless
            // it's a different window id
            let mut window_id = self
                .style
                .default_width
                .map_or(self.window_id, |w| self.window_id.with(w.to_string()));

            window_id = self
                .style
                .default_height
                .map_or(window_id, |h| window_id.with(h.to_string()));

            let mut window = Window::new("")
                .id(window_id)
                .open(&mut modal_state.is_open)
                .title_bar(false)
                .anchor(Align2::CENTER_CENTER, [0., 0.])
                .resizable(false);

            let recalculating_height =
                self.style.default_height.is_some() && modal_state.last_frame_height.is_none();

            if let Some(default_height) = self.style.default_height {
                window = window.default_height(default_height);
            }

            if let Some(default_width) = self.style.default_width {
                window = window.default_width(default_width);
            }

            let response = window.show(&ctx_clone, add_contents);

            if let Some(inner_response) = response {
                ctx_clone.move_to_top(inner_response.response.layer_id);
                if recalculating_height {
                    let mut modal_state = ModalState::load(&self.ctx, self.id);
                    modal_state.last_frame_height = Some(inner_response.response.rect.height());
                    modal_state.save(&self.ctx, self.id);
                }
            }
        }
    }

    /// Open the modal as a dialog. This is a shorthand way of defining a [`Modal::show`] once,
    /// for example, if a function returns an `Error`. This should be used in conjunction with
    /// [`Modal::show_dialog`].
    #[deprecated(since = "0.3.0", note = "use `Modal::dialog`")]
    pub fn open_dialog(
        &self,
        title: Option<impl std::fmt::Display>,
        body: Option<impl std::fmt::Display>,
        icon: Option<Icon>,
    ) {
        let modal_data = DialogData {
            title: title.map(|s| s.to_string()),
            body: body.map(|s| s.to_string()),
            icon,
        };
        let mut modal_state = ModalState::load(&self.ctx, self.id);
        modal_state.modal_type = ModalType::Dialog(modal_data);
        modal_state.is_open = true;
        modal_state.save(&self.ctx, self.id);
    }

    /// Create a `DialogBuilder` for this modal. Make sure to use `DialogBuilder::open`
    /// to open the dialog.
    pub fn dialog(&self) -> DialogBuilder {
        DialogBuilder {
            data: DialogData::default(),
            modal_id: self.id.clone(),
            ctx: self.ctx.clone(),
        }
    }

    /// Needed in order to use [`Modal::dialog`]. Make sure this is called every frame, as
    /// it renders the necessary ui when using a modal as a dialog.
    pub fn show_dialog(&mut self) {
        let modal_state = ModalState::load(&self.ctx, self.id);
        if let ModalType::Dialog(modal_data) = modal_state.modal_type {
            self.close_on_outside_click = true;
            self.show(|ui| {
                if let Some(title) = modal_data.title {
                    self.title(ui, title)
                }
                self.frame(ui, |ui| {
                    if modal_data.body.is_none() {
                        if let Some(icon) = modal_data.icon {
                            self.icon(ui, icon)
                        }
                    } else if modal_data.icon.is_none() {
                        if let Some(body) = modal_data.body {
                            self.body(ui, body)
                        }
                    } else if modal_data.icon.is_some() && modal_data.icon.is_some() {
                        self.body_and_icon(ui, modal_data.body.unwrap(), modal_data.icon.unwrap())
                    }
                });
                self.buttons(ui, |ui| {
                    ui.with_layout(Layout::top_down_justified(Align::Center), |ui| {
                        self.button(ui, &self.style.dialog_ok_text)
                    })
                })
            });
        }
    }
}

impl DialogBuilder {
    /// Construct this dialog with the given title.
    pub fn with_title(mut self, title: impl std::fmt::Display) -> Self {
        self.data.title = Some(title.to_string());
        self
    }
    /// Construct this dialog with the given body.
    pub fn with_body(mut self, body: impl std::fmt::Display) -> Self {
        self.data.body = Some(body.to_string());
        self
    }
    /// Construct this dialog with the given icon.
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.data.icon = Some(icon);
        self
    }
    /// Open the dialog.
    ///
    /// ⚠️ WARNING ⚠️: This function requires a write lock to the [`egui::Context`]. Using it within
    /// closures within functions like [`egui::Ui::input_mut`] will result in a deadlock. [Tracking issue](https://github.com/n00kii/egui-modal/issues/15)
    pub fn open(self) {
        let mut modal_state = ModalState::load(&self.ctx, self.modal_id);
        modal_state.modal_type = ModalType::Dialog(self.data);
        modal_state.is_open = true;
        modal_state.save(&self.ctx, self.modal_id);
    }
}
