//! Drawing annotations onto the edited image and keeping them drawn there as they change.

use super::*;

/// Length and half the width of the head of an arrow
pub(super) fn arrow_head(width: f32, length: f32) -> (f32, f32) {
    let head = (width * 3. + 10.).min(length * 0.6);
    (head, head * 0.5)
}

/// The three points of an arrowhead at `to`: the tip and the two back corners.
pub(super) fn arrow_head_corners(to: [i32; 2], width: f32) -> Option<[Point; 3]> {
    let (x, y) = (to[0] as f32, to[1] as f32);
    let length = x.hypot(y);
    if length < 1. {
        return None;
    }
    let (dx, dy) = (x / length, y / length);
    let (head, half) = arrow_head(width, length);
    let (bx, by) = (x - dx * head, y - dy * head);
    Some([
        Point::from_xy(x, y),
        Point::from_xy(bx - dy * half, by + dx * half),
        Point::from_xy(bx + dy * half, by - dx * half),
    ])
}

/// The line and the head of an arrow from where it starts to `to`
fn arrow(to: [i32; 2], width: f32) -> Option<(Path, Path)> {
    let [tip, left, right] = arrow_head_corners(to, width)?;
    let length = tip.x.hypot(tip.y);
    let head = arrow_head(width, length).0;
    let mut line = PathBuilder::new();
    line.move_to(0., 0.);
    // into the head, which covers the round end
    let into = 1. - head * 0.5 / length;
    line.line_to(tip.x * into, tip.y * into);
    let mut corners = PathBuilder::new();
    corners.move_to(tip.x, tip.y);
    corners.line_to(left.x, left.y);
    corners.line_to(right.x, right.y);
    corners.close();
    Some((line.finish()?, corners.finish()?))
}

/// The part of the edited image an annotation draws on
pub(super) fn area(a: &Annotation, t: Transform, size: (u32, u32)) -> Option<Rect> {
    let [left, top, right, bottom] = extent(a, t, a.reach())?;
    // smoothed edges reach a little further
    let smooth = if matches!(a.shape, Shape::Redact { .. }) {
        0.
    } else {
        2.
    };
    let (left, top) = (
        (left - smooth).floor().max(0.),
        (top - smooth).floor().max(0.),
    );
    let (right, bottom) = (
        (right + smooth).ceil().min(size.0 as f32),
        (bottom + smooth).ceil().min(size.1 as f32),
    );
    (right > left && bottom > top).then_some(Rect {
        x: left as u32,
        y: top as u32,
        width: (right - left) as u32,
        height: (bottom - top) as u32,
    })
}

