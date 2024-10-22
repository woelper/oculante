use log::warn;
use notan::draw::*;
use notan::egui::EguiRegisterTexture;
use notan::egui::SizedTexture;

use crate::settings::PersistentSettings;
use crate::Draw;
use image::imageops;

use image::RgbaImage;

use log::error;
use notan::prelude::{BlendMode, Graphics, Texture, TextureFilter};

pub struct TexWrap {
    texture_array: Vec<TexturePair>,
    pub col_count: u32,
    pub row_count: u32,
    pub col_translation: u32,
    pub row_translation: u32,
    pub size_vec: (f32, f32), // The whole Texture Array size
    pub texture_count: usize,
}

#[derive(Default)]
pub struct TextureWrapperManager {
    current_texture: Option<TexWrap>,
}

impl TextureWrapperManager {
    pub fn set(&mut self, tex: Option<TexWrap>, gfx: &mut Graphics) {
        let mut texture_taken: Option<TexWrap> = self.current_texture.take();
        if let Some(texture) = &mut texture_taken {
            texture.unregister_textures(gfx);
        }

        self.current_texture = tex;
    }

    pub fn get(&mut self) -> &mut Option<TexWrap> {
        &mut self.current_texture
    }

    //TODO: Extend for clearing textures
    pub fn clear(&mut self /*, gfx: &mut Graphics */) {
        /*if let Some(texture) = &mut self.current_texture {
            texture.unregister_textures(gfx);
        }*/
        self.current_texture = None;
    }
}

pub struct TextureResponse<'a> {
    pub texture: &'a TexturePair,

    pub x_offset_texture: i32,
    pub y_offset_texture: i32,

    pub texture_width: i32,
    pub texture_height: i32,
    pub offset_width: i32,
    pub offset_height: i32,

    pub x_tex_left_global: i32,
    pub y_tex_top_global: i32,
    pub x_tex_right_global: i32,
    pub y_tex_bottom_global: i32,
}

pub struct TexturePair {
    pub texture: Texture,
    pub texture_egui: SizedTexture,
}

impl TexWrap {
    pub fn from_rgbaimage(
        gfx: &mut Graphics,
        settings: &PersistentSettings,
        image: &RgbaImage,
    ) -> Option<TexWrap> {
        Self::gen_from_rgbaimage(gfx, settings, image, Self::gen_texture_standard)
    }

    pub fn from_rgbaimage_premult(
        gfx: &mut Graphics,
        settings: &PersistentSettings,
        image: &RgbaImage,
    ) -> Option<TexWrap> {
        Self::gen_from_rgbaimage(gfx, settings, image, Self::gen_texture_premult)
    }

    fn gen_texture_standard(
        gfx: &mut Graphics,
        bytes: &[u8],
        width: u32,
        height: u32,
        settings: &PersistentSettings,
        size_ok: bool,
    ) -> Option<Texture> {
        gfx.create_texture()
            .from_bytes(bytes, width, height)
            .with_mipmaps(settings.use_mipmaps && size_ok)
            // .with_format(notan::prelude::TextureFormat::SRgba8)
            // .with_premultiplied_alpha()
            .with_filter(
                if settings.linear_min_filter {
                    TextureFilter::Linear
                } else {
                    TextureFilter::Nearest
                },
                if settings.linear_mag_filter {
                    TextureFilter::Linear
                } else {
                    TextureFilter::Nearest
                },
            )
            // .with_wrap(TextureWrap::Clamp, TextureWrap::Clamp)
            .build()
            .ok()
    }

    fn gen_texture_premult(
        gfx: &mut Graphics,
        bytes: &[u8],
        width: u32,
        height: u32,
        settings: &PersistentSettings,
        size_ok: bool,
    ) -> Option<Texture> {
        gfx.create_texture()
            .from_bytes(bytes, width, height)
            .with_premultiplied_alpha()
            .with_mipmaps(settings.use_mipmaps && size_ok)
            // .with_format(notan::prelude::TextureFormat::SRgba8)
            // .with_premultiplied_alpha()
            .with_filter(
                if settings.linear_min_filter {
                    TextureFilter::Linear
                } else {
                    TextureFilter::Nearest
                },
                if settings.linear_mag_filter {
                    TextureFilter::Linear
                } else {
                    TextureFilter::Nearest
                },
            )
            // .with_wrap(TextureWrap::Clamp, TextureWrap::Clamp)
            .build()
            .ok()
    }

