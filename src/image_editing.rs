use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};

use crate::icons::*;
use crate::paint::PaintStroke;
use crate::settings::VolatileSettings;
use crate::ui::EguiExt;
use crate::{appstate::ImageGeometry, utils::pos_from_coord};
#[cfg(not(feature = "file_open"))]
use crate::{filebrowser, utils::SUPPORTED_EXTENSIONS};
use anyhow::{Context as _, Result, bail};
use egui::epaint::PathShape;
use egui::{
    self, Align2, Color32, DragValue, FontId, Id, Pos2, Rect, Response, Sense, Stroke, StrokeKind,
    Ui, Vec2, lerp, vec2,
};
use evalexpr::*;
use fast_image_resize::{self as fr, ResizeOptions};
use image::{ColorType, DynamicImage, Rgba, RgbaImage, imageops};
use imageproc::geometric_transformations::Interpolation;
use log::{debug, error, info};
use nalgebra::{Vector2, Vector4};
use num_integer::gcd;
use palette::{Hsl, IntoColor, rgb::Rgb};
use rand::RngExt;
use rayon::{
    iter::{IndexedParallelIterator, ParallelIterator},
    slice::{ParallelSlice, ParallelSliceMut},
};
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
    /// Whether to completely bypass processing the edit stack
    #[serde(skip)]
    pub skip_processing: bool,
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
        EditState {
            image_op_stack: self
                .image_op_stack
                .iter()
                .map(|op| ImgOpItem::new(op.clone()))
                .collect(),
            pixel_op_stack: self
                .pixel_op_stack
                .iter()
                .map(|op| ImgOpItem::new(op.clone()))
                .collect(),
            ..Default::default()
        }
    }
}

/// Edits saved with "Save edits" next to an image, or with "Save directory
/// edits" for all images of a folder, with a message for the user. `None` if
/// there are none, an error if the file of the image can not be read.
pub fn saved_edits(image_path: &Path) -> Option<Result<(EditState, &'static str), String>> {
    let own = image_path.with_extension("oculante");
    if own.is_file() {
        let loaded = "Edits have been loaded for this image.";
        let text = match std::fs::read_to_string(&own) {
            Ok(text) => text,
            Err(e) => return Some(Err(format!("Edits could not be loaded: {e}"))),
        };
        if let Ok(edit_state) = serde_json::from_str::<EditState>(&text) {
            return Some(Ok((edit_state, loaded)));
        }
        // Edits saved by older versions are upgraded, and saved again that way
        if let Ok(legacy) = serde_json::from_str::<LegacyEditState>(&text) {
            let edit_state = legacy.upgrade();
            if let Ok(file) = std::fs::File::create(&own) {
                _ = serde_json::to_writer_pretty(file, &edit_state);
            }
            return Some(Ok((edit_state, loaded)));
        }
        return Some(Err("Edits could not be loaded.".into()));
    }
    let folder = image_path.parent()?.join(".oculante");
    let edit_state =
        serde_json::from_str::<EditState>(&std::fs::read_to_string(folder).ok()?).ok()?;
    Some(Ok((
        edit_state,
        "Directory edits have been loaded for this image.",
    )))
}

