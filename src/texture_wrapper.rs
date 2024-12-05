use crate::settings::PersistentSettings;
use image::imageops;
use image::DynamicImage;
use image::EncodableLayout;
use log::debug;
use log::error;
use log::warn;
use notan::draw::*;
use notan::prelude::{BlendMode, Graphics, ShaderSource, Texture, TextureFilter};
use zune_png::zune_core::bit_depth::ByteEndian;
pub struct TexWrap {
    texture_array: Vec<TexturePair>,
    pub col_count: u32,
    pub row_count: u32,
    pub col_translation: u32,
    pub row_translation: u32,
    pub size_vec: (f32, f32), // The whole Texture Array size
    pub texture_count: usize,
    pipeline: Option<notan::prelude::Pipeline>,
    pub format: notan::prelude::TextureFormat,
    pub image_format: image::ColorType,
}

#[derive(Default)]
pub struct TextureWrapperManager {
    current_texture: Option<TexWrap>,
}

impl TextureWrapperManager {
    /*#[deprecated(note = "please use `set_image` instead")]
    pub fn set(&mut self, tex: Option<TexWrap>, gfx: &mut Graphics) {
        let mut texture_taken: Option<TexWrap> = self.current_texture.take();
        if let Some(texture) = &mut texture_taken {
            texture.unregister_textures(gfx);
        }

        self.current_texture = tex;
    }*/

    pub fn set_image(
        &mut self,
        img: &DynamicImage,
        gfx: &mut Graphics,
        settings: &PersistentSettings,
    ) {
        //First: try to update an existing texture
        if let Some(tex) = &mut self.current_texture {
            if tex.width() as u32 == img.width()
                && tex.height() as u32 == img.height()
                && img.color() == tex.image_format
            {
                debug!("Re-using texture as it is the same size and type.");
                tex.update_textures(gfx, img);
                return;
            }
        }

        //If update not possible: Remove existing texture and generate a new one
        {
            self.clear();
            debug!("Updating or creating texture with new size.");
        }

        self.current_texture = TexWrap::from_dynamic_image(gfx, settings, img);
    }

    pub fn get(&mut self) -> &mut Option<TexWrap> {
        &mut self.current_texture
    }

    pub fn clear(&mut self /*, gfx: &mut Graphics */) {
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
}

//language=glsl
const FRAGMENT_GRAYSCALE: ShaderSource = notan::fragment_shader! {
    r#"
    #version 450
    precision mediump float;

    layout(location = 0) in vec2 v_uvs;
    layout(location = 1) in vec4 v_color;

    layout(binding = 0) uniform sampler2D u_texture;

    layout(location = 0) out vec4 color;

    void main() {
        vec4 tex_col = texture(u_texture, v_uvs);
        color = vec4(tex_col.r, tex_col.r,tex_col.r, 1.0) * v_color;
    }
    "#
};

impl TexWrap {
    pub fn from_dynamic_image(
        gfx: &mut Graphics,
        settings: &PersistentSettings,
        image: &DynamicImage,
    ) -> Option<TexWrap> {
        Self::gen_from_dynamic_image(gfx, settings, image, Self::gen_texture_standard)
    }

    /*pub fn from_rgbaimage_premult(
        gfx: &mut Graphics,
        settings: &PersistentSettings,
        image: &DynamicImage,
    ) -> Option<TexWrap> {
        Self::gen_from_dynamic_image(gfx, settings, image, Self::gen_texture_premult)
    }*/

    fn gen_texture_standard(
        gfx: &mut Graphics,
        bytes: &[u8],
        width: u32,
        height: u32,
        format: notan::prelude::TextureFormat,
        settings: &PersistentSettings,
        size_ok: bool,
    ) -> Option<Texture> {
        let texture_result = gfx
            .create_texture()
            .from_bytes(bytes, width, height)
            .with_mipmaps(settings.use_mipmaps && size_ok)
            .with_format(format)
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
            .build();

        let _ = match texture_result {
            Ok(texture) => return Some(texture),
            Err(error) => panic!("Problem generating texture: {error:?}"),
        };
    }

    /*fn gen_texture_premult(
        gfx: &mut Graphics,
        bytes: &[u8],
        width: u32,
        height: u32,
        _format:notan::prelude::TextureFormat,
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
    }*/

