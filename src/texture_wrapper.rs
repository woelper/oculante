use crate::settings::PersistentSettings;
use image::imageops;
use image::DynamicImage;
use image::EncodableLayout;
use image::GenericImageView;
use log::debug;
use log::error;
use log::warn;
use notan::draw::*;
use notan::egui::EguiRegisterTexture;
use notan::egui::SizedTexture;
use notan::prelude::{BlendMode, Graphics, ShaderSource, Texture, TextureFilter};
use notan::{
    egui::{self, *}};
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
    #[deprecated(note = "please use `set_image` instead")]
    pub fn set(&mut self, tex: Option<TexWrap>, gfx: &mut Graphics) {
        let mut texture_taken: Option<TexWrap> = self.current_texture.take();
        if let Some(texture) = &mut texture_taken {
            texture.unregister_textures(gfx);
        }

        self.current_texture = tex;
    }

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
                debug!("Re-using texture as it is the same size.");
                tex.update_textures(gfx, img);
                return;
            }
        }

        //If update not possible: Remove existing texture and generate a new one
        let mut texture_taken: Option<TexWrap> = self.current_texture.take();
        if let Some(texture) = &mut texture_taken {
            debug!("Updating texture with new size.");
            texture.unregister_textures(gfx);
        } else {
            debug!("No current texture. Creating and setting texture");
        }

        self.current_texture = TexWrap::from_dynamic_image(gfx, settings, img);
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

        if im_w < 1 || im_h < 1 {
            error!("Image width smaller than 1!");//TODO: fix t
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

                let tex: Option<Texture>;

                match image {
                    DynamicImage::ImageLuma8(luma8_image) => {
                        /*let sub_img = imageops::crop_imm(
                            luma8_image,
                            tex_start_x,
                            tex_start_y,
                            tex_width,
                            tex_height,
                        );*/
                        //let my_img = sub_img.to_image();

                        let mut buff:Vec<u8> = vec![0;tex_width as usize*tex_height as usize];
                        
                        unsafe {
                            let mut ptr_src = luma8_image.as_bytes().as_ptr();
                            let mut ptr = buff.as_mut_ptr(); // y=0, x=0                            
                            ptr_src = ptr_src.add((tex_start_x + tex_start_y*im_w) as usize);
                            for _y in 0..tex_height{                                
                                std::ptr::copy_nonoverlapping(ptr_src, ptr, tex_width as usize);
                                ptr_src = ptr_src.add(im_w as usize);
                                ptr = ptr.add(tex_width as usize);
                            }
                        }

                        tex = texture_generator_function(
                            gfx,
                            buff.as_bytes(),
                            tex_width,
                            tex_height,
                            format,
                            settings,
                            allow_mipmap,
                        );
                    }
                    other_image_type => {
                        let sub_img = imageops::crop_imm(
                            other_image_type,
                            tex_start_x,
                            tex_start_y,
                            tex_width,
                            tex_height,
                        );
                        let my_img = sub_img.to_image();
                        tex = texture_generator_function(
                            gfx,
                            my_img.as_ref(),
                            sub_img.width(),
                            sub_img.height(),
                            format,
                            settings,
                            allow_mipmap,
                        );
                    }
                }

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
        self.begin_draw(draw);

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
        self.end_draw(draw);
    }

    pub fn draw_zoomed(
        &self,
        draw: &mut Draw,
        translation_x: f32,
        translation_y: f32,
        width: f32,
        // xy, size
        center: (f32, f32),
        texture_relsz: (f64, f64)
    ) {
        self.begin_draw(draw);
        let size = self.size();
        let width_tex = (texture_relsz.0*size.0 as f64) as i32;
        let xy_size = (
            (width_tex) as i32,
            (width_tex) as i32,
        );
    
        let xy_center = (
            (translation_x) as i32,
            (translation_y) as i32,
        ); //(16384,1024);//
        
        let sc1 = (
            2.0 * xy_size.0 as f64 / width as f64,
            2.0 * xy_size.1 as f64 / width as f64,
        );
    
        //coordinates of the image-view
        let mut bbox_tl = egui::pos2(f32::MAX, f32::MAX);
        let mut bbox_br = egui::pos2(f32::MIN, f32::MIN);
    
        //Ui position to start at
        let base_ui_curs = nalgebra::Vector2::new(translation_x as f64, translation_y as f64);
        let mut curr_ui_curs = base_ui_curs;
        //our start position
    
        //Loop control variables, start end end coordinates of interest
        let x_coordinate_end = (xy_center.0 + xy_size.0) as i32;
        let mut y_coordinate = xy_center.1 - xy_size.1;
        let y_coordinate_end = (xy_center.1 + xy_size.1) as i32;
    
        while y_coordinate <= y_coordinate_end {
            let mut y_coordinate_increment = 0; //increment for y coordinate after x loop
            let mut x_coordinate = xy_center.0 - xy_size.0;
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
                    curr_tex_end.x += (x_coordinate_end - curr_tex_response.x_tex_right_global).max(0);
                }
    
                if curr_tex_response.y_tex_bottom_global as f32 >= self.height() - 1.0f32 {
                    y_coordinate_increment = y_coordinate_end - y_coordinate + 1;
                    curr_tex_end.y += (y_coordinate_end - curr_tex_response.y_tex_bottom_global).max(0);
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
                let display_size =
                    nalgebra::Vector2::new(tile_size.x as f64 / sc1.0, tile_size.y as f64 / sc1.1);
    
                //Texture display range
                let uv_start = nalgebra::Vector2::new(
                    curr_tex_response.x_offset_texture as f64 / curr_tex_response.texture_width as f64,
                    curr_tex_response.y_offset_texture as f64 / curr_tex_response.texture_height as f64,
                );
    
                let uv_end = nalgebra::Vector2::new(
                    (curr_tex_end.x - curr_tex_response.x_tex_left_global + 1) as f64
                        / curr_tex_response.texture_width as f64,
                    (curr_tex_end.y - curr_tex_response.y_tex_top_global + 1) as f64
                        / curr_tex_response.texture_height as f64,
                );
    
                //let tex_id2 = gfx.egui_register_texture(curr_tex_response.texture);
                let draw_tl_32 = Pos2::new(curr_ui_curs.x as f32, curr_ui_curs.y as f32);
                let draw_br_32 = Pos2::new(
                    (curr_ui_curs.x + display_size.x) as f32,
                    (curr_ui_curs.y + display_size.y) as f32,
                );
                let r_ret = egui::Rect::from_min_max(draw_tl_32, draw_br_32);
    
                
                draw.image(&curr_tex_response.texture.texture)
                .blend_mode(BlendMode::NORMAL)
                .size(display_size.x as f32, display_size.y as f32)
                .crop((curr_tex_response.x_offset_texture as f32, curr_tex_response.y_offset_texture as f32), ((tile_size.x) as f32, (tile_size.y) as f32))
                // .crop((size.0 * 0.5, size.1 * 0.5), (200., 200.))
                .translate(curr_ui_curs.x as f32, curr_ui_curs.y as f32);
                
                
                /*egui::Image::new(curr_tex_response.texture.texture_egui)
                    .maintain_aspect_ratio(false)
                    .fit_to_exact_size(egui::Vec2::new(
                        display_size.x as f32,
                        display_size.y as f32,
                    ))
                    .uv(egui::Rect::from_x_y_ranges(
                        uv_start.x as f32..=uv_end.x as f32,
                        uv_start.y as f32..=uv_end.y as f32,
                    ))
                    .paint_at(ui, r_ret);*/
    
                //Update display cursor
                curr_ui_curs.x += display_size.x;
                last_display_size_y = display_size.y;
    
                //Update coordinates for preview rectangle
                bbox_tl.x = bbox_tl.x.min(r_ret.left());
                bbox_tl.y = bbox_tl.y.min(r_ret.top());
                bbox_br.x = bbox_br.x.max(r_ret.right());
                bbox_br.y = bbox_br.y.max(r_ret.bottom());
            }
            //Update y coordinates
            y_coordinate += y_coordinate_increment;
            curr_ui_curs.y += last_display_size_y;
        }


























        /*let size = self.size();
        let width_tex = (texture_relsz.0*size.0 as f64) as i32;
        let scale_disp = width/width_tex as f32;

        let textures_start = (center.0 as i32-width_tex/2, center.1 as i32-width_tex/2);
        //TODO: Looped render

        //Loop control variables, start end end coordinates of interest
        let x_coordinate_end = (textures_start.0 + width_tex) as i32;
        let mut y_coordinate = textures_start.1 as i32;
        let y_coordinate_end = (textures_start.1 + width_tex) as i32;
        let base_ui_curs = nalgebra::Vector2::new(translation_x, translation_y);
        let mut curr_ui_curs = base_ui_curs;

        while y_coordinate <= y_coordinate_end {
            let mut y_coordinate_increment = 0; //increment for y coordinate after x loop
            let mut x_coordinate = textures_start.0;
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
                    curr_tex_end.x += (x_coordinate_end - curr_tex_response.x_tex_right_global).max(0);
                }
                if curr_tex_response.y_tex_bottom_global as f32 >= self.height() - 1.0f32 {
                    y_coordinate_increment = y_coordinate_end - y_coordinate + 1;
                    curr_tex_end.y += (y_coordinate_end - curr_tex_response.y_tex_bottom_global).max(0);
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

                draw.image(&curr_tex_response.texture.texture)
                    .blend_mode(BlendMode::NORMAL)
                    .size(width, width)
                    .crop((x_coordinate as f32, y_coordinate as f32), ((x_coordinate-curr_tex_end.x) as f32, (y_coordinate-curr_tex_end.y) as f32))
                    // .crop((size.0 * 0.5, size.1 * 0.5), (200., 200.))
                    .translate(translation_x as f32, translation_y as f32);

                curr_ui_curs.x      = (curr_tex_end.x-x_coordinate) as f32*scale_disp as f32;
                last_display_size_y = (curr_tex_end.y-y_coordinate) as f64*scale_disp as f64;

            }
            //Update y coordinates
            y_coordinate += y_coordinate_increment;
            curr_ui_curs.y += last_display_size_y as f32;
        }



        let mut tex_idx = 0;
        /*for _row_idx in 0..self.row_count {
            for _col_idx in 0..self.col_count {
                draw.image(&self.texture_array[tex_idx].texture)
                    .blend_mode(BlendMode::NORMAL)
                    .size(width, width)
                    .crop(textures_start, (width_tex, width_tex))
                    // .crop((size.0 * 0.5, size.1 * 0.5), (200., 200.))
                    .translate(translation_x as f32, translation_y as f32);
                tex_idx += 1;
            }
        }*/*/
        self.end_draw(draw);
    }

    pub fn unregister_textures(&mut self, gfx: &mut Graphics) {
        for text in &self.texture_array {
            gfx.egui_remove_texture(text.texture_egui.id);
        }
    }

    pub fn update_textures(&mut self, gfx: &mut Graphics, image: &DynamicImage) {        
        if self.col_count == 1 && self.row_count == 1 && self.image_format != image::ColorType::Rgb8{ //If it's rgb, use crop_imm and to_image to convert it to rgba
            if let Err(e) = gfx
                .update_texture(&mut self.texture_array[0].texture)
                .with_data(image.as_bytes())
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

                        match image {
                            DynamicImage::ImageLuma8(luma8_image) => {                           
                            let sub_img = imageops::crop_imm(
                                luma8_image,
                                tex_start_x,
                                tex_start_y,
                                tex_width,
                                tex_height,
                            );
                            let my_img = sub_img.to_image(); //TODO: This is an unnecessary copy!
                            if let Err(e) = gfx
                                .update_texture(&mut self.texture_array[tex_index].texture)
                                .with_data(my_img.as_ref())
                                .update()
                            {
                                error!("{e}");
                            }
                        }
                        other_image_type => {
                            let sub_img = imageops::crop_imm(
                                other_image_type,
                                tex_start_x,
                                tex_start_y,
                                tex_width,
                                tex_height,
                            );
                            let my_img = sub_img.to_image();

                            if let Err(e) = gfx
                                .update_texture(&mut self.texture_array[tex_index].texture)
                                .with_data(my_img.as_ref())
                                .update()
                            {
                                error!("{e}");
                            }
                        }
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

    fn begin_draw(&self, draw: &mut Draw) {
        if let Some(pip) = &self.pipeline {
            draw.image_pipeline().pipeline(pip);
        }
    }

    fn end_draw(&self, draw: &mut Draw) {
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
