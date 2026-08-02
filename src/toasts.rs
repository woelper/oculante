use egui::{
    Align2, Area, Context, CornerRadius, FontId, Frame, Id, Margin, Order, Rect, Sense, Stroke,
    Style, TextStyle, Ui, Vec2, pos2, vec2,
};
use std::time::Duration;

use crate::icons;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Anchor {
    TopLeft,
    TopRight,
    #[default]
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastLevel {
    fn icon(self) -> &'static str {
        match self {
            ToastLevel::Info => icons::INFO,
            ToastLevel::Success => icons::CHECK,
            ToastLevel::Warning => icons::WARNING_CIRCLE,
            ToastLevel::Error => icons::ERROR_CIRCLE,
        }
    }

    fn default_duration(self) -> f32 {
        match self {
            ToastLevel::Info | ToastLevel::Success => 3.0,
            ToastLevel::Warning => 4.5,
            ToastLevel::Error => 6.0,
        }
    }
}

const MARGIN: f32 = 12.0;
const SPACING: f32 = 8.0;
const WIDTH: f32 = 300.0;
const APPEAR_SECS: f32 = 0.18;
const DISAPPEAR_SECS: f32 = 0.2;
const SLIDE_PX: f32 = 18.0;

const ICON_BOX: f32 = 20.0;
const ICON_SIZE: f32 = 17.0;
const CLOSE_BOX: f32 = 20.0;
const CLOSE_SIZE: f32 = 13.0;
const GAP: f32 = 8.0;

struct Toast {
    id: u64,
    text: String,
    level: ToastLevel,
    total: Option<f32>,
    remaining: Option<f32>,
    shown: f32,
    closing: bool,
    last_height: f32,
}

pub struct ToastHandle<'a> {
    toast: &'a mut Toast,
}

impl ToastHandle<'_> {
    pub fn duration(self, duration: impl Into<Option<Duration>>) -> Self {
        let secs = duration.into().map(|d| d.as_secs_f32().max(0.05));
        self.toast.total = secs;
        self.toast.remaining = secs;
        self
    }
}

pub struct Toasts {
    anchor: Anchor,
    items: Vec<Toast>,
    next_id: u64,
}

impl Default for Toasts {
    fn default() -> Self {
        Self {
            anchor: Anchor::BottomLeft,
            items: Vec::new(),
            next_id: 0,
        }
    }
}

