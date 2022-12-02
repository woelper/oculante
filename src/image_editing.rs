use std::fmt;

use crate::paint::PaintStroke;
use crate::ui::EguiExt;
use evalexpr::*;
use image::{imageops, RgbaImage};
use imageops::FilterType::*;
use nalgebra::Vector4;
use notan::egui::{self, DragValue, Sense, Vec2};
use notan::egui::{Response, Ui};
use palette::Pixel;
use rand::{thread_rng, Rng};
use rayon::{iter::ParallelIterator, slice::ParallelSliceMut};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EditState {
    #[serde(skip)]
    pub result_pixel_op: RgbaImage,
    #[serde(skip)]
    pub result_image_op: RgbaImage,
    pub painting: bool,
    pub non_destructive_painting: bool,
    pub paint_strokes: Vec<PaintStroke>,
    pub paint_fade: bool,
    #[serde(skip, default = "default_brushes")]
    pub brushes: Vec<RgbaImage>,
    pub pixel_op_stack: Vec<ImageOperation>,
    pub image_op_stack: Vec<ImageOperation>,
    pub export_extension: String,
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            result_pixel_op: RgbaImage::default(),
            result_image_op: RgbaImage::default(),
            painting: Default::default(),
            non_destructive_painting: Default::default(),
            paint_strokes: Default::default(),
            paint_fade: false,
            brushes: default_brushes(),
            pixel_op_stack: vec![],
            image_op_stack: vec![],
            export_extension: "png".into(),
        }
    }
}

