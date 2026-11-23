//! The tools that draw annotations and the icons of their buttons.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// Selects and moves, draws nothing
    Select,
    Arrow,
    Line,
    Frame,
    Circle,
    Text,
    Pen,
    Redact,
    Spotlight,
    Counter,
    Stamp(Stamp),
}

impl Tool {
    /// Those that draw, a tile each
    pub(super) const ALL: [Tool; 9] = [
        Tool::Arrow,
        Tool::Frame,
        Tool::Circle,
        Tool::Text,
        Tool::Pen,
        Tool::Line,
        Tool::Redact,
        Tool::Spotlight,
        Tool::Counter,
    ];

    pub(super) fn name(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Arrow => "Arrow",
            Tool::Line => "Line",
            Tool::Frame => "Frame",
            Tool::Circle => "Circle",
            Tool::Text => "Text",
            Tool::Pen => "Pen",
            Tool::Redact => "Redact",
            Tool::Spotlight => "Spotlight",
            Tool::Counter => "Counter",
            Tool::Stamp(stamp) => stamp.name(),
        }
    }

    pub(super) fn hint(self) -> &'static str {
        match self {
            Tool::Select if cfg!(target_os = "macos") => {
                "Click what to change, drag a box over what to select, Cmd+A selects everything"
            }
            Tool::Select => {
                "Click what to change, drag a box over what to select, Ctrl+A selects everything"
            }
            Tool::Arrow => {
                "Drag from where the arrow starts to what it points at, with Shift in steps of 15°"
            }
            Tool::Line => "Drag a straight line, with Shift in steps of 15°",
            Tool::Frame => "Drag a frame around something, with Shift a square",
            Tool::Circle => "Drag a circle around something, with Shift a round one",
            Tool::Text => "Click where the text goes, click a text to change it",
            Tool::Pen => "Draw freely",
            Tool::Redact => "Drag over what nobody should see",
            Tool::Spotlight => "Drag over what to show, the rest is dimmed, with Shift a square",
            Tool::Counter => "Click where the next number goes or drag how large",
            Tool::Stamp(_) => {
                "Click where it goes or drag how large, right-click for another symbol"
            }
        }
    }

    pub(super) fn icon(self) -> Icon {
        match self {
            Tool::Select => Icon::Mouse,
            Tool::Arrow => Icon::Arrow,
            Tool::Line => Icon::Line,
            Tool::Frame => Icon::Frame,
            Tool::Circle => Icon::Circle,
            Tool::Text => Icon::Text,
            Tool::Pen => Icon::Pen,
            Tool::Redact => Icon::Pixelate,
            Tool::Spotlight => Icon::Spotlight,
            Tool::Counter => Icon::Counter,
            Tool::Stamp(stamp) => Icon::Stamp(stamp),
        }
    }
}

/// What the button of a tool or of a way to hide shows
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Icon {
    Mouse,
    Arrow,
    Line,
    Frame,
    Circle,
    Text,
    Pen,
    Pixelate,
    Fill,
    Spotlight,
    Counter,
    Stamp(Stamp),
}

/// Draws an icon in the middle of `rect`, in lines as thick as those of the icon
/// font, `scale` times as large
pub(super) fn paint_icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    icon: Icon,
    color: Color32,
    scale: f32,
) {
    // in a box of 16 by 16
    let at = |x: f32, y: f32| rect.center() + egui::vec2(x - 8., y - 8.) * scale;
    let stroke = Stroke::new(1.6 * scale, color);
    let square = egui::Rect::from_min_max(at(2.5, 3.5), at(13.5, 12.5));
    match icon {
        Icon::Mouse => {
            painter.text(
                at(8., 8.),
                egui::Align2::CENTER_CENTER,
                crate::icons::MOUSE,
                egui::FontId::proportional(14. * scale),
                color,
            );
        }
        Icon::Arrow => {
            painter.line_segment([at(2.5, 13.5), at(13., 3.)], stroke);
            painter.add(egui::Shape::line(
                vec![at(7., 3.), at(13., 3.), at(13., 9.)],
                stroke,
            ));
            for end in [at(2.5, 13.5), at(7., 3.), at(13., 3.), at(13., 9.)] {
                painter.circle_filled(end, stroke.width / 2., color);
            }
        }
        Icon::Line => {
            let ends = [at(2.5, 13.5), at(13.5, 2.5)];
            painter.line_segment(ends, stroke);
            for end in ends {
                painter.circle_filled(end, stroke.width / 2., color);
            }
        }
        Icon::Frame => {
            painter.rect_stroke(square, 1., stroke, egui::StrokeKind::Middle);
        }
        Icon::Circle => {
            painter.circle_stroke(at(8., 8.), 5.8 * scale, stroke);
        }
        Icon::Text => {
            painter.text(
                at(8., 8.),
                egui::Align2::CENTER_CENTER,
                "Aa",
                egui::FontId::proportional(13. * scale),
                color,
            );
        }
        Icon::Pen => {
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke(
                [at(2., 12.), at(5., 0.), at(10., 17.), at(14., 4.)],
                false,
                TRANSPARENT,
                stroke,
            ));
        }
        Icon::Pixelate => {
            // blocks of two shades, as a pixelated area
            for row in 0..3 {
                for column in 0..3 {
                    let shade = if (row + column) % 2 == 0 {
                        color
                    } else {
                        opacity(color, Opacity::Light)
                    };
                    let min = at(2. + column as f32 * 4., 2. + row as f32 * 4.);
                    painter.rect_filled(
                        egui::Rect::from_min_size(min, egui::vec2(4., 4.) * scale),
                        0.,
                        shade,
                    );
                }
            }
        }
        Icon::Fill => {
            painter.rect_filled(square, 1., color);
        }
        Icon::Counter => {
            painter.circle_stroke(at(8., 8.), 5.8 * scale, stroke);
            let thin = Stroke::new(1.4 * scale, color);
            painter.line_segment([at(8.6, 5.), at(8.6, 11.)], thin);
            painter.line_segment([at(8.6, 5.), at(6.8, 6.5)], thin);
        }
        Icon::Spotlight => {
            painter.rect_filled(square, 1., opacity(color, Opacity::Light));
            painter.circle_filled(at(8., 8.), 3.4 * scale, color);
        }
        Icon::Stamp(stamp) => {
            painter.text(
                at(8., 8.),
                egui::Align2::CENTER_CENTER,
                stamp.glyph(),
                egui::FontId::proportional(14. * scale),
                color,
            );
        }
    }
}
