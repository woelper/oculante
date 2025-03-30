use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use crate::icons::*;
use crate::paint::PaintStroke;
use crate::settings::VolatileSettings;
use crate::ui::EguiExt;
use crate::{appstate::ImageGeometry, utils::pos_from_coord};
#[cfg(not(feature = "file_open"))]
use crate::{filebrowser, utils::SUPPORTED_EXTENSIONS};
use anyhow::{bail, Result};
use evalexpr::*;
use fast_image_resize::{self as fr, ResizeOptions};
use image::{imageops, ColorType, DynamicImage, Rgba, RgbaImage};
use imageproc::geometric_transformations::Interpolation;
use log::{debug, error, info};
use nalgebra::{Vector2, Vector4};
use notan::egui::epaint::PathShape;
use notan::egui::{
    self, lerp, vec2, Align2, Color32, DragValue, FontId, Id, Pos2, Rect, Sense, Stroke,
    StrokeKind, Vec2,
};
use notan::egui::{Response, Ui};
use palette::{rgb::Rgb, Hsl, IntoColor};
use rand::{thread_rng, Rng};
use rayon::{iter::ParallelIterator, slice::ParallelSliceMut};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, IntoEnumIterator};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EditState {
    #[serde(skip)]
    /// The final result of image modifications
    pub result_pixel_op: DynamicImage,
    #[serde(skip)]
    /// The image after all non-per-pixel operations completed (expensive, so only updated if changed)
    pub result_image_op: DynamicImage,
    pub painting: bool,
    #[serde(skip)]
    pub block_panning: bool,
    pub non_destructive_painting: bool,
    pub paint_strokes: Vec<PaintStroke>,
    pub paint_fade: bool,
    #[serde(skip, default = "default_brushes")]
    pub brushes: Vec<RgbaImage>,
    pub pixel_op_stack: Vec<ImgOpItem>,
    pub image_op_stack: Vec<ImgOpItem>,
    pub export_extension: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LegacyEditState {
    pub painting: bool,
    pub non_destructive_painting: bool,
    pub paint_strokes: Vec<PaintStroke>,
    pub paint_fade: bool,
    pub pixel_op_stack: Vec<ImageOperation>,
    pub image_op_stack: Vec<ImageOperation>,
    pub export_extension: String,
}

impl LegacyEditState {
    pub fn upgrade(&self) -> EditState {
        let mut ne = EditState::default();
        ne.image_op_stack = self
            .image_op_stack
            .iter()
            .map(|op| ImgOpItem::new(op.clone()))
            .collect();
        ne.pixel_op_stack = self
            .pixel_op_stack
            .iter()
            .map(|op| ImgOpItem::new(op.clone()))
            .collect();
        ne
    }
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            result_pixel_op: Default::default(),
            result_image_op: Default::default(),
            painting: Default::default(),
            block_panning: false,
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
        image::load_from_memory(include_bytes!("../res/brushes/brush6.png"))
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

pub struct ImgOpItem {
    pub active: bool,
    pub operation: ImageOperation,
}

impl ImgOpItem {
    pub fn new(op: ImageOperation) -> Self {
        Self {
            active: true,
            operation: op,
        }
    }
}

impl fmt::Display for ImgOpItem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.operation)
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub enum ImageOperation {
    ColorConverter(ColorTypeExt),
    Brightness(i32),
    /// discard pixels around a threshold: position and range, bool for mode.
    Slice(u8, u8, bool),
    Expression(String),
    Desaturate(u8),
    Posterize(u8),
    Filter3x3([i32; 9]),
    GradientMap(Vec<GradientStop>),
    Exposure(i32),
    Equalize((i32, i32)),
    ScaleImageMinMax,
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
    CropPerspective {
        points: [(u32, u32); 4],
        original_size: (u32, u32),
    },
    Measure {
        shapes: Vec<MeasureShape>,
    },
    LUT(String),
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize)]
pub enum MeasureShape {
    Line {
        points: Vec<(u32, u32)>,
        color: [u8; 4],
        width: u8,
    },
    Rect {
        points: Vec<(u32, u32)>,
        color: [u8; 4],
        width: u8,
    },
}

impl MeasureShape {
    pub fn new_line(points: Vec<(u32, u32)>) -> Self {
        Self::Line {
            points,
            color: [255, 255, 255, 255],
            width: 1,
        }
    }
    pub fn new_rect(points: Vec<(u32, u32)>) -> Self {
        Self::Rect {
            points,
            color: [255, 255, 255, 255],
            width: 1,
        }
    }
}