impl Toasts {
    pub fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    fn push(&mut self, text: impl Into<String>, level: ToastLevel) -> ToastHandle<'_> {
        let id = self.next_id;
        self.next_id += 1;
        let secs = Some(level.default_duration());
        self.items.push(Toast {
            id,
            text: text.into(),
            level,
            total: secs,
            remaining: secs,
            shown: 0.0,
            closing: false,
            last_height: 48.0,
        });
        ToastHandle {
            toast: self.items.last_mut().expect("just pushed a toast"),
        }
    }

    pub fn info(&mut self, text: impl Into<String>) -> ToastHandle<'_> {
        self.push(text, ToastLevel::Info)
    }

    pub fn success(&mut self, text: impl Into<String>) -> ToastHandle<'_> {
        self.push(text, ToastLevel::Success)
    }

    pub fn warning(&mut self, text: impl Into<String>) -> ToastHandle<'_> {
        self.push(text, ToastLevel::Warning)
    }

    pub fn error(&mut self, text: impl Into<String>) -> ToastHandle<'_> {
        self.push(text, ToastLevel::Error)
    }

    /// Draws and animates all active toasts. Call this once per frame.
    pub fn show(&mut self, ctx: &Context) {
        if self.items.is_empty() {
            return;
        }

        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        let screen = ctx.content_rect();
        let style = ctx.style_of(ctx.theme());

        let (mut cursor_y, top_down) = match self.anchor {
            Anchor::TopLeft | Anchor::TopRight => (screen.top() + MARGIN, true),
            Anchor::BottomLeft | Anchor::BottomRight => (screen.bottom() - MARGIN, false),
        };
        let right_aligned = matches!(self.anchor, Anchor::TopRight | Anchor::BottomRight);
        let x = if right_aligned {
            screen.right() - MARGIN - WIDTH
        } else {
            screen.left() + MARGIN
        };

        for toast in &mut self.items {
            let top_left = pos2(
                x,
                if top_down {
                    cursor_y
                } else {
                    cursor_y - toast.last_height
                },
            );

            let eased = ease_out_cubic(toast.shown);
            let slide = (1.0 - eased) * SLIDE_PX * if top_down { -1.0 } else { 1.0 };
            let pos = top_left + vec2(0.0, slide);

            let area_id = Id::new(("oculante_toast", toast.id));
            let (close_clicked, hovered, height) = Area::new(area_id)
                .order(Order::Foreground)
                .movable(false)
                .fade_in(false)
                .fixed_pos(pos)
                .show(ctx, |ui| {
                    ui.set_opacity(eased);
                    ui.set_width(WIDTH);
                    let close_clicked = draw_toast(ui, toast, &style);
                    let card = ui.min_rect();
                    (close_clicked, ui.rect_contains_pointer(card), card.height())
                })
                .inner;

            if close_clicked {
                toast.closing = true;
            }

            if toast.closing {
                toast.shown -= dt / DISAPPEAR_SECS;
            } else {
                toast.shown = (toast.shown + dt / APPEAR_SECS).min(1.0);
                if let Some(remaining) = toast.remaining.as_mut()
                    && !hovered {
                        *remaining -= dt;
                        if *remaining <= 0.0 {
                            toast.closing = true;
                        }
                    }
            }

            toast.last_height = height.max(1.0);

            let step = toast.last_height + SPACING;
            cursor_y += if top_down { step } else { -step };
        }

        self.items.retain(|t| !(t.closing && t.shown <= 0.0));

        ctx.request_repaint();
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Draws a single toast card and returns whether its close button was clicked this frame.
fn draw_toast(ui: &mut Ui, toast: &Toast, style: &Style) -> bool {
    let accent = style.visuals.selection.bg_fill;
    let bg = style.visuals.window_fill();
    let fg = style.visuals.text_color();

    let mut close_clicked = false;

    Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(8))
        .stroke(Stroke::new(1.0, accent.gamma_multiply(0.5)))
        .shadow(style.visuals.window_shadow)
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            let row_w = ui.available_width();

            let font = TextStyle::Body.resolve(style);
            let text_w = (row_w - ICON_BOX - CLOSE_BOX - 2.0 * GAP).max(40.0);
            let galley = ui
                .painter()
                .layout(toast.text.clone(), font.clone(), fg, text_w);

            let row_h = galley.size().y.max(ICON_BOX).max(CLOSE_BOX);
            let (row, _) = ui.allocate_exact_size(vec2(row_w, row_h), Sense::hover());
            let mid = row.center().y;

            let icon_box = Rect::from_center_size(
                pos2(row.left() + ICON_BOX / 2.0, mid),
                Vec2::splat(ICON_BOX),
            );
            ui.painter().text(
                icon_box.center(),
                Align2::CENTER_CENTER,
                toast.level.icon(),
                FontId::new(ICON_SIZE, font.family.clone()),
                accent,
            );

            ui.painter().galley(
                pos2(icon_box.right() + GAP, mid - galley.size().y / 2.0),
                galley,
                fg,
            );

            let close_box = Rect::from_center_size(
                pos2(row.right() - CLOSE_BOX / 2.0, mid),
                Vec2::splat(CLOSE_BOX),
            );
            let close = ui.interact(
                close_box,
                ui.id().with(("toast_close", toast.id)),
                Sense::click(),
            );
            if close.hovered() {
                ui.painter()
                    .rect_filled(close_box, CornerRadius::same(4), fg.gamma_multiply(0.08));
            }
            ui.painter().text(
                close_box.center(),
                Align2::CENTER_CENTER,
                icons::X,
                FontId::new(CLOSE_SIZE, font.family.clone()),
                fg.gamma_multiply(if close.hovered() { 1.0 } else { 0.6 }),
            );
            close_clicked = close.clicked();

            if let (Some(total), Some(remaining)) = (toast.total, toast.remaining)
                && total > 0.0 {
                    ui.add_space(8.0);
                    let frac = (remaining / total).clamp(0.0, 1.0);
                    let (rect, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 3.0), Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 1.5, accent.gamma_multiply(0.25));
                    let filled =
                        Rect::from_min_size(rect.min, vec2(rect.width() * frac, rect.height()));
                    ui.painter().rect_filled(filled, 1.5, accent);
                }
        });

    close_clicked
}
