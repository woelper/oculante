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
    /// Of all new ones but redactions and spotlights, off at first: how dark at the edge and
    /// how far out, as wide as a line is twice once an image tells
    shadow: u32,
    shadow_size: u32,
    /// Of new counters: their size when they are clicked, not dragged, and of their number
    pub(super) counter_size: u32,
    number_color: [u8; 4],
    /// Of new spotlights
    shade: [u8; 4],
    spot_oval: bool,
    spot_radius: u32,
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
            // the rest of the image at a quarter of how bright it was
            shade: opacity(BLACK, Opacity::Strong).to_srgba_unmultiplied(),
            spot_oval: false,
            spot_radius: 0,
            counter_size: 0,
            number_color: white(),
            shadow: 0,
            shadow_size: 0,
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
            self.counter_size = self.width * 7;
            self.shadow_size = self.width * 2;
        }
    }

    pub(super) fn new_annotation(&self, tool: Tool) -> Option<Annotation> {
        let (shape, color, width) = match tool {
            Tool::Select => return None,
            Tool::Arrow => (Shape::Arrow([0, 0]), self.color, self.width),
            Tool::Line => (Shape::Line([0, 0]), self.color, self.width),
            Tool::Frame => (Shape::Rect([0, 0]), self.color, self.width),
            Tool::Circle => (Shape::Ellipse([0, 0]), self.color, self.width),
            Tool::Pen => (Shape::Pen(vec![[0, 0]]), self.color, self.width),
            Tool::Text => (Shape::Text(String::new()), self.color, self.text_size),
            Tool::Stamp(stamp) => (Shape::Stamp(stamp), self.color, self.stamp_size),
            // numbered when it is placed
            Tool::Counter => (Shape::Counter(1), self.color, self.counter_size),
            Tool::Spotlight => {
                let shape = Shape::Spotlight {
                    size: [0, 0],
                    oval: self.spot_oval,
                };
                let mut a = Annotation::new(shape, self.shade, 0, [0, 0]);
                a.radius = self.spot_radius;
                return Some(a);
            }
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
        a.shadow = self.shadow;
        a.shadow_size = self.shadow_size;
        match tool {
            Tool::Frame => a.radius = self.radius,
            Tool::Text => a.font = self.font.clone(),
            Tool::Counter => a.number_color = self.number_color,
            _ => {}
        }
        Some(a)
    }

    /// The next annotations look like the previous
    pub(super) fn remember(&mut self, a: &Annotation) {
        match &a.shape {
            Shape::Redact { style, .. } => {
                self.redaction = *style;
                self.blocks = a.width;
                self.redact_color = a.color;
                return;
            }
            Shape::Spotlight { oval, .. } => {
                self.shade = a.color;
                self.spot_oval = *oval;
                self.spot_radius = a.radius;
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
            Shape::Arrow(_) | Shape::Line(_) | Shape::Ellipse(_) | Shape::Pen(_) => {
                self.width = a.width
            }
            Shape::Stamp(_) => self.stamp_size = a.width,
            // its color is that of the others
            Shape::Counter(_) => {
                self.counter_size = a.width;
                self.number_color = a.number_color;
            }
        }
        self.color = a.color;
        self.outline = a.outline;
        self.outline_color = a.outline_color;
        self.shadow = a.shadow;
        self.shadow_size = a.shadow_size;
    }
}