    /*fn gen_single_from_dynamic_image(gfx: &mut Graphics,
        settings: &PersistentSettings,
        image: &DynamicImage,
        texture_generator_function: fn(
            &mut Graphics,
            &[u8],
            u32,
            u32,
            notan::prelude::TextureFormat,
            &PersistentSettings,
            bool,
        ) -> Option<Texture>,
    ) -> Option<Texture>
    {

    }*/

    fn image_color_supported(img: &DynamicImage) -> bool {
        let supported_type =
            img.color() == image::ColorType::L8 || img.color() == image::ColorType::Rgba8;
        return supported_type;
    }

    fn image_bytesize_expected(img: &DynamicImage) -> usize {
        let pixel_count = (img.width() * img.height()) as usize;
        let mut byte_count: usize;
        match img.color() {
            image::ColorType::L8 => byte_count = pixel_count,
            image::ColorType::Rgba8 => byte_count = pixel_count * 4,
            _ => {
                error!("Passed non supported colored image!");
                byte_count = 0;
            }
        }

        if img.as_bytes().len() < byte_count {
            error!("Pixel buffer is smaller than expected!");
            byte_count = 0;
        }
        return byte_count;
    }

    fn image_bytes_slice(img: &DynamicImage) -> Option<&[u8]> {
        let byte_buffer = img.as_bytes();
        let byte_count = Self::image_bytesize_expected(img);
        if byte_count == 0 {
            return None;
        }
        if byte_count < byte_buffer.len() {
            warn!("Image byte buffer is bigger than expected. Will truncate.");
        }
        let (buff, _) = byte_buffer.split_at(byte_count);
        return Some(buff);
    }

    fn get_dyn_image_part(
        image: &DynamicImage,
        offset: (u32, u32),
        size: (u32, u32),
    ) -> Option<DynamicImage> {
        let color_supported = Self::image_color_supported(image);
        if !color_supported {
            debug!(
                "Current image pixel type {:?} is not supported, will convert to rgba8",
                image.color()
            );
        }
        if offset.0 == 0 && offset.1 == 0 && size.0 == image.width() && size.1 == image.height() {
            //Whole image
            if color_supported {
                return None;
            } else {
                //Convert to rgba8 if current image is not supported
                let img_rgba = image.to_rgba8();
                return Some(DynamicImage::ImageRgba8(img_rgba));
            }
        } else {
            match image {
                DynamicImage::ImageLuma8(luma8_image) => {
                    // Creating luma 8 sub image
                    let mut buff: Vec<u8> = vec![0; size.0 as usize * size.1 as usize];
                    let bytes_src = luma8_image.as_bytes();
                    let mut dst_idx_start = 0 as usize;
                    let mut src_idx_start =
                        offset.0 as usize + offset.1 as usize * image.width() as usize;

                    for _y in 0..size.1 {
                        let dst_idx_end = dst_idx_start + size.0 as usize;
                        let src_idx_end = src_idx_start + size.0 as usize;
                        buff[dst_idx_start..dst_idx_end]
                            .copy_from_slice(&bytes_src[src_idx_start..src_idx_end]);
                        dst_idx_start = dst_idx_end;
                        src_idx_start += image.width() as usize;
                    }
                    let gi: image::GrayImage =
                        image::GrayImage::from_raw(size.0, size.1, buff).unwrap(); //ImageBuffer::from_raw(size.0, size.1, buff).context("Spast.");
                    return Some(DynamicImage::ImageLuma8(gi));
                }
                other_image_type => {
                    //rgba8 automatically
                    let sub_img =
                        imageops::crop_imm(other_image_type, offset.0, offset.1, size.0, size.1);
                    let my_img = sub_img.to_image();
                    return Some(DynamicImage::ImageRgba8(my_img));
                }
            }
        }
    }

    fn get_texture_type_and_pipe(
        gfx: &mut Graphics,
        image: &DynamicImage,
    ) -> (
        notan::prelude::TextureFormat,
        Option<notan::prelude::Pipeline>,
    ) {
        let mut format: notan::prelude::TextureFormat = notan::app::TextureFormat::Rgba32;
        let mut pipeline: Option<notan::prelude::Pipeline> = None;
        debug!("{:?}", image.color());
        match image.color() {
            image::ColorType::L8 => {
                format = notan::prelude::TextureFormat::R8;
                pipeline = Some(create_image_pipeline(gfx, Some(&FRAGMENT_GRAYSCALE)).unwrap());
            }
            _ => {}
        }
        return (format, pipeline);
    }

