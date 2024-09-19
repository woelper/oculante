use notan::draw::*;

use crate::Draw;
use crate::settings::PersistentSettings;
use image::imageops;


use image::RgbaImage;

use notan::prelude::{Graphics, Texture, TextureFilter, BlendMode};
use log::error;

pub struct TexWrap{
    texture_array:Vec<Texture>,
    pub col_count:u32,
    pub row_count:u32,
    pub col_translation:u32,
    pub row_translation:u32,
    pub size_vec:(f32,f32) // The whole Texture Array size
}

pub struct TextureResponse<'a>{
    pub texture: &'a Texture,
    pub u_tex_left_global : f64, 
    pub v_tex_top_global : f64, 
    pub u_offset_texture : f64, 
    pub v_offset_texture : f64, 
    pub u_tex_right_global : f64, 
    pub v_tex_bottom_global : f64,
    pub u_tex_next_right_global : f64, 
    pub v_tex_next_bottom_global : f64,
    pub u_scale:f64,
    pub v_scale:f64
}

impl TexWrap{
    /*pub fn new(texture: Texture) -> Self{
        let s = texture.size();
        TexWrap { tex: texture, size_vec:s }        
    }*/

    //pub fn from_rgba_image_premult(gfx: &mut Graphics, linear_mag_filter: bool, image: &RgbaImage) -> Option<TexWrap>{}

    pub fn from_rgbaimage(gfx: &mut Graphics, settings: &PersistentSettings, image: &RgbaImage) -> Option<TexWrap>{
        Self::gen_from_rgbaimage(gfx, settings, image, Self::gen_texture_standard)
    }

    pub fn from_rgbaimage_premult(gfx: &mut Graphics, settings: &PersistentSettings, image: &RgbaImage) -> Option<TexWrap>{
        Self::gen_from_rgbaimage(gfx, settings, image, Self::gen_texture_premult)
    }

