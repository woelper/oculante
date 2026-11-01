//! What new annotations look like: lines as wide as suits the image at first, then like the
//! annotation changed last.

use super::*;

/// What new annotations look like
#[derive(Debug, Clone)]
pub struct Style {
    color: [u8; 4],
    /// For new lines and arrows, 0 until an image tells how wide
    width: u32,
    text_size: u32,
    /// For a new stamp that is clicked, not dragged
    pub(super) stamp_size: u32,
    redaction: Redaction,
    outline: u32,
    outline_color: [u8; 4],
    radius: u32,
    font: Font,
    blocks: u32,
    redact_color: [u8; 4],
}

impl Default for Style {
    fn default() -> Self {
        Self {
            color: RED.to_srgba_unmultiplied(),
            width: 0,
            text_size: 0,
            stamp_size: 0,
            redaction: Redaction::Pixelate,
            outline: 0,
            outline_color: white(),
            radius: 0,
            font: Font::default(),
            blocks: 0,
            redact_color: [0, 0, 0, 255],
        }
    }
}

impl Style {
    /// Line widths scaled to an image of size `original`, calculates on first use.
    pub(super) fn fit(&mut self, original: (u32, u32)) {
        if self.width == 0 && original.0 > 0 && original.1 > 0 {
            self.width = (original.0.min(original.1) / 120).clamp(3, 80);
            self.text_size = self.width * 6;
            self.stamp_size = self.width * 8;
            self.blocks = (self.width * 3).max(8);
        }
    }

    pub(super) fn new_annotation(&self, tool: Tool) -> Option<Annotation> {
        let (shape, color, width) = match tool {
            Tool::Select => return None,
            Tool::Arrow => (Shape::Arrow([0, 0]), self.color, self.width),
            Tool::Frame => (Shape::Rect([0, 0]), self.color, self.width),
            Tool::Circle => (Shape::Ellipse([0, 0]), self.color, self.width),
            Tool::Pen => (Shape::Pen(vec![[0, 0]]), self.color, self.width),
            Tool::Text => (Shape::Text(String::new()), self.color, self.text_size),
            Tool::Stamp(stamp) => (Shape::Stamp(stamp), self.color, self.stamp_size),
            Tool::Redact => {
                let style = self.redaction;
                let shape = Shape::Redact {
                    size: [0, 0],
                    style,
                };
                return Some(Annotation::new(
                    shape,
                    self.redact_color,
                    self.blocks,
                    [0, 0],
                ));
            }
        };
        let mut a = Annotation::new(shape, color, width, [0, 0]);
        a.outline = self.outline;
        a.outline_color = self.outline_color;
        match tool {
            Tool::Frame => a.radius = self.radius,
            Tool::Text => a.font = self.font.clone(),
            _ => {}
        }
        Some(a)
    }

    /// The next annotations look like this one
    pub(super) fn remember(&mut self, a: &Annotation) {
        match &a.shape {
            Shape::Redact { style, .. } => {
                self.redaction = *style;
                self.blocks = a.width;
                self.redact_color = a.color;
                return;
            }
            Shape::Text(_) => {
                self.text_size = a.width;
                self.font = a.font.clone();
            }
            Shape::Rect(_) => {
                self.width = a.width;
                self.radius = a.radius;
            }
            Shape::Arrow(_) | Shape::Ellipse(_) | Shape::Pen(_) => self.width = a.width,
            Shape::Stamp(_) => self.stamp_size = a.width,
        }
        self.color = a.color;
        self.outline = a.outline;
        self.outline_color = a.outline_color;
    }
}
