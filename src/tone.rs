//! Levels and Curves, as in Photoshop: for all colors and for red, green and blue
//! on their own. A channel's own setting is applied first, then the one for all
//! colors, as in GIMP.

use std::sync::Arc;

use egui::{Color32, Id, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, pos2, vec2};
use image::{DynamicImage, GenericImageView};
use serde::{Deserialize, Serialize};

/// The channels of a setting: 0 is all colors, 1 to 3 red, green and blue
pub const CHANNELS: [&str; 4] = ["RGB", "Red", "Green", "Blue"];

/// Under this id the edit panel keeps the histogram of the image the pixel
/// operations start from, for the panels of Levels and Curves
pub const HISTOGRAM: &str = "tone_histogram";

/// The most points of a curve, the two ends included, as in Photoshop
pub const MAX_POINTS: usize = 16;

/// Levels of one channel. Input black and white become output black and white,
/// the midtones bend with gamma.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize, Deserialize)]
pub struct Levels {
    pub black: u8,
    pub white: u8,
    /// In hundredths, 100 changes nothing. 0.10 to 9.99 as in Photoshop, more
    /// brightens the midtones.
    pub gamma: u16,
    pub out_black: u8,
    pub out_white: u8,
}

impl Default for Levels {
    fn default() -> Self {
        Self {
            black: 0,
            white: 255,
            gamma: 100,
            out_black: 0,
            out_white: 255,
        }
    }
}

impl Levels {
    pub const GAMMA: std::ops::RangeInclusive<u16> = 10..=999;

    /// The value from 0 to 1 of `v`, also from 0 to 1. Values outside are those
    /// of the ends.
    pub fn map(&self, v: f32) -> f32 {
        let black = self.black as f32 / 255.;
        let white = self.white.max(self.black.saturating_add(1)) as f32 / 255.;
        let t = ((v - black) / (white - black)).clamp(0., 1.);
        let t = t.powf(100. / self.gamma.clamp(*Self::GAMMA.start(), *Self::GAMMA.end()) as f32);
        let (out_black, out_white) = (self.out_black as f32 / 255., self.out_white as f32 / 255.);
        out_black + t * (out_white - out_black)
    }

    /// Where the midtone handle is, from 0 at black to 1 at white: the input that
    /// comes out half way between the output black and white
    fn midtone(&self) -> f32 {
        0.5_f32.powf(self.gamma as f32 / 100.)
    }

    fn set_midtone(&mut self, t: f32) {
        let t = t.clamp(0.001, 0.999);
        let gamma = (t.ln() / 0.5_f32.ln() * 100.).round() as u16;
        self.gamma = gamma.clamp(*Self::GAMMA.start(), *Self::GAMMA.end());
    }
}

/// The points of a curve, input and output from 0 to 255, as in Photoshop
pub fn straight_curve() -> Vec<(u8, u8)> {
    vec![(0, 0), (255, 255)]
}

/// A curve through its points, a natural cubic spline as in Photoshop. Before the
/// first point and after the last it stays at their output.
pub struct Spline {
    x: Vec<f32>,
    y: Vec<f32>,
    /// The second derivative at each point
    m: Vec<f32>,
}

impl Spline {
    pub fn new(points: &[(u8, u8)]) -> Self {
        let mut points = points.to_vec();
        points.sort();
        points.dedup_by_key(|p| p.0);
        if points.is_empty() {
            points = straight_curve();
        }
        let x: Vec<f32> = points.iter().map(|p| p.0 as f32 / 255.).collect();
        let y: Vec<f32> = points.iter().map(|p| p.1 as f32 / 255.).collect();
        let n = x.len();
        let mut m = vec![0.; n];
        if n > 2 {
            // the tridiagonal system of a natural spline, the ends without bend
            let mut c = vec![0.; n];
            let mut d = vec![0.; n];
            for i in 1..n - 1 {
                let (h0, h1) = (x[i] - x[i - 1], x[i + 1] - x[i]);
                let a = h0;
                let b = 2. * (h0 + h1);
                let r = 6. * ((y[i + 1] - y[i]) / h1 - (y[i] - y[i - 1]) / h0);
                let denominator = b - a * c[i - 1];
                c[i] = h1 / denominator;
                d[i] = (r - a * d[i - 1]) / denominator;
            }
            for i in (1..n - 1).rev() {
                m[i] = d[i] - c[i] * m[i + 1];
            }
        }
        Self { x, y, m }
    }