    fn gen_from_rgbaimage(
        gfx: &mut Graphics,
        settings: &PersistentSettings,
        image: &RgbaImage,
        texture_generator_function: fn(
            &mut Graphics,
            &[u8],
            u32,
            u32,
            &PersistentSettings,
            bool,
        ) -> Option<Texture>,
    ) -> Option<TexWrap> {
        const MAX_PIXEL_COUNT: usize = 8192 * 8192;
        let (im_w, im_h) = image.dimensions();
        if im_w < 1 || im_h < 1 {
            error!("Image width smaller than 1!");
            return None;
        }

        let im_pixel_count = (im_w * im_h) as usize;
        let allow_mipmap = im_pixel_count < MAX_PIXEL_COUNT;

        if !allow_mipmap {
            warn!(
                "Image with {0} pixels too large (max {1} pixels), disabling mipmaps",
                im_pixel_count, MAX_PIXEL_COUNT
            );
        }

        let im_size = (im_w as f32, im_h as f32);
        let max_texture_size = gfx.limits().max_texture_size;
        let col_count = (im_w as f32 / max_texture_size as f32).ceil() as u32;
        let row_count = (im_h as f32 / max_texture_size as f32).ceil() as u32;

        let mut texture_vec: Vec<TexturePair> = Vec::new();
        let row_increment = std::cmp::min(max_texture_size, im_h);
        let col_increment = std::cmp::min(max_texture_size, im_w);
        let mut fine = true;

        for row_index in 0..row_count {
            let tex_start_y = row_index * row_increment;
            let tex_height = std::cmp::min(row_increment, im_h - tex_start_y);
            for col_index in 0..col_count {
                let tex_start_x = col_index * col_increment;
                let tex_width = std::cmp::min(col_increment, im_w - tex_start_x);

                let sub_img =
                    imageops::crop_imm(image, tex_start_x, tex_start_y, tex_width, tex_height);
                let my_img = sub_img.to_image();
                let tex = texture_generator_function(
                    gfx,
                    my_img.as_ref(),
                    my_img.width(),
                    my_img.height(),
                    settings,
                    allow_mipmap,
                );

                if let Some(t) = tex {
                    let egt = gfx.egui_register_texture(&t);
                    let te = TexturePair {
                        texture: t,
                        texture_egui: egt,
                    };
                    texture_vec.push(te);
                } else {
                    //On error
                    texture_vec.clear();
                    fine = false;
                    break;
                }
            }
            if !fine {
                //early exit if we failed
                break;
            }
        }

        if fine {
            let texture_count = texture_vec.len();
            Some(TexWrap {
                size_vec: im_size,
                col_count: col_count,
                row_count: row_count,
                texture_array: texture_vec,
                col_translation: col_increment,
                row_translation: row_increment,
                texture_count,
            })
        } else {
            None
        }
    }

    pub fn draw_textures(
        &self,
        draw: &mut Draw,
        translation_x: f32,
        translation_y: f32,
        scale: f32,
    ) {
        let mut tex_idx = 0;
        for row_idx in 0..self.row_count {
            let translate_y =
                translation_y as f64 + scale as f64 * row_idx as f64 * self.row_translation as f64;
            for col_idx in 0..self.col_count {
                let translate_x = translation_x as f64
                    + scale as f64 * col_idx as f64 * self.col_translation as f64;
                draw.image(&self.texture_array[tex_idx].texture)
                    .blend_mode(BlendMode::NORMAL)
                    .scale(scale, scale)
                    .translate(translate_x as f32, translate_y as f32);
                tex_idx += 1;
            }
        }
    }

    pub fn unregister_textures(&mut self, gfx: &mut Graphics) {
        for text in &self.texture_array {
            gfx.egui_remove_texture(text.texture_egui.id);
        }
    }

    pub fn update_textures(&mut self, gfx: &mut Graphics, image: &RgbaImage) {
        if self.col_count == 1 && self.row_count == 1 {
            if let Err(e) = gfx
                .update_texture(&mut self.texture_array[0].texture)
                .with_data(image)
                .update()
            {
                error!("{e}");
            }
        } else {
            let mut tex_index = 0;
            for row_index in 0..self.row_count {
                let tex_start_y = row_index * self.row_translation;
                let tex_height = std::cmp::min(self.row_translation, image.height() - tex_start_y);
                for col_index in 0..self.col_count {
                    let tex_start_x = col_index * self.col_translation;
                    let tex_width =
                        std::cmp::min(self.col_translation, image.width() - tex_start_x);

                    let sub_img =
                        imageops::crop_imm(image, tex_start_x, tex_start_y, tex_width, tex_height);
                    let my_img = sub_img.to_image();
                    if let Err(e) = gfx
                        .update_texture(&mut self.texture_array[tex_index].texture)
                        .with_data(my_img.as_ref())
                        .update()
                    {
                        error!("{e}");
                    }
                    tex_index += 1;
                }
            }
        }
    }

    pub fn get_texture_at_xy(&self, xa: i32, ya: i32) -> TextureResponse {
        let x = xa.max(0).min(self.width() as i32 - 1);
        let y = ya.max(0).min(self.height() as i32 - 1);

        //Div by zero possible, never allow zero sized textures!
        let x_idx = x / self.col_translation as i32;
        let y_idx = y / self.row_translation as i32;
        let tex_idx =
            (y_idx * self.col_count as i32 + x_idx).min(self.texture_array.len() as i32 - 1);
        let my_tex_pair = &self.texture_array[tex_idx as usize];
        let my_tex = &my_tex_pair.texture;
        let width = my_tex.width() as i32;
        let height = my_tex.height() as i32;

        let tex_left = x_idx * self.col_translation as i32;
        let tex_top = y_idx * self.row_translation as i32;
        let tex_right = tex_left + width - 1;
        let tex_bottom = tex_top + height - 1;

        let x_offset_texture = xa - tex_left;
        let y_offset_texture = ya - tex_top;

        let remaining_width = width - x_offset_texture;
        let remaining_height = height - y_offset_texture;

        TextureResponse {
            texture: my_tex_pair,
            x_offset_texture: x_offset_texture,
            y_offset_texture: y_offset_texture,

            texture_width: width,
            texture_height: height,

            x_tex_left_global: tex_left,
            y_tex_top_global: tex_top,
            x_tex_right_global: tex_right,
            y_tex_bottom_global: tex_bottom,

            offset_width: remaining_width,
            offset_height: remaining_height,
        }
    }

    pub fn size(&self) -> (f32, f32) {
        return self.size_vec;
    }

    pub fn width(&self) -> f32 {
        return self.size_vec.0;
    }

    pub fn height(&self) -> f32 {
        return self.size_vec.1;
    }
}