/// Draws an annotation, which reaches no further than `area`
pub(super) fn draw<P: Pixel<Subpixel = u8>>(img: &mut Pixels<P>, a: &Annotation, t: Transform, area: Rect) {
    if let Shape::Redact { style, .. } = &a.shape {
        match style {
            Redaction::Pixelate => {
                pixelate(img, area, (a.width as f32 * scale_of(t)).round() as u32)
            }
            Redaction::Fill => {
                let [r, g, b, _] = a.color;
                let color = *P::from_slice(&[r, g, b, 255][..channels::<P>()]);
                for y in area.y..area.y + area.height {
                    for x in area.x..area.x + area.width {
                        img.put_pixel(x, y, color);
                    }
                }
            }
        }
        return;
    }
    let Some(mut layer) = Pixmap::new(area.width, area.height) else {
        return;
    };
    let t = t.post_translate(-(area.x as f32), -(area.y as f32));
    let paint = |[r, g, b, alpha]: [u8; 4]| {
        let mut paint = Paint::default();
        paint.set_color_rgba8(r, g, b, alpha);
        paint.anti_alias = true;
        paint
    };
    // the outline first, the annotation over it
    let outline = a.outline_width();
    let mut passes = vec![(paint(a.color), 0.)];
    if outline > 0. {
        passes.insert(0, (paint(a.outline_color), outline));
    }
    for (paint, outline) in passes {
        let stroke = tiny_skia::Stroke {
            width: a.line_width() + outline * 2.,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        match &a.shape {
            Shape::Arrow(to) => {
                if let Some((line, tip)) = arrow(*to, a.line_width()) {
                    layer.stroke_path(&line, &paint, &stroke, t, None);
                    layer.fill_path(&tip, &paint, FillRule::Winding, t, None);
                    if outline > 0. {
                        let around = tiny_skia::Stroke {
                            width: outline * 2.,
                            ..stroke.clone()
                        };
                        layer.stroke_path(&tip, &paint, &around, t, None);
                    }
                }
            }
            Shape::Rect(size) => {
                if let Some(frame) = frame_path(*size, a.corner_radius()) {
                    let stroke = tiny_skia::Stroke {
                        line_join: LineJoin::Miter,
                        ..stroke
                    };
                    layer.stroke_path(&frame, &paint, &stroke, t, None);
                }
            }
            Shape::Ellipse(size) => {
                let oval = tiny_skia::Rect::from_xywh(0., 0., size[0] as f32, size[1] as f32)
                    .and_then(PathBuilder::from_oval);
                if let Some(oval) = oval {
                    layer.stroke_path(&oval, &paint, &stroke, t, None);
                }
            }
            Shape::Pen(points) => {
                if let [only] = points.as_slice() {
                    let dot =
                        PathBuilder::from_circle(only[0] as f32, only[1] as f32, stroke.width / 2.);
                    if let Some(dot) = dot {
                        layer.fill_path(&dot, &paint, FillRule::Winding, t, None);
                    }
                } else if let Some(line) = polyline(points) {
                    layer.stroke_path(&line, &paint, &stroke, t, None);
                }
            }
            // the outline of a text is a part of it
            Shape::Text(text) if outline == 0. => {
                if let Some(layout) = text_layout(text, a) {
                    resvg::render(&layout.tree, t, &mut layer.as_mut());
                }
            }
            Shape::Stamp(stamp) => {
                if let Some(glyph) = stamp_path(*stamp, a.width as f32) {
                    // around it, under the symbol as around a text
                    if outline > 0. {
                        let around = tiny_skia::Stroke {
                            width: outline * 2.,
                            ..stroke
                        };
                        layer.stroke_path(&glyph, &paint, &around, t, None);
                    } else {
                        layer.fill_path(&glyph, &paint, FillRule::Winding, t, None);
                    }
                }
            }
            Shape::Text(_) | Shape::Redact { .. } => {}
        }
    }
    blend(img, &layer, area);
}

/// A frame this large, its corners rounded by `radius`
pub(super) fn frame_path(size: [i32; 2], radius: f32) -> Option<Path> {
    let (w, h) = (size[0] as f32, size[1] as f32);
    let mut frame = PathBuilder::new();
    if radius <= 0. {
        frame.move_to(0., 0.);
        frame.line_to(w, 0.);
        frame.line_to(w, h);
        frame.line_to(0., h);
    } else {
        // a quarter of a circle, as a curve
        let r = radius;
        let k = r * (1. - 0.552_284_8);
        frame.move_to(r, 0.);
        frame.line_to(w - r, 0.);
        frame.cubic_to(w - k, 0., w, k, w, r);
        frame.line_to(w, h - r);
        frame.cubic_to(w, h - k, w - k, h, w - r, h);
        frame.line_to(r, h);
        frame.cubic_to(k, h, 0., h - k, 0., h - r);
        frame.line_to(0., r);
        frame.cubic_to(0., k, k, 0., r, 0.);
    }
    frame.close();
    frame.finish()
}

/// The icon font, read once
static ICONS: LazyLock<Option<ttf_parser::Face<'static>>> =
    LazyLock::new(|| ttf_parser::Face::parse(crate::ICON_FONT, 0).ok());

/// A stamp in a square this large, in the middle of it, as wide or as high
pub(super) fn stamp_path(stamp: Stamp, size: f32) -> Option<Path> {
    struct Outline(PathBuilder);
    impl ttf_parser::OutlineBuilder for Outline {
        fn move_to(&mut self, x: f32, y: f32) {
            self.0.move_to(x, y);
        }
        fn line_to(&mut self, x: f32, y: f32) {
            self.0.line_to(x, y);
        }
        fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
            self.0.quad_to(x1, y1, x, y);
        }
        fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
            self.0.cubic_to(x1, y1, x2, y2, x, y);
        }
        fn close(&mut self) {
            self.0.close();
        }
    }
    let face = ICONS.as_ref()?;
    let glyph = face.glyph_index(stamp.glyph().chars().next()?)?;
    let mut outline = Outline(PathBuilder::new());
    let b = face.outline_glyph(glyph, &mut outline)?;
    let (w, h) = (b.width() as f32, b.height() as f32);
    let scale = size / w.max(h).max(1.);
    // the font goes up from its baseline
    let fit = Transform::from_row(
        scale,
        0.,
        0.,
        -scale,
        (size - w * scale) / 2. - b.x_min as f32 * scale,
        (size - h * scale) / 2. + b.y_max as f32 * scale,
    );
    outline.0.finish()?.transform(fit)
}

/// A line through the points of a pen
pub(super) fn polyline(points: &[[i32; 2]]) -> Option<Path> {
    let mut line = PathBuilder::new();
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            line.move_to(p[0] as f32, p[1] as f32);
        } else {
            line.line_to(p[0] as f32, p[1] as f32);
        }
    }
    line.finish()
}