    fn gen_texture_standard(gfx: &mut Graphics, bytes:&[u8], width:u32, height:u32, settings: &PersistentSettings)-> Option<Texture>
    {
        gfx.create_texture()
                    .from_bytes(bytes, width, height)
                    .with_mipmaps(settings.use_mipmaps)
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

    fn gen_texture_premult(gfx: &mut Graphics, bytes:&[u8], width:u32, height:u32, settings: &PersistentSettings)-> Option<Texture>
    {
        gfx.create_texture()
                    .from_bytes(bytes, width, height)
                    .with_premultiplied_alpha()
                    .with_mipmaps(settings.use_mipmaps)
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

    fn gen_from_rgbaimage(gfx: &mut Graphics, settings: &PersistentSettings, image: &RgbaImage, fuuun: fn (&mut Graphics, &[u8], u32, u32, &PersistentSettings)-> Option<Texture>) -> Option<TexWrap>{
        
        let im_w = image.width();
        let im_h = image.height();
        let s = (im_w as f32, im_h as f32);
        let max_texture_size = gfx.limits().max_texture_size; //128;//
        let col_count = (im_w as f32/max_texture_size as f32).ceil() as u32;       
        let row_count = (im_h as f32/max_texture_size as f32).ceil() as u32;        

        let mut a:Vec<Texture> = Vec::new();
        let row_increment = std::cmp::min(max_texture_size, im_h);
        let col_increment = std::cmp::min(max_texture_size, im_w);
        let mut fine = true;
        
        for row_index in  0..row_count {
            let tex_start_y = row_index*row_increment;
            let tex_height = std::cmp::min(
                row_increment,
                im_h-tex_start_y
            );
            for col_index in  0..col_count {
                let tex_start_x = col_index*col_increment;
                let tex_width = std::cmp::min(
                    col_increment,
                    im_w-tex_start_x
                );
                
                let sub_img = imageops::crop_imm(image, tex_start_x, tex_start_y, tex_width, tex_height);
                let my_img = sub_img.to_image();
                let tex = fuuun(gfx, my_img.as_ref(), my_img.width(), my_img.height(), settings);
                
                    if let Some(t) = tex {
                        a.push(t);
                    }
                    else{
                        a.clear();
                        fine = false;
                        break;
                    }                  
            }
            if fine == false {
                break;
            }
        }
        
        if fine {
        Some(TexWrap {size_vec:s, col_count:col_count, row_count:row_count,texture_array:a, col_translation:col_increment, row_translation:row_increment })
    }
    else {
        None
    }
    }

    pub fn draw_textures(&self,draw: &mut Draw, translation_x:f32, translation_y:f32, scale: f32){
        let mut tex_idx = 0;
            for row_idx in 0..self.row_count
            {
                let translate_y = translation_y+scale*row_idx as f32*self.row_translation as f32;
                for col_idx in 0..self.col_count
                {
                    let translate_x = translation_x+scale*col_idx as f32 *self.col_translation as f32;
                    draw.image(&self.texture_array[tex_idx])
                        .blend_mode(BlendMode::NORMAL)
                        .scale(scale, scale)
                        .translate(translate_x.trunc(), translate_y.trunc()); //truncation to avoid artifacts
                    tex_idx += 1;
                }
            }
    }

    pub fn update_textures(&mut self, gfx: &mut Graphics, image: &RgbaImage){
        if self.col_count==1 && self.row_count==1 {
            if let Err(e) = gfx.update_texture(&mut self.texture_array[0]).with_data(image).update() {
                error!("{e}");
            }
        }else{
            let mut tex_index = 0;
            for row_index in  0..self.row_count {
                let tex_start_y = row_index*self.row_translation;
                let tex_height = std::cmp::min(
                    self.row_translation,
                    image.height()-tex_start_y
                );
                for col_index in  0..self.col_count {
                    let tex_start_x = col_index*self.col_translation;
                    let tex_width = std::cmp::min(
                        self.col_translation,
                        image.width()-tex_start_x
                    );
                    
                    let sub_img = imageops::crop_imm(image, tex_start_x, tex_start_y, tex_width, tex_height);
                    let my_img = sub_img.to_image();
                    if let Err(e) = gfx.update_texture(&mut self.texture_array[tex_index]).with_data(my_img.as_ref()).update() {
                        error!("{e}");
                    }
                    tex_index += 1;
                }
            }
        }
    }

    pub fn get_texture_at_uv(&self, ua:f64, va:f64)->TextureResponse {
        let xa = ua as f64*self.width()as f64;
        let ya = va as f64*self.height()as f64;
        
        let v =  (va).max(0.0).min(1.0)as f64;
        let u =  ua.max(0.0).min(1.0)as f64;
        let x = u*self.width()as f64;
        let y = v*self.height()as f64;

        let x_idx = (x /self.col_translation as f64).floor() as i32;
        let y_idx = (y /self.row_translation as f64).floor() as i32;
        let tex_idx = (y_idx*self.col_count as i32+x_idx).min(self.texture_array.len() as i32 -1);
        let my_tex = &self.texture_array[tex_idx as usize];
        
        let tex_left = x_idx*self.col_translation as i32;
        let tex_top = y_idx*self.row_translation as i32;
        
        //the current texture might be smaller than the set translation. (Last element on x or y axis)
        let tex_right_next = tex_left+my_tex.width() as i32;
        let tex_bottom_next = tex_top+my_tex.height() as i32;
        let tex_right = tex_right_next;
        let tex_bottom = tex_bottom_next;
        let u_scale = my_tex.width() as f64/self.width() as f64;
        let v_scale = my_tex.height() as f64/self.height() as f64;

        
        let u_tex_left_global = tex_left as f64/self.width() as f64;
        let v_tex_top_global = tex_top as f64/self.height() as f64;
        
        let u_offset = (xa-tex_left as f64)/my_tex.width()as f64;
        let v_offset = (ya-tex_top as f64)/my_tex.height()as f64;
        
        let u_tex_right = tex_right as f64 /self.width()as f64;
        let v_tex_bottom = tex_bottom as f64 /self.height()as f64;
        let u_tex_next_right_global = tex_right_next as f64 /self.width()as f64;
        let v_tex_next_bottom_global = tex_bottom_next as f64 /self.height()as f64;

        
        
        TextureResponse {texture: my_tex, 
            u_tex_left_global,
            v_tex_top_global,
            u_offset_texture:u_offset, 
            v_offset_texture:v_offset, 
            u_tex_right_global:u_tex_right, 
            v_tex_bottom_global:v_tex_bottom,
            u_tex_next_right_global,
            v_tex_next_bottom_global,
            u_scale:u_scale,
            v_scale:v_scale }
    }

    pub fn size(&self)->(f32,f32){
        return self.size_vec;
    }

    pub fn width(&self)-> f32 {
        return self.size_vec.0;
    }

    pub fn height(&self)-> f32 {
        return self.size_vec.1;
    }
}