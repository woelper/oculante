use image::{Pixel, Rgba, RgbaImage};
use notan::egui::{Color32, Pos2};
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

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
        // Calculate the brush: use a fraction of the smallest image size
        let max_brush_size = img.width().min(img.height());

        let mut brush = image::imageops::resize(
            &brushes[self.brush_index],
            (self.width * max_brush_size as f32) as u32,
            (self.width * max_brush_size as f32) as u32,
            image::imageops::Triangle,
        );

        // transform points from UV into image space
        let abs_points = self
            .points
            .iter()
            .map(|p| Pos2::new(img.width() as f32 * p.0, img.height() as f32 * p.1))
            .collect::<Vec<_>>();

        let points = notan::egui::Shape::dotted_line(
            &abs_points,
            Color32::DARK_RED,
            (brush.width() as f32 / 4.0).max(1.5), // .min(60.)
            0.,
        );

        for (i, p) in points.iter().enumerate() {
            let pos_on_line = p.visual_bounding_rect().center();

            if self.flip_random {
                // seed by brush position so randomness only changes per brush instance
                let mut rng =
                    ChaCha8Rng::seed_from_u64(pos_on_line.x as u64 + pos_on_line.y as u64);

                let flip_x: bool = rng.gen();
                let flip_y: bool = rng.gen();

                if flip_x {
                    image::imageops::flip_horizontal_in_place(&mut brush);
                }
                if flip_y {
                    image::imageops::flip_vertical_in_place(&mut brush);
                }
            }

            let mut stroke_color = self.color;

            if self.fade {
                let fraction = 1.0 - i as f32 / points.len() as f32;
                stroke_color[3] *= fraction;
            }

            if self.highlight {
                stroke_color[0] *= 2.5;
                stroke_color[1] *= 2.5;
                stroke_color[2] *= 2.5;
                stroke_color[3] *= 2.5;
            }
            paint_at(img, &brush, &pos_on_line, stroke_color);
        }
    }
}

pub fn paint_at(img: &mut RgbaImage, brush: &RgbaImage, pos: &Pos2, color: [f32; 4]) {
    // To test
    // img.put_pixel(pos.x as u32, pos.y as u32, color_to_pixel(color));
    // return;

    let brush_offset = Pos2::new(brush.width() as f32 / 2., brush.height() as f32 / 2.);

    for (b_x, b_y, b_pixel) in brush.enumerate_pixels() {
        if let Some(p) = img.get_pixel_mut_checked(
            (*pos - brush_offset).x as u32 + b_x,
            (*pos - brush_offset).y as u32 + b_y,
        ) {
            // multiply brush with user color os it's tinted
            let colored_pixel = Rgba([
                (color[0] * b_pixel[0] as f32) as u8,
                (color[1] * b_pixel[1] as f32) as u8,
                (color[2] * b_pixel[2] as f32) as u8,
                (color[3] * b_pixel[3] as f32) as u8,
            ]);
            // colored_pixel.blend(&color_to_pixel(color));
            p.blend(&colored_pixel);
        }
    }
}
