use std::fmt;

use image::{Rgba, Rgba32FImage, RgbaImage, imageops};
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
    Blur(u8)
}

impl fmt::Display for ImageOperation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Brightness(_) => write!(f, "☀ Brightness"),
            Self::Desaturate(_) => write!(f, "🌁 Desaturate"),
            Self::Contrast(_) => write!(f, "◑ Contrast"),
            Self::Mult(_) => write!(f, "✖ Mult color"),
            Self::Add(_) => write!(f, "➕ Add color"),
            Self::Blur(_) => write!(f, "💧 Blur"),
            Self::Invert => write!(f, "！ Invert"),
            Self::SwapRG => write!(f, "⬌ Swap R / G"),
            Self::SwapRB => write!(f, "⬌ Swap R / B"),
            Self::SwapBG => write!(f, "⬌ Swap B / G"),
            _ => write!(f, "Not implemented Display"),
        }
    }
}

impl ImageOperation {
    pub fn ui(&mut self, ui: &mut Ui) -> Response {
        // ui.label_i(&format!("{}", self));
        match self {
            Self::Brightness(val) => ui.add(Slider::new(val, -255..=255)),
            Self::Blur(val) => ui.add(Slider::new(val, 0..=20)),
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

    pub fn process_image(&self, img: &mut RgbaImage) {
        match self {
            Self::Blur(amt) => {
                if *amt != 0 {
                    *img = imageops::blur(img, *amt as f32);
                }
            },
            _ => ()
        }
    }

    pub fn process_pixel(&self, p: &mut Rgba<f32>) {
        match self {
            Self::Brightness(amt) => {
                p[0] = p[0]  + *amt as f32 / 255.;
                p[1] = p[1]  + *amt as f32 / 255.;
                p[2] = p[2]  + *amt as f32 / 255.;
            }
            Self::Desaturate(amt) => {
                desaturate(p, *amt as f32 / 100.);
            }
            Self::Mult(amt) => {
                p[0] = p[0] * amt[0] as f32 / 255.;
                p[1] = p[1] * amt[1] as f32 / 255.;
                p[2] = p[2] * amt[2] as f32 / 255.;
            }
            Self::Add(amt) => {
                p[0] = p[0] + amt[0] as f32 / 255.;
                p[1] = p[1] + amt[1] as f32 / 255.;
                p[2] = p[2] + amt[2] as f32 / 255.;
            }
            Self::Invert => {
                p[0] = 1. - p[0];
                p[1] = 1. - p[1];
                p[2] = 1. - p[2];
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
                let factor: f32 = (1.015686275 * (*val as f32/255. + 1.0)) / (1.0 * (1.015686275 - *val as f32/255.)) as f32;
                p[0] = ((factor * p[0] - 0.5) + 0.5).clamp(0.0, 1.0);
                p[1] = ((factor * p[1] - 0.5) + 0.5).clamp(0.0, 1.0);
                p[2] = ((factor * p[2] - 0.5) + 0.5).clamp(0.0, 1.0);
                // p[1] = ((factor * p[1] as f32 - 128.) + 128.).clamp(0.0, 255.0) as u8;
                // p[2] = ((factor * p[2] as f32 - 128.) + 128.).clamp(0.0, 255.0) as u8;
            }
            _ => (),
        }
    }
}

pub fn desaturate(p: &mut Rgba<f32>, factor: f32) {
    // G*.59+R*.3+B*.11
    let val = p[0]  * 0.59 + p[1]  * 0.3 + p[2]  * 0.11;
    p[0] = egui::lerp(p[0] as f32..=val, factor);
    p[1] = egui::lerp(p[1] as f32..=val, factor);
    p[2] = egui::lerp(p[2] as f32..=val, factor);
}