    fn gen_from_dynamic_image(
        gfx: &mut Graphics,
        settings: &PersistentSettings,
        image: &DynamicImage,
        texture_generator_function: fn(
            &mut Graphics,
            &[u8],
            u32,
            u32,
            notan::prelude::TextureFormat,
            &PersistentSettings,
            bool,
        ) -> Option<Texture>,
    ) -> Option<TexWrap> {
        const MAX_PIXEL_COUNT: usize = 8192 * 8192;

        let im_w = image.width();
        let im_h = image.height();

        let (format, pipeline) = Self::get_texture_type_and_pipe(gfx, image);

        if im_w < 1 || im_h < 1 {
            error!("Image width smaller than 1!"); //TODO: fix t
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

                let mut tex: Option<Texture> = None;

                let sub_img_opt = Self::get_dyn_image_part(
                    image,
                    (tex_start_x, tex_start_y),
                    (tex_width, tex_height),
                );

                if let Some(suba_img) = sub_img_opt {
                    let byte_slice = Self::image_bytes_slice(&suba_img);
                    if let Some(bt_slice) = byte_slice {
                        tex = texture_generator_function(
                            gfx,
                            bt_slice,
                            tex_width,
                            tex_height,
                            format,
                            settings,
                            allow_mipmap,
                        );
                    }
                } else {
                    let byte_slice = Self::image_bytes_slice(&image);
                    if let Some(bt_slice) = byte_slice {
                        tex = texture_generator_function(
                            gfx,
                            bt_slice,
                            tex_width,
                            tex_height,
                            format,
                            settings,
                            allow_mipmap,
                        );
                    }
                }

                if let Some(t) = tex {
                    let te = TexturePair { texture: t };
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
                error!("Texture generation failed!");
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
                pipeline,
                format,
                image_format: image.color(),
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
        self.add_draw_shader(draw);

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
        self.remove_draw_shader(draw);
    }

    pub fn draw_zoomed(
        &self,
        draw: &mut Draw,
        translation_x: f32,
        translation_y: f32,
        width: f32,
        // xy, size
        center: (f32, f32),
        scale: f32,
    ) {
        self.add_draw_shader(draw);

        let width_tex = (width / scale) as i32;

        let xy_tex_size = ((width_tex) as i32, (width_tex) as i32);

        let xy_tex_center = ((center.0) as i32, (center.1) as i32);

        //Ui position to start at
        let base_ui_curs = nalgebra::Vector2::new(translation_x as f64, translation_y as f64);
        let mut curr_ui_curs = base_ui_curs;

        //Loop control variables, start end end coordinates of interest
        let x_coordinate_end = (xy_tex_center.0 + xy_tex_size.0) as i32;
        let mut y_coordinate = xy_tex_center.1 - xy_tex_size.1;
        let y_coordinate_end = (xy_tex_center.1 + xy_tex_size.1) as i32;

        while y_coordinate <= y_coordinate_end {
            let mut y_coordinate_increment = 0; //increment for y coordinate after x loop
            let mut x_coordinate = xy_tex_center.0 - xy_tex_size.0;
            curr_ui_curs.x = base_ui_curs.x;
            let mut last_display_size_y: f64 = 0.0;
            while x_coordinate <= x_coordinate_end {
                //get texture tile
                let curr_tex_response =
                    self.get_texture_at_xy(x_coordinate as i32, y_coordinate as i32);

                //increment coordinates by usable width/height
                y_coordinate_increment = curr_tex_response.offset_height;
                x_coordinate += curr_tex_response.offset_width;

                //Handling last texture in a row or col
                let mut curr_tex_end = nalgebra::Vector2::new(
                    i32::min(curr_tex_response.x_tex_right_global, x_coordinate_end),
                    i32::min(curr_tex_response.y_tex_bottom_global, y_coordinate_end),
                );

                //Handling positive coordinate overflow
                if curr_tex_response.x_tex_right_global as f32 >= self.width() - 1.0f32 {
                    x_coordinate = x_coordinate_end + 1;
                    curr_tex_end.x +=
                        (x_coordinate_end - curr_tex_response.x_tex_right_global).max(0);
                }

                if curr_tex_response.y_tex_bottom_global as f32 >= self.height() - 1.0f32 {
                    y_coordinate_increment = y_coordinate_end - y_coordinate + 1;
                    curr_tex_end.y +=
                        (y_coordinate_end - curr_tex_response.y_tex_bottom_global).max(0);
                }

                //Usable tile size, depending on offsets
                let tile_size = nalgebra::Vector2::new(
                    curr_tex_end.x
                        - curr_tex_response.x_offset_texture
                        - curr_tex_response.x_tex_left_global
                        + 1,
                    curr_tex_end.y
                        - curr_tex_response.y_offset_texture
                        - curr_tex_response.y_tex_top_global
                        + 1,
                );

                //Display size - tile size scaled
                let display_size = nalgebra::Vector2::new(
                    ((tile_size.x - 1) as f64 / (2 * width_tex) as f64) * width as f64,
                    ((tile_size.y - 1) as f64 / (2 * width_tex) as f64) * width as f64,
                );

                draw.image(&curr_tex_response.texture.texture)
                    .blend_mode(BlendMode::NORMAL)
                    .size(display_size.x as f32, display_size.y as f32)
                    .crop(
                        (
                            curr_tex_response.x_offset_texture as f32,
                            curr_tex_response.y_offset_texture as f32,
                        ),
                        ((tile_size.x) as f32, (tile_size.y) as f32),
                    )
                    .translate(curr_ui_curs.x as f32, curr_ui_curs.y as f32);

                //Update display cursor
                curr_ui_curs.x += display_size.x;
                last_display_size_y = display_size.y;
            }
            //Update y coordinates
            y_coordinate += y_coordinate_increment;
            curr_ui_curs.y += last_display_size_y;
        }
        self.remove_draw_shader(draw);

        //Draw crosshair
        let half_width = 1.0;
        draw.rect(
            (translation_x + width / 2.0 - half_width, translation_y),
            (2.0 * half_width, width),
        );
        draw.rect(
            (translation_x, translation_y + width / 2.0 - half_width),
            (width, 2.0 * half_width),
        );
    }

    /*pub fn unregister_textures(&mut self, gfx: &mut Graphics) {
        for text in &self.texture_array {
            gfx.egui_remove_texture(text.texture_egui.id);
        }
    }*/

    pub fn update_textures(&mut self, gfx: &mut Graphics, image: &DynamicImage) {
        let mut tex_index = 0;
        for row_index in 0..self.row_count {
            let tex_start_y = row_index * self.row_translation;
            let tex_height = std::cmp::min(self.row_translation, image.height() - tex_start_y);
            for col_index in 0..self.col_count {
                let tex_start_x = col_index * self.col_translation;
                let tex_width = std::cmp::min(self.col_translation, image.width() - tex_start_x);

                let sub_img_opt = Self::get_dyn_image_part(
                    image,
                    (tex_start_x, tex_start_y),
                    (tex_width, tex_height),
                );

                if let Some(suba_img) = sub_img_opt {
                    let byte_slice = Self::image_bytes_slice(&suba_img);
                    if let Some(bt_slice) = byte_slice {
                        if let Err(e) = gfx
                            .update_texture(&mut self.texture_array[tex_index].texture)
                            .with_data(bt_slice)
                            .update()
                        {
                            error!("{e}");
                            return;
                        }
                    } else {
                        //Error!
                        return;
                    }
                } else {
                    let byte_slice = Self::image_bytes_slice(&image);
                    if let Some(bt_slice) = byte_slice {
                        if let Err(e) = gfx
                            .update_texture(&mut self.texture_array[tex_index].texture)
                            .with_data(bt_slice)
                            .update()
                        {
                            error!("{e}");
                            return;
                        }
                    } else {
                        //Error!
                        return;
                    }
                }

                tex_index += 1;
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

    fn add_draw_shader(&self, draw: &mut Draw) {
        if let Some(pip) = &self.pipeline {
            draw.image_pipeline().pipeline(pip);
        }
    }

    fn remove_draw_shader(&self, draw: &mut Draw) {
        if self.pipeline.is_some() {
            draw.image_pipeline().remove();
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
