use std::fmt;

use image::{imageops, Rgba, Rgba32FImage, RgbaImage};
use imageops::FilterType::Gaussian;
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
    Blur(u8),
    Resize {
        dimensions: (u32, u32),
        aspect: bool,
    },
    Crop((u32, u32, u32, u32)),
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
            Self::Crop(_) => write!(f, "✂ Crop"),
            Self::Invert => write!(f, "！ Invert"),
            Self::SwapRG => write!(f, "⬌ Swap R / G"),
            Self::SwapRB => write!(f, "⬌ Swap R / B"),
            Self::SwapBG => write!(f, "⬌ Swap B / G"),
            Self::Resize { .. } => write!(f, "⬜ Resize"),
            _ => write!(f, "Not implemented Display"),
        }
    }
}

impl ImageOperation {
    pub fn is_per_pixel(&self) -> bool {
        match self {
            Self::Blur(_) => false,
            Self::Resize { .. } => false,
            Self::Crop(_) => false,
            _ => true,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) -> Response {
        // ui.label_i(&format!("{}", self));
        match self {
            Self::Brightness(val) => ui.add(Slider::new(val, -255..=255)),
            Self::Blur(val) => ui.add(Slider::new(val, 0..=20)),
            Self::Desaturate(val) => ui.add(Slider::new(val, 0..=100)),
            Self::Contrast(val) => ui.add(Slider::new(val, -128..=128)),
            Self::Crop(bounds) => {
                let available_w_single_spacing =
                    ui.available_width() - ui.style().spacing.item_spacing.x * 3.;
                ui.horizontal(|ui| {
                    let mut r1 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut bounds.0)
                            .speed(4.)
                            .clamp_range(0..=10000)
                            .prefix("⏴ "),
                    );
                    let r2 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut bounds.2)
                            .speed(4.)
                            .clamp_range(0..=10000)
                            .prefix("⏵ "),
                    );
                    let r3 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut bounds.1)
                            .speed(4.)
                            .clamp_range(0..=10000)
                            .prefix("⏶ "),
                    );
                    let r4 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut bounds.3)
                            .speed(4.)
                            .clamp_range(0..=10000)
                            .prefix("⏷ "),
                    );
                    // TODO rewrite with any
                    if r2.changed() || r3.changed() || r4.changed() {
                        r1.changed = true;
                    }
                    r1
                })
                .inner
            }
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
            Self::Resize { dimensions, aspect } => {
                let ratio = dimensions.1 as f32 / dimensions.0 as f32;

                ui.horizontal(|ui| {
                    let mut r0 = ui.add(
                        egui::DragValue::new(&mut dimensions.0)
                            .speed(4.)
                            .clamp_range(1..=10000)
                            .prefix("X "),
                    );
                    let r1 = ui.add(
                        egui::DragValue::new(&mut dimensions.1)
                            .speed(4.)
                            .clamp_range(1..=10000)
                            .prefix("Y "),
                    );

                    if r0.changed() {
                        if *aspect {
                            dimensions.1 = (dimensions.0 as f32 * ratio) as u32
                        }
                    }

                    if r1.changed() {
                        r0.changed = true;
                        if *aspect {
                            dimensions.0 = (dimensions.1 as f32 / ratio) as u32
                        }
                    }

                    let r2 = ui.checkbox(aspect, "Locked");

                    if r2.changed() {
                        r0.changed = true;

                        if *aspect {
                            dimensions.1 = (dimensions.0 as f32 * ratio) as u32;
                        }
                    }

                    r0
                })
                .inner
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
            }
            Self::Crop(amt) => {
                if *amt != (0, 0, 0, 0) {
                    let sub_img = image::imageops::crop_imm(
                        img,
                        amt.0.max(0),
                        amt.1.max(0),
                        (img.width() as i32 - amt.2 as i32).max(0) as u32,
                        (img.height() as i32 - amt.3 as i32).max(0) as u32,
                    );
                    *img = sub_img.to_image();
                }
            }
            Self::Resize { dimensions, .. } => {
                if *dimensions != Default::default() {
                    *img = image::imageops::resize(img, dimensions.0, dimensions.1, Gaussian);
                }
            }
            _ => (),
        }
    }

    pub fn process_pixel(&self, p: &mut Rgba<f32>) {
        match self {
            Self::Brightness(amt) => {
                p[0] = p[0] + *amt as f32 / 255.;
                p[1] = p[1] + *amt as f32 / 255.;
                p[2] = p[2] + *amt as f32 / 255.;
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
                let factor: f32 = (1.015686275 * (*val as f32 / 255. + 1.0))
                    / (1.0 * (1.015686275 - *val as f32 / 255.)) as f32;
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
    let val = p[0] * 0.59 + p[1] * 0.3 + p[2] * 0.11;
    p[0] = egui::lerp(p[0] as f32..=val, factor);
    p[1] = egui::lerp(p[1] as f32..=val, factor);
    p[2] = egui::lerp(p[2] as f32..=val, factor);
}