    /// The output from 0 to 1 for `v` from 0 to 1
    pub fn at(&self, v: f32) -> f32 {
        let n = self.x.len();
        if n == 1 || v <= self.x[0] {
            return self.y[0];
        }
        if v >= self.x[n - 1] {
            return self.y[n - 1];
        }
        let i = self.x.partition_point(|&x| x <= v).clamp(1, n - 1) - 1;
        let h = self.x[i + 1] - self.x[i];
        let a = (self.x[i + 1] - v) / h;
        let b = (v - self.x[i]) / h;
        let y = a * self.y[i]
            + b * self.y[i + 1]
            + ((a * a * a - a) * self.m[i] + (b * b * b - b) * self.m[i + 1]) * h * h / 6.;
        y.clamp(0., 1.)
    }
}

/// How often each value of 0 to 255 is in an image: of the brightness, and of
/// red, green and blue
#[derive(Debug, Clone, PartialEq)]
pub struct Histogram(pub [[u32; 256]; 4]);

impl Histogram {
    /// About this many pixels are counted, spread over the image. The shape of the
    /// histogram is the same and a large image does not take long.
    const SAMPLES: u64 = 250_000;

    pub fn of(image: &DynamicImage) -> Self {
        let mut bins = [[0; 256]; 4];
        let (width, height) = image.dimensions();
        let pixels = width as u64 * height as u64;
        let step = ((pixels as f64 / Self::SAMPLES as f64).sqrt().ceil() as u32).max(1);
        for y in (0..height).step_by(step as usize) {
            for x in (0..width).step_by(step as usize) {
                let [r, g, b, _] = image.get_pixel(x, y).0;
                let luma = (r as f32 * 0.2126 + g as f32 * 0.7152 + b as f32 * 0.0722).round();
                bins[0][luma as usize] += 1;
                bins[1][r as usize] += 1;
                bins[2][g as usize] += 1;
                bins[3][b as usize] += 1;
            }
        }
        Self(bins)
    }
}

/// The color a channel is drawn in
fn channel_color(ui: &Ui, channel: usize) -> Color32 {
    match channel {
        1 => Color32::from_rgb(235, 80, 80),
        2 => Color32::from_rgb(80, 200, 90),
        3 => Color32::from_rgb(90, 140, 245),
        _ => ui.visuals().strong_text_color(),
    }
}

/// The channel picked in the panel of an operation, kept by egui and not saved
fn channel_picker(ui: &mut Ui, item_id: u64) -> usize {
    let id = Id::new(("tone_channel", item_id));
    let mut channel = ui.ctx().data(|d| d.get_temp::<usize>(id)).unwrap_or(0);
    ui.horizontal(|ui| {
        for (i, name) in CHANNELS.iter().enumerate() {
            ui.selectable_value(&mut channel, i, *name);
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(id, channel));
    channel
}

/// The histogram of the channel behind the controls, if the edit panel made one
fn paint_histogram(ui: &Ui, rect: Rect, channel: usize) {
    let Some(histogram) = ui
        .ctx()
        .data(|d| d.get_temp::<Arc<Histogram>>(Id::new(HISTOGRAM)))
    else {
        return;
    };
    let bins = &histogram.0[channel];
    // the ends are often full of clipped pixels, they would flatten the rest
    let tallest = bins[1..255].iter().copied().max().unwrap_or(0).max(1) as f32;
    let color = channel_color(ui, channel).gamma_multiply(0.35);
    let width = rect.width() / 256.;
    for (i, &count) in bins.iter().enumerate() {
        let height = (count as f32 / tallest).min(1.) * rect.height();
        if height < 0.5 {
            continue;
        }
        let x = rect.left() + i as f32 * width;
        ui.painter().rect_filled(
            Rect::from_min_max(
                pos2(x, rect.bottom() - height),
                pos2(x + width + 0.5, rect.bottom()),
            ),
            0.,
            color,
        );
    }
}

/// A triangle below a value, pointing up at it
fn paint_handle(ui: &Ui, tip: Pos2, fill: Color32, active: bool) {
    let size = if active { 7. } else { 6. };
    let points = vec![
        tip,
        pos2(tip.x - size, tip.y + size * 1.6),
        pos2(tip.x + size, tip.y + size * 1.6),
    ];
    ui.painter().add(Shape::convex_polygon(
        points,
        fill,
        Stroke::new(1., ui.visuals().strong_text_color()),
    ));
}

/// A bar from one color to another
fn paint_gradient(ui: &Ui, rect: Rect, from: Color32, to: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), from);
    mesh.colored_vertex(rect.left_bottom(), from);
    mesh.colored_vertex(rect.right_top(), to);
    mesh.colored_vertex(rect.right_bottom(), to);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    ui.painter().add(Shape::mesh(mesh));
}

