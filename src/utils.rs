use nalgebra::{Vector2, clamp};

pub fn scale_pt(
        origin: Vector2<f64>,
        pt: Vector2<f64>,
        scale: f64,
        scale_inc: f64,
    ) -> Vector2<f64> {
        ((pt - origin) * scale_inc) / scale
    }

pub fn pos_from_coord(origin: Vector2<f64>, pt: Vector2<f64>, bounds: Vector2<f64>, scale: f64) -> Vector2<f64> {
        let mut size = (pt - origin) / scale;
        size.x = clamp(size.x, 0.0, bounds.x-1.0);
        size.y = clamp(size.y, 0.0, bounds.y-1.0);
        size

    }