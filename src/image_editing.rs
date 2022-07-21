use std::fmt;

use image::Rgba;
use notan::egui;
use notan::egui::{Response, Slider, Ui};


#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum ImageOperation {
    Brightness(i32),
    Desaturate(u8),
    Mult([u8; 3]),
    Add([u8; 3]),
    Contrast(i32),
    SwapRG,
    SwapRB,
    SwapBG,
    Invert,
}

impl fmt::Display for ImageOperation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Brightness(_) => write!(f, "☀ Brightness"),
            Self::Desaturate(_) => write!(f, "🌁 Desaturate"),
            Self::Contrast(_) => write!(f, "◑ Contrast"),
            Self::Mult(_) => write!(f, "✖ Mult color"),
            Self::Add(_) => write!(f, "➕ Add color"),
            Self::Invert => write!(f, "！ Invert"),
            Self::SwapRG => write!(f, "↔ Swap R and G"),
            Self::SwapRB => write!(f, "↔ Swap R and B"),
            Self::SwapBG => write!(f, "↔ Swap B and G"),
            _ => write!(f, "Not implemented Display"),
        }
    }
}

impl ImageOperation {
    pub fn ui(&mut self, ui: &mut Ui) -> Response {
        // ui.label_i(&format!("{}", self));
        match self {
            Self::Brightness(val) => ui.add(Slider::new(val, -255..=255)),
            Self::Desaturate(val) => ui.add(Slider::new(val, 0..=100)),
            Self::Contrast(val) => ui.add(Slider::new(val, -128..=128)),
            Self::Mult(val) => {
                let mut color: [f32; 3] = [
                    val[0] as f32 / 255.,
                    val[1] as f32 / 255.,
                    val[2] as f32 / 255.,
                ];

                let r = ui.color_edit_button_rgb(&mut color);
                if r.changed() {
                    val[0] = (color[0] * 255.) as u8;
                    val[1] = (color[1] * 255.) as u8;
                    val[2] = (color[2] * 255.) as u8;
                }
                r
            }
            Self::Add(val) => {
                let mut color: [f32; 3] = [
                    val[0] as f32 / 255.,
                    val[1] as f32 / 255.,
                    val[2] as f32 / 255.,
                ];

                let r = ui.color_edit_button_rgb(&mut color);
                if r.changed() {
                    val[0] = (color[0] * 255.) as u8;
                    val[1] = (color[1] * 255.) as u8;
                    val[2] = (color[2] * 255.) as u8;
                }
                r
            }
            _ => ui.label("Filter has no options."),
        }
    }

    pub fn process_pixel(&self, p: &mut Rgba<u8>) {
        match self {
            Self::Brightness(amt) => {
                p[0] = (p[0] as i32 + amt).clamp(0, 255) as u8;
                p[1] = (p[1] as i32 + amt).clamp(0, 255) as u8;
                p[2] = (p[2] as i32 + amt).clamp(0, 255) as u8;
            }
            Self::Desaturate(amt) => {
                desaturate(p, *amt as f32 / 100.);
            }
            Self::Mult(amt) => {
                p[0] = (p[0] as f32 * amt[0] as f32 / 255.) as u8;
                p[1] = (p[1] as f32 * amt[1] as f32 / 255.) as u8;
                p[2] = (p[2] as f32 * amt[2] as f32 / 255.) as u8;
            }
            Self::Add(amt) => {
                p[0] = (p[0] as i32 + amt[0] as i32).clamp(0, 255) as u8;
                p[1] = (p[1] as i32 + amt[1] as i32).clamp(0, 255) as u8;
                p[2] = (p[2] as i32 + amt[2] as i32).clamp(0, 255) as u8;
            }
            Self::Invert => {
                p[0] = 255 - p[0];
                p[1] = 255 - p[1];
                p[2] = 255 - p[2];
            }
            Self::SwapRG => {
                let r = p[0];
                p[0] = p[1];
                p[1] = r;
            }
            Self::SwapBG => {
                let b = p[1];
                p[2] = p[1];
                p[1] = b;
            }
            Self::SwapRB => {
                let r = p[0];
                p[0] = p[2];
                p[2] = r;
            }
            Self::Contrast(val) => {
                let factor: f32 = (259 * (*val + 255)) as f32 / (255 * (259 - val)) as f32;
                p[0] = ((factor * p[0] as f32 - 128.) + 128.).clamp(0.0, 255.0) as u8;
                p[1] = ((factor * p[1] as f32 - 128.) + 128.).clamp(0.0, 255.0) as u8;
                p[2] = ((factor * p[2] as f32 - 128.) + 128.).clamp(0.0, 255.0) as u8;
            }
            _ => (),
        }
    }
}

pub fn desaturate(p: &mut Rgba<u8>, factor: f32) {
    // G*.59+R*.3+B*.11
    let val = p[0] as f32 * 0.59 + p[1] as f32 * 0.3 + p[2] as f32 * 0.11;
    p[0] = egui::lerp(p[0] as f32..=val, factor) as u8;
    p[1] = egui::lerp(p[1] as f32..=val, factor) as u8;
    p[2] = egui::lerp(p[2] as f32..=val, factor) as u8;
}