/// Handles that move along a strip below a bar. `values` are from 0 to 1, the
/// handle that is dragged is returned with its new place.
fn handle_strip(
    ui: &mut Ui,
    width: f32,
    values: &[f32],
    fills: &[Color32],
    drag_id: Id,
) -> Option<(usize, f32)> {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 14.), Sense::drag());
    let x_of = |v: f32| rect.left() + v * rect.width();
    let dragged = ui.ctx().data(|d| d.get_temp::<usize>(drag_id));
    if response.drag_started()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        // the nearest, the last of those on the same place so that black and
        // white can be pulled apart when they meet
        let nearest = (0..values.len()).rev().min_by(|&a, &b| {
            (x_of(values[a]) - pointer.x)
                .abs()
                .total_cmp(&(x_of(values[b]) - pointer.x).abs())
        });
        if let Some(i) = nearest {
            ui.ctx().data_mut(|d| d.insert_temp(drag_id, i));
        }
    }
    if response.drag_stopped() {
        ui.ctx().data_mut(|d| d.remove::<usize>(drag_id));
    }
    for (i, &v) in values.iter().enumerate() {
        paint_handle(ui, pos2(x_of(v), rect.top()), fills[i], dragged == Some(i));
    }
    let i = dragged?;
    let pointer = response.interact_pointer_pos()?;
    response
        .dragged()
        .then(|| (i, ((pointer.x - rect.left()) / rect.width()).clamp(0., 1.)))
}

/// Number fields that edit a value of 0 to 255
fn byte_field(ui: &mut Ui, value: &mut u8, range: std::ops::RangeInclusive<u8>) -> bool {
    ui.add(egui::DragValue::new(value).range(range)).changed()
}

