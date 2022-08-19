use std::fmt;

use image::{imageops, Rgba, RgbaImage};
use imageops::FilterType::Gaussian;
use log::info;
use notan::egui::{self, DragValue};
use notan::egui::{Response, Slider, Ui};
use palette::Pixel;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum ImageOperation {
    Brightness(i32),
    Desaturate(u8),
    Mult([u8; 3]),
    Add([u8; 3]),
    Fill([u8; 3]),
    Contrast(i32),
    Flip(bool),
    Rotate(bool),
    HSV((u16, u8, u8)),
    ChromaticAberration(u8),
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
            Self::Fill(_) => write!(f, "🍺 Fill color"),
            Self::Blur(_) => write!(f, "💧 Blur"),
            Self::Crop(_) => write!(f, "✂ Crop"),
            Self::Flip(_) => write!(f, "⬌ Flip"),
            Self::Rotate(_) => write!(f, "⟳ Rotate"),
            Self::Invert => write!(f, "！ Invert"),
            Self::SwapRG => write!(f, "⬌ Swap R / G"),
            Self::SwapRB => write!(f, "⬌ Swap R / B"),
            Self::SwapBG => write!(f, "⬌ Swap B / G"),
            Self::HSV(_) => write!(f, "◔ HSV"),
            Self::ChromaticAberration(_) => write!(f, "○ Chromatic Aberration"),
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
            Self::Rotate(_) => false,
            Self::Flip(_) => false,
            Self::Fill(_) => false,
            Self::ChromaticAberration(_) => false,
            _ => true,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) -> Response {
        // ui.label_i(&format!("{}", self));
        match self {
            Self::Brightness(val) => ui.add(Slider::new(val, -255..=255)),
            Self::ChromaticAberration(val) => ui.add(Slider::new(val, 0..=255)),
            Self::HSV(val) => {
                let mut r = ui.add(DragValue::new(&mut val.0).clamp_range(0..=360));
                if ui
                    .add(DragValue::new(&mut val.1).clamp_range(0..=100))
                    .changed()
                {
                    r.changed = true;
                }
                if ui
                    .add(DragValue::new(&mut val.2).clamp_range(0..=100))
                    .changed()
                {
                    r.changed = true;
                }
                r
            }
            Self::Blur(val) => ui.add(Slider::new(val, 0..=20)),
            Self::Flip(horizontal) => {
                let mut r = ui.radio_value(horizontal, true, "V");
                if ui.radio_value(horizontal, false, "H").changed() {
                    r.changed = true
                }
                r
            }
            Self::Rotate(ccw) => {
                let mut r = ui.radio_value(ccw, true, "CCW");
                if ui.radio_value(ccw, false, "CW").changed() {
                    r.changed = true
                }
                r
            }
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
            Self::Fill(val) => {
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

                    // For this operator, we want to update on release, not on change.
                    // Since all operators are processed the same, we use the hack to emit `changed` just on release.
                    // Users dragging the resize values will now only trigger a resize on release, which feels
                    // more snappy.
                    r0.changed = r0.drag_released();

                    r0
                })
                .inner
            }
            _ => ui.label("Filter has no options."),
        }
    }

    /// Process all image operators (All things that modify the image and are not "per pixel")
    pub fn process_image(&self, img: &mut RgbaImage) {
        match self {
            Self::Blur(amt) => {
                if *amt != 0 {
                    *img = imageops::blur(img, *amt as f32);
                }
            }
            Self::Crop(dim) => {
                if *dim != (0, 0, 0, 0) {
                    let sub_img = image::imageops::crop_imm(
                        img,
                        dim.0.max(0),
                        dim.1.max(0),
                        (img.width() as i32 - dim.2 as i32 - dim.0 as i32).max(0) as u32,
                        (img.height() as i32 - dim.3 as i32 - dim.1 as i32).max(0) as u32,
                    );
                    *img = sub_img.to_image();
                }
            }
            Self::Resize { dimensions, .. } => {
                if *dimensions != Default::default() {
                    *img = image::imageops::resize(img, dimensions.0, dimensions.1, Gaussian);
                }
            }
            Self::Rotate(ccw) => {
                if *ccw {
                    *img = image::imageops::rotate270(img);
                } else {
                    *img = image::imageops::rotate90(img);
                }
            }
            Self::Flip(vert) => {
                if *vert {
                    *img = image::imageops::flip_vertical(img);
                }
                *img = image::imageops::flip_horizontal(img);
            }
            Self::Fill(color) => {
                *img = RgbaImage::from_pixel(
                    img.width(),
                    img.height(),
                    image::Rgba([color[0], color[1], color[2], 255]),
                )
            }
            Self::ChromaticAberration(amt) => {
                let center = (img.width() as i32 / 2, img.height() as i32 / 2);
                let img_c = img.clone();

                for (x, y, p) in img.enumerate_pixels_mut() {
                    // let dist_to_center = (((center.0 as f32 - x as f32).powi(2)
                    //     + (center.1 as f32 - y as f32).powi(2))
                    // .sqrt()/ center.0.max(center.1) as f32) * *amt as f32 / 10.;
                    let dist_to_center = (x as i32 - center.0, y as i32 - center.1);
                    let dist_to_center = ((dist_to_center.0 as f32 / center.0 as f32) * *amt as f32, (dist_to_center.1 as f32 / center.1 as f32) * *amt as f32);
                    // info!("{:?}", dist_to_center);
                    // info!("D {}", dist_to_center);
                    if let Some(l) = img_c
                        .get_pixel_checked((x as i32 + dist_to_center.0 as i32).max(0) as u32, (y as i32 + dist_to_center.1 as i32).max(0) as u32)
                    {
                        p[0] = l[0];
                    }
                }
            }

            _ => (),
        }
    }

    /// Process a single pixel.
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
            Self::HSV(amt) => {
                // rgb_to_hsv(p);
                use palette::{rgb::Rgb, Hsl, IntoColor};
                // let rgb: Rgb = Rgb::new(p[0], p[1], p[2]);
                let rgb: Rgb = *Rgb::from_raw(&p.0);

                let mut hsv: Hsl = rgb.into_color();
                hsv.hue += amt.0 as f32;
                hsv.saturation *= amt.1 as f32 / 100.;
                hsv.lightness *= amt.2 as f32 / 100.;
                let rgb: Rgb = hsv.into_color();

                *p = image::Rgba([rgb.red, rgb.green, rgb.blue, p[3]]);
                p[0] = rgb.red;
                p[1] = rgb.green;
                p[2] = rgb.blue;
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

fn rgb_to_hsv(p: &mut Rgba<f32>) {
    let cmax = p[0].max(p[1]).max(p[2]);
    let cmin = p[0].min(p[1]).min(p[2]);
    let delta = cmax - cmin;
    let mut h = 0.;
    let mut s = 0.;
    let mut v = 0.;

    if delta == 0. {
        h = 0.;
    } else if cmax == p[0] {
        h = ((p[1] - p[2]) / delta) % 6.;
    } else if cmax == p[1] {
        h = ((p[2] - p[0]) / delta) % 2.;
    } else {
        h = ((p[0] - p[1]) / delta) % 4.;
    }
    h = (h * 60.).round();

    if h < 0. {
        h += 360.;
    }

    // Calculate lightness
    v = (cmax + cmin) / 2.;

    // Calculate saturation
    s = if delta == 0. {
        0.
    } else {
        delta / (1. - (2. * v - 1.).abs())
    };

    // Multiply l and s by 100
    //   s = +(s * 100.);
    //   v = +(l * 100.);
}