impl fmt::Display for ImageOperation {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Self::ColorConverter(_) => write!(f, "Color Type"),
            Self::Brightness(_) => write!(f, "Brightness"),
            Self::Slice(..) => write!(f, "Slice"),
            Self::Noise { .. } => write!(f, "Noise"),
            Self::Desaturate(_) => write!(f, "Desaturate"),
            Self::Posterize(_) => write!(f, "Posterize"),
            Self::Contrast(_) => write!(f, "Contrast"),
            Self::Exposure(_) => write!(f, "Exposure"),
            Self::Equalize(_) => write!(f, "Equalize"),
            Self::Mult(_) => write!(f, "Mult color"),
            Self::Add(_) => write!(f, "Add color"),
            Self::Fill(_) => write!(f, "Fill color"),
            Self::Blur(_) => write!(f, "Blur"),
            Self::Crop(_) => write!(f, "Crop"),
            Self::CropPerspective { .. } => write!(f, "Perspective crop"),
            Self::Measure { .. } => write!(f, "Measure"),
            Self::Flip(_) => write!(f, "Flip"),
            Self::Rotate(_) => write!(f, "Rotate"),
            Self::Invert => write!(f, "Invert"),
            Self::ChannelSwap(_) => write!(f, "Channel Copy"),
            Self::HSV(_) => write!(f, "HSV"),
            Self::ChromaticAberration(_) => write!(f, "Color Fringe"),
            Self::Resize { .. } => write!(f, "Resize"),
            Self::GradientMap { .. } => write!(f, "Gradient Map"),
            Self::Expression(_) => write!(f, "Expression"),
            Self::MMult => write!(f, "Multiply with alpha"),
            Self::ScaleImageMinMax => write!(f, "Scale image min max"),
            Self::MDiv => write!(f, "Divide by alpha"),
            Self::LUT(_) => write!(f, "Apply Color LUT"),
            Self::Filter3x3(_) => write!(f, "3x3 Filter"),
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
            Self::CropPerspective { .. } => false,
            Self::Rotate(_) => false,
            Self::ColorConverter(_) => false,
            Self::Flip(_) => false,
            Self::ChromaticAberration(_) => false,
            Self::LUT(_) => false,
            Self::Filter3x3(_) => false,
            Self::ScaleImageMinMax => false,
            _ => true,
        }
    }

    // Add functionality about how to draw UI here
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        geo: &ImageGeometry,
        block_panning: &mut bool,
        settings: &mut VolatileSettings,
    ) -> Response {
        match self {
            Self::ColorConverter(ct) => {
                let mut x = ui.allocate_response(vec2(0.0, 0.0), Sense::click_and_drag());
                egui::ComboBox::from_id_source("color_types")
                    .selected_text(ct.to_string())
                    .show_ui(ui, |ui| {
                        for t in ColorTypeExt::iter() {
                            if ui.selectable_value(ct, t.clone(), t.to_string()).clicked() {
                                x.mark_changed();
                            }
                        }
                    });
                if x.changed() {
                    info!("set to {}", ct);
                }
                x
            }
            Self::Brightness(val) => ui.styled_slider(val, -255..=255),
            Self::Slice(position, range, hard) => {
                let mut x = ui.allocate_response(vec2(0.0, 0.0), Sense::click_and_drag());
                ui.label("Position");
                if ui.styled_slider(position, 0..=255).changed() {
                    x.mark_changed();
                }
                ui.label("Range");
                if ui.styled_slider(range, 0..=255).changed() {
                    x.mark_changed();
                }
                if ui.styled_checkbox(hard, "Smooth").changed() {
                    x.mark_changed();
                }
                x
            }
            Self::Exposure(val) => ui.styled_slider(val, -100..=100),
            Self::ChromaticAberration(val) => ui.styled_slider(val, 0..=255),
            Self::Filter3x3(val) => {
                let mut x = ui.allocate_response(vec2(0.0, 0.0), Sense::click_and_drag());

                let presets = [
                    ("Sharpen", [0, -100, 0, -100, 500, -100, 0, -100, 0]),
                    ("Blur", [6, 12, 6, 12, 25, 12, 6, 12, 6]),
                    ("Emboss", [-200, -100, 0, -100, 100, 100, 0, 100, 200]),
                ];

                ui.vertical(|ui| {
                    egui::ComboBox::from_label("Presets")
                        .selected_text(
                            presets
                                .iter()
                                .filter(|p| p.1 == *val)
                                .map(|p| p.0)
                                .nth(0)
                                .unwrap_or("Select"),
                        )
                        .show_ui(ui, |ui| {
                            for p in presets {
                                if ui.selectable_value(val, p.1, p.0).clicked() {
                                    x.mark_changed();
                                }
                            }
                        });

                    for triplet in val.chunks_mut(3) {
                        ui.horizontal(|ui| {
                            for v in triplet {
                                if ui
                                    .add(egui::DragValue::new(v).clamp_range(-255..=255))
                                    .changed()
                                {
                                    x.mark_changed();
                                }
                                ui.add_space(30.);
                            }
                        });
                    }
                });
                x
            }
            Self::Posterize(val) => ui.styled_slider(val, 1..=255),
            Self::Expression(expr) => ui.text_edit_singleline(expr),
            Self::LUT(lut_name) => {
                ui.scope(|ui| {
                    let mut x = ui.allocate_response(vec2(0.0, 0.0), Sense::click_and_drag());
                    ui.vertical(|ui| {
                        let lut_fname = Path::new(lut_name)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy();

                        egui::ComboBox::from_label("")
                            .selected_text(lut_fname)
                            .show_ui(ui, |ui| {
                                let lut_names = builtin_luts();
                                let mut lut_names = lut_names.keys().collect::<Vec<_>>();
                                lut_names.sort();
                                for lut in lut_names {
                                    if ui
                                        .selectable_value(
                                            lut_name,
                                            lut.clone(),
                                            Path::new(lut)
                                                .file_name()
                                                .unwrap_or_default()
                                                .to_string_lossy(),
                                        )
                                        .clicked()
                                    {
                                        x.mark_changed();
                                    }
                                }
                            });

                        #[cfg(not(feature = "file_open"))]
                        {
                            if ui.button("Load lut").clicked() {
                                ui.ctx().memory_mut(|w| w.open_popup(Id::new("LUT")));
                            }

                            if ui.ctx().memory(|w| w.is_popup_open(Id::new("LUT"))) {
                                filebrowser::browse_modal(
                                    false,
                                    SUPPORTED_EXTENSIONS,
                                    settings,
                                    |p| {
                                        *lut_name = p.to_string_lossy().to_string();
                                        x.mark_changed();
                                    },
                                    ui.ctx(),
                                );
                            }
                        }

                        #[cfg(feature = "file_open")]
                        {
                            // let last_folder: &mut PathBuf = ui.ctx().data_mut(|w| w.get_temp_mut_or_default::<PathBuf>(Id::new("lutsrc")));
                            let last_folder: Option<std::path::PathBuf> = ui.ctx().data_mut(|w| {
                                w.get_persisted::<std::path::PathBuf>(Id::new("lutsrc"))
                            });
                            if ui
                                .button("Load from disk")
                                .on_hover_ui(|ui| {
                                    ui.label("Load Hald CLUT");
                                })
                                .clicked()
                            {
                                if let Some(lut_file) = rfd::FileDialog::new()
                                    .set_directory(last_folder.unwrap_or_default())
                                    .pick_file()
                                {
                                    *lut_name = lut_file.to_string_lossy().to_string();
                                    let parent = lut_file
                                        .parent()
                                        .map(|p| p.to_path_buf())
                                        .unwrap_or_default();
                                    ui.ctx().data_mut(|w| {
                                        w.insert_persisted(Id::new("lutsrc"), parent)
                                    });
                                }
                                x.mark_changed();
                            }
                        }

                        ui.label("Find more LUTs here:");

                        if ui
                            .link("Cédric Eberhardt's collection")
                            .on_hover_text("You can find more interesting LUTs here to use")
                            .clicked()
                        {
                            _ = webbrowser::open("https://github.com/cedeber/hald-clut");
                        }
                    });
                    x
                })
                .inner
            }
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
                                    r.mark_changed();
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
                                    r.mark_changed();
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
                    r.mark_changed();
                }
                if ui
                    .add(DragValue::new(&mut val.2).clamp_range(0..=200))
                    .changed()
                {
                    r.mark_changed();
                }
                r
            }
            Self::Blur(val) => ui.styled_slider(val, 0..=254),
            Self::Noise { amt, mono } => {
                let mut r = ui.styled_slider(amt, 0..=100);
                if ui.styled_checkbox(mono, "Grey").changed() {
                    r.mark_changed();
                }
                r
            }

            Self::GradientMap(pts) => {
                ui.vertical(|ui| {
                    use egui::epaint::*;

                    let rect_width = 255.;

                    let (gradient_rect, mut response) =
                        ui.allocate_at_least(vec2(rect_width, 50.), Sense::click_and_drag());

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
                        if i == len_with_extra_pts - 1 && color.pos < rect_width as u8 {
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
                                (hover.x - gradient_rect.left()).clamp(0.0, rect_width) as i32;

                            // check which point is closest

                            if closest_pt(&pts_cpy, mouse_pos_in_gradient as u8) as usize == ptnum {
                                is_hovered = true;

                                // on click, set the id
                                if ui.ctx().input(|i| i.pointer.primary_down())
                                    && ui
                                        .ctx()
                                        .data(|d| d.get_temp::<usize>("gradient".into()).is_none())
                                {
                                    ui.ctx().data_mut(|d| {
                                        d.insert_temp::<usize>("gradient".into(), gradient_stop.id)
                                    });
                                    debug!("insert");
                                }
                            }

                            // Button down: move point with matching id
                            if ui.ctx().input(|i| i.pointer.primary_down())
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
                    r.mark_changed();
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
            Self::Desaturate(val) => ui.styled_slider(val, 0..=100),
            Self::Contrast(val) => ui.styled_slider(val, -128..=128),
            Self::CropPerspective {
                points,
                original_size,
            } => {
                let id = Id::new("crop");
                let points_transformed = points
                    .iter()
                    .map(|p| {
                        (
                            geo.scale * p.0 as f32 + geo.offset.x,
                            geo.scale * p.1 as f32 + geo.offset.y,
                        )
                    })
                    .collect::<Vec<_>>();
                // create a fake response to alter
                let mut r = ui.allocate_response(Vec2::ZERO, Sense::click_and_drag());

                if ui.data(|r| r.get_temp::<bool>(id)).is_some() {
                    if ui.button(format!("{ARROW_U_UP_LEFT} Reset")).clicked() {
                        ui.data_mut(|w| w.remove_temp::<bool>(id));
                        r.mark_changed();
                        *points = [
                            (0, 0),
                            (original_size.0, 0),
                            (0, original_size.1),
                            (original_size.0, original_size.1),
                        ];
                    }
                } else {
                    let cursor_abs = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();

                    let cursor_relative = pos_from_coord(
                        geo.offset,
                        Vector2::new(cursor_abs.x, cursor_abs.y),
                        Vector2::new(geo.dimensions.0 as f32, geo.dimensions.1 as f32),
                        geo.scale,
                    );

                    for (i, pt) in points_transformed.iter().enumerate() {
                        let maxdist = 20.;
                        let d = Pos2::new(pt.0, pt.1).distance(cursor_abs);

                        if d < maxdist {
                            if ui.input(|i| i.pointer.any_down()) {
                                *block_panning = true;
                                ui.ctx().data_mut(|w| w.insert_temp("pt".into(), i));
                            }
                            if ui.input(|r| r.pointer.any_released()) {
                                *block_panning = false;
                                ui.ctx().data_mut(|w| w.remove_temp::<usize>("pt".into()));
                            }
                        }

                        let col = if d < maxdist {
                            Color32::LIGHT_BLUE
                        } else {
                            Color32::GOLD
                        };

                        // ui.painter().debug_text(
                        //     Pos2::new(pt.0, pt.1),
                        //     Align2::CENTER_CENTER,
                        //     col,
                        //     format!("X"),
                        // );

                        ui.painter().rect_filled(
                            Rect::from_center_size(Pos2::new(pt.0, pt.1), Vec2::splat(15.)),
                            2.,
                            col,
                        );
                    }

                    // egui shape needs these in a different order
                    let pts = vec![
                        Pos2::new(points_transformed[0].0, points_transformed[0].1),
                        Pos2::new(points_transformed[1].0, points_transformed[1].1),
                        Pos2::new(points_transformed[3].0, points_transformed[3].1),
                        Pos2::new(points_transformed[2].0, points_transformed[2].1),
                    ];

                    // make a black background covering everything
                    ui.painter().rect_filled(
                        Rect::EVERYTHING,
                        0.,
                        Color32::from_rgba_premultiplied(0, 0, 0, 70),
                    );

                    let shape = PathShape::convex_polygon(
                        pts,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 10),
                        Stroke::new(1., Color32::GOLD),
                    );
                    ui.painter().add(shape);

                    if let Some(pt) = ui.ctx().data(|r| r.get_temp::<usize>("pt".into())) {
                        points[pt].0 = cursor_relative.x as u32;
                        points[pt].1 = cursor_relative.y as u32;
                    }

                    if ui
                        .button(format!("{CHECK} Apply"))
                        .on_hover_text("Apply the crop. You don't lose image data by this.")
                        .clicked()
                    {
                        ui.ctx().data_mut(|w| w.insert_temp(id, true));
                        r.mark_changed();
                    }
                }

                r
            }
            Self::Measure { shapes } => {
                // create a fake response to alter
                let r = ui.allocate_response(Vec2::ZERO, Sense::click_and_drag());
                // enable this if this is used to draw
                // let id = Id::new("shapes");

                // let cursor_abs = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();

                // let cursor_relative = pos_from_coord(
                //     geo.offset,
                //     Vector2::new(cursor_abs.x, cursor_abs.y),
                //     Vector2::new(geo.dimensions.0 as f32, geo.dimensions.1 as f32),
                //     geo.scale,
                // );

                // draw shapes
                for shape in shapes {
                    match shape {
                        MeasureShape::Line {
                            points,
                            color,
                            width,
                        } => {
                            let points_transformed = points
                                .iter()
                                .map(|p| {
                                    (
                                        geo.scale * p.0 as f32 + geo.offset.x,
                                        geo.scale * p.1 as f32 + geo.offset.y,
                                    )
                                })
                                .collect::<Vec<_>>();
                            for p in points_transformed.chunks(2) {
                                ui.painter().line_segment(
                                    [
                                        Pos2::new(p[0].0 as f32, p[0].1 as f32),
                                        Pos2::new(p[1].0 as f32, p[1].1 as f32),
                                    ],
                                    Stroke::new(
                                        *width as f32,
                                        Color32::from_rgb(color[0], color[1], color[2]),
                                    ),
                                );
                            }
                        }
                        MeasureShape::Rect {
                            points,
                            color,
                            width,
                        } => {
                            let points_transformed = points
                                .iter()
                                .map(|p| {
                                    (
                                        geo.scale * p.0 as f32 + geo.offset.x,
                                        geo.scale * p.1 as f32 + geo.offset.y,
                                    )
                                })
                                .collect::<Vec<_>>();

                            let rect = Rect {
                                min: Pos2::new(points_transformed[0].0, points_transformed[0].1),
                                max: Pos2::new(points_transformed[1].0, points_transformed[1].1),
                            };

                            let rect_orig = Rect {
                                min: Pos2::new(points[0].0 as f32, points[0].1 as f32),
                                max: Pos2::new(points[1].0 as f32, points[1].1 as f32),
                            };

                            ui.painter().rect_stroke(
                                rect,
                                0.0,
                                Stroke::new(*width as f32, Color32::BLACK),
                                StrokeKind::Inside,
                            );
                            ui.painter().rect_stroke(
                                rect,
                                0.0,
                                Stroke::new(*width as f32 / 2., Color32::WHITE),
                                StrokeKind::Inside,
                            );

                            ui.painter().text(
                                rect.expand(14.).center_bottom(),
                                Align2::CENTER_CENTER,
                                format!(
                                    "{}x{}",
                                    rect_orig.width() as i32,
                                    rect_orig.height() as i32
                                ),
                                FontId::proportional(14.),
                                Color32::from_rgb(color[0], color[1], color[2]),
                            );

                            ui.painter().line_segment(
                                [rect.left_center(), rect.right_center()],
                                Stroke::new(1., Color32::from_rgb_additive(60, 60, 60)),
                            );

                            ui.painter().line_segment(
                                [rect.center_top(), rect.center_bottom()],
                                Stroke::new(1., Color32::from_rgb_additive(60, 60, 60)),
                            );
                        }
                    }
                }

                // for (i, pt) in points_transformed.iter().enumerate() {
                //     let maxdist = 20.;
                //     let d = Pos2::new(pt.0, pt.1).distance(cursor_abs);

                //     if d < maxdist {
                //         if ui.input(|i| i.pointer.any_down()) {
                //             *block_panning = true;
                //             ui.ctx().data_mut(|w| w.insert_temp("pt".into(), i));
                //         }
                //         if ui.input(|r| r.pointer.any_released()) {
                //             *block_panning = false;
                //             ui.ctx().data_mut(|w| w.remove_temp::<usize>("pt".into()));
                //         }
                //     }

                //     let col = if d < maxdist {
                //         Color32::LIGHT_BLUE
                //     } else {
                //         Color32::GOLD
                //     };

                //     ui.painter().rect_filled(
                //         Rect::from_center_size(Pos2::new(pt.0, pt.1), Vec2::splat(15.)),
                //         2.,
                //         col,
                //     );
                // }

                // if let Some(pt) = ui.ctx().data(|r| r.get_temp::<usize>("pt".into())) {
                //     points[pt].0 = cursor_relative.x as u32;
                //     points[pt].1 = cursor_relative.y as u32;
                // }

                r
            }
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
                        r1.mark_changed();
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
                        r1.mark_changed();
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

                let mut r = ui.allocate_response(Vec2::ZERO, Sense::hover());

                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        let r0 = ui.add(
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
                            dimensions.1 = (dimensions.0 as f32 * ratio) as u32;
                        }

                        if r1.changed() {
                            if *aspect {
                                dimensions.0 = (dimensions.1 as f32 / ratio) as u32
                            }
                        }
                        let r2 = ui
                            .styled_checkbox(aspect, "🔒")
                            .on_hover_text("Lock aspect ratio");

                        if r2.changed() {
                            if *aspect {
                                dimensions.1 = (dimensions.0 as f32 * ratio) as u32;
                            }
                        }
                        // For this operator, we want to update on release, not on change.
                        // Since all operators are processed the same, we use the hack to emit `changed` just on release.
                        // Users dragging the resize values will now only trigger a resize on release, which feels
                        // more snappy.
                        if r0.drag_stopped() || r1.drag_stopped() || r2.changed() {
                            r.mark_changed();
                        }
                    });

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
                                    r.mark_changed();
                                }
                            }
                        });

                    r
                })
                .inner
            }
            _ => ui.label("Filter has no options."),
        }
    }

    /// Process all image operators (All things that modify the image and are not "per pixel")
    pub fn process_image(&self, dyn_img: &mut DynamicImage) -> Result<()> {
        match dyn_img {
            DynamicImage::ImageRgba8(img) => {
                match self {
                    Self::Blur(amt) => {
                        if *amt != 0 {
                            let i = img.clone();
                            let mut data = i.into_raw();
                            libblur::stack_blur(
                                data.as_mut_slice(),
                                img.width() * 4,
                                img.width(),
                                img.height(),
                                (*amt as u32).clamp(2, 254),
                                libblur::FastBlurChannels::Channels4,
                                libblur::ThreadingPolicy::Adaptive,
                            );
                            use anyhow::Context;
                            *img = RgbaImage::from_raw(img.width(), img.height(), data)
                                .context("Can't construct image from blur result")?;
                        }
                    }
                    Self::Filter3x3(amt) => {
                        let kernel = amt
                            .into_iter()
                            .map(|a| *a as f32 / 100.)
                            .collect::<Vec<_>>();
                        *img = imageops::filter3x3(img, &kernel);
                    }
                    Self::LUT(lut_name) => {
                        use lutgen::identity::correct_image;
                        let mut external_image = DynamicImage::ImageRgba8(img.clone()).to_rgb8();
                        if let Some(lut_data) = builtin_luts().get(lut_name) {
                            let lut_img = image::load_from_memory(&lut_data).unwrap().to_rgb8();
                            correct_image(&mut external_image, &lut_img);
                        } else {
                            if let Ok(lut_img) = image::open(&lut_name) {
                                correct_image(&mut external_image, &lut_img.to_rgb8());
                            }
                        }
                        *img = DynamicImage::ImageRgb8(external_image).to_rgba8();
                    }
                    Self::Crop(dim) => {
                        if *dim != [0, 0, 0, 0] {
                            let window = cropped_range(dim, &(img.width(), img.height()));
                            let sub_img = image::imageops::crop_imm(
                                img, window[0], window[1], window[2], window[3],
                            );
                            *img = sub_img.to_image();
                        }
                    }
                    Self::CropPerspective { points, .. } => {
                        let img_dim = img.dimensions();

                        let max_width = points[1].0.max(points[3].0);
                        let min_width = points[0].0.min(points[2].0);
                        let max_height = points[2].1.max(points[3].1);
                        let min_height = points[0].1.min(points[1].1);
                        let x = max_width - min_width;
                        let y = max_height - min_height;

                        let from = [
                            (points[0].0 as f32, points[0].1 as f32),
                            (points[1].0 as f32, points[1].1 as f32),
                            (points[2].0 as f32, points[2].1 as f32),
                            (points[3].0 as f32, points[3].1 as f32),
                        ];

                        let to = [
                            (0 as f32, 0 as f32),
                            (img_dim.0 as f32, 0 as f32),
                            (0 as f32, img_dim.1 as f32),
                            (img_dim.0 as f32, img_dim.1 as f32),
                        ];

                        if let Some(proj) =
                            imageproc::geometric_transformations::Projection::from_control_points(
                                from, to,
                            )
                        {
                            let default_p: Rgba<u8> = [0, 0, 0, 0].into();

                            *img = imageproc::geometric_transformations::warp(
                                img,
                                &proj,
                                Interpolation::Bicubic,
                                default_p,
                            );

                            *img = imageops::resize(img, x, y, imageops::FilterType::CatmullRom);
                        } else {
                            error!("Projection failed")
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

                            let src_image = fr::images::Image::from_vec_u8(
                                img.width(),
                                img.height(),
                                img.clone().into_raw(),
                                fr::PixelType::U8x4,
                            )?;

                            // Create container for data of destination image
                            let mut dst_image = fr::images::Image::new(
                                dimensions.0,
                                dimensions.1,
                                src_image.pixel_type(),
                            );

                            let mut resizer = fr::Resizer::new();

                            resizer.resize(
                                &src_image,
                                &mut dst_image,
                                Some(
                                    &ResizeOptions::new().resize_alg(
                                        fast_image_resize::ResizeAlg::Convolution(filter),
                                    ),
                                ),
                            )?;

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
                    Self::Rotate(angle) => match angle {
                        90 => *img = image::imageops::rotate90(img),
                        -90 => *img = image::imageops::rotate270(img),
                        270 => *img = image::imageops::rotate270(img),
                        180 => *img = image::imageops::rotate180(img),
                        _ => (),
                    },
                    Self::Flip(vert) => {
                        if *vert {
                            *img = image::imageops::flip_vertical(img);
                        } else {
                            *img = image::imageops::flip_horizontal(img);
                        }
                    }
                    Self::ScaleImageMinMax => {
                        //Step 0: Get color channel min and max values
                        let mut min = 255u8;
                        let mut max = 0u8;

                        img.chunks_mut(4).for_each(|px| {
                            min = std::cmp::min(min, px[0]);
                            min = std::cmp::min(min, px[1]);
                            min = std::cmp::min(min, px[2]);

                            max = std::cmp::max(max, px[0]);
                            max = std::cmp::max(max, px[1]);
                            max = std::cmp::max(max, px[2]);
                        });
                        let min_f = min as f64;
                        let max_f = max as f64;

                        //Step 1: Don't do zero division
                        if min != max {
                            //Step 2: Create 8-Bit LUT
                            let mut lut: [u8; 256] = [0u8; 256];

                            for n in min as usize..=max as usize {
                                let g = n as f64;
                                lut[n] = (255.0 * (g - min_f) / (max_f - min_f)) as u8;
                            }

                            //Step 3: Apply 8-Bit LUT
                            img.par_chunks_mut(4).for_each(|px| {
                                px[0] = lut[px[0] as usize];
                                px[1] = lut[px[1] as usize];
                                px[2] = lut[px[2] as usize];
                            });
                        }
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
            // These must work on all types
            _ => {
                info!("Proc with color type {:?}", dyn_img.color());

                match self {
                    Self::ColorConverter(t) => match t {
                        ColorTypeExt::L8 => {
                            *dyn_img = DynamicImage::ImageLuma8(dyn_img.to_luma8());
                        }
                        ColorTypeExt::La8 => {
                            *dyn_img = DynamicImage::ImageLumaA8(dyn_img.to_luma_alpha8());
                        }
                        ColorTypeExt::Rgb8 => {
                            *dyn_img = DynamicImage::ImageRgb8(dyn_img.to_rgb8());
                        }
                        ColorTypeExt::Rgba8 => {
                            *dyn_img = DynamicImage::ImageRgba8(dyn_img.to_rgba8());
                        }
                        ColorTypeExt::L16 => {
                            *dyn_img = DynamicImage::ImageLuma16(dyn_img.to_luma16());
                        }
                        ColorTypeExt::La16 => {
                            *dyn_img = DynamicImage::ImageLumaA16(dyn_img.to_luma_alpha16());
                        }
                        ColorTypeExt::Rgb16 => {
                            *dyn_img = DynamicImage::ImageRgb16(dyn_img.to_rgb16());
                        }
                        ColorTypeExt::Rgba16 => {
                            *dyn_img = DynamicImage::ImageRgba16(dyn_img.to_rgba16());
                        }
                        ColorTypeExt::Rgb32F => {
                            *dyn_img = DynamicImage::ImageRgb32F(dyn_img.to_rgb32f());
                        }
                        ColorTypeExt::Rgba32F => {
                            *dyn_img = DynamicImage::ImageRgba32F(dyn_img.to_rgba32f());
                        }
                    },
                    Self::Flip(vert) => {
                        if *vert {
                            *dyn_img = dyn_img.flipv();
                        } else {
                            *dyn_img = dyn_img.fliph();
                        }
                    }
                    Self::Rotate(angle) => match angle {
                        90 => *dyn_img = dyn_img.rotate90(),
                        -90 => *dyn_img = dyn_img.rotate270(),
                        270 => *dyn_img = dyn_img.rotate270(),
                        180 => *dyn_img = dyn_img.rotate180(),
                        _ => (),
                    },
                    Self::Blur(amt) => {
                        if *amt != 0 {
                            *dyn_img = dyn_img.fast_blur(*amt as f32 / 2.5);
                        }
                    }
                    Self::Resize {
                        dimensions, filter, ..
                    } => {
                        let filter = match filter {
                            ScaleFilter::CatmullRom => imageops::FilterType::CatmullRom,
                            ScaleFilter::Lanczos3 => imageops::FilterType::Lanczos3,
                            _ => imageops::FilterType::Gaussian,
                        };
                        *dyn_img = dyn_img.resize(dimensions.0, dimensions.0, filter);
                    }
                    _ => {
                        bail!("This color type is unsupported: {:?}", dyn_img.color())
                    }
                }
            }
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
            Self::Slice(position, range, smooth) => {
                let normalized_pos = (*position as f32) / 255.;
                let normalized_range = (*range as f32) / 255.;
                for i in 0..=2 {
                    if *smooth {
                        let distance = 1.0 - (normalized_pos - p[i]).abs();
                        p[i] *= distance + normalized_range;
                    } else {
                        if p[i] > normalized_pos + normalized_range {
                            p[i] = 0.;
                        }
                        if p[i] < (normalized_pos - normalized_range) {
                            p[i] = 0.;
                        }
                    }
                }
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
pub fn builtin_luts() -> HashMap<String, Vec<u8>> {
    let mut luts = HashMap::new();
    luts.insert(
        "Fuji Superia 1600 2".to_string(),
        include_bytes!("../res/LUT/Fuji Superia 1600 2.png").to_vec(),
    );
    luts.insert(
        "Lomography Redscale 100".to_string(),
        include_bytes!("../res/LUT/Lomography Redscale 100.png").to_vec(),
    );
    luts.insert(
        "Lomography X-Pro Slide 200".to_string(),
        include_bytes!("../res/LUT/Lomography X-Pro Slide 200.png").to_vec(),
    );
    luts.insert(
        "Polaroid Polachrome".to_string(),
        include_bytes!("../res/LUT/Polaroid Polachrome.png").to_vec(),
    );
    luts
}

pub fn process_pixels(dynimage: &mut DynamicImage, operators: &Vec<ImageOperation>) -> Result<()> {
    match dynimage {
        DynamicImage::ImageRgb8(buffer) => {
            buffer.par_chunks_mut(3).for_each(|px| {
                let mut float_pixel =
                    Vector4::new(px[0] as f32, px[1] as f32, px[2] as f32, 1.0 as f32) / 255.;
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
            });
        }
        DynamicImage::ImageRgba8(buffer) => {
            buffer.par_chunks_mut(4).for_each(|px| {
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
        DynamicImage::ImageLuma8(buffer) => {
            buffer.par_chunks_mut(1).for_each(|px| {
                let mut float_pixel = Vector4::new(px[0] as f32, 0.0, 0.0, 0.0) / 255.;
                // run pixel operations
                for operation in operators {
                    if let Err(e) = operation.process_pixel(&mut float_pixel) {
                        error!("{e}")
                    }
                }
                float_pixel *= 255.;
                px[0] = (float_pixel[0]) as u8;
            });
        }
        DynamicImage::ImageLumaA8(buffer) => {
            buffer.par_chunks_mut(2).for_each(|px| {
                let mut float_pixel = Vector4::new(px[0] as f32, 0.0, 0.0, px[1] as f32) / 255.;
                // run pixel operations
                for operation in operators {
                    if let Err(e) = operation.process_pixel(&mut float_pixel) {
                        error!("{e}")
                    }
                }
                float_pixel *= 255.;
                px[0] = (float_pixel[0]) as u8;
                px[1] = (float_pixel[1]) as u8;
            });
        }
        DynamicImage::ImageRgb32F(buffer) => {
            buffer.par_chunks_mut(3).for_each(|px| {
                let mut float_pixel = Vector4::new(px[0], px[1], px[2], 0.0);
                for operation in operators {
                    if let Err(e) = operation.process_pixel(&mut float_pixel) {
                        error!("{e}")
                    }
                }
                px[0] = float_pixel[0];
                px[1] = float_pixel[1];
                px[2] = float_pixel[2];
            });
        }
        DynamicImage::ImageRgba32F(buffer) => {
            buffer.par_chunks_mut(3).for_each(|px| {
                let mut float_pixel = Vector4::new(px[0], px[1], px[2], px[3]);
                for operation in operators {
                    if let Err(e) = operation.process_pixel(&mut float_pixel) {
                        error!("{e}")
                    }
                }
                px[0] = float_pixel[0];
                px[1] = float_pixel[1];
                px[2] = float_pixel[2];
                px[3] = float_pixel[3];
            });
        }
        _ => {
            bail!("Pixel operators are not yet supported for this image type.");
        }
    }
    return Ok(());
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
#[derive(
    Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Serialize, Deserialize, EnumIter, Display,
)]
pub enum ColorTypeExt {
    // Pixel is 8-bit luminance
    L8,
    /// Pixel is 8-bit luminance with an alpha channel
    La8,
    /// Pixel contains 8-bit R, G and B channels
    Rgb8,
    /// Pixel is 8-bit RGB with an alpha channel
    Rgba8,
    /// Pixel is 16-bit luminance
    L16,
    /// Pixel is 16-bit luminance with an alpha channel
    La16,
    /// Pixel is 16-bit RGB
    Rgb16,
    /// Pixel is 16-bit RGBA
    Rgba16,
    /// Pixel is 32-bit float RGB
    Rgb32F,
    /// Pixel is 32-bit float RGBA
    Rgba32F,
}

impl ColorTypeExt {
    pub fn _from_image(ct: ColorType) -> Self {
        match ct {
            ColorType::L8 => ColorTypeExt::L8,
            ColorType::La8 => ColorTypeExt::La8,
            ColorType::Rgb8 => ColorTypeExt::Rgb8,
            ColorType::Rgba8 => ColorTypeExt::Rgba8,
            ColorType::L16 => ColorTypeExt::L16,
            ColorType::La16 => ColorTypeExt::La16,
            ColorType::Rgb16 => ColorTypeExt::Rgb16,
            ColorType::Rgba16 => ColorTypeExt::Rgba16,
            ColorType::Rgb32F => ColorTypeExt::Rgb32F,
            ColorType::Rgba32F => ColorTypeExt::Rgba32F,
            _ => ColorTypeExt::Rgba8,
        }
    }
}