pub fn levels_ui(ui: &mut Ui, levels: &mut [Levels; 4], item_id: u64) -> Response {
    let mut response = ui.allocate_response(vec2(0., 0.), Sense::hover());
    let channel = channel_picker(ui, item_id);
    let level = &mut levels[channel];
    let width = ui.available_width();
    let color = channel_color(ui, channel);

    let (histogram, _) = ui.allocate_exact_size(vec2(width, 80.), Sense::hover());
    ui.painter()
        .rect_filled(histogram, 2., ui.visuals().extreme_bg_color);
    paint_histogram(ui, histogram, channel);

    let midtone = level.black as f32 / 255.
        + level.midtone() * (level.white as f32 - level.black as f32) / 255.;
    let gray = Color32::from_gray(128);
    if let Some((handle, v)) = handle_strip(
        ui,
        width,
        &[
            level.black as f32 / 255.,
            midtone,
            level.white as f32 / 255.,
        ],
        &[Color32::BLACK, gray, Color32::WHITE],
        Id::new(("levels_input", item_id)),
    ) {
        let value = (v * 255.).round() as u8;
        match handle {
            0 => level.black = value.min(level.white.saturating_sub(2)),
            2 => level.white = value.max(level.black.saturating_add(2)),
            _ => {
                let span = (level.white as f32 - level.black as f32).max(1.);
                level.set_midtone((v * 255. - level.black as f32) / span);
            }
        }
        response.mark_changed();
    }
    ui.horizontal(|ui| {
        ui.label("Input");
        let white = level.white;
        if byte_field(ui, &mut level.black, 0..=white.saturating_sub(2)) {
            response.mark_changed();
        }
        let mut gamma = level.gamma as f32 / 100.;
        if ui
            .add(
                egui::DragValue::new(&mut gamma)
                    .range(0.1..=9.99)
                    .speed(0.01)
                    .fixed_decimals(2),
            )
            .on_hover_text("Midtones, more than 1 brightens them")
            .changed()
        {
            level.gamma = (gamma * 100.).round() as u16;
            response.mark_changed();
        }
        let black = level.black;
        if byte_field(ui, &mut level.white, black.saturating_add(2)..=255) {
            response.mark_changed();
        }
    });

    ui.add_space(4.);
    let (bar, _) = ui.allocate_exact_size(vec2(width, 10.), Sense::hover());
    let end = if channel == 0 { Color32::WHITE } else { color };
    paint_gradient(ui, bar, Color32::BLACK, end);
    if let Some((handle, v)) = handle_strip(
        ui,
        width,
        &[level.out_black as f32 / 255., level.out_white as f32 / 255.],
        &[Color32::BLACK, Color32::WHITE],
        Id::new(("levels_output", item_id)),
    ) {
        let value = (v * 255.).round() as u8;
        if handle == 0 {
            level.out_black = value;
        } else {
            level.out_white = value;
        }
        response.mark_changed();
    }
    ui.horizontal(|ui| {
        ui.label("Output");
        if byte_field(ui, &mut level.out_black, 0..=255) {
            response.mark_changed();
        }
        if byte_field(ui, &mut level.out_white, 0..=255) {
            response.mark_changed();
        }
        if *level != Levels::default() && ui.button("Reset").clicked() {
            *level = Levels::default();
            response.mark_changed();
        }
    });
    response
}