/// Hides an area in blocks, each the average color of its pixels
fn pixelate<P: Pixel<Subpixel = u8>>(img: &mut Pixels<P>, area: Rect, block: u32) {
    let n = channels::<P>();
    let block = block.max(2);
    let (right, bottom) = (area.x + area.width, area.y + area.height);
    for top in (area.y..bottom).step_by(block as usize) {
        for left in (area.x..right).step_by(block as usize) {
            let (r, b) = ((left + block).min(right), (top + block).min(bottom));
            // weighted by alpha, transparent pixels have no color to give
            let mut sum = [0u64; 4];
            for y in top..b {
                for x in left..r {
                    let p = img.get_pixel(x, y).channels();
                    // without alpha the image is opaque
                    let alpha = p.get(3).map_or(255, |a| *a as u64);
                    for c in 0..3 {
                        sum[c] += p[c] as u64 * alpha;
                    }
                    sum[3] += alpha;
                }
            }
            let count = ((r - left) * (b - top)) as u64;
            let color = match sum[3] {
                0 => [0; 4],
                alpha => [
                    (sum[0] / alpha) as u8,
                    (sum[1] / alpha) as u8,
                    (sum[2] / alpha) as u8,
                    (alpha / count) as u8,
                ],
            };
            for y in top..b {
                for x in left..r {
                    img.put_pixel(x, y, *P::from_slice(&color[..n]));
                }
            }
        }
    }
}

/// Puts a layer, its colors multiplied by alpha, over a part of the image
fn blend<P: Pixel<Subpixel = u8>>(img: &mut Pixels<P>, layer: &Pixmap, area: Rect) {
    let n = channels::<P>();
    let stride = img.width() as usize * n;
    let row_length = area.width as usize * 4;
    for (row, src) in layer.data().chunks_exact(row_length).enumerate() {
        let start = (area.y as usize + row) * stride + area.x as usize * n;
        let dst = &mut img.as_mut()[start..start + area.width as usize * n];
        let (src, _) = src.as_chunks::<4>();
        for (d, s) in dst.chunks_exact_mut(n).zip(src) {
            match s[3] {
                0 => {}
                255 => d.copy_from_slice(&s[..n]),
                alpha => {
                    let sa = alpha as f32 / 255.;
                    // what shows of the pixel below, all of it without alpha
                    let below = d.get(3).map_or(1., |a| *a as f32 / 255.) * (1. - sa);
                    let out = sa + below;
                    for c in 0..3 {
                        d[c] = ((s[c] as f32 + d[c] as f32 * below) / out).round() as u8;
                    }
                    if let Some(a) = d.get_mut(3) {
                        *a = (out * 255.).round() as u8;
                    }
                }
            }
        }
    }
}

/// The annotations in the edited image, kept up to date by drawing only what changed
#[derive(Debug, Clone, Default)]
pub struct AnnotationLayer {
    /// What was under the annotations, where `base_area` says
    base: Base,
    base_area: Option<Rect>,
    /// The annotations as they are in the image, with the part each one drew on
    drawn: Vec<(Annotation, Option<Rect>)>,
    fonts: u32,
}

impl AnnotationLayer {
    /// The image was made anew, without annotations
    pub fn reset(&mut self) {
        self.base_area = None;
        self.drawn.clear();
    }

    /// Whether any annotation is in the image
    pub fn is_empty(&self) -> bool {
        self.drawn.iter().all(|(_, area)| area.is_none())
    }

    /// Brings the annotations in the image up to date. Returns the part of the image
    /// that changed.
    pub fn update<P: Pixel<Subpixel = u8>>(
        &mut self,
        img: &mut Pixels<P>,
        annotations: &[Annotation],
        mapping: &Mapping,
    ) -> Option<Rect> {
        let size = img.dimensions();
        let now: Vec<(Annotation, Option<Rect>)> = annotations
            .iter()
            .map(|a| (a.clone(), area(a, a.transform(mapping), size)))
            .collect();
        let fonts = FONTS_VERSION.load(Ordering::Relaxed);
        let everything = fonts != self.fonts;
        let mut dirty: Option<Rect> = None;
        for i in 0..self.drawn.len().max(now.len()) {
            let (was, is) = (self.drawn.get(i), now.get(i));
            if everything || was != is {
                for area in [was, is].into_iter().flatten().filter_map(|e| e.1) {
                    dirty = Some(dirty.map_or(area, |d| rect_union(d, area)));
                }
            }
        }
        self.fonts = fonts;
        let Some(mut dirty) = dirty else {
            self.drawn = now;
            return None;
        };
        // all that is drawn again is drawn whole, a redaction needs all of its blocks
        loop {
            let mut grown = false;
            for area in now.iter().filter_map(|e| e.1) {
                if overlap(area, dirty) && rect_union(area, dirty) != dirty {
                    dirty = rect_union(area, dirty);
                    grown = true;
                }
            }
            if !grown {
                break;
            }
        }
        let kept = self.base_area.map_or(dirty, |b| rect_union(b, dirty));
        keep_base(&mut self.base, &mut self.base_area, img, kept);
        restore(img, &self.base, dirty);
        for (a, area) in &now {
            if let Some(area) = area.filter(|area| overlap(*area, dirty)) {
                draw(img, a, a.transform(mapping), area);
            }
        }
        self.drawn = now;
        Some(dirty)
    }
}

fn overlap(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}
