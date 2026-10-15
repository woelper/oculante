use egui::Pos2;
use image::RgbaImage;
use image::math::Rect;
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PaintStroke {
    pub points: Vec<(f32, f32)>,
    pub fade: bool,
    pub color: [f32; 4],
    /// brush width from 0-1. 1 is equal to 1/10th of the smallest image dimension.
    pub width: f32,
    pub brush_index: usize,
    /// For ui preview: if highlit, paint brush stroke differently
    pub highlight: bool,
    /// Finished, and without the paint history it is no longer in the list of strokes
    pub committed: bool,
    pub flip_random: bool,
}

impl PaintStroke {
    pub fn without_points(&self) -> Self {
        Self {
            points: vec![],
            ..self.clone()
        }
    }

    pub fn new() -> Self {
        Self {
            color: [1., 1., 1., 1.],
            width: 0.05,
            ..Default::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    // render brush stroke
    pub fn render(&self, img: &mut RgbaImage, brushes: &[RgbaImage]) {
        let mut drawing = StrokeDrawing::start(self, img, brushes);
        drawing.walk(self);
        drawing.draw_dots(self, img, 0);
    }

    /// The color the brush is tinted with, brighter if highlit
    fn brush_color(&self) -> [f32; 4] {
        let mut color = self.color;
        if self.highlight {
            for c in &mut color {
                *c *= 2.5;
            }
        }
        color
    }
}

/// A stroke as far as it is drawn. While a stroke grows, only its new dots are drawn,
/// it does not start over with every point. The dots of a stroke stay where they are
/// when points are added, only a fading stroke changes all of them.
#[derive(Debug, Clone, Default)]
pub struct StrokeDrawing {
    /// Points of the stroke that were walked along
    points: usize,
    /// How far from the last point the next dot is
    to_next_dot: f32,
    /// Dots are this far apart
    spacing: f32,
    /// Where the dots are, and which way their brush is flipped
    dots: Vec<(Pos2, usize)>,
    /// Which way the brush is flipped at the last dot: 1 in x, 2 in y
    flipped: usize,
    /// The brush in the color of the stroke. If the stroke flips it, then also
    /// flipped in x, in y and in both.
    brushes: Vec<RgbaImage>,
    /// Size of the image the stroke is drawn on
    size: (u32, u32),
    /// The image without the stroke, a fading stroke is drawn on it again. Only where
    /// the stroke is, the rest stays empty.
    base: RgbaImage,
    /// The part of `base` that is filled in
    base_area: Option<Rect>,
}

impl StrokeDrawing {
    fn start(stroke: &PaintStroke, img: &RgbaImage, brushes: &[RgbaImage]) -> Self {
        // The brush is a fraction of the shorter side of the image
        let size = (stroke.width * img.width().min(img.height()) as f32) as u32;
        let mut brush = if size == 0 {
            RgbaImage::default()
        } else {
            sized_brush(brushes, stroke.brush_index, size)
                .as_ref()
                .clone()
        };
        // Tinted once for the whole stroke, alpha is done per dot
        let color = stroke.brush_color();
        let tint: [[u8; 256]; 3] = std::array::from_fn(|c| times_table(color[c]));
        for p in brush.pixels_mut() {
            for c in 0..3 {
                p[c] = tint[c][p[c] as usize];
            }
        }
        let mut brushes = vec![brush];
        if stroke.flip_random {
            let x = image::imageops::flip_horizontal(&brushes[0]);
            let y = image::imageops::flip_vertical(&brushes[0]);
            let both = image::imageops::flip_vertical(&x);
            brushes.extend([x, y, both]);
        }
        Self {
            spacing: (size as f32 / 4.0).max(1.5),
            brushes,
            size: img.dimensions(),
            ..Default::default()
        }
    }

    /// Draw what is new of the stroke that is being drawn. Returns the part of the
    /// image that changed.
    pub fn draw(
        &mut self,
        stroke: &PaintStroke,
        img: &mut RgbaImage,
        brushes: &[RgbaImage],
    ) -> Option<Rect> {
        if self.points == 0 || self.points > stroke.points.len() || self.size != img.dimensions() {
            *self = Self::start(stroke, img, brushes);
        }
        let first_new = self.dots.len();
        self.walk(stroke);
        if self.dots.len() == first_new {
            return None;
        }
        if !stroke.fade {
            return self.draw_dots(stroke, img, first_new);
        }
        // A fading stroke fades along all of its dots, every new one changes them all.
        // Drawn again where it is, on the image without it.
        let area = self.area()?;
        self.keep_base(img, area);
        let row = area.width as usize * 4;
        let stride = img.width() as usize * 4;
        let start = area.y as usize * stride + area.x as usize * 4;
        let rows = start..start + (area.height as usize - 1) * stride + row;
        img.as_mut()[rows.clone()]
            .par_chunks_mut(stride)
            .zip(self.base.as_raw()[rows].par_chunks(stride))
            .for_each(|(out, base)| out[..row].copy_from_slice(&base[..row]));
        self.draw_dots(stroke, img, 0)
    }

    /// Keep what is under the stroke where its area grew to `area`, before it is drawn there
    fn keep_base(&mut self, img: &RgbaImage, area: Rect) {
        if self.base.dimensions() != img.dimensions() {
            // zeroed memory, only the part that is filled in is ever touched
            self.base = RgbaImage::new(img.width(), img.height());
            self.base_area = None;
        }
        let stride = img.width() as usize * 4;
        for y in area.y..area.y + area.height {
            let row = y as usize * stride;
            let (left, right) = (
                row + area.x as usize * 4,
                row + (area.x + area.width) as usize * 4,
            );
            // the area only grows, what it was is kept already
            let kept = self
                .base_area
                .filter(|kept| (kept.y..kept.y + kept.height).contains(&y))
                .map(|kept| {
                    (
                        row + kept.x as usize * 4,
                        row + (kept.x + kept.width) as usize * 4,
                    )
                });
            let parts = match kept {
                Some((kept_left, kept_right)) => [left..kept_left, kept_right..right],
                None => [left..right, right..right],
            };
            for part in parts {
                self.base.as_mut()[part.clone()].copy_from_slice(&img.as_raw()[part]);
            }
        }
        self.base_area = Some(area);
    }

    /// Find the dots for the points that were added. Like egui's dotted line: a dot at
    /// the first point, then one every `spacing` along the way. A click is one dot.
    fn walk(&mut self, stroke: &PaintStroke) {
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        let at = |p: (f32, f32)| Pos2::new(w * p.0, h * p.1);
        if self.points == 0
            && let Some(first) = stroke.points.first()
        {
            self.add_dot(at(*first), stroke.flip_random);
            self.to_next_dot = self.spacing;
            self.points = 1;
        }
        for i in self.points..stroke.points.len() {
            let start = at(stroke.points[i - 1]);
            let vector = at(stroke.points[i]) - start;
            let length = vector.length();
            while self.to_next_dot < length {
                self.add_dot(
                    start + vector * (self.to_next_dot / length),
                    stroke.flip_random,
                );
                self.to_next_dot += self.spacing;
            }
            self.to_next_dot -= length;
        }
        self.points = self.points.max(stroke.points.len());
    }

    fn add_dot(&mut self, pos: Pos2, flip_random: bool) {
        if flip_random {
            // seed by brush position so randomness only changes per brush instance
            let mut rng = ChaCha8Rng::seed_from_u64(pos.x as u64 + pos.y as u64);
            if rng.random::<bool>() {
                self.flipped ^= 1;
            }
            if rng.random::<bool>() {
                self.flipped ^= 2;
            }
        }
        self.dots.push((pos, self.flipped));
    }

    /// Draw the dots from `first` on. Returns the part of the image they cover.
    fn draw_dots(&self, stroke: &PaintStroke, img: &mut RgbaImage, first: usize) -> Option<Rect> {
        let alpha = stroke.brush_color()[3];
        let mut table = times_table(alpha);
        let mut area = None;
        for (i, (pos, flipped)) in self.dots.iter().enumerate().skip(first) {
            if stroke.fade {
                table = times_table(alpha * (1.0 - i as f32 / self.dots.len() as f32));
            }
            area = union(area, dab(img, &self.brushes[*flipped], *pos, &table));
        }
        area
    }

    /// The part of the image the dots cover
    fn area(&self) -> Option<Rect> {
        let brush = &self.brushes[0];
        self.dots.iter().fold(None, |area, (pos, _)| {
            union(area, dab_area(self.size, brush.dimensions(), *pos))
        })
    }
}

/// Brushes at the sizes they were last drawn at, the newest first. Making a large one
/// takes a while, and all strokes are drawn again when one in the list changes. The
/// brushes are always the ones that come with Oculante, their index tells them apart.
/// Kept are the newest that fit into four megapixels, 16 MB.
static SIZED_BRUSHES: Mutex<Vec<SizedBrush>> = Mutex::new(Vec::new());

struct SizedBrush {
    index: usize,
    size: u32,
    brush: Arc<RgbaImage>,
}

fn sized_brush(brushes: &[RgbaImage], index: usize, size: u32) -> Arc<RgbaImage> {
    let mut sized = SIZED_BRUSHES.lock().unwrap_or_else(|e| e.into_inner());
    let brush = match sized
        .iter()
        .position(|b| b.index == index && b.size == size)
    {
        Some(i) => sized.remove(i).brush,
        None => Arc::new(image::imageops::resize(
            &brushes[index],
            size,
            size,
            image::imageops::Triangle,
        )),
    };
    sized.insert(
        0,
        SizedBrush {
            index,
            size,
            brush: brush.clone(),
        },
    );
    let mut pixels = 0;
    sized.retain(|b| {
        pixels += b.size as u64 * b.size as u64;
        pixels <= 4_000_000
    });
    sized.truncate(8);
    brush
}

/// What the levels 0 to 255 become times `factor`
fn times_table(factor: f32) -> [u8; 256] {
    std::array::from_fn(|level| (factor * level as f32) as u8)
}

/// Where a brush centered at `pos` is on the image, cut to the image
fn dab_area(size: (u32, u32), brush: (u32, u32), pos: Pos2) -> Option<Rect> {
    let x = (pos.x - brush.0 as f32 / 2.).floor() as i64;
    let y = (pos.y - brush.1 as f32 / 2.).floor() as i64;
    let (x0, x1) = (
        x.max(0),
        x.saturating_add(brush.0 as i64).min(size.0 as i64),
    );
    let (y0, y1) = (
        y.max(0),
        y.saturating_add(brush.1 as i64).min(size.1 as i64),
    );
    (x0 < x1 && y0 < y1).then(|| Rect {
        x: x0 as u32,
        y: y0 as u32,
        width: (x1 - x0) as u32,
        height: (y1 - y0) as u32,
    })
}

/// Blend the brush onto the image, centered at `pos`. Returns the part it covers.
fn dab(img: &mut RgbaImage, brush: &RgbaImage, pos: Pos2, alpha: &[u8; 256]) -> Option<Rect> {
    let area = dab_area(img.dimensions(), brush.dimensions(), pos)?;
    // where the brush starts, it can be outside of the image
    let bx = area.x as i64 - (pos.x - brush.width() as f32 / 2.).floor() as i64;
    let by = area.y as i64 - (pos.y - brush.height() as f32 / 2.).floor() as i64;
    let stride = img.width() as usize * 4;
    let brush_row = brush.width() as usize * 4;
    let brush = brush.as_raw();
    let rows =
        &mut img.as_mut()[area.y as usize * stride..(area.y + area.height) as usize * stride];
    let row = |(i, row): (usize, &mut [u8])| {
        let out = &mut row[area.x as usize * 4..(area.x + area.width) as usize * 4];
        let start = (by as usize + i) * brush_row + bx as usize * 4;
        let (out, _) = out.as_chunks_mut::<4>();
        let (from, _) = brush[start..start + area.width as usize * 4].as_chunks::<4>();
        for (o, b) in out.iter_mut().zip(from) {
            blend(o, b, alpha);
        }
    };
    if area.width * area.height > 128 * 128 {
        rows.par_chunks_mut(stride).enumerate().for_each(row);
    } else {
        rows.chunks_mut(stride).enumerate().for_each(row);
    }
    Some(area)
}

/// The brush over the image, like `Rgba::blend` but in whole numbers
fn blend(out: &mut [u8; 4], brush: &[u8; 4], alpha: &[u8; 256]) {
    let fa = alpha[brush[3] as usize] as u32;
    if fa == 0 {
        return;
    }
    if fa == 255 {
        out[..3].copy_from_slice(&brush[..3]);
        out[3] = 255;
        return;
    }
    let ba = out[3] as u32;
    if ba == 255 {
        for c in 0..3 {
            out[c] = ((brush[c] as u32 * fa + out[c] as u32 * (255 - fa)) / 255) as u8;
        }
    } else {
        // alpha times 255
        let alpha = fa * 255 + ba * (255 - fa);
        for c in 0..3 {
            out[c] = ((brush[c] as u32 * fa * 255 + out[c] as u32 * ba * (255 - fa)) / alpha) as u8;
        }
        out[3] = (alpha / 255) as u8;
    }
}

fn union(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (Some(a), Some(b)) => Some(crate::utils::rect_union(a, b)),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Pixel, Rgba};

    fn brushes() -> Vec<RgbaImage> {
        crate::image_editing::EditState::default().brushes
    }

    fn stroke(points: &[(f32, f32)]) -> PaintStroke {
        PaintStroke {
            points: points.to_vec(),
            color: [0.9, 0.5, 0.2, 0.8],
            width: 0.1,
            ..PaintStroke::new()
        }
    }

    /// A wiggly line, as a mouse would draw it
    fn path() -> Vec<(f32, f32)> {
        (0..60)
            .map(|i| {
                let t = i as f32 / 59.0;
                (0.1 + 0.8 * t, 0.5 + 0.3 * (t * 9.0).sin())
            })
            .collect()
    }

    #[test]
    fn drawn_point_by_point_is_drawn_at_once() {
        let brushes = brushes();
        let base = RgbaImage::from_pixel(400, 300, Rgba([90, 120, 60, 200]));
        for (fade, flip_random) in [(false, false), (false, true), (true, false), (true, true)] {
            let whole = PaintStroke {
                fade,
                flip_random,
                ..stroke(&path())
            };
            let mut at_once = base.clone();
            whole.render(&mut at_once, &brushes);

            let mut drawn = base.clone();
            let mut drawing = StrokeDrawing::default();
            let mut growing = PaintStroke {
                points: vec![],
                ..whole.clone()
            };
            for p in &whole.points {
                growing.points.push(*p);
                let before = drawn.clone();
                let area = drawing.draw(&growing, &mut drawn, &brushes);
                // nothing changed outside the area that is said to have changed
                let area = area.unwrap_or(Rect {
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0,
                });
                for (x, y, px) in drawn.enumerate_pixels() {
                    let inside = x >= area.x
                        && x < area.x + area.width
                        && y >= area.y
                        && y < area.y + area.height;
                    assert!(
                        inside || px == before.get_pixel(x, y),
                        "{x},{y} outside {area:?}"
                    );
                }
            }
            assert!(
                at_once != base,
                "fade {fade} flip {flip_random}: nothing drawn"
            );
            assert!(
                at_once == drawn,
                "fade {fade} flip {flip_random}: not the same"
            );
        }
    }

    #[test]
    fn same_as_blending_pixels() {
        let brush = RgbaImage::from_fn(16, 16, |x, y| Rgba([200, 30, 90, (x * 16 + y) as u8]));
        let color = [0.9, 0.5, 0.2, 0.8];
        for bg_alpha in [255, 128, 0] {
            let mut ours = RgbaImage::from_pixel(16, 16, Rgba([90, 120, 60, bg_alpha]));
            let mut theirs = ours.clone();
            let stroke = PaintStroke {
                color,
                ..PaintStroke::new()
            };
            let drawing = StrokeDrawing {
                brushes: vec![{
                    let mut b = brush.clone();
                    for p in b.pixels_mut() {
                        for c in 0..3 {
                            p[c] = (color[c] * p[c] as f32) as u8;
                        }
                    }
                    b
                }],
                dots: vec![(Pos2::new(8.0, 8.0), 0)],
                size: (16, 16),
                ..Default::default()
            };
            drawing.draw_dots(&stroke, &mut ours, 0);
            for (b, t) in brush.pixels().zip(theirs.pixels_mut()) {
                let tinted = Rgba(std::array::from_fn(|c| (color[c] * b[c] as f32) as u8));
                t.blend(&tinted);
            }
            for (o, t) in ours.pixels().zip(theirs.pixels()) {
                for c in 0..4 {
                    assert!(
                        o[c].abs_diff(t[c]) <= 1,
                        "on alpha {bg_alpha}: {o:?} and {t:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn dots_at_the_edge_stay_where_they_are() {
        // How far right a 100 pixel brush reaches, centered at `x`
        let reaches = |x: f32| {
            let mut img = RgbaImage::from_pixel(1000, 1000, Rgba([0, 0, 0, 255]));
            stroke(&[(x / 1000.0, 0.5)]).render(&mut img, &brushes());
            (0..1000)
                .filter(|x| (0..1000).any(|y| img.get_pixel(*x, y)[0] > 0))
                .max()
                .map(|right| right as f32 - x)
        };
        assert!(reaches(500.0).is_some());
        assert_eq!(reaches(10.0), reaches(500.0));
    }

    #[test]
    fn a_click_is_a_dot() {
        let mut img = RgbaImage::from_pixel(200, 200, Rgba([0, 0, 0, 255]));
        let mut drawing = StrokeDrawing::default();
        let area = drawing.draw(&stroke(&[(0.5, 0.5)]), &mut img, &brushes());
        assert_eq!(
            area,
            Some(Rect {
                x: 90,
                y: 90,
                width: 20,
                height: 20
            })
        );
        assert!(img.pixels().any(|p| p[0] > 0));
        // the mouse held still: nothing new
        assert_eq!(
            drawing.draw(&stroke(&[(0.5, 0.5), (0.5, 0.5)]), &mut img, &brushes()),
            None
        );
    }
}
