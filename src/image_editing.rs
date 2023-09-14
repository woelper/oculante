use std::fmt;
use std::num::NonZeroU32;

use crate::paint::PaintStroke;
use crate::ui::EguiExt;

use anyhow::Result;
use evalexpr::*;
use fast_image_resize as fr;
use image::{imageops, RgbaImage};
use log::{debug, error};
use nalgebra::Vector4;
use notan::egui::{self, lerp, DragValue, Sense, Vec2};
use notan::egui::{Response, Ui};
use palette::{rgb::Rgb, Hsl, IntoColor};
use rand::{thread_rng, Rng};
use rayon::{iter::ParallelIterator, slice::ParallelSliceMut};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EditState {
    #[serde(skip)]
    /// The final result of image modifications
    pub result_pixel_op: RgbaImage,
    #[serde(skip)]
    /// The image after all non-per-pixel operations completed (expensive, so only updated if changed)
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
    Box,
    Bilinear,
    Hamming,
    CatmullRom,
    Mitchell,
    Lanczos3,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub enum ImageOperation {
    Brightness(i32),
    Expression(String),
    Desaturate(u8),
    Posterize(u8),
    GradientMap(Vec<GradientStop>),
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
    Rotate(i16),
    HSV((u16, i32, i32)),
    ChromaticAberration(u8),
    ChannelSwap((Channel, Channel)),
    Invert,
    Blur(u8),
    MMult,
    MDiv,
    Resize {
        dimensions: (u32, u32),
        aspect: bool,
        filter: ScaleFilter,
    },
    /// Left, right, top, bottom
    // x,y (top left corner of crop), width, height
    // 1.0 equals 10000
    Crop([u32; 4]),
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
            Self::GradientMap { .. } => write!(f, "🗠 Gradient Map"),
            Self::Expression(_) => write!(f, "📄 Expression"),
            Self::MMult => write!(f, "✖ Multiply with alpha"),
            Self::MDiv => write!(f, "➗ Divide by alpha"),
            // _ => write!(f, "Not implemented Display"),
        }
    }
}