impl EditState {
    /// The result of the image operations is kept when that saves work: when
    /// there is an operation other than the conversion of the color type. The
    /// conversion alone is cheap to do again, and keeping its result would hold
    /// a second copy of the image.
    pub fn keeps_image_op_result(&self) -> bool {
        self.image_op_stack
            .iter()
            .any(|op| op.active && !matches!(op.operation, ImageOperation::ColorConverter(_)))
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
            skip_processing: false,
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
    pub id: u64,
    pub active: bool,
    pub operation: ImageOperation,
}

impl ImgOpItem {
    pub fn new(op: ImageOperation) -> Self {
        Self {
            id: rand::rng().random(),
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
    pub fn new_rect(points: Vec<(u32, u32)>) -> Self {
        Self::Rect {
            points,
            color: [255, 255, 255, 255],
            width: 4,
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

/// Name under which the app stores the area of the window that shows the image,
/// the part that no panel covers.
pub const CANVAS_RECT: &str = "canvas_rect";

/// The area of the window that shows the image, as of the last frame.
pub fn canvas_rect(ctx: &egui::Context) -> Rect {
    ctx.data(|data| data.get_temp::<Rect>(Id::new(CANVAS_RECT)))
        .unwrap_or(Rect::EVERYTHING)
}

impl ImageOperation {
    pub fn is_per_pixel(&self) -> bool {
        !matches!(
            self,
            Self::Blur(_)
                | Self::Resize { .. }
                | Self::Crop(_)
                | Self::CropPerspective { .. }
                | Self::Rotate(_)
                | Self::ColorConverter(_)
                | Self::Flip(_)
                | Self::ChromaticAberration(_)
                | Self::Filter3x3(_)
                | Self::ScaleImageMinMax,
        )
    }

    // Add functionality about how to draw UI here
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        geo: &ImageGeometry,
        block_panning: &mut bool,
        #[allow(unused)] settings: &mut VolatileSettings,
        item_id: u64,
    ) -> Response {
        match self {
            Self::ColorConverter(ct) => {
                let mut x = ui.allocate_response(vec2(0.0, 0.0), Sense::click_and_drag());
                egui::ComboBox::from_id_salt("color_types")
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
                                if ui.add(egui::DragValue::new(v).range(-255..=255)).changed() {
                                    x.mark_changed();
                                }
                                ui.add_space(30.);
                            }
                        });
                    }
                });
                x
            }
            Self::Posterize(val) => ui.styled_slider(val, 2..=255),
            Self::Expression(expr) => {
                let r = ui.text_edit_singleline(expr);
                // Until it works it is left out, here is why. Some mistakes only show
                // when it runs, `r =` parses, so it runs once on a gray pixel.
                let check = build_operator_tree::<DefaultNumericTypes>(expr)
                    .and_then(|tree| PixelVariables::new().run(&tree, &mut Vector4::repeat(0.5)));
                if let Err(e) = check {
                    ui.colored_label(ui.visuals().error_fg_color, e.to_string());
                }
                r
            }
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
                                let mut lut_names =
                                    builtin_luts().iter().map(|l| l.0).collect::<Vec<_>>();
                                lut_names.sort();
                                for lut in lut_names {
                                    if ui
                                        .selectable_value(
                                            lut_name,
                                            lut.to_string(),
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
                                crate::ui::open_popup(ui.ctx(), Id::new("LUT"));
                            }

                            if crate::ui::is_popup_open(ui.ctx(), Id::new("LUT")) {
                                filebrowser::browse_modal(
                                    false,
                                    SUPPORTED_EXTENSIONS,
                                    settings,
                                    |p| {
                                        *lut_name = p.to_string_lossy().to_string();
                                        x.mark_changed();
                                    },
                                    ui.ctx(),
                                    Id::new("LUT"),
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
                            _ = open::that("https://github.com/cedeber/hald-clut");
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
                    egui::ComboBox::from_id_salt(format!("ccopy 0 {}", val.0 as usize))
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

                    egui::ComboBox::from_id_salt(format!("ccopy 1 {}", val.1 as usize))
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
                let mut r = ui.add(DragValue::new(&mut val.0).range(0..=360));
                if ui.add(DragValue::new(&mut val.1).range(0..=200)).changed() {
                    r.mark_changed();
                }
                if ui.add(DragValue::new(&mut val.2).range(0..=200)).changed() {
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

                            if closest_pt(&pts_cpy, mouse_pos_in_gradient as u8) == ptnum {
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
                                        .range(0..=255)
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
                            if len > 2 && ui.button("🗑").clicked() {
                                delete = Some(i);
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

                    pts.sort_by_key(|a| a.pos);

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
                // Whether the crop was applied and which corner is dragged belongs to this
                // operation. A crop that is added later starts fresh.
                let id = Id::new("crop").with(item_id);
                let dragged_point_id = Id::new("crop_point").with(item_id);
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
                // The crop is drawn over the image, not into the panel this ui belongs to,
                // whose painter would clip it away. The darkening stays on the image area,
                // the handles and the outline go on top of the panels as well, so they can
                // always be seen and grabbed.
                let layer =
                    egui::LayerId::new(egui::Order::Middle, Id::new("crop_overlay").with(item_id));
                let handle_painter = ui.ctx().layer_painter(layer);
                let painter = handle_painter.with_clip_rect(canvas_rect(ui.ctx()));

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

                    let mut handles = vec![];
                    for (i, pt) in points_transformed.iter().enumerate() {
                        let maxdist = 20.;
                        let d = Pos2::new(pt.0, pt.1).distance(cursor_abs);

                        if d < maxdist {
                            if ui.input(|i| i.pointer.any_down()) {
                                *block_panning = true;
                                ui.ctx().data_mut(|w| w.insert_temp(dragged_point_id, i));
                            }
                            if ui.input(|r| r.pointer.any_released()) {
                                *block_panning = false;
                                ui.ctx()
                                    .data_mut(|w| w.remove_temp::<usize>(dragged_point_id));
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

                        handles.push((Pos2::new(pt.0, pt.1), col));
                    }

                    // egui shape needs these in a different order
                    let pts = vec![
                        Pos2::new(points_transformed[0].0, points_transformed[0].1),
                        Pos2::new(points_transformed[1].0, points_transformed[1].1),
                        Pos2::new(points_transformed[3].0, points_transformed[3].1),
                        Pos2::new(points_transformed[2].0, points_transformed[2].1),
                    ];

                    // make a black background covering the image area
                    painter.rect_filled(
                        Rect::EVERYTHING,
                        0.,
                        Color32::from_rgba_premultiplied(0, 0, 0, 70),
                    );

                    let shape = PathShape::convex_polygon(
                        pts,
                        Color32::from_rgba_unmultiplied(255, 255, 255, 10),
                        Stroke::new(1., Color32::GOLD),
                    );
                    handle_painter.add(shape);
                    for (center, color) in handles {
                        handle_painter.rect_filled(
                            Rect::from_center_size(center, Vec2::splat(15.)),
                            2.,
                            color,
                        );
                    }

                    if let Some(pt) = ui.ctx().data(|r| r.get_temp::<usize>(dragged_point_id)) {
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
                // Draw on the middle layer, above the image (CentralPanel), and keep it
                // off the side panels
                let painter = ui
                    .ctx()
                    .layer_painter(egui::LayerId::new(
                        egui::Order::Middle,
                        Id::new("measure_overlay"),
                    ))
                    .with_clip_rect(canvas_rect(ui.ctx()));

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
                                painter.line_segment(
                                    [Pos2::new(p[0].0, p[0].1), Pos2::new(p[1].0, p[1].1)],
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
                            // nothing was measured yet
                            if points[0] == points[1] {
                                continue;
                            }
                            let points_transformed = points
                                .iter()
                                .map(|p| {
                                    (
                                        geo.scale * p.0 as f32 + geo.offset.x,
                                        geo.scale * p.1 as f32 + geo.offset.y,
                                    )
                                })
                                .collect::<Vec<_>>();

                            // the drag can go in any direction
                            let rect = Rect::from_two_pos(
                                Pos2::new(points_transformed[0].0, points_transformed[0].1),
                                Pos2::new(points_transformed[1].0, points_transformed[1].1),
                            );

                            let rect_orig = Rect::from_two_pos(
                                Pos2::new(points[0].0 as f32, points[0].1 as f32),
                                Pos2::new(points[1].0 as f32, points[1].1 as f32),
                            );

                            painter.rect_stroke(
                                rect,
                                0.0,
                                Stroke::new(*width as f32, Color32::BLACK),
                                StrokeKind::Inside,
                            );

                            painter.rect_filled(
                                rect,
                                0.0,
                                Color32::from_rgba_unmultiplied(0, 0, 0, 80),
                            );

                            painter.text(
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

                            painter.line_segment(
                                [rect.left_center(), rect.right_center()],
                                Stroke::new(1., Color32::from_rgba_unmultiplied(255, 255, 255, 10)),
                            );

                            painter.line_segment(
                                [rect.center_top(), rect.center_bottom()],
                                Stroke::new(1., Color32::from_rgba_unmultiplied(255, 255, 255, 10)),
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
                            .range(0.0..=1.0)
                            // X
                            .prefix("⏵ "),
                    );
                    let r2 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut float_bounds[2])
                            .speed(0.004)
                            .range(0.0..=1.0)
                            // WIDTH
                            .prefix("⏴ "),
                    );
                    let r3 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut float_bounds[1])
                            .speed(0.004)
                            .range(0.0..=1.0)
                            // Y
                            .prefix("⏷ "),
                    );
                    let r4 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut float_bounds[3])
                            .speed(0.004)
                            .range(0.0..=1.0)
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
                            .range(-128..=128)
                            .prefix("dark "),
                    );
                    let r2 = ui.add_sized(
                        egui::vec2(available_w_single_spacing / 4., ui.available_height()),
                        egui::DragValue::new(&mut bounds.1)
                            .speed(2.)
                            .range(64..=2000)
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
                let mut r = ui.allocate_response(Vec2::ZERO, Sense::hover());
                let aspect_ratio_id = Id::new("resize_aspect_ratio").with(item_id);

                ui.vertical(|ui| {
                    // This handles the initial state when the filter is first added.
                    if *aspect
                        && ui
                            .ctx()
                            .data(|d| d.get_temp::<f64>(aspect_ratio_id).is_none())
                    {
                        let ratio_to_store = dimensions.0 as f64 / dimensions.1 as f64;
                        ui.ctx()
                            .data_mut(|d| d.insert_temp(aspect_ratio_id, ratio_to_store));
                    }

                    // Get the aspect ratio. Use the one stored in egui if it exists, otherwise calculate it.
                    let aspect_ratio = ui
                        .ctx()
                        .data(|d| d.get_temp(aspect_ratio_id))
                        .unwrap_or_else(|| {
                            if dimensions.1 > 0 {
                                dimensions.0 as f64 / dimensions.1 as f64
                            } else {
                                geo.dimensions.0 as f64 / geo.dimensions.1 as f64
                            }
                        });

                    let g = gcd(dimensions.1, dimensions.0);
                    let w = dimensions.0 / g;
                    let h = dimensions.1 / g;

                    ui.label(format!("Aspect ratio: {}:{} ({:.5})", w, h, aspect_ratio));

                    ui.horizontal(|ui| {
                        let x_response = ui.add(
                            DragValue::new(&mut dimensions.0)
                                .speed(0.5)
                                .range(10..=10000)
                                .prefix("X "),
                        );

                        let y_response = ui.add(
                            DragValue::new(&mut dimensions.1)
                                .speed(0.5)
                                .range(10..=10000)
                                .prefix("Y "),
                        );

                        if *aspect {
                            if x_response.changed() {
                                dimensions.1 = (dimensions.0 as f64 / aspect_ratio).round() as u32;
                            } else if y_response.changed() {
                                dimensions.0 = (dimensions.1 as f64 * aspect_ratio).round() as u32;
                            }
                        }

                        if x_response.changed() || y_response.changed() {
                            r.mark_changed();
                        }

                        // TODO: Replace the lock emoji with an actual icon - @Stoppedpuma
                        if ui.styled_checkbox(aspect, "🔒").clicked() {
                            // TODO: This code can still be slightly off by a pixel or two but I'm not too worried about it for now since it has significantly less problems than the previous solution, but it definitely should be fixed in the future. - @Stoppedpuma
                            if *aspect {
                                // If locking, calculate and store the current aspect ratio.
                                let ratio_to_store = if dimensions.1 > 0 {
                                    dimensions.0 as f64 / dimensions.1 as f64
                                } else {
                                    geo.dimensions.0 as f64 / geo.dimensions.1 as f64
                                };
                                ui.ctx()
                                    .data_mut(|d| d.insert_temp(aspect_ratio_id, ratio_to_store));
                            }
                            r.mark_changed();
                        }
                    });

                    egui::ComboBox::from_id_salt("filter")
                        .width(ui.available_width())
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

                    ui.vertical_centered_justified(|ui| {
                        if ui.button("Reset").clicked() {
                            // Reset dimensions to original
                            *dimensions = geo.dimensions;

                            // Reset aspect lock to default (true)
                            *aspect = true;

                            // Remove the stored aspect ratio from egui's memory.
                            // This will cause it to be recalculated from the original dimensions
                            // on the next frame, effectively resetting it.
                            ui.ctx().data_mut(|d| d.remove_temp::<f64>(aspect_ratio_id));

                            // Mark the UI as changed
                            r.mark_changed();
                        }
                    });
                });
                r
            }
            _ => ui.label("Filter has no options."),
        }
    }

    /// Process all image operators (All things that modify the image and are not "per pixel")
    pub fn process_image(&self, dyn_img: &mut DynamicImage) -> Result<()> {
        // the same for every layout
        if let Self::ColorConverter(t) = self {
            convert_color_type(dyn_img, t);
            return Ok(());
        }
        match dyn_img {
            DynamicImage::ImageRgba8(img) => {
                match self {
                    Self::Blur(amt) if *amt != 0 => {
                        let (width, height) = img.dimensions();
                        // Blurred as they are, transparent pixels give their color, mostly
                        // black, to the ones around them: a dark fringe. With the color
                        // multiplied by the alpha first they have none to give.
                        let transparent = img.as_raw().par_chunks(4).any(|px| px[3] < 255);
                        if transparent {
                            premultiply(img);
                        }
                        // blurs the pixels in place
                        let mut pixels = libblur::BlurImageMut::borrow(
                            img,
                            width,
                            height,
                            libblur::FastBlurChannels::Channels4,
                        );
                        libblur::stack_blur(
                            &mut pixels,
                            libblur::AnisotropicRadius::new((*amt as u32).clamp(2, 254)),
                            libblur::ThreadingPolicy::Adaptive,
                        )
                        .map_err(|e| anyhow::anyhow!("Can't blur the image: {e}"))?;
                        if transparent {
                            unpremultiply(img);
                        }
                    }
                    Self::Filter3x3(amt) => {
                        *img = filter3x3(img, &amt.map(|a| a as f32 / 100.));
                    }
                    // Edits saved before the LUT became a pixel operation have it here
                    Self::LUT(lut_name) => {
                        let lut = hald_clut(lut_name)?;
                        img.par_chunks_mut(4).for_each(|px| {
                            let rgb = lut.map([px[0], px[1], px[2]]);
                            px[..3].copy_from_slice(&rgb);
                        });
                    }
                    Self::Crop(dim) if *dim != [0, 0, 0, 0] => {
                        let window = cropped_range(dim, &(img.width(), img.height()));
                        let sub_img = image::imageops::crop_imm(
                            img, window[0], window[1], window[2], window[3],
                        );
                        *img = sub_img.to_image();
                    }
                    Self::CropPerspective { points, .. } => {
                        use imageproc::geometric_transformations::{Border, Projection, warp_into};
                        // The result is as large as the area the corners span. Taken from
                        // the right corners less the left ones, the size went below zero
                        // when they were dragged past each other.
                        let span = |coord: fn(&(u32, u32)) -> u32| {
                            let values = points.map(|p| coord(&p));
                            values.iter().max().unwrap_or(&0) - values.iter().min().unwrap_or(&0)
                        };
                        let (width, height) = (span(|p| p.0), span(|p| p.1));
                        let from = points.map(|p| (p.0 as f32, p.1 as f32));
                        let to = [
                            (0., 0.),
                            (width as f32, 0.),
                            (0., height as f32),
                            (width as f32, height as f32),
                        ];
                        let projection = Projection::from_control_points(from, to)
                            .filter(|_| width > 0 && height > 0)
                            .context("The corners of the perspective crop are on one line")?;
                        // straight to the size, one resampling
                        let mut out = RgbaImage::new(width, height);
                        warp_into(
                            img,
                            projection,
                            Interpolation::Bicubic,
                            Border::Constant(Rgba([0, 0, 0, 0])),
                            &mut out,
                        );
                        *img = out;
                    }
                    Self::Resize {
                        dimensions, filter, ..
                    } if *dimensions != Default::default() => {
                        let filter = match filter {
                            ScaleFilter::Box => fr::FilterType::Box,
                            ScaleFilter::Bilinear => fr::FilterType::Bilinear,
                            ScaleFilter::Hamming => fr::FilterType::Hamming,
                            ScaleFilter::CatmullRom => fr::FilterType::CatmullRom,
                            ScaleFilter::Mitchell => fr::FilterType::Mitchell,
                            ScaleFilter::Lanczos3 => fr::FilterType::Lanczos3,
                        };

                        let src_image = fr::images::ImageRef::new(
                            img.width(),
                            img.height(),
                            img.as_raw(),
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
                                &ResizeOptions::new()
                                    .resize_alg(fast_image_resize::ResizeAlg::Convolution(filter)),
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
                        let (min, max) = img
                            .as_raw()
                            .par_chunks(4)
                            .map(|px| (px[0].min(px[1]).min(px[2]), px[0].max(px[1]).max(px[2])))
                            .reduce(|| (255, 0), |a, b| (a.0.min(b.0), a.1.max(b.1)));
                        let min_f = min as f64;
                        let max_f = max as f64;

                        //Step 1: Don't do zero division
                        if min != max {
                            //Step 2: Create 8-Bit LUT
                            let mut lut: [u8; 256] = [0u8; 256];

                            for n in min..=max {
                                let g = n as f64;
                                lut[n as usize] = (255.0 * (g - min_f) / (max_f - min_f)) as u8;
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
                        let row_len = img.width() as usize * 4;

                        // row by row in parallel
                        img.par_chunks_mut(row_len.max(1))
                            .enumerate()
                            .for_each(|(y, row)| {
                                for (x, p) in row.chunks_mut(4).enumerate() {
                                    let dist_to_center = (x as i32 - center.0, y as i32 - center.1);
                                    let dist_to_center = (
                                        (dist_to_center.0 as f32 / center.0 as f32) * *amt as f32
                                            / 10.,
                                        (dist_to_center.1 as f32 / center.1 as f32) * *amt as f32
                                            / 10.,
                                    );
                                    if let Some(l) = img_c.get_pixel_checked(
                                        (x as i32 + dist_to_center.0 as i32).max(0) as u32,
                                        (y as i32 + dist_to_center.1 as i32).max(0) as u32,
                                    ) {
                                        p[0] = l[0];
                                    }
                                }
                            });
                    }

                    _ => (),
                }
            }
            // These must work on all types
            _ => {
                info!("Proc with color type {:?}", dyn_img.color());

                match self {
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
                        if *dimensions != Default::default() {
                            *dyn_img = dyn_img.resize_exact(dimensions.0, dimensions.1, filter);
                        }
                    }
                    _ => {
                        bail!("This color type is unsupported: {:?}", dyn_img.color())
                    }
                }
            }
        }

        Ok(())
    }

    /// Process a single pixel, for the operations that need nothing worked out
    /// beforehand. The others are in [`PixelOp`].
    fn process_pixel(&self, p: &mut Vector4<f32>) {
        match self {
            Self::Brightness(amt) => {
                let amt = *amt as f32 / 255.;
                *p += Vector4::new(amt, amt, amt, 0.0);
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
                // without alpha there is no color to get back, it stays
                if p[3] > 0. {
                    p[0] /= p[3];
                    p[1] /= p[3];
                    p[2] /= p[3];
                }
            }
            Self::Contrast(val) => {
                let factor: f32 = (1.015_686_3 * (*val as f32 / 255. + 1.0))
                    / (1.0 * (1.015_686_3 - *val as f32 / 255.));
                // away from the middle, or towards it
                p[0] = factor * (p[0] - 0.5) + 0.5;
                p[1] = factor * (p[1] - 0.5) + 0.5;
                p[2] = factor * (p[2] - 0.5) + 0.5;
            }
            _ => (),
        }
    }
}

pub fn desaturate(p: &mut Vector4<f32>, factor: f32) {
    // G*.59+R*.3+B*.11
    // the luma, green counts most
    let val = p[0] * 0.299 + p[1] * 0.587 + p[2] * 0.114;
    p[0] = egui::lerp(p[0]..=val, factor);
    p[1] = egui::lerp(p[1]..=val, factor);
    p[2] = egui::lerp(p[2]..=val, factor);
}
/// The LUTs that come with Oculante, by name
static BUILTIN_LUTS: [(&str, &[u8]); 4] = [
    (
        "Fuji Superia 1600 2",
        include_bytes!("../res/LUT/Fuji Superia 1600 2.png") as &[u8],
    ),
    (
        "Lomography Redscale 100",
        include_bytes!("../res/LUT/Lomography Redscale 100.png") as &[u8],
    ),
    (
        "Lomography X-Pro Slide 200",
        include_bytes!("../res/LUT/Lomography X-Pro Slide 200.png") as &[u8],
    ),
    (
        "Polaroid Polachrome",
        include_bytes!("../res/LUT/Polaroid Polachrome.png") as &[u8],
    ),
];

pub fn builtin_luts() -> &'static [(&'static str, &'static [u8])] {
    &BUILTIN_LUTS
}

/// A Hald CLUT, checked and decoded
struct HaldClut {
    image: image::RgbImage,
    level: u8,
}

impl HaldClut {
    fn new(image: image::RgbImage) -> Result<Self> {
        let (w, h) = image.dimensions();
        // square, with the cube of the level as its side: 8, 27, 64 ... 512 ... 1728
        let level = (w as f64).cbrt().round() as u32;
        if w != h || !(2..=255).contains(&level) || level.pow(3) != w {
            bail!(
                "This is not a Hald CLUT: a {w}x{h} image, it has to be square with a side of 64, 512 or 1728 pixels or the like"
            );
        }
        Ok(Self {
            image,
            level: level as u8,
        })
    }

    fn map(&self, rgb: [u8; 3]) -> [u8; 3] {
        lutgen::identity::correct_pixel(&rgb, &self.image, self.level)
    }
}

/// Hald CLUTs, decoded once: a builtin one by its name, one from disk by its path
/// and when it was changed. A few at most, they are 9 MB each.
static HALD_CLUTS: LazyLock<Mutex<HashMap<String, Arc<HaldClut>>>> =
    LazyLock::new(Default::default);

/// A builtin LUT by its name, or a LUT from disk by its path
fn hald_clut(name: &str) -> Result<Arc<HaldClut>> {
    let builtin = BUILTIN_LUTS.iter().find(|l| l.0 == name).map(|l| l.1);
    let key = match builtin {
        Some(_) => name.to_string(),
        None => format!(
            "{name} {:?}",
            std::fs::metadata(name).and_then(|m| m.modified()).ok()
        ),
    };
    let cached = HALD_CLUTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .cloned();
    if let Some(lut) = cached {
        return Ok(lut);
    }
    let image = match builtin {
        Some(data) => image::load_from_memory(data),
        None => image::open(name),
    }
    .with_context(|| format!("Can't load the LUT {name}"))?;
    let lut = Arc::new(HaldClut::new(image.into_rgb8())?);
    let mut cache = HALD_CLUTS.lock().unwrap_or_else(|e| e.into_inner());
    if cache.len() >= 4 {
        cache.clear();
    }
    cache.insert(key, lut.clone());
    Ok(lut)
}

/// sRGB encoded to linear light
fn srgb_to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light to sRGB encoded
fn linear_to_srgb(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1. / 2.4) - 0.055
    }
}

/// An exposure: sRGB to linear light, times the factor, and back
fn exposure(v: f32, factor: f32) -> f32 {
    linear_to_srgb(srgb_to_linear(v) * factor)
}

/// A function of a channel from 0 to 1, worked out once in 4096 steps and
/// interpolated between them. Much faster than its powers for every pixel, and
/// off by less than a hundredth of a step of 8 bits for an exposure.
struct Curve(Vec<f32>);

impl Curve {
    const STEPS: usize = 4096;

    fn new(f: impl Fn(f32) -> f32) -> Self {
        Self(
            (0..=Self::STEPS)
                .map(|i| f(i as f32 / Self::STEPS as f32))
                .collect(),
        )
    }

    /// The function at `v`. Outside of 0 to 1 the curve does not know it, `exact`
    /// works it out.
    fn at(&self, v: f32, exact: impl Fn(f32) -> f32) -> f32 {
        if !(0.0..=1.0).contains(&v) {
            return exact(v);
        }
        let x = v * Self::STEPS as f32;
        let i = (x as usize).min(Self::STEPS - 1);
        self.0[i] + (self.0[i + 1] - self.0[i]) * (x - i as f32)
    }
}

/// A number from 0 to 1 that looks random, but is the same for the same pixel,
/// channel and seed every time (SplitMix64)
fn noise(pixel: u64, channel: u64, seed: u64) -> f32 {
    let mut x = (pixel.wrapping_mul(4) + channel) ^ seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// What an expression sees: r, g, b and a of the pixel, and the variables it sets
/// itself. Unlike in evalexpr's HashMapContext a variable can change its type,
/// `r = 1` works as `r = 1.0` does.
struct PixelVariables {
    rgba: [Value<DefaultNumericTypes>; 4],
    own: Vec<(String, Value<DefaultNumericTypes>)>,
}

impl PixelVariables {
    fn new() -> Self {
        Self {
            rgba: std::array::from_fn(|_| Value::Float(0.)),
            own: vec![],
        }
    }

    /// Runs the expression on the pixel, which keeps its color if it fails
    fn run(
        &mut self,
        expression: &Node<DefaultNumericTypes>,
        p: &mut Vector4<f32>,
    ) -> EvalexprResult<Value<DefaultNumericTypes>, DefaultNumericTypes> {
        for c in 0..4 {
            self.rgba[c] = Value::Float(p[c] as f64);
        }
        // nothing carries over from the pixel before
        self.own.clear();
        let result = expression.eval_with_context_mut(self);
        if result.is_ok() {
            for c in 0..4 {
                if let Ok(v) = self.rgba[c].as_number() {
                    p[c] = v as f32;
                }
            }
        }
        result
    }
}

impl Context for PixelVariables {
    type NumericTypes = DefaultNumericTypes;

    fn get_value(&self, identifier: &str) -> Option<&Value<DefaultNumericTypes>> {
        match identifier {
            "r" => Some(&self.rgba[0]),
            "g" => Some(&self.rgba[1]),
            "b" => Some(&self.rgba[2]),
            "a" => Some(&self.rgba[3]),
            _ => self.own.iter().find(|v| v.0 == identifier).map(|v| &v.1),
        }
    }

    fn call_function(
        &self,
        identifier: &str,
        _argument: &Value<DefaultNumericTypes>,
    ) -> EvalexprResult<Value<DefaultNumericTypes>, DefaultNumericTypes> {
        // only the builtin ones
        Err(EvalexprError::FunctionIdentifierNotFound(
            identifier.to_string(),
        ))
    }

    fn are_builtin_functions_disabled(&self) -> bool {
        false
    }

    fn set_builtin_functions_disabled(
        &mut self,
        disabled: bool,
    ) -> EvalexprResult<(), DefaultNumericTypes> {
        if disabled {
            Err(EvalexprError::BuiltinFunctionsCannotBeDisabled)
        } else {
            Ok(())
        }
    }
}

impl ContextWithMutableVariables for PixelVariables {
    fn set_value(
        &mut self,
        identifier: String,
        value: Value<DefaultNumericTypes>,
    ) -> EvalexprResult<(), DefaultNumericTypes> {
        let slot = match identifier.as_str() {
            "r" => &mut self.rgba[0],
            "g" => &mut self.rgba[1],
            "b" => &mut self.rgba[2],
            "a" => &mut self.rgba[3],
            _ => match self.own.iter().position(|v| v.0 == identifier) {
                Some(i) => &mut self.own[i].1,
                None => {
                    self.own.push((identifier, value));
                    return Ok(());
                }
            },
        };
        *slot = value;
        Ok(())
    }
}

/// A pixel operation made ready for a run over a whole image: what it needs is
/// parsed, decoded or worked out once, not for every pixel.
enum PixelOp<'a> {
    /// Needs nothing beforehand
    Plain(&'a ImageOperation),
    /// The factor for the light, 2 for one stop more, and the curve it makes
    Exposure {
        factor: f32,
        curve: Curve,
    },
    /// Steps from black to white, one less than the levels
    Posterize(f32),
    Expression(Node<DefaultNumericTypes>),
    Lut(Arc<HaldClut>),
    Noise {
        amount: f32,
        mono: bool,
        seed: u64,
    },
    /// Left out: an expression that does not parse yet, or a LUT that can't be
    /// loaded
    Nothing,
}

impl<'a> PixelOp<'a> {
    /// `index` is the place of the operation in the stack
    fn new(op: &'a ImageOperation, index: usize) -> Result<Self> {
        Ok(match op {
            ImageOperation::Exposure(amt) => {
                let factor = 2_f32.powf(*amt as f32 / 25.);
                Self::Exposure {
                    factor,
                    curve: Curve::new(|v| exposure(v, factor)),
                }
            }
            ImageOperation::Posterize(levels) => Self::Posterize((*levels).max(2) as f32 - 1.),
            // the panel says why it does not parse
            ImageOperation::Expression(expr) => build_operator_tree(expr)
                .map(Self::Expression)
                .unwrap_or(Self::Nothing),
            ImageOperation::LUT(name) => Self::Lut(hald_clut(name)?),
            // two noise operations give different noise
            ImageOperation::Noise { amt, mono } => Self::Noise {
                amount: *amt as f32 / 100.,
                mono: *mono,
                seed: index as u64,
            },
            op => Self::Plain(op),
        })
    }

    /// `pixel` is the number of the pixel in the image, for noise that stays the
    /// same from one run to the next
    fn apply(&self, p: &mut Vector4<f32>, pixel: u64, variables: &mut PixelVariables) {
        match self {
            Self::Plain(op) => op.process_pixel(p),
            // in linear light, where one stop is twice the light
            Self::Exposure { factor, curve } => {
                for c in 0..3 {
                    p[c] = curve.at(p[c], |v| exposure(v, *factor));
                }
            }
            Self::Posterize(steps) => {
                for c in 0..3 {
                    p[c] = (p[c] * steps).round() / steps;
                }
            }
            Self::Expression(expression) => {
                _ = variables.run(expression, p);
            }
            Self::Lut(lut) => {
                let rgb = lut.map([0, 1, 2].map(|c| (p[c] * 255.).round() as u8));
                for c in 0..3 {
                    p[c] = rgb[c] as f32 / 255.;
                }
            }
            Self::Noise { amount, mono, seed } => {
                for c in 0..3 {
                    let n = noise(pixel, if *mono { 0 } else { c as u64 }, *seed);
                    p[c] = egui::lerp(p[c]..=n, *amount);
                }
            }
            Self::Nothing => (),
        }
    }
}

/// Multiplies the color of every pixel by its alpha
fn premultiply(img: &mut RgbaImage) {
    img.par_chunks_mut(4).for_each(|px| {
        let a = px[3] as u32;
        for c in &mut px[..3] {
            *c = ((*c as u32 * a + 127) / 255) as u8;
        }
    });
}

/// Undoes [`premultiply`], as far as 8 bits allow
fn unpremultiply(img: &mut RgbaImage) {
    img.par_chunks_mut(4).for_each(|px| {
        let a = px[3] as u32;
        for c in &mut px[..3] {
            // without alpha the color stays 0
            if let Some(v) = (*c as u32 * 255 + a / 2).checked_div(a) {
                *c = v.min(255) as u8;
            }
        }
    });
}

/// Filters the color of an image with a 3x3 kernel, row by row in parallel. The
/// weights are divided by their sum, unless that is 0. Alpha stays as it is, so an
/// edge filter does not make the image transparent, and so do the outermost
/// pixels, which lack neighbours.
fn filter3x3(img: &RgbaImage, kernel: &[f32; 9]) -> RgbaImage {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut out = img.clone();
    if w < 3 || h < 3 {
        return out;
    }
    let sum: f32 = kernel.iter().sum();
    let scale = if sum == 0. { 1. } else { 1. / sum };
    let src = img.as_raw();
    let stride = w * 4;
    out.par_chunks_mut(stride)
        .enumerate()
        .skip(1)
        .take(h - 2)
        .for_each(|(y, row)| {
            let rows = [
                &src[(y - 1) * stride..y * stride],
                &src[y * stride..(y + 1) * stride],
                &src[(y + 1) * stride..(y + 2) * stride],
            ];
            for x in 1..w - 1 {
                for c in 0..3 {
                    let mut t = 0.;
                    for (ky, r) in rows.iter().enumerate() {
                        for kx in 0..3 {
                            t += kernel[ky * 3 + kx] * r[(x + kx - 1) * 4 + c] as f32;
                        }
                    }
                    // `as` keeps it from 0 to 255
                    row[x * 4 + c] = (t * scale).round() as u8;
                }
            }
        });
    out
}

/// The luma of a color, with the weights the image crate takes for gray
fn luma(p: &Vector4<f32>) -> f32 {
    p[0] * 0.2126 + p[1] * 0.7152 + p[2] * 0.0722
}

/// 0 to 1 as 0 to 255. `as` keeps it in that range, and makes NaN 0.
fn to_u8(v: f32) -> u8 {
    (v * 255.).round() as u8
}

/// Runs the operations over every pixel of `data`, in parallel. `load` makes RGBA
/// of a pixel and `store` writes it back.
fn run_pixel_ops<T: Send>(
    data: &mut [T],
    channels: usize,
    ops: &[PixelOp],
    load: impl Fn(&[T]) -> Vector4<f32> + Sync,
    store: impl Fn(Vector4<f32>, &mut [T]) + Sync,
) {
    data.par_chunks_mut(channels).enumerate().for_each_init(
        PixelVariables::new,
        |variables, (i, px)| {
            let mut p = load(px);
            for op in ops {
                op.apply(&mut p, i as u64, variables);
            }
            store(p, px);
        },
    );
}

pub fn process_pixels(dynimage: &mut DynamicImage, operators: &Vec<ImageOperation>) -> Result<()> {
    // Made ready once. An operation that can't be is left out, the others run,
    // and the error is returned after.
    let mut problem = None;
    let ops = operators
        .iter()
        .enumerate()
        .map(|(i, op)| {
            PixelOp::new(op, i).unwrap_or_else(|e| {
                problem.get_or_insert(e);
                PixelOp::Nothing
            })
        })
        .collect::<Vec<_>>();

    match dynimage {
        DynamicImage::ImageRgb8(buffer) => run_pixel_ops(
            buffer,
            3,
            &ops,
            |px| Vector4::new(px[0] as f32, px[1] as f32, px[2] as f32, 255.) / 255.,
            |p, px| {
                for c in 0..3 {
                    px[c] = to_u8(p[c]);
                }
            },
        ),
        DynamicImage::ImageRgba8(buffer) => run_pixel_ops(
            buffer,
            4,
            &ops,
            |px| Vector4::new(px[0] as f32, px[1] as f32, px[2] as f32, px[3] as f32) / 255.,
            |p, px| {
                for c in 0..4 {
                    px[c] = to_u8(p[c]);
                }
            },
        ),
        // Gray is the same in every channel, opaque without alpha. It was red
        // and transparent, and the alpha of gray with alpha came from green.
        DynamicImage::ImageLuma8(buffer) => run_pixel_ops(
            buffer,
            1,
            &ops,
            |px| {
                let v = px[0] as f32 / 255.;
                Vector4::new(v, v, v, 1.)
            },
            |p, px| px[0] = to_u8(luma(&p)),
        ),
        DynamicImage::ImageLumaA8(buffer) => run_pixel_ops(
            buffer,
            2,
            &ops,
            |px| {
                let v = px[0] as f32 / 255.;
                Vector4::new(v, v, v, px[1] as f32 / 255.)
            },
            |p, px| {
                px[0] = to_u8(luma(&p));
                px[1] = to_u8(p[3]);
            },
        ),
        // opaque, as for 8 bit RGB
        DynamicImage::ImageRgb32F(buffer) => run_pixel_ops(
            buffer,
            3,
            &ops,
            |px| Vector4::new(px[0], px[1], px[2], 1.0),
            |p, px| px.copy_from_slice(&[p[0], p[1], p[2]]),
        ),
        DynamicImage::ImageRgba32F(buffer) => run_pixel_ops(
            buffer,
            4,
            &ops,
            |px| Vector4::new(px[0], px[1], px[2], px[3]),
            |p, px| px.copy_from_slice(p.as_slice()),
        ),
        _ => {
            bail!("Pixel operators are not yet supported for this image type.");
        }
    }
    match problem {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Converts the image to another color type
fn convert_color_type(image: &mut DynamicImage, t: &ColorTypeExt) {
    match t {
        ColorTypeExt::L8 => {
            *image = DynamicImage::ImageLuma8(image.to_luma8());
        }
        ColorTypeExt::La8 => {
            *image = DynamicImage::ImageLumaA8(image.to_luma_alpha8());
        }
        ColorTypeExt::Rgb8 => {
            *image = DynamicImage::ImageRgb8(image.to_rgb8());
        }
        ColorTypeExt::Rgba8 => {
            *image = DynamicImage::ImageRgba8(image.to_rgba8());
        }
        ColorTypeExt::L16 => {
            *image = DynamicImage::ImageLuma16(image.to_luma16());
        }
        ColorTypeExt::La16 => {
            *image = DynamicImage::ImageLumaA16(image.to_luma_alpha16());
        }
        ColorTypeExt::Rgb16 => {
            *image = DynamicImage::ImageRgb16(image.to_rgb16());
        }
        ColorTypeExt::Rgba16 => {
            *image = DynamicImage::ImageRgba16(image.to_rgba16());
        }
        ColorTypeExt::Rgb32F => {
            *image = DynamicImage::ImageRgb32F(image.to_rgb32f());
        }
        ColorTypeExt::Rgba32F => {
            *image = DynamicImage::ImageRgba32F(image.to_rgba32f());
        }
    }
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

    let mut transform = transform;
    // Blocks at the right and bottom edge that are not complete can not be
    // moved to the left or the top. They would stay where they are, a strip of
    // the old image at an edge of the new one. They are left out instead, a few
    // pixels at most.
    transform.trim = true;

    // make sure crop is aligned to mcu bounds
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

fn interpolate_u8(data: &[GradientStop], pt: u8) -> [u8; 3] {
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

fn closest_pt(data: &[GradientStop], value: u8) -> usize {
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
            id: rand::rng().random::<u64>() as usize,
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
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("RUST_LOG", "debug") };
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

#[cfg(test)]
mod operation_tests {
    use super::*;
    use image::RgbaImage;

    /// 16x12 pixels with different values in every channel, the last pixel transparent
    fn sample() -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_fn(16, 12, |x, y| {
            let alpha = if (x, y) == (15, 11) { 0 } else { 255 };
            image::Rgba([
                (x * 16 + 5) as u8,
                (y * 21 + 3) as u8,
                (250 - x * 9 - y * 7) as u8,
                alpha,
            ])
        }))
    }

    fn apply(op: ImageOperation, image: &DynamicImage) -> DynamicImage {
        let mut out = image.clone();
        if op.is_per_pixel() {
            process_pixels(&mut out, &vec![op]).unwrap();
        } else {
            op.process_image(&mut out).unwrap();
        }
        out
    }

    /// Every pixel of `out` against `expected(pixel of input)`
    fn assert_each(op: ImageOperation, expected: impl Fn([u8; 4]) -> [u8; 4]) {
        let input = sample().to_rgba8();
        let out = apply(op.clone(), &DynamicImage::ImageRgba8(input.clone())).to_rgba8();
        for (x, y, p) in input.enumerate_pixels() {
            assert_eq!(
                out.get_pixel(x, y).0,
                expected(p.0),
                "{op:?} at {x},{y} from {:?}",
                p.0
            );
        }
    }

    fn scale(v: u8, f: f32) -> u8 {
        (v as f32 * f).round().clamp(0., 255.) as u8
    }

    #[test]
    fn neutral_settings_change_nothing() {
        let input = sample();
        for op in [
            ImageOperation::Brightness(0),
            ImageOperation::Exposure(0),
            ImageOperation::Contrast(0),
            ImageOperation::Equalize((0, 255)),
            ImageOperation::Desaturate(0),
            ImageOperation::Mult([255, 255, 255]),
            ImageOperation::Add([0, 0, 0]),
            ImageOperation::Fill([255, 0, 0, 0]),
            ImageOperation::Expression("r = r".into()),
            ImageOperation::ChannelSwap((Channel::Red, Channel::Red)),
            ImageOperation::Noise {
                amt: 0,
                mono: false,
            },
            ImageOperation::Blur(0),
            ImageOperation::Rotate(0),
            ImageOperation::Crop([0, 0, 0, 0]),
            ImageOperation::Filter3x3([0, 0, 0, 0, 100, 0, 0, 0, 0]),
            ImageOperation::Resize {
                dimensions: (0, 0),
                aspect: true,
                filter: ScaleFilter::Bilinear,
            },
        ] {
            assert_eq!(apply(op.clone(), &input), input, "{op:?}");
        }
        // HSL and back may differ by a rounding step
        let hsv = apply(ImageOperation::HSV((0, 100, 100)), &input).to_rgba8();
        for (a, b) in hsv.pixels().zip(input.to_rgba8().pixels()) {
            for c in 0..4 {
                assert!(
                    a.0[c].abs_diff(b.0[c]) <= 1,
                    "HSV: {:?} from {:?}",
                    a.0,
                    b.0
                );
            }
        }
    }

    #[test]
    fn pixel_operations_give_the_exact_values() {
        assert_each(ImageOperation::Invert, |p| {
            [255 - p[0], 255 - p[1], 255 - p[2], p[3]]
        });
        assert_each(ImageOperation::Brightness(30), |p| {
            [
                p[0].saturating_add(30),
                p[1].saturating_add(30),
                p[2].saturating_add(30),
                p[3],
            ]
        });
        assert_each(ImageOperation::Add([10, 20, 30]), |p| {
            [
                p[0].saturating_add(10),
                p[1].saturating_add(20),
                p[2].saturating_add(30),
                p[3],
            ]
        });
        assert_each(ImageOperation::Mult([255, 128, 0]), |p| {
            [p[0], scale(p[1], 128. / 255.), 0, p[3]]
        });
        assert_each(ImageOperation::Fill([10, 20, 30, 255]), |_| {
            [10, 20, 30, 255]
        });
        assert_each(
            ImageOperation::ChannelSwap((Channel::Red, Channel::Blue)),
            |p| [p[2], p[1], p[2], p[3]],
        );
        assert_each(ImageOperation::Expression("r = 1.0 - r".into()), |p| {
            [255 - p[0], p[1], p[2], p[3]]
        });
        assert_each(ImageOperation::Posterize(1), |p| {
            [
                scale(p[0], 1. / 255.) * 255,
                scale(p[1], 1. / 255.) * 255,
                scale(p[2], 1. / 255.) * 255,
                p[3],
            ]
        });
        assert_each(ImageOperation::Equalize((50, 100)), |p| {
            let f = |v: u8| (50. + v as f32 / 255. * 50.).round() as u8;
            [f(p[0]), f(p[1]), f(p[2]), p[3]]
        });
        // MMult multiplies by alpha: the transparent pixel turns black
        assert_each(ImageOperation::MMult, |p| {
            let a = p[3] as f32 / 255.;
            [scale(p[0], a), scale(p[1], a), scale(p[2], a), p[3]]
        });
        // gray as the luma of the color, the same weights as the gradient map
        assert_each(ImageOperation::Desaturate(100), |p| {
            let v = (p[0] as f32 * 0.299 + p[1] as f32 * 0.587 + p[2] as f32 * 0.114).round() as u8;
            [v, v, v, p[3]]
        });
    }

    /// Scaling to the range of the image stretches its darkest value to 0 and its
    /// brightest to 255
    #[test]
    fn scale_to_min_max() {
        let input = sample();
        let values = |img: &DynamicImage| {
            let rgba = img.to_rgba8();
            let all = rgba
                .pixels()
                .flat_map(|p| p.0[..3].to_vec())
                .collect::<Vec<_>>();
            (*all.iter().min().unwrap(), *all.iter().max().unwrap())
        };
        assert_eq!(values(&input), (3, 250));
        assert_eq!(
            values(&apply(ImageOperation::ScaleImageMinMax, &input)),
            (0, 255)
        );
    }

    /// A 3x3 filter leaves no black frame around the image
    #[test]
    fn filter_keeps_the_border() {
        let input = sample();
        let sharpened = apply(
            ImageOperation::Filter3x3([0, -100, 0, -100, 500, -100, 0, -100, 0]),
            &input,
        )
        .to_rgba8();
        let original = input.to_rgba8();
        for (x, y) in [(0, 0), (15, 0), (0, 11), (7, 0), (0, 5)] {
            assert_eq!(
                sharpened.get_pixel(x, y),
                original.get_pixel(x, y),
                "at {x},{y}"
            );
        }
    }

    /// Inverting twice gives the image back
    #[test]
    fn invert_twice_is_the_image() {
        let input = sample();
        let twice = apply(
            ImageOperation::Invert,
            &apply(ImageOperation::Invert, &input),
        );
        assert_eq!(twice, input);
    }

    /// Contrast pushes values away from the middle, the middle stays
    #[test]
    fn contrast_keeps_the_middle() {
        let gray = |v: u8| {
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, image::Rgba([v, v, v, 255])))
        };
        let out = |v: u8| {
            apply(ImageOperation::Contrast(60), &gray(v))
                .to_rgba8()
                .get_pixel(0, 0)
                .0[0]
        };
        assert!(out(64) < 64, "dark gets darker: {}", out(64));
        assert!(out(192) > 192, "bright gets brighter: {}", out(192));
        assert!(
            out(128).abs_diff(128) <= 1,
            "the middle stays: {}",
            out(128)
        );
        let less = |v: u8| {
            apply(ImageOperation::Contrast(-60), &gray(v))
                .to_rgba8()
                .get_pixel(0, 0)
                .0[0]
        };
        assert!(
            less(64) > 64 && less(192) < 192,
            "less contrast moves values to the middle"
        );
    }

    #[test]
    fn hsv_turns_red_into_cyan() {
        let red =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255])));
        let out = apply(ImageOperation::HSV((180, 100, 100)), &red)
            .to_rgba8()
            .get_pixel(0, 0)
            .0;
        assert_eq!(out, [0, 255, 255, 255]);
    }