fn default_brushes() -> Vec<RgbaImage> {
    vec![
        image::load_from_memory(include_bytes!("../res/brushes/brush1.png"))
            .expect("Brushes must always load")
            .into_rgba8(),
        image::load_from_memory(include_bytes!("../res/brushes/brush2.png"))
            .expect("Brushes must always load")
            .into_rgba8(),
        image::load_from_memory(include_bytes!("../res/brushes/brush3.png"))
            .expect("Brushes must always load")
            .into_rgba8(),
        image::load_from_memory(include_bytes!("../res/brushes/brush4.png"))
            .expect("Brushes must always load")
            .into_rgba8(),
        image::load_from_memory(include_bytes!("../res/brushes/brush5.png"))
            .expect("Brushes must always load")
            .into_rgba8(),
    ]
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum Channel {
    Red,
    Green,
    Blue,
    Alpha,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub enum ScaleFilter {
    Lanzcos3,
    Gaussian,
    Nearest,
    Triangle,
    CatmullRom,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub enum ImageOperation {
    Brightness(i32),
    Expression(String),
    Desaturate(u8),
    Posterize(u8),
    Exposure(i32),
    Equalize((i32, i32)),
    Mult([u8; 3]),
    Add([u8; 3]),
    Fill([u8; 4]),
    Contrast(i32),
    Flip(bool),
    Noise {
        amt: u8,
        mono: bool,
    },
    Rotate(bool),
    HSV((u16, i32, i32)),
    ChromaticAberration(u8),
    ChannelSwap((Channel, Channel)),
    Invert,
    Blur(u8),
    Resize {
        dimensions: (u32, u32),
        aspect: bool,
        filter: ScaleFilter,
    },
    Crop((u32, u32, u32, u32)),
}

impl fmt::Display for ImageOperation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::Brightness(_) => write!(f, "☀ Brightness"),
            Self::Noise { .. } => write!(f, "〰 Noise"),
            Self::Desaturate(_) => write!(f, "🌁 Desaturate"),
            Self::Posterize(_) => write!(f, "🖼 Posterize"),
            Self::Contrast(_) => write!(f, "◑ Contrast"),
            Self::Exposure(_) => write!(f, "✴ Exposure"),
            Self::Equalize(_) => write!(f, "☯ Equalize"),
            Self::Mult(_) => write!(f, "✖ Mult color"),
            Self::Add(_) => write!(f, "➕ Add color"),
            Self::Fill(_) => write!(f, "🍺 Fill color"),
            Self::Blur(_) => write!(f, "💧 Blur"),
            Self::Crop(_) => write!(f, "✂ Crop"),
            Self::Flip(_) => write!(f, "⬌ Flip"),
            Self::Rotate(_) => write!(f, "⟳ Rotate"),
            Self::Invert => write!(f, "！ Invert"),
            Self::ChannelSwap(_) => write!(f, "⬌ Channel Copy"),
            Self::HSV(_) => write!(f, "◔ HSV"),
            Self::ChromaticAberration(_) => write!(f, "📷 Color Fringe"),
            Self::Resize { .. } => write!(f, "⬜ Resize"),
            Self::Expression(_) => write!(f, "📄 Expression"),
            // _ => write!(f, "Not implemented Display"),
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
            Self::ChromaticAberration(_) => false,
            _ => true,
        }
    }

    // Add functionality about how to draw UI here
    pub fn ui(&mut self, ui: &mut Ui) -> Response {
        // ui.label_i(&format!("{}", self));
        match self {
            Self::Brightness(val) => ui.slider_styled(val, -255..=255),
            Self::Exposure(val) => ui.slider_styled(val, -100..=100),
            Self::ChromaticAberration(val) => ui.slider_styled(val, 0..=255),
            Self::Posterize(val) => ui.slider_styled(val, 1..=255),
            Self::Expression(expr) => ui.text_edit_singleline(expr),
            Self::ChannelSwap(val) => {
                let mut r = ui.allocate_response(Vec2::ZERO, Sense::click());
                let combo_width = 50.;

                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_source(format!("ccopy 0 {}", val.0 as usize))
                        .selected_text(format!("{:?}", val.0))
                        .width(combo_width)
                        .show_ui(ui, |ui| {
                            for f in [Channel::Red, Channel::Green, Channel::Blue, Channel::Alpha] {
                                if ui
                                    .selectable_value(&mut val.0, f, format!("{:?}", f))
                                    .clicked()
                                {
                                    r.changed = true;
                                }
                            }
                        });

                    ui.label("=");

                    egui::ComboBox::from_id_source(format!("ccopy 1 {}", val.1 as usize))
                        .selected_text(format!("{:?}", val.1))
                        .width(combo_width)
                        .show_ui(ui, |ui| {
                            for f in [Channel::Red, Channel::Green, Channel::Blue, Channel::Alpha] {
                                if ui
                                    .selectable_value(&mut val.1, f, format!("{:?}", f))
                                    .clicked()
                                {
                                    r.changed = true;
                                }
                            }
                        });
                });

                r
            }
            Self::HSV(val) => {
                let mut r = ui.add(DragValue::new(&mut val.0).clamp_range(0..=360));
                if ui
                    .add(DragValue::new(&mut val.1).clamp_range(0..=200))
                    .changed()
                {
                    r.changed = true;
                }
                if ui
                    .add(DragValue::new(&mut val.2).clamp_range(0..=200))
                    .changed()
                {
                    r.changed = true;
                }
                r
            }
            Self::Blur(val) => ui.slider_styled(val, 0..=20),
            Self::Noise { amt, mono } => {
                let mut r = ui.slider_styled(amt, 0..=100);
                if ui.checkbox(mono, "Grey").changed() {
                    r.changed = true
                }
                r
            }
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
            Self::Desaturate(val) => ui.slider_styled(val, 0..=100),
            Self::Contrast(val) => ui.slider_styled(val, -128..=128),
            Self::Crop(bounds) => {
                let available_w_single_spacing =
                    ui.available_width() - 60. - ui.style().spacing.item_spacing.x * 3.;
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
            Self::Equalize(bounds) => {
                let available_w_single_spacing =
                    ui.available_width() - ui.style().spacing.item_spacing.x * 1.;
                ui.horizontal(|ui| {
                    let mut r1 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut bounds.0)
                            // .speed(2.)
                            .clamp_range(-128..=128)
                            .prefix("dark "),
                    );
                    let r2 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut bounds.1)
                            .speed(2.)
                            .clamp_range(64..=2000)
                            .prefix("bright "),
                    );

                    // TODO rewrite with any
                    if r2.changed() {
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
                let mut color: [f32; 4] = [
                    val[0] as f32 / 255.,
                    val[1] as f32 / 255.,
                    val[2] as f32 / 255.,
                    val[3] as f32 / 255.,
                ];

                let r = ui.color_edit_button_rgba_premultiplied(&mut color);
                if r.changed() {
                    val[0] = (color[0] * 255.) as u8;
                    val[1] = (color[1] * 255.) as u8;
                    val[2] = (color[2] * 255.) as u8;
                    val[3] = (color[3] * 255.) as u8;
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
            Self::Resize {
                dimensions,
                aspect,
                filter,
            } => {
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

                    let r2 = ui.checkbox(aspect, "🔗").on_hover_text("Lock aspect ratio");

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

                    egui::ComboBox::from_id_source("filter")
                        .selected_text(format!("{:?}", filter))
                        .show_ui(ui, |ui| {
                            for f in [
                                ScaleFilter::Triangle,
                                ScaleFilter::Gaussian,
                                ScaleFilter::CatmullRom,
                                ScaleFilter::Nearest,
                                ScaleFilter::Lanzcos3,
                            ] {
                                if ui.selectable_value(filter, f, format!("{:?}", f)).clicked() {
                                    r0.changed = true;
                                }
                            }
                        });

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
            Self::Resize {
                dimensions, filter, ..
            } => {
                if *dimensions != Default::default() {
                    let filter = match filter {
                        ScaleFilter::Lanzcos3 => Lanczos3,
                        ScaleFilter::Gaussian => Gaussian,
                        ScaleFilter::Nearest => Nearest,
                        ScaleFilter::Triangle => Triangle,
                        ScaleFilter::CatmullRom => CatmullRom,
                    };

                    *img = image::imageops::resize(img, dimensions.0, dimensions.1, filter);
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
            Self::ChromaticAberration(amt) => {
                let center = (img.width() as i32 / 2, img.height() as i32 / 2);
                let img_c = img.clone();

                for (x, y, p) in img.enumerate_pixels_mut() {
                    let dist_to_center = (x as i32 - center.0, y as i32 - center.1);
                    let dist_to_center = (
                        (dist_to_center.0 as f32 / center.0 as f32) * *amt as f32 / 10.,
                        (dist_to_center.1 as f32 / center.1 as f32) * *amt as f32 / 10.,
                    );
                    // info!("{:?}", dist_to_center);
                    // info!("D {}", dist_to_center);
                    if let Some(l) = img_c.get_pixel_checked(
                        (x as i32 + dist_to_center.0 as i32).max(0) as u32,
                        (y as i32 + dist_to_center.1 as i32).max(0) as u32,
                    ) {
                        p[0] = l[0];
                    }
                }
            }

            _ => (),
        }
    }

    /// Process a single pixel.
    pub fn process_pixel(&self, p: &mut Vector4<f32>) {
        match self {
            Self::Brightness(amt) => {
                let amt = *amt as f32 / 255.;
                *p = *p + Vector4::new(amt, amt, amt, 0.0);
            }
            Self::Exposure(amt) => {
                let amt = (*amt as f32 / 100.) * 4.;

                // *p = *p * Vector4::new(2., 2., 2., 2.).;

                p[0] = p[0] * (2 as f32).powf(amt);
                p[1] = p[1] * (2 as f32).powf(amt);
                p[2] = p[2] * (2 as f32).powf(amt);
            }
            Self::Equalize(bounds) => {
                let bounds = (bounds.0 as f32 / 255., bounds.1 as f32 / 255.);
                // *p = lerp_col(Vector4::splat(bounds.0), Vector4::splat(bounds.1), *p);
                // 0, 0.2, 1.0

                p[0] = egui::lerp(bounds.0..=bounds.1, p[0] as f32);
                p[1] = egui::lerp(bounds.0..=bounds.1, p[1] as f32);
                p[2] = egui::lerp(bounds.0..=bounds.1, p[2] as f32);
            }
            Self::Expression(expr) => {
                let mut context = context_map! {
                    "r" => p[0] as f64,
                    "g" => p[1] as f64,
                    "b" => p[2] as f64,
                    "a" => p[3] as f64,
                }
                .unwrap(); // Do proper error handling here

                if let Ok(_) = eval_empty_with_context_mut(expr, &mut context) {
                    if let Some(r) = context.get_value("r") {
                        if let Ok(r) = r.as_float() {
                            p[0] = r as f32
                        }
                    }
                    if let Some(g) = context.get_value("g") {
                        if let Ok(g) = g.as_float() {
                            p[1] = g as f32
                        }
                    }
                    if let Some(b) = context.get_value("b") {
                        if let Ok(b) = b.as_float() {
                            p[2] = b as f32
                        }
                    }
                    if let Some(a) = context.get_value("a") {
                        if let Ok(a) = a.as_float() {
                            p[3] = a as f32
                        }
                    }
                }
            }
            Self::Posterize(levels) => {
                p[0] = (p[0] * *levels as f32).round() / *levels as f32;
                p[1] = (p[1] * *levels as f32).round() / *levels as f32;
                p[2] = (p[2] * *levels as f32).round() / *levels as f32;
                // 0.65 * 10.0 = 6.5 / 10
            }
            Self::Noise { amt, mono } => {
                let amt = *amt as f32 / 100.;

                let mut rng = thread_rng();
                let n_r: f32 = rng.gen();
                let n_g: f32 = if *mono { n_r } else { rng.gen() };
                let n_b: f32 = if *mono { n_r } else { rng.gen() };

                p[0] = egui::lerp(p[0]..=n_r, amt);
                p[1] = egui::lerp(p[1]..=n_g, amt);
                p[2] = egui::lerp(p[2]..=n_b, amt);
            }
            Self::Fill(col) => {
                let target =
                    Vector4::new(col[0] as f32, col[1] as f32, col[2] as f32, col[3] as f32) / 255.;
                *p = p.lerp(&target, target[3]);
            }
            Self::Desaturate(amt) => {
                desaturate(p, *amt as f32 / 100.);
            }
            Self::ChannelSwap(channels) => {
                p[channels.0 as usize] = p[channels.1 as usize];
            }
            Self::Mult(amt) => {
                let amt =
                    Vector4::new(amt[0] as f32, amt[1] as f32, amt[2] as f32, 255. as f32) / 255.;

                // p[0] = p[0] * amt[0] as f32 / 255.;
                // p[1] = p[1] * amt[1] as f32 / 255.;
                // p[2] = p[2] * amt[2] as f32 / 255.;
                *p = p.component_mul(&amt);
            }
            Self::Add(amt) => {
                let amt = Vector4::new(amt[0] as f32, amt[1] as f32, amt[2] as f32, 0.0) / 255.;
                // p[0] = p[0] + amt[0] as f32 / 255.;
                // p[1] = p[1] + amt[1] as f32 / 255.;
                // p[2] = p[2] + amt[2] as f32 / 255.;
                *p += amt;
            }
            Self::HSV(amt) => {
                use palette::{rgb::Rgb, Hsl, IntoColor};
                let rgb: Rgb = *Rgb::from_raw(p.as_slice());

                let mut hsv: Hsl = rgb.into_color();
                hsv.hue += amt.0 as f32;
                hsv.saturation *= amt.1 as f32 / 100.;
                hsv.lightness *= amt.2 as f32 / 100.;
                let rgb: Rgb = hsv.into_color();

                // *p = image::Rgba([rgb.red, rgb.green, rgb.blue, p[3]]);
                p[0] = rgb.red;
                p[1] = rgb.green;
                p[2] = rgb.blue;
            }
            Self::Invert => {
                p[0] = 1. - p[0];
                p[1] = 1. - p[1];
                p[2] = 1. - p[2];
            }
            Self::Contrast(val) => {
                let factor: f32 = (1.015686275 * (*val as f32 / 255. + 1.0))
                    / (1.0 * (1.015686275 - *val as f32 / 255.)) as f32;
                p[0] = (factor * p[0] - 0.5) + 0.5;
                p[1] = (factor * p[1] - 0.5) + 0.5;
                p[2] = (factor * p[2] - 0.5) + 0.5;
            }
            _ => (),
        }
    }
}

pub fn desaturate(p: &mut Vector4<f32>, factor: f32) {
    // G*.59+R*.3+B*.11
    let val = p[0] * 0.59 + p[1] * 0.3 + p[2] * 0.11;
    p[0] = egui::lerp(p[0] as f32..=val, factor);
    p[1] = egui::lerp(p[1] as f32..=val, factor);
    p[2] = egui::lerp(p[2] as f32..=val, factor);
}

pub fn process_pixels(buffer: &mut RgbaImage, operators: &Vec<ImageOperation>) {
    // use pulp::Arch;
    // let arch = Arch::new();

    // arch.dispatch(|| {
    //         for x in &mut buffer.into_vec() {
    //             *x = 12 as u8;
    //         }
    //     });

    buffer
        // .chunks_mut(4)
        .par_chunks_mut(4)
        .for_each(|px| {
            // let mut float_pixel = image::Rgba([
            //     px[0] as f32 / 255.,
            //     px[1] as f32 / 255.,
            //     px[2] as f32 / 255.,
            //     px[3] as f32 / 255.,
            // ]);

            let mut float_pixel =
                Vector4::new(px[0] as f32, px[1] as f32, px[2] as f32, px[3] as f32) / 255.;

            // run pixel operations
            for operation in operators {
                operation.process_pixel(&mut float_pixel);
            }

            float_pixel *= 255.;

            px[0] = (float_pixel[0]) as u8;
            px[1] = (float_pixel[1]) as u8;
            px[2] = (float_pixel[2]) as u8;
            px[3] = (float_pixel[3]) as u8;
        });
}