pub fn curves_ui(ui: &mut Ui, curves: &mut [Vec<(u8, u8)>; 4], item_id: u64) -> Response {
    let mut response = ui.allocate_response(vec2(0., 0.), Sense::hover());
    let channel = channel_picker(ui, item_id);
    let side = ui.available_width();
    let (rect, graph) = ui.allocate_exact_size(vec2(side, side), Sense::click_and_drag());
    let selected_id = Id::new(("curve_point", item_id, channel));
    let mut selected = ui.ctx().data(|d| d.get_temp::<usize>(selected_id));

    let to_screen = |(x, y): (f32, f32)| {
        pos2(
            rect.left() + x * rect.width(),
            rect.bottom() - y * rect.height(),
        )
    };
    let to_curve = |p: Pos2| {
        let x = ((p.x - rect.left()) / rect.width()).clamp(0., 1.);
        let y = ((rect.bottom() - p.y) / rect.height()).clamp(0., 1.);
        ((x * 255.).round() as u8, (y * 255.).round() as u8)
    };

    // a point to drag: the one near the pointer, or a new one there
    let points = &mut curves[channel];
    if points.len() < 2 {
        *points = straight_curve();
    }
    let near = |points: &[(u8, u8)], pointer: Pos2| {
        points
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| {
                (
                    i,
                    to_screen((x as f32 / 255., y as f32 / 255.)).distance(pointer),
                )
            })
            .filter(|(_, distance)| *distance < 10.)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    if (graph.drag_started() || graph.clicked())
        && let Some(pointer) = graph.interact_pointer_pos()
    {
        selected = near(points, pointer);
        if selected.is_none() && points.len() < MAX_POINTS {
            let point = to_curve(pointer);
            // a point needs its own input, next to another it is that one
            match points.binary_search_by_key(&point.0, |p| p.0) {
                Ok(i) => selected = Some(i),
                Err(i) => {
                    points.insert(i, point);
                    selected = Some(i);
                    response.mark_changed();
                }
            }
        }
    }
    if graph.secondary_clicked()
        && let Some(pointer) = graph.interact_pointer_pos()
        && let Some(i) = near(points, pointer)
        && i != 0
        && i != points.len() - 1
    {
        points.remove(i);
        selected = None;
        response.mark_changed();
    }
    if graph.dragged()
        && let Some(i) = selected.filter(|&i| i < points.len())
        && let Some(pointer) = graph.interact_pointer_pos()
    {
        let last = points.len() - 1;
        // dragged off the graph, a point in between goes, as in Photoshop
        if i != 0 && i != last && !rect.expand(24.).contains(pointer) {
            points.remove(i);
            selected = None;
        } else {
            let (x, y) = to_curve(pointer);
            let low = if i == 0 {
                0
            } else {
                points[i - 1].0.saturating_add(1)
            };
            let high = if i == last {
                255
            } else {
                points[i + 1].0.saturating_sub(1)
            };
            points[i] = (x.clamp(low, high.max(low)), y);
        }
        response.mark_changed();
    }
    ui.ctx().data_mut(|d| match selected {
        Some(i) => _ = d.insert_temp(selected_id, i),
        None => d.remove::<usize>(selected_id),
    });

    // the graph
    let painter = ui.painter_at(rect.expand(6.));
    painter.rect_filled(rect, 2., ui.visuals().extreme_bg_color);
    paint_histogram(ui, rect, channel);
    let faint = Stroke::new(1., ui.visuals().weak_text_color().gamma_multiply(0.4));
    for i in 1..4 {
        let t = i as f32 / 4.;
        painter.line_segment([to_screen((t, 0.)), to_screen((t, 1.))], faint);
        painter.line_segment([to_screen((0., t)), to_screen((1., t))], faint);
    }
    painter.line_segment([to_screen((0., 0.)), to_screen((1., 1.))], faint);
    let line = |points: &[(u8, u8)]| {
        let spline = Spline::new(points);
        let steps = (rect.width() as usize / 2).max(32);
        (0..=steps)
            .map(|s| {
                let x = s as f32 / steps as f32;
                to_screen((x, spline.at(x)))
            })
            .collect::<Vec<_>>()
    };
    // the other channels that are bent, faintly
    for (other, points) in curves.iter().enumerate() {
        if other != channel && *points != straight_curve() {
            let stroke = Stroke::new(1., channel_color(ui, other).gamma_multiply(0.5));
            painter.add(Shape::line(line(points), stroke));
        }
    }
    let points = &mut curves[channel];
    painter.add(Shape::line(
        line(points),
        Stroke::new(2., channel_color(ui, channel)),
    ));
    for (i, &(x, y)) in points.iter().enumerate() {
        let center = to_screen((x as f32 / 255., y as f32 / 255.));
        let stroke = Stroke::new(1.5, ui.visuals().strong_text_color());
        if selected == Some(i) {
            painter.circle(center, 5., ui.visuals().strong_text_color(), stroke);
        } else {
            painter.circle(center, 4., ui.visuals().extreme_bg_color, stroke);
        }
    }

    ui.horizontal(|ui| {
        if let Some(i) = selected.filter(|&i| i < points.len()) {
            let last = points.len() - 1;
            let low = if i == 0 {
                0
            } else {
                points[i - 1].0.saturating_add(1)
            };
            let high = if i == last {
                255
            } else {
                points[i + 1].0.saturating_sub(1)
            };
            ui.label("Input");
            if byte_field(ui, &mut points[i].0, low..=high.max(low)) {
                response.mark_changed();
            }
            ui.label("Output");
            if byte_field(ui, &mut points[i].1, 0..=255) {
                response.mark_changed();
            }
        } else {
            ui.weak("Click to add a point").on_hover_text(
                "Drag a point to move it, off the graph or with a right click to remove it",
            );
        }
        if *points != straight_curve() && ui.button("Reset").clicked() {
            *points = straight_curve();
            ui.ctx().data_mut(|d| d.remove::<usize>(selected_id));
            response.mark_changed();
        }
    });
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn byte(v: f32) -> u8 {
        (v * 255.).round() as u8
    }

    #[test]
    fn levels_stretch_from_black_to_white() {
        let levels = Levels {
            black: 50,
            white: 200,
            ..Default::default()
        };
        for (input, output) in [(50, 0), (200, 255), (126, 129), (10, 0), (250, 255)] {
            assert_eq!(byte(levels.map(input as f32 / 255.)), output, "{input}");
        }
    }

    #[test]
    fn levels_gamma_bends_the_midtones() {
        let at = |gamma, v: f32| {
            byte(
                Levels {
                    gamma,
                    ..Default::default()
                }
                .map(v),
            )
        };
        assert_eq!(at(200, 0.5), 180, "0.5 to the power of 1/2");
        assert_eq!(at(50, 0.5), 64, "0.5 squared");
        for gamma in [10, 50, 200, 999] {
            assert_eq!(
                (at(gamma, 0.), at(gamma, 1.)),
                (0, 255),
                "the ends stay, {gamma}"
            );
        }
    }

    #[test]
    fn levels_output_range() {
        let levels = Levels {
            out_black: 20,
            out_white: 220,
            ..Default::default()
        };
        assert_eq!([0., 0.5, 1.].map(|v| byte(levels.map(v))), [20, 120, 220]);
        // swapped, it inverts
        let inverted = Levels {
            out_black: 255,
            out_white: 0,
            ..Default::default()
        };
        assert_eq!(byte(inverted.map(0.2)), 204);
    }

    #[test]
    fn the_midtone_handle_stays_where_it_is_put() {
        for t in [0.1, 0.3, 0.5, 0.8, 0.9] {
            let mut levels = Levels::default();
            levels.set_midtone(t);
            assert!(
                (levels.midtone() - t).abs() < 0.01,
                "{t}: {}",
                levels.midtone()
            );
        }
        let mut levels = Levels::default();
        levels.set_midtone(0.5);
        assert_eq!(levels.gamma, 100);
    }

    #[test]
    fn a_straight_curve_changes_nothing() {
        let spline = Spline::new(&straight_curve());
        for v in 0..=255 {
            assert_eq!(byte(spline.at(v as f32 / 255.)), v);
        }
    }

    #[test]
    fn a_curve_goes_through_its_points() {
        let points = [(0, 0), (64, 100), (128, 128), (192, 220), (255, 255)];
        let spline = Spline::new(&points);
        for (x, y) in points {
            assert_eq!(byte(spline.at(x as f32 / 255.)), y, "at {x}");
        }
        // in any order
        let mut shuffled = points.to_vec();
        shuffled.reverse();
        assert_eq!(Spline::new(&shuffled).at(0.3), spline.at(0.3));
    }

    #[test]
    fn a_curve_is_flat_beyond_its_ends() {
        let spline = Spline::new(&[(64, 0), (192, 255)]);
        assert_eq!(spline.at(0.1), 0.);
        assert_eq!(spline.at(0.9), 1.);
        assert_eq!(byte(spline.at(128. / 255.)), 128);
    }

    #[test]
    fn a_steep_curve_stays_from_0_to_1() {
        let spline = Spline::new(&[(0, 0), (30, 255), (60, 0), (255, 255)]);
        for v in 0..=255 {
            let out = spline.at(v as f32 / 255.);
            assert!((0. ..=1.).contains(&out), "{v}: {out}");
        }
    }

    #[test]
    fn histogram_of_each_channel() {
        let image =
            DynamicImage::ImageRgb8(image::RgbImage::from_pixel(10, 10, image::Rgb([255, 0, 0])));
        let histogram = Histogram::of(&image);
        assert_eq!(histogram.0[1][255], 100, "red");
        assert_eq!(histogram.0[2][0], 100, "green");
        assert_eq!(histogram.0[3][0], 100, "blue");
        assert_eq!(histogram.0[0][54], 100, "brightness: a fifth of red");
    }

    #[test]
    fn a_large_image_is_sampled() {
        let image =
            DynamicImage::ImageLuma8(image::GrayImage::from_pixel(4000, 3000, image::Luma([77])));
        let counted: u32 = Histogram::of(&image).0[0].iter().sum();
        assert!(
            (100_000..=Histogram::SAMPLES as u32).contains(&counted),
            "{counted}"
        );
        assert_eq!(
            Histogram::of(&image).0[1][77],
            counted,
            "all of them gray 77"
        );
    }
}