    #[test]
    fn gradient_map_from_black_to_white_is_the_brightness() {
        let stops = vec![
            GradientStop::new(0, [0, 0, 0]),
            GradientStop::new(255, [255, 255, 255]),
        ];
        let out = apply(ImageOperation::GradientMap(stops), &sample()).to_rgba8();
        for p in out.pixels() {
            assert!(p.0[0] == p.0[1] && p.0[1] == p.0[2], "not gray: {:?}", p.0);
        }
    }

    #[test]
    fn slice_keeps_only_the_range() {
        let out = |v: u8| {
            let gray =
                DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, image::Rgba([v, v, v, 255])));
            apply(ImageOperation::Slice(128, 20, false), &gray)
                .to_rgba8()
                .get_pixel(0, 0)
                .0[0]
        };
        assert_eq!(out(128), 128);
        assert_eq!(out(60), 0);
        assert_eq!(out(220), 0);
    }

    /// Noise stays gray on a gray image when it is mono, and changes the image
    #[test]
    fn noise() {
        let gray = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            8,
            8,
            image::Rgba([100, 100, 100, 255]),
        ));
        let mono = apply(
            ImageOperation::Noise {
                amt: 50,
                mono: true,
            },
            &gray,
        )
        .to_rgba8();
        assert!(mono.pixels().all(|p| p.0[0] == p.0[1] && p.0[1] == p.0[2]));
        assert_ne!(DynamicImage::ImageRgba8(mono.clone()), gray);
        // the same every time, it does not flicker when another slider moves
        let again = apply(
            ImageOperation::Noise {
                amt: 50,
                mono: true,
            },
            &gray,
        );
        assert_eq!(again, DynamicImage::ImageRgba8(mono));
    }

    #[test]
    fn geometry() {
        let input = sample();
        let (w, h) = (input.width(), input.height());
        let corner = input.to_rgba8().get_pixel(0, 0).0;
        let at = |img: &DynamicImage, x: u32, y: u32| img.to_rgba8().get_pixel(x, y).0;

        let turned = apply(ImageOperation::Rotate(90), &input);
        assert_eq!((turned.width(), turned.height()), (h, w));
        assert_eq!(at(&turned, h - 1, 0), corner, "clockwise");
        let turned = apply(ImageOperation::Rotate(-90), &input);
        assert_eq!(at(&turned, 0, w - 1), corner, "counter clockwise");
        assert_eq!(apply(ImageOperation::Rotate(270), &input), turned);
        let turned = apply(ImageOperation::Rotate(180), &input);
        assert_eq!(at(&turned, w - 1, h - 1), corner);

        assert_eq!(
            at(&apply(ImageOperation::Flip(true), &input), 0, h - 1),
            corner,
            "vertical"
        );
        assert_eq!(
            at(&apply(ImageOperation::Flip(false), &input), w - 1, 0),
            corner,
            "horizontal"
        );

        // a quarter off every side, in ten thousandths
        let cropped = apply(ImageOperation::Crop([2500, 2500, 2500, 2500]), &input);
        assert_eq!((cropped.width(), cropped.height()), (8, 6));
        assert_eq!(at(&cropped, 0, 0), at(&input, 4, 3));
        assert_eq!(cropped_range(&[2500; 4], &(100, 100)), [25, 25, 50, 50]);

        // the whole image as the perspective crop keeps it
        let whole = apply(
            ImageOperation::CropPerspective {
                points: [(0, 0), (w, 0), (0, h), (w, h)],
                original_size: (w, h),
            },
            &input,
        );
        assert_eq!((whole.width(), whole.height()), (w, h));
    }

    /// Resize gives the size it is asked for, in every layout. Other layouts than
    /// RGBA used the width for the height too, and kept the aspect ratio.
    #[test]
    fn resize_gives_the_size_asked_for() {
        let resize = ImageOperation::Resize {
            dimensions: (7, 3),
            aspect: false,
            filter: ScaleFilter::Lanczos3,
        };
        let input = sample();
        for layout in [
            input.clone(),
            DynamicImage::ImageRgb8(input.to_rgb8()),
            DynamicImage::ImageLuma16(input.to_luma16()),
        ] {
            let out = apply(resize.clone(), &layout);
            assert_eq!((out.width(), out.height()), (7, 3), "{:?}", layout.color());
            assert_eq!(out.color(), layout.color());
        }
    }

    /// The conversion of the color type works on every layout. It did nothing on RGBA.
    #[test]
    fn color_conversion_on_every_layout() {
        let input = sample();
        for layout in [input.clone(), DynamicImage::ImageRgb8(input.to_rgb8())] {
            let out = apply(ImageOperation::ColorConverter(ColorTypeExt::L8), &layout);
            assert_eq!(
                out.color(),
                image::ColorType::L8,
                "from {:?}",
                layout.color()
            );
        }
    }

    /// The other whole-image operations change the image, keep its size and give
    /// the same result every time
    #[test]
    fn image_operations_are_deterministic() {
        // with structure, a sharpened gradient is the gradient
        let input = DynamicImage::ImageRgba8(RgbaImage::from_fn(16, 12, |x, y| {
            let v = ((x * 31 + y * 17) * 7 % 256) as u8;
            image::Rgba([
                v,
                255 - v,
                v / 2 + 40,
                if (x, y) == (15, 11) { 0 } else { 255 },
            ])
        }));
        for op in [
            ImageOperation::Blur(10),
            ImageOperation::Filter3x3([0, -100, 0, -100, 500, -100, 0, -100, 0]),
            ImageOperation::LUT("Fuji Superia 1600 2".into()),
            ImageOperation::ChromaticAberration(30),
        ] {
            let first = apply(op.clone(), &input);
            assert_eq!(
                (first.width(), first.height()),
                (input.width(), input.height()),
                "{op:?}"
            );
            assert_ne!(first, input, "{op:?} changed nothing");
            assert_eq!(
                apply(op.clone(), &input),
                first,
                "{op:?} differs from run to run"
            );
        }
        // a LUT keeps the alpha
        let lut = apply(ImageOperation::LUT("Fuji Superia 1600 2".into()), &input).to_rgba8();
        assert_eq!(lut.get_pixel(15, 11).0[3], 0);
    }

    fn gray(v: u8) -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, image::Rgba([v, v, v, 255])))
    }

    fn first_pixel(op: ImageOperation, image: &DynamicImage) -> [u8; 4] {
        apply(op, image).to_rgba8().get_pixel(0, 0).0
    }

    /// One stop more is twice the light, in linear light. Twice the sRGB value
    /// made a middle gray white.
    #[test]
    fn exposure_is_in_stops_of_light() {
        let out = |amount: i32, v: u8| first_pixel(ImageOperation::Exposure(amount), &gray(v))[0];
        assert_eq!(out(25, 128), 176);
        assert_eq!(out(25, 64), 90);
        assert_eq!(out(-25, 128), 92);
        assert_eq!(out(-25, 255), 188);
        assert_eq!(out(50, 128), 239);
        assert_eq!(out(25, 0), 0);
        assert_eq!(out(25, 200), 255);
    }

    /// The curve of an exposure is the exposure, near enough
    #[test]
    fn exposure_curve() {
        for amount in (-100..=100).step_by(10) {
            let PixelOp::Exposure { factor, curve } =
                PixelOp::new(&ImageOperation::Exposure(amount), 0).unwrap()
            else {
                panic!("not an exposure");
            };
            for i in -100..=10100 {
                let v = i as f32 / 10000.;
                let (exact, fast) = (exposure(v, factor), curve.at(v, |v| exposure(v, factor)));
                assert!(
                    (exact - fast).abs() < 0.01 / 255. * exact.max(1.),
                    "{amount} at {v}: {fast} for {exact}"
                );
            }
        }
    }

    /// Posterize gives as many levels as it says, it gave one more
    #[test]
    fn posterize_gives_the_levels_asked_for() {
        let ramp = DynamicImage::ImageRgba8(RgbaImage::from_fn(256, 1, |x, _| {
            image::Rgba([x as u8, x as u8, x as u8, 255])
        }));
        for levels in [2, 3, 8, 255] {
            let out = apply(ImageOperation::Posterize(levels), &ramp).to_rgba8();
            let mut values = out.pixels().map(|p| p.0[0]).collect::<Vec<_>>();
            values.dedup();
            assert_eq!(values.len(), levels as usize, "{values:?}");
            assert_eq!((values[0], values[values.len() - 1]), (0, 255));
        }
        // 1 was the setting for 2
        assert_eq!(
            apply(ImageOperation::Posterize(1), &ramp),
            apply(ImageOperation::Posterize(2), &ramp)
        );
    }

    /// Gray, with or without alpha, is gray in every channel for the pixel
    /// operations. It was red and transparent, and the alpha of gray with alpha
    /// came from green: every pixel operation made it transparent.
    #[test]
    fn pixel_operations_on_gray() {
        let l8 = DynamicImage::ImageLuma8(image::GrayImage::from_pixel(2, 2, image::Luma([200])));
        let la8 = DynamicImage::ImageLumaA8(image::GrayAlphaImage::from_pixel(
            2,
            2,
            image::LumaA([200, 180]),
        ));
        let stops = vec![
            GradientStop::new(0, [0, 0, 0]),
            GradientStop::new(255, [255, 255, 255]),
        ];
        for op in [
            ImageOperation::Desaturate(100),
            ImageOperation::HSV((0, 100, 100)),
            ImageOperation::Mult([255, 255, 255]),
            ImageOperation::GradientMap(stops),
        ] {
            let mut out = l8.clone();
            process_pixels(&mut out, &vec![op.clone()]).unwrap();
            assert!(
                out.as_luma8().unwrap().get_pixel(0, 0).0[0].abs_diff(200) <= 1,
                "{op:?} on gray: {:?}",
                out.as_luma8().unwrap().get_pixel(0, 0)
            );
            let mut out = la8.clone();
            process_pixels(&mut out, &vec![op.clone()]).unwrap();
            let p = out.as_luma_alpha8().unwrap().get_pixel(0, 0).0;
            assert!(
                p[0].abs_diff(200) <= 1 && p[1] == 180,
                "{op:?} on gray with alpha: {p:?}"
            );
        }
        // multiplied by an alpha of 1, not 0
        let mut out = l8.clone();
        process_pixels(&mut out, &vec![ImageOperation::MMult]).unwrap();
        assert_eq!(out.as_luma8().unwrap().get_pixel(0, 0).0[0], 200);
    }

    /// Whole numbers work, an expression can have its own variables, and one that
    /// does not parse changes nothing
    #[test]
    fn expressions() {
        let out = |e: &str| first_pixel(ImageOperation::Expression(e.into()), &gray(100));
        assert_eq!(out("r = 1"), [255, 100, 100, 255]);
        assert_eq!(out("g = 0; r = 1.0"), [255, 0, 100, 255]);
        assert_eq!(out("l = (r + g + b) / 3; r = l * 2"), [200, 100, 100, 255]);
        assert_eq!(out("r = min(r * 2, 1)"), [200, 100, 100, 255]);
        assert_eq!(out("r = "), [100, 100, 100, 255]);
        assert_eq!(out("r = true"), [100, 100, 100, 255]);
        // Nothing is left over from the pixel before: x is not there yet when r
        // takes it, for every pixel
        let input = sample();
        assert_eq!(
            apply(ImageOperation::Expression("r = x; x = 0.5".into()), &input),
            input
        );
    }

    /// Without alpha there is no color to get back, it stays. It turned white.
    #[test]
    fn divide_by_no_alpha() {
        let clear =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, image::Rgba([100, 50, 20, 0])));
        assert_eq!(first_pixel(ImageOperation::MDiv, &clear), [100, 50, 20, 0]);
    }

    /// A filter whose weights add up to 0 finds edges, and keeps the alpha. It
    /// made the image transparent.
    #[test]
    fn edge_filter_keeps_the_alpha() {
        let input = sample();
        let edges = apply(
            ImageOperation::Filter3x3([-100, -100, -100, -100, 800, -100, -100, -100, -100]),
            &input,
        )
        .to_rgba8();
        for (x, y, p) in edges.enumerate_pixels() {
            assert_eq!(p.0[3], input.to_rgba8().get_pixel(x, y).0[3], "at {x},{y}");
        }
        // the sample is a gradient, its inside has no edges
        assert_eq!(edges.get_pixel(5, 5).0[..3], [0, 0, 0]);
    }

    /// The 3x3 filter rounds as the image crate did not, a blur of an even
    /// color is that color
    #[test]
    fn filter_of_an_even_color() {
        let even =
            DynamicImage::ImageRgba8(RgbaImage::from_pixel(5, 5, image::Rgba([201, 7, 99, 255])));
        let blurred = apply(
            ImageOperation::Filter3x3([6, 12, 6, 12, 25, 12, 6, 12, 6]),
            &even,
        );
        assert_eq!(blurred, even);
    }

    /// Blur next to transparent pixels keeps the color, it turned gray
    #[test]
    fn blur_next_to_transparent() {
        let half = DynamicImage::ImageRgba8(RgbaImage::from_fn(64, 8, |x, _| {
            if x < 32 {
                image::Rgba([0, 0, 0, 0])
            } else {
                image::Rgba([255, 255, 255, 255])
            }
        }));
        let out = apply(ImageOperation::Blur(10), &half).to_rgba8();
        for x in 28..36 {
            let p = out.get_pixel(x, 4).0;
            if p[3] > 20 {
                assert!(p[..3].iter().all(|c| *c >= 250), "at {x}: {p:?}");
            }
        }
        assert!(out.get_pixel(31, 4).0[3] > 0, "it blurred");
    }

    /// A LUT has to be a Hald CLUT. Any other picture crashed the app.
    #[test]
    fn a_lut_that_is_not_one() {
        let dir = std::env::temp_dir().join(format!("oculante-lut-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let photo = dir.join("photo.png");
        image::RgbImage::from_pixel(100, 80, image::Rgb([1, 2, 3]))
            .save(&photo)
            .unwrap();
        let op = ImageOperation::LUT(photo.to_string_lossy().into());
        let mut image = sample();
        let error = process_pixels(&mut image, &vec![op.clone(), ImageOperation::Invert])
            .unwrap_err()
            .to_string();
        assert!(error.contains("not a Hald CLUT"), "{error}");
        // the other operations still ran
        assert_eq!(image, apply(ImageOperation::Invert, &sample()));
        // in the stack of edits saved before, too
        assert!(op.process_image(&mut sample()).is_err());
        assert!(
            ImageOperation::LUT(dir.join("missing.png").to_string_lossy().into())
                .process_image(&mut sample())
                .is_err()
        );
        _ = std::fs::remove_dir_all(dir);
    }

    /// The LUT is a pixel operation now, it takes its place among them: after an
    /// exposure that comes before it. Edits saved with it among the image
    /// operations give the same.
    #[test]
    fn lut_among_the_pixel_operations() {
        let lut = ImageOperation::LUT("Polaroid Polachrome".into());
        assert!(lut.is_per_pixel());
        let mut as_pixel_op = sample();
        process_pixels(&mut as_pixel_op, &vec![lut.clone()]).unwrap();
        let mut as_image_op = sample();
        lut.process_image(&mut as_image_op).unwrap();
        assert_eq!(as_pixel_op, as_image_op);

        let mut exposed_first = sample();
        process_pixels(
            &mut exposed_first,
            &vec![ImageOperation::Exposure(25), lut.clone()],
        )
        .unwrap();
        let mut lut_first = sample();
        process_pixels(&mut lut_first, &vec![lut, ImageOperation::Exposure(25)]).unwrap();
        assert_ne!(exposed_first, lut_first);
    }

    /// Corners dragged past each other mirror the crop. The size went below zero:
    /// a crash.
    #[test]
    fn perspective_crop_with_crossed_corners() {
        let input = sample();
        let crop = |points| {
            let mut out = input.clone();
            ImageOperation::CropPerspective {
                points,
                original_size: (16, 12),
            }
            .process_image(&mut out)
            .map(|_| out)
        };
        // a rectangle is that part of the image
        let part = crop([(2, 2), (10, 2), (2, 8), (10, 8)]).unwrap().to_rgba8();
        assert_eq!(part.dimensions(), (8, 6));
        assert_eq!(part.get_pixel(0, 0), input.to_rgba8().get_pixel(2, 2));
        // the right corners left of the left ones
        let mirrored = crop([(10, 2), (2, 2), (10, 8), (2, 8)]).unwrap().to_rgba8();
        assert_eq!(mirrored.dimensions(), (8, 6));
        // corners on one line give an error
        assert!(crop([(0, 0), (16, 0), (0, 0), (16, 0)]).is_err());
    }
}