impl ImageOperation {
    pub fn is_per_pixel(&self) -> bool {
        match self {
            Self::Blur(_) => false,
            Self::Resize { .. } => false,
            // Self::GradientMap { .. } => false,
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
                                    .selectable_value(&mut val.0, f, format!("{f:?}"))
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
                                    .selectable_value(&mut val.1, f, format!("{f:?}"))
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

            Self::GradientMap(pts) => {
                ui.vertical(|ui| {
                    use egui::epaint::*;

                    const RECT_WIDTH: usize = 255;

                    let (gradient_rect, mut response) =
                        ui.allocate_at_least(vec2(RECT_WIDTH as f32, 50.), Sense::click_and_drag());

                    let mut len_with_extra_pts = pts.len();
                    let len = pts.len();
                    if len < 2 {
                        error!("You need at least two points in your gradient");
                    }
                    let mut mesh = Mesh::default();

                    let mut i: usize = 0;

                    // paint gradient
                    for &color in pts.iter() {
                        let t = color.pos as f32 / u8::MAX as f32;
                        let x = lerp(gradient_rect.x_range(), t);
                        let egui_color =
                            // Color32::from_rgb(color.1[0], color.1[1], color.1[2]).additive();
                            Color32::from_rgb(color.r(), color.g(), color.b());

                        // if first point is shifted, so we clamp and insert first
                        if i == 0 && color.pos > 0 {
                            let x = gradient_rect.left();
                            mesh.colored_vertex(pos2(x, gradient_rect.top()), egui_color);
                            mesh.colored_vertex(pos2(x, gradient_rect.bottom()), egui_color);
                            mesh.add_triangle(2 * i as u32, 2 * i as u32 + 1, 2 * i as u32 + 2);
                            mesh.add_triangle(2 * i as u32 + 1, 2 * i as u32 + 2, 2 * i as u32 + 3);
                            i += 1;
                            len_with_extra_pts += 1;
                        }

                        // draw regular point
                        mesh.colored_vertex(pos2(x, gradient_rect.top()), egui_color);
                        mesh.colored_vertex(pos2(x, gradient_rect.bottom()), egui_color);
                        if i < len_with_extra_pts - 1 {
                            let i = i as u32;
                            mesh.add_triangle(2 * i, 2 * i + 1, 2 * i + 2);
                            mesh.add_triangle(2 * i + 1, 2 * i + 2, 2 * i + 3);
                        }

                        // if last point is shifted, insert extra one at end
                        if i == len_with_extra_pts - 1 && color.pos < RECT_WIDTH as u8 {
                            let x = gradient_rect.right();
                            mesh.colored_vertex(pos2(x, gradient_rect.top()), egui_color);
                            mesh.colored_vertex(pos2(x, gradient_rect.bottom()), egui_color);
                            mesh.add_triangle(2 * i as u32, 2 * i as u32 + 1, 2 * i as u32 + 2);
                            mesh.add_triangle(2 * i as u32 + 1, 2 * i as u32 + 2, 2 * i as u32 + 3);
                            i += 1;
                            len_with_extra_pts += 1;
                        }
                        i += 1;
                    }

                    ui.painter().add(Shape::mesh(mesh));

                    let pts_cpy = pts.clone();

                    for (ptnum, gradient_stop) in pts.iter_mut().enumerate() {
                        let mut is_hovered = false;

                        if let Some(hover) = response.hover_pos() {
                            let mouse_pos_in_gradient =
                                (hover.x - gradient_rect.left()).clamp(0.0, 255.) as i32;

                            // check which point is closest

                            if closest_pt(&pts_cpy, mouse_pos_in_gradient as u8) as usize == ptnum {
                                is_hovered = true;

                                // on click, set the id
                                if ui
                                    .ctx()
                                    .input(|i|i.pointer.primary_down())
                                    
                                    && ui
                                        .ctx()
                                        .data(|d| d.get_temp::<usize>("gradient".into()).is_none())
                                        
                                {
                                    ui.ctx()
                                        .data_mut(|d| d
                                        .insert_temp::<usize>("gradient".into(), gradient_stop.id));
                                    debug!("insert");
                                }
                            }

                            // Button down: move point with matching id
                            if ui
                                .ctx()
                                .input(|i| i
                                .pointer
                                .primary_down())
                                && ui.ctx().data(|d| d.get_temp::<usize>("gradient".into()))
                                    == Some(gradient_stop.id)
                            {
                                gradient_stop.pos = mouse_pos_in_gradient as u8;
                                response.mark_changed();
                            }
                        }

                        if ui.ctx().input(|i| i.pointer.any_released()) {
                            ui.ctx().data_mut(|d| d.remove::<usize>("gradient".into()));
                            debug!("clear dta");
                        }

                        ui.painter().vline(
                            gradient_rect.left() + gradient_stop.pos as f32,
                            gradient_rect.bottom()..=(gradient_rect.top() + 25.),
                            Stroke::new(
                                if is_hovered { 4. } else { 1. },
                                Color32::from_rgb(
                                    255 - gradient_stop.r(),
                                    255 - gradient_stop.r(),
                                    255 - gradient_stop.r(),
                                ),
                            ),
                        );
                    }

                    let mut delete = None;
                    for (i, p) in pts.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.color_edit_button_srgb(&mut p.col).changed() {
                                response.mark_changed();
                            }

                            if ui
                                .add(
                                    egui::DragValue::new(&mut p.pos)
                                        .speed(0.1)
                                        .clamp_range(0..=255)
                                        .custom_formatter(|n, _| {
                                            let n = n / 256.;
                                            format!("{n:.2}")
                                        })
                                        .suffix(" pos"),
                                )
                                .changed()
                            {
                                response.mark_changed();
                            }

                            // make sure we have at least two points
                            if len > 2 {
                                if ui.button("🗑").clicked() {
                                    delete = Some(i);
                                }
                            }
                        });
                    }

                    if ui.button("Add point").clicked() {
                        pts.push(GradientStop::new(128, [0, 0, 0]));
                        response.mark_changed();
                    }
                    if let Some(del) = delete {
                        pts.remove(del);
                        response.mark_changed();
                    }

                    // Make sure points are monotonic ascending by position

                    pts.sort_by(|a, b| a.pos.cmp(&b.pos));

                    response
                })
                .inner
            }
            Self::Flip(horizontal) => {
                let mut r = ui.radio_value(horizontal, true, "V");
                if ui.radio_value(horizontal, false, "H").changed() {
                    r.changed = true
                }
                r
            }
            Self::Rotate(angle) => {
                let mut r = ui.selectable_value(angle, 90, "➡ 90°");

                if r.clicked() {
                    r.mark_changed();
                }

                if ui.selectable_value(angle, 270, "⬅ -90°").clicked() {
                    r.mark_changed();
                }
                if ui.selectable_value(angle, 180, "⬇ 180°").clicked() {
                    r.mark_changed();
                }

                r
            }
            Self::Desaturate(val) => ui.slider_styled(val, 0..=100),
            Self::Contrast(val) => ui.slider_styled(val, -128..=128),
            Self::Crop(bounds) => {
                let mut float_bounds = bounds.map(|b| b as f32 / 10000.);
                // debug!("Float bounds {:?}", float_bounds);

                let available_w_single_spacing =
                    ui.available_width() - 60. - ui.style().spacing.item_spacing.x * 3.;
                ui.horizontal(|ui| {
                    let mut r1 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut float_bounds[0])
                            .speed(0.004)
                            .clamp_range(0.0..=1.0)
                            // X
                            .prefix("⏵ "),
                    );
                    let r2 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut float_bounds[2])
                            .speed(0.004)
                            .clamp_range(0.0..=1.0)
                            // WIDTH
                            .prefix("⏴ "),
                    );
                    let r3 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut float_bounds[1])
                            .speed(0.004)
                            .clamp_range(0.0..=1.0)
                            // Y
                            .prefix("⏷ "),
                    );
                    let r4 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut float_bounds[3])
                            .speed(0.004)
                            .clamp_range(0.0..=1.0)
                            // HEIGHT
                            .prefix("⏶ "),
                    );
                    // TODO rewrite with any
                    if r2.changed() || r3.changed() || r4.changed() {
                        r1.changed = true;
                    }
                    if r1.changed() {
                        // commit back changed vals
                        *bounds = float_bounds.map(|b| (b * 10000.) as u32);
                        debug!("changed bounds {:?}", bounds);
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

                    if r0.changed() && *aspect {
                        dimensions.1 = (dimensions.0 as f32 * ratio) as u32
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
                    r0.changed = r0.drag_released() || r1.drag_released() || r2.changed();

                    egui::ComboBox::from_id_source("filter")
                        .selected_text(format!("{filter:?}"))
                        .show_ui(ui, |ui| {
                            for f in [
                                ScaleFilter::Box,
                                ScaleFilter::Bilinear,
                                ScaleFilter::Hamming,
                                ScaleFilter::CatmullRom,
                                ScaleFilter::Mitchell,
                                ScaleFilter::Lanczos3,
                            ] {
                                if ui.selectable_value(filter, f, format!("{f:?}")).clicked() {
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
    pub fn process_image(&self, img: &mut RgbaImage) -> Result<()> {
        match self {
            Self::Blur(amt) => {
                if *amt != 0 {
                    *img = imageops::blur(img, *amt as f32);
                }
            }
            Self::Crop(dim) => {
                if *dim != [0, 0, 0, 0] {
                    let window = cropped_range(dim, &(img.width(), img.height()));
                    let sub_img =
                        image::imageops::crop_imm(img, window[0], window[1], window[2], window[3]);
                    *img = sub_img.to_image();
                }
            }
            Self::Resize {
                dimensions, filter, ..
            } => {
                if *dimensions != Default::default() {
                    let filter = match filter {
                        ScaleFilter::Box => fr::FilterType::Box,
                        ScaleFilter::Bilinear => fr::FilterType::Bilinear,
                        ScaleFilter::Hamming => fr::FilterType::Hamming,
                        ScaleFilter::CatmullRom => fr::FilterType::CatmullRom,
                        ScaleFilter::Mitchell => fr::FilterType::Mitchell,
                        ScaleFilter::Lanczos3 => fr::FilterType::Lanczos3,
                    };

                    let width = NonZeroU32::new(img.width()).unwrap_or(anyhow::Context::context(
                        NonZeroU32::new(1),
                        "Can't create nonzero",
                    )?);
                    let height = NonZeroU32::new(img.height()).unwrap_or(anyhow::Context::context(
                        NonZeroU32::new(1),
                        "Can't create nonzero",
                    )?);
                    let mut src_image = fr::Image::from_vec_u8(
                        width,
                        height,
                        img.clone().into_raw(),
                        fr::PixelType::U8x4,
                    )?;

                    let mapper = fr::create_gamma_22_mapper();
                    mapper.forward_map_inplace(&mut src_image.view_mut())?;

                    // Create container for data of destination image
                    let dst_width = NonZeroU32::new(dimensions.0).unwrap_or(
                        anyhow::Context::context(NonZeroU32::new(1), "Can't create nonzero")?,
                    );
                    let dst_height = NonZeroU32::new(dimensions.1).unwrap_or(
                        anyhow::Context::context(NonZeroU32::new(1), "Can't create nonzero")?,
                    );
                    let mut dst_image =
                        fr::Image::new(dst_width, dst_height, src_image.pixel_type());

                    let mut resizer = fr::Resizer::new(fr::ResizeAlg::Convolution(filter));

                    resizer.resize(&src_image.view(), &mut dst_image.view_mut())?;

                    mapper.backward_map_inplace(&mut dst_image.view_mut())?;

                    *img = anyhow::Context::context(
                        image::RgbaImage::from_raw(
                            dimensions.0,
                            dimensions.1,
                            dst_image.into_vec(),
                        ),
                        "Can't create RgbaImage",
                    )?;
                }
            }
            Self::Rotate(angle) => {
                match angle {
                    90 => *img = image::imageops::rotate90(img),
                    -90 => *img = image::imageops::rotate270(img),
                    270 => *img = image::imageops::rotate270(img),
                    180 => *img = image::imageops::rotate180(img),
                    // 270 => *img = image::imageops::rotate270(img),
                    _ => (),
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
        Ok(())
    }

    /// Process a single pixel.
    pub fn process_pixel(&self, p: &mut Vector4<f32>) -> Result<()> {
        match self {
            Self::Brightness(amt) => {
                let amt = *amt as f32 / 255.;
                *p += Vector4::new(amt, amt, amt, 0.0);
            }
            Self::Exposure(amt) => {
                let amt = (*amt as f32 / 100.) * 4.;

                // *p = *p * Vector4::new(2., 2., 2., 2.).;

                p[0] = p[0] * (2_f32).powf(amt);
                p[1] = p[1] * (2_f32).powf(amt);
                p[2] = p[2] * (2_f32).powf(amt);
            }
            Self::Equalize(bounds) => {
                let bounds = (bounds.0 as f32 / 255., bounds.1 as f32 / 255.);
                // *p = lerp_col(Vector4::splat(bounds.0), Vector4::splat(bounds.1), *p);
                // 0, 0.2, 1.0

                p[0] = egui::lerp(bounds.0..=bounds.1, p[0]);
                p[1] = egui::lerp(bounds.0..=bounds.1, p[1]);
                p[2] = egui::lerp(bounds.0..=bounds.1, p[2]);
            }

            Self::GradientMap(col) => {
                let brightness = 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2];
                // let res = interpolate_spline(col, brightness);
                // let res = interpolate(col, brightness);
                let res = interpolate_u8(col, (brightness * 255.) as u8);
                p[0] = res[0] as f32 / 255.;
                p[1] = res[1] as f32 / 255.;
                p[2] = res[2] as f32 / 255.;
            }
            Self::Expression(expr) => {
                let mut context = context_map! {
                    "r" => p[0] as f64,
                    "g" => p[1] as f64,
                    "b" => p[2] as f64,
                    "a" => p[3] as f64,
                }?;

                if eval_empty_with_context_mut(expr, &mut context).is_ok() {
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
                let amt = Vector4::new(amt[0] as f32, amt[1] as f32, amt[2] as f32, 255_f32) / 255.;

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
                let rgb: Rgb = Rgb::from_components((p.x, p.y, p.z));

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
            Self::MMult => {
                p[0] *= p[3];
                p[1] *= p[3];
                p[2] *= p[3];
            }
            Self::MDiv => {
                p[0] /= p[3];
                p[1] /= p[3];
                p[2] /= p[3];
            }
            Self::Contrast(val) => {
                let factor: f32 = (1.015_686_3 * (*val as f32 / 255. + 1.0))
                    / (1.0 * (1.015_686_3 - *val as f32 / 255.));
                p[0] = (factor * p[0] - 0.5) + 0.5;
                p[1] = (factor * p[1] - 0.5) + 0.5;
                p[2] = (factor * p[2] - 0.5) + 0.5;
            }
            _ => (),
        }
        Ok(())
    }
}

pub fn desaturate(p: &mut Vector4<f32>, factor: f32) {
    // G*.59+R*.3+B*.11
    let val = p[0] * 0.59 + p[1] * 0.3 + p[2] * 0.11;
    p[0] = egui::lerp(p[0]..=val, factor);
    p[1] = egui::lerp(p[1]..=val, factor);
    p[2] = egui::lerp(p[2]..=val, factor);
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
                if let Err(e) = operation.process_pixel(&mut float_pixel) {
                    error!("{e}")
                }
            }

            float_pixel *= 255.;

            px[0] = (float_pixel[0]) as u8;
            px[1] = (float_pixel[1]) as u8;
            px[2] = (float_pixel[2]) as u8;
            px[3] = (float_pixel[3]) as u8;
        });
}

/// Crop a left,top (x,y) plus x/y window safely into absolute pixel units.
/// The crop is expected in UV coords, 0-1, encoded as 8 bit (0-255)
pub fn cropped_range(crop: &[u32; 4], img_dim: &(u32, u32)) -> [u32; 4] {
    let crop = crop.map(|c| c as f32 / 10000.);
    debug!("crop range fn: {:?}", crop);

    let crop = [
        crop[0].max(0.0),
        crop[1].max(0.0),
        (1.0 - crop[2] - crop[0]).max(0.0),
        (1.0 - crop[3] - crop[1]).max(0.0),
    ];

    debug!("crop range window: {:?}", crop);

    let crop = [
        (crop[0] * img_dim.0 as f32) as u32,
        (crop[1] * img_dim.1 as f32) as u32,
        (crop[2] * img_dim.0 as f32) as u32,
        (crop[3] * img_dim.1 as f32) as u32,
    ];

    debug!("crop range window abs: {:?} res: {:?}", crop, img_dim);

    crop
}

/// Transform a JPEG losslessly
#[cfg(feature = "turbo")]
pub fn lossless_tx(p: &std::path::Path, transform: turbojpeg::Transform) -> anyhow::Result<()> {
    let jpeg_data = std::fs::read(p)?;

    let mut decompressor = turbojpeg::Decompressor::new()?;

    // read the JPEG header
    let header = decompressor.read_header(&jpeg_data)?;
    let mcu_h = header.subsamp.mcu_height();
    let mcu_w = header.subsamp.mcu_width();

    debug!("h {mcu_h} w {mcu_w}");

    // make sure crop is aligned to mcu bounds
    let mut transform = transform;
    if let Some(c) = transform.crop.as_mut() {
        c.x = (c.x as f32 / mcu_w as f32) as usize * mcu_w;
        c.y = (c.y as f32 / mcu_h as f32) as usize * mcu_h;
        // the start point may have shifted, make sure we don't go over bounds
        // if let Some(crop_w) = c.width.as_mut() {
        //     *crop_w = *crop_w;
        // }
        // if let Some(crop_h) = c.height.as_mut() {
        //     // *crop_h = (*crop_h + c.y).min(header.height - c.y);
        // }
        debug!("jpg crop transform {:#?}", c);
    }

    // apply the transformation
    let transformed_data = turbojpeg::transform(&transform, &jpeg_data)?;

    // write the changed JPEG back to disk
    std::fs::write(p, &transformed_data)?;
    Ok(())
}

fn interpolate_u8(data: &Vec<GradientStop>, pt: u8) -> [u8; 3] {
    // debug!("Pt is {pt}");

    for i in 0..data.len() {
        let current = data[i];

        // return direct hit
        if current.pos == pt {
            return current.col;
        }

        // pt is below first stop
        if i == 0 && current.pos > pt {
            return current.col;
        }

        if let Some(next) = data.get(i + 1) {
            if current.pos < pt && next.pos > pt {
                let range = next.pos - current.pos;
                let pos_in_range = pt - current.pos;
                let rel = pos_in_range as f32 / range as f32;

                let r = lerp(current.r() as f32..=next.r() as f32, rel) as u8;
                let g = lerp(current.g() as f32..=next.g() as f32, rel) as u8;
                let b = lerp(current.b() as f32..=next.b() as f32, rel) as u8;

                return [r, g, b];
            }
        } else {
            return current.col;
            //this was the last point
        }
    }

    [0, 255, 0]
}

fn closest_pt(data: &Vec<GradientStop>, value: u8) -> usize {
    // go thru all points of gradient
    for (i, current) in data.iter().enumerate() {
        // make sure there is a next point
        if let Some(next) = data.get(i + 1) {
            // clamped left: special case
            if value <= current.pos && i == 0 {
                return 0;
            }

            //is this value between these?
            if current.pos <= value && next.pos >= value {
                let l_dist = (value as i32 - current.pos as i32).abs();
                let r_dist = (next.pos as i32 - value as i32).abs();
                if l_dist <= r_dist {
                    return i;
                } else {
                    return i + 1;
                }
            }
        } else {
            return i;
        }
    }
    0
    // res
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub struct GradientStop {
    pub id: usize,
    pub pos: u8,
    pub col: [u8; 3],
}

impl GradientStop {
    fn r(&self) -> u8 {
        self.col[0]
    }
    fn g(&self) -> u8 {
        self.col[1]
    }
    fn b(&self) -> u8 {
        self.col[2]
    }

    pub fn new(pos: u8, rgb: [u8; 3]) -> Self {
        GradientStop {
            id: rand::thread_rng().gen(),
            pos,
            col: rgb,
        }
    }
}

#[test]
fn range_test() {
    // for i in [0.0, 0.25,0.5, 0.75, 1.0] {
    //     let r = map_range(i, 0.0, 1.0,0.5, 1.0,);
    //     dbg!(r);
    //     let r1 = map_between_ranges(r, 0.5, 1.0,0., 1.0,);
    //     // let r = map_range(r, 0.5, 1.0,0.0, 1.0,);
    //     dbg!(r1);

    // }

    let map = vec![
        GradientStop::new(0, [155, 33, 180]),
        GradientStop::new(128, [255, 83, 0]),
        GradientStop::new(255, [224, 255, 0]),
    ];
    std::env::set_var("RUST_LOG", "debug");
    let _ = env_logger::try_init();
    let res = interpolate_u8(&map, 5);

    debug!("result: {:?}", res);
}
