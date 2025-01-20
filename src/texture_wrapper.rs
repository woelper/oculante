use crate::settings::PersistentSettings;
use crate::utils::ColorChannel;
use image::imageops;
use image::DynamicImage;
use log::debug;
use log::error;
use log::warn;
use notan::draw::*;
use notan::math::{Mat4, Vec4};
use notan::prelude::{BlendMode, Buffer, Graphics, ShaderSource, Texture, TextureFilter};
pub struct TexWrap {
    texture_array: Vec<Texture>,
    texture_boundary: Texture,
    pub col_count: u32,
    pub row_count: u32,
    pub col_translation: u32,
    pub row_translation: u32,
    pub size_vec: (f32, f32), // The whole Texture Array size
    pub texture_count: usize,
    pipeline: Option<notan::prelude::Pipeline>,
    pub format: notan::prelude::TextureFormat,
    pub image_format: image::ColorType,
    uniform_swizzle_mask: Buffer,
    uniform_offset_vec: Buffer,
}

#[derive(Default)]
pub struct TextureWrapperManager {
    current_texture: Option<TexWrap>,
}

impl TextureWrapperManager {
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

        let (swizzle_mat, offset_vec) = Self::get_mat_vec(settings.current_channel, img.color());
        self.current_texture = TexWrap::from_dynamic_image(gfx, settings, img, swizzle_mat, offset_vec);
    }

    pub fn update_color_selection(&mut self, gfx: &mut Graphics, settings: &PersistentSettings) {
        if let Some(tex) = &mut self.current_texture {
            let (mask, vec) = Self::get_mat_vec(settings.current_channel, tex.image_format);
            tex.update_uniform_buffer(gfx, mask, vec);
        }
    }

    fn get_mat_vec(channel_selection: ColorChannel, image_color: image::ColorType) -> (Mat4, Vec4) {
        //Currently we have two types of textures: rgba and gray ones.
        //All other types will be converted to rgba, so we only need to take care of those types here
        if image_color == image::ColorType::L8 || image_color == image::ColorType::L16 {
            Self::get_mat_vec_gray(channel_selection)
        } else {
            Self::get_mat_vec_rgba(channel_selection)
        }
    }

    fn get_mat_vec_gray(channel_selection: ColorChannel) -> (Mat4, Vec4) {
        let mut swizzle_mat = Mat4::ZERO;
        let mut offset_vec = Vec4::ZERO;
        match channel_selection {
            ColorChannel::Alpha => {
                offset_vec = Vec4::ONE; // Just plain white
            }
            _ => {
                //Every channel is the same, so we don't care
                swizzle_mat.x_axis = Vec4::new(1.0, 1.0, 1.0, 0.0);
                offset_vec.w = 1.0; //Alpha constant 1.0
            }
        }
        (swizzle_mat, offset_vec)
    }

    fn get_mat_vec_rgba(channel_selection: ColorChannel) -> (Mat4, Vec4) {
        let mut swizzle_mat = Mat4::ZERO;
        let mut offset_vec = Vec4::ZERO;
        
        match channel_selection {
            ColorChannel::Red => {
                swizzle_mat.x_axis = Vec4::new(1.0, 1.0, 1.0, 0.0);
                offset_vec.w = 1.0; //Alpha constant 1.0
            }
            ColorChannel::Green => {
                swizzle_mat.y_axis = Vec4::new(1.0, 1.0, 1.0, 0.0);
                offset_vec.w = 1.0; //Alpha constant 1.0
            }

            ColorChannel::Blue => {
                swizzle_mat.z_axis = Vec4::new(1.0, 1.0, 1.0, 0.0);
                offset_vec.w = 1.0; //Alpha constant 1.0
            }
            ColorChannel::Alpha => {
                swizzle_mat.w_axis = Vec4::new(1.0, 1.0, 1.0, 0.0);
                offset_vec.w = 1.0; //Alpha constant 1.0
            }
            ColorChannel::Rgb => {
                swizzle_mat = Mat4::from_rotation_x(0.0); //Diag
                swizzle_mat.w_axis = Vec4::new(0.0, 0.0, 0.0, 0.0); // Kill alpha
                offset_vec.w = 1.0; //Alpha constant 1.0
            }
            ColorChannel::Rgba => {
                swizzle_mat = Mat4::from_rotation_x(0.0); //Diag
            }
        }

        (swizzle_mat, offset_vec)
    }

    pub fn get(&mut self) -> &mut Option<TexWrap> {
        &mut self.current_texture
    }

    pub fn clear(&mut self /*, gfx: &mut Graphics */) {
        self.current_texture = None;
    }
}

pub struct TextureResponse<'a> {
    pub texture: &'a Texture,

    pub x_offset_texture: i32,
    pub y_offset_texture: i32,

    pub x_tex_left_global: i32,
    pub y_tex_top_global: i32,
    pub x_tex_right_global: i32,
    pub y_tex_bottom_global: i32,
}

//language=glsl
const FRAGMENT_IMAGE_RENDER: ShaderSource = notan::fragment_shader! {
    r#"
    #version 450
    precision mediump float;

    layout(location = 0) in vec2 v_uvs;
    layout(location = 1) in vec4 v_color;

    layout(binding = 0) uniform sampler2D u_texture;

    layout(binding = 1) uniform SwizzleMask {
        mat4 swizzle_mat;
    };

    layout(binding = 2) uniform OffsetVector {
        vec4 offset;
    };
    
    layout(location = 0) out vec4 color;

    void main() {
        vec4 tex_col = texture(u_texture, v_uvs);
        color = ((swizzle_mat*tex_col)+offset) * v_color;
    }
    "#
};

impl TexWrap {
    pub fn from_dynamic_image(
        gfx: &mut Graphics,
        settings: &PersistentSettings,
        image: &DynamicImage,
        swizzle_mask: Mat4,
        offset_vec: Vec4,
    ) -> Option<TexWrap> {
        Self::gen_from_dynamic_image(
            gfx,
            settings,
            image,
            Self::gen_texture_standard,
            swizzle_mask,
            offset_vec,
        )
    }

    fn gen_uniform_buffer_swizzle_mask(
        gfx: &mut Graphics,
        swizzle_mask: Mat4,
        offset_vec: Vec4,
    ) -> (Buffer, Buffer) {
        let uniform_swizzle_mask = gfx
            .create_uniform_buffer(1, "SwizzleMask")
            .with_data(&swizzle_mask)
            .build()
            .unwrap();

        let uniform_offset_vector = gfx
            .create_uniform_buffer(2, "OffsetVector")
            .with_data(&offset_vec)
            .build()
            .unwrap();

        (uniform_swizzle_mask, uniform_offset_vector)
    }

    fn update_uniform_buffer(&self, gfx: &mut Graphics, swizzle_mat: Mat4, offset_vec: Vec4) {
        gfx.set_buffer_data(&self.uniform_swizzle_mask, &swizzle_mat);
        gfx.set_buffer_data(&self.uniform_offset_vec, &offset_vec);
    }

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
            .build();

        match texture_result {
            Ok(texture) => return Some(texture),
            Err(error) => panic!("Problem generating texture: {error:?}"),
        };
    }

    fn image_color_supported(img: &DynamicImage) -> bool {
        img.color() == image::ColorType::L8
            //|| img.color() == image::ColorType::L16 TODO: Re-Enable when 16 Bit is implemented in notan
            || img.color() == image::ColorType::Rgba8
            || img.color() == image::ColorType::Rgba32F
    }

    fn image_bytesize_expected(img: &DynamicImage) -> Option<usize> {
        let pixel_count = (img.width() * img.height()) as usize;
        let byte_count: usize = img.color().bytes_per_pixel() as usize * pixel_count;

        // TODO: we could just do .filter(|byte_count| img.as_bytes().len()< byte_count) here
        // and have the function return an error

        if img.as_bytes().len() < byte_count {
            error!("Pixel buffer is smaller than expected!");
            return None;
        }
        Some(byte_count)
    }

    fn image_bytes_slice(img: &DynamicImage) -> Option<&[u8]> {
        Self::image_bytesize_expected(img).map(|byte_count| {
            let byte_buffer = img.as_bytes();
            if byte_count < byte_buffer.len() {
                warn!("Image byte buffer is bigger than expected. Will truncate.");
            }
            let (buff, _) = byte_buffer.split_at(byte_count);
            buff
        })
    }

    fn u16_to_u8_slice(slice: &mut [u16]) -> &mut [u8] {
        let byte_len = 2 * slice.len();
        unsafe { std::slice::from_raw_parts_mut(slice.as_mut_ptr().cast::<u8>(), byte_len) }
    }

    fn f32_to_u8_slice(slice: &mut [f32]) -> &mut [u8] {
        let byte_len = 4 * slice.len();
        unsafe { std::slice::from_raw_parts_mut(slice.as_mut_ptr().cast::<u8>(), byte_len) }
    }

    fn raw_copy_image_tile(
        byte_dest_buffer: &mut [u8],
        byte_src_buffer: &[u8],
        src_image_width: usize,
        offset_x_y: (usize, usize),
        size_w_h: (usize, usize),
        bytes_per_pixel: usize,
    ) {
        let mut dst_idx_start = 0 as usize;
        let mut src_idx_start = (offset_x_y.0 + offset_x_y.1 * src_image_width) * bytes_per_pixel;

        let row_increment_src = src_image_width * bytes_per_pixel;
        let row_increment_dst = size_w_h.0 * bytes_per_pixel;

        for _y in 0..size_w_h.1 {
            let dst_idx_end = dst_idx_start + row_increment_dst;
            let src_idx_end = src_idx_start + row_increment_dst;

            (*byte_dest_buffer)[dst_idx_start..dst_idx_end]
                .copy_from_slice(&byte_src_buffer[src_idx_start..src_idx_end]);
            dst_idx_start = dst_idx_end;

            src_idx_start += row_increment_src;
        }
    }

    fn image_tile_u8(image: &DynamicImage, offset: (u32, u32), size: (u32, u32)) -> Vec<u8> {
        let pixel_byte = image.color().bytes_per_pixel() as usize;
        // Creating luma 8 sub image
        let mut buff: Vec<u8> = vec![0; size.0 as usize * size.1 as usize * pixel_byte];
        let bytes_src = image.as_bytes();
        Self::raw_copy_image_tile(
            &mut buff,
            bytes_src,
            image.width() as usize,
            (offset.0 as usize, offset.1 as usize),
            (size.0 as usize, size.1 as usize),
            pixel_byte,
        );
        buff
    }

    fn image_tile_u16(image: &DynamicImage, offset: (u32, u32), size: (u32, u32)) -> Vec<u16> {
        let pixel_byte = image.color().bytes_per_pixel() as usize;
        let pixel_elements = pixel_byte / size_of::<u16>();
        let mut buff: Vec<u16> = vec![0; size.0 as usize * size.1 as usize * pixel_elements];
        let bytes_src = image.as_bytes();
        let slice: &mut [u8] = Self::u16_to_u8_slice(&mut buff);
        Self::raw_copy_image_tile(
            slice,
            bytes_src,
            image.width() as usize,
            (offset.0 as usize, offset.1 as usize),
            (size.0 as usize, size.1 as usize),
            pixel_byte,
        );
        buff
    }

    fn image_tile_f32(image: &DynamicImage, offset: (u32, u32), size: (u32, u32)) -> Vec<f32> {
        let pixel_byte = image.color().bytes_per_pixel() as usize;
        let pixel_elements = pixel_byte / size_of::<f32>();
        let mut buff: Vec<f32> = vec![0.0; size.0 as usize * size.1 as usize * pixel_elements];
        let bytes_src = image.as_bytes();
        let slice: &mut [u8] = Self::f32_to_u8_slice(&mut buff);
        Self::raw_copy_image_tile(
            slice,
            bytes_src,
            image.width() as usize,
            (offset.0 as usize, offset.1 as usize),
            (size.0 as usize, size.1 as usize),
            pixel_byte,
        );
        buff
    }

    fn get_dyn_image_part(
        image: &DynamicImage,
        offset: (u32, u32),
        size: (u32, u32),
    ) -> Option<DynamicImage> {
        let color_supported = Self::image_color_supported(image);

        if offset.0 == 0 && offset.1 == 0 && size.0 == image.width() && size.1 == image.height() {
            //Whole image, no tiling involved
            if color_supported {
                return None;
            } else {
                let depth = image.color().bytes_per_pixel() / image.color().channel_count();
                if depth == 1 {
                    debug!(
                        "Current image pixel type {:?} is not supported, will convert to rgba8",
                        image.color()
                    );
                    //Convert to rgba8 if current image is not supported
                    let img_rgba = image.to_rgba8();
                    return Some(DynamicImage::ImageRgba8(img_rgba));
                } else {
                    //Convert to rgba32 if current image is not supported
                    debug!(
                        "Current image pixel type {:?} is not supported, will convert to rgba32f",
                        image.color()
                    );
                    let img_rgba = image.to_rgba32f();
                    Some(DynamicImage::ImageRgba32F(img_rgba))
                }
            }
        } else {
            match image {
                //8 Bit types
                DynamicImage::ImageLuma8(_) => {
                    let bytes = Self::image_tile_u8(image, offset, size);
                    debug!("tiling {:?} to l8", image.color());
                    let gi: image::GrayImage =
                        image::GrayImage::from_raw(size.0, size.1, bytes).unwrap();
                    Some(DynamicImage::ImageLuma8(gi))
                }

                DynamicImage::ImageLumaA8(_) => {
                    let bytes = Self::image_tile_u8(image, offset, size);
                    debug!("tiling {:?} to rgba8", image.color());
                    let gi = DynamicImage::ImageLumaA8(
                        image::GrayAlphaImage::from_raw(size.0, size.1, bytes).unwrap(),
                    )
                    .to_rgba8();
                    Some(DynamicImage::ImageRgba8(gi))
                }

                DynamicImage::ImageRgba8(_) => {
                    let bytes = Self::image_tile_u8(image, offset, size);
                    debug!("tiling {:?} to rgba8", image.color());
                    let gi: image::RgbaImage =
                        image::RgbaImage::from_raw(size.0, size.1, bytes).unwrap();
                    return Some(DynamicImage::ImageRgba8(gi));
                }
                DynamicImage::ImageRgb8(_) => {
                    let bytes = Self::image_tile_u8(image, offset, size);
                    debug!("tiling {:?} to rgba8", image.color());
                    let gi = DynamicImage::ImageRgb8(
                        image::RgbImage::from_raw(size.0, size.1, bytes).unwrap(),
                    )
                    .to_rgba8();
                    Some(DynamicImage::ImageRgba8(gi))
                }

                //16 Bit types
                DynamicImage::ImageLuma16(_) => {
                    // Creating luma 16 sub image
                    let raw_data = Self::image_tile_u16(image, offset, size);
                    debug!("tiling {:?} to rgba32f", image.color());
                    let gi = image::ImageBuffer::<image::Luma<u16>, Vec<u16>>::from_raw(
                        size.0, size.1, raw_data,
                    )
                    .unwrap();
                    let converted_image = DynamicImage::ImageLuma16(gi).to_rgba32f(); //TODO: Remove when 16 Bit is implemented in notan
                    Some(DynamicImage::ImageRgba32F(converted_image))
                }

                DynamicImage::ImageLumaA16(_) => {
                    let raw_data = Self::image_tile_u16(image, offset, size);
                    debug!("tiling {:?} to rgba32f", image.color());
                    let gi = image::ImageBuffer::<image::LumaA<u16>, Vec<u16>>::from_raw(
                        size.0, size.1, raw_data,
                    )
                    .unwrap();
                    let converted_image = DynamicImage::ImageLumaA16(gi).to_rgba32f();
                    return Some(DynamicImage::ImageRgba32F(converted_image));
                }

                DynamicImage::ImageRgb16(_) => {
                    let raw_data = Self::image_tile_u16(image, offset, size);
                    debug!("tiling {:?} to rgba32f", image.color());

                    let gi = image::ImageBuffer::<image::Rgb<u16>, Vec<u16>>::from_raw(
                        size.0, size.1, raw_data,
                    )
                    .unwrap();
                    let converted_image = DynamicImage::ImageRgb16(gi).to_rgba32f();
                    Some(DynamicImage::ImageRgba32F(converted_image))
                }

                DynamicImage::ImageRgba16(_) => {
                    let raw_data = Self::image_tile_u16(image, offset, size);
                    debug!("tiling {:?} to rgba32f", image.color());

                    let gi = image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::from_raw(
                        size.0, size.1, raw_data,
                    )
                    .unwrap();
                    let converted_image = DynamicImage::ImageRgba16(gi).to_rgba32f();
                    Some(DynamicImage::ImageRgba32F(converted_image))
                }

                //32 Bit types
                DynamicImage::ImageRgb32F(_) => {
                    let raw_data = Self::image_tile_f32(image, offset, size);
                    debug!("tiling {:?} to rgba32f", image.color());
                    let gi = DynamicImage::ImageRgb32F(
                        image::Rgb32FImage::from_raw(size.0, size.1, raw_data).unwrap(),
                    )
                    .to_rgba32f();
                    Some(DynamicImage::ImageRgba32F(gi))
                }

                DynamicImage::ImageRgba32F(_) => {
                    let raw_data = Self::image_tile_f32(image, offset, size);
                    debug!("tiling {:?} to rgba32f", image.color());
                    let gi = image::Rgba32FImage::from_raw(size.0, size.1, raw_data).unwrap();
                    Some(DynamicImage::ImageRgba32F(gi))
                }

                other_image_type => {
                    //will be converted to rgba8 automatically
                    let sub_img =
                        imageops::crop_imm(other_image_type, offset.0, offset.1, size.0, size.1);
                    let my_img = sub_img.to_image();
                    Some(DynamicImage::ImageRgba8(my_img))
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
        let pipeline: Option<notan::prelude::Pipeline> =
            Some(create_image_pipeline(gfx, Some(&FRAGMENT_IMAGE_RENDER)).unwrap());
        debug!("{:?}", image.color());
        match image.color() {
            image::ColorType::L8 => {
                format = notan::prelude::TextureFormat::R8;
                //pipeline = Some(create_image_pipeline(gfx, Some(&FRAGMENT_GRAYSCALE)).unwrap());
            }
            //TODO: Re-Enable when 16 Bit is implemented in notan
            /*image::ColorType::L16 => {
                format = notan::prelude::TextureFormat::R16Uint;
                //pipeline = Some(create_image_pipeline(gfx, Some(&FRAGMENT_GRAYSCALE)).unwrap());
            }*/            
            image::ColorType::Rgba32F => {
                format = notan::prelude::TextureFormat::Rgba32Float;
            }
            _ => {
                //All non supported formats will be converted, make sure we set the right type then. Everything deeper than 1 byte per pixel will be rgba32f then.
                let depth = image.color().bytes_per_pixel() / image.color().channel_count();
                if depth > 1 {
                    format = notan::prelude::TextureFormat::Rgba32Float;
                }
            }
        }
        (format, pipeline)
    }

    fn gen_from_dynamic_image(
        gfx: &mut Graphics,
        settings: &PersistentSettings,
        src_image: &DynamicImage,
        texture_generator_function: fn(
            &mut Graphics,
            &[u8],
            u32,
            u32,
            notan::prelude::TextureFormat,
            &PersistentSettings,
            bool,
        ) -> Option<Texture>,
        swizzle_mask: Mat4,
        add_vec: Vec4,
    ) -> Option<TexWrap> {
        const MAX_PIXEL_COUNT: usize = 8192 * 8192;

        let image = src_image;

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

        let mut texture_vec: Vec<Texture> = Vec::new();
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
                    if let Some(bt_slice) = Self::image_bytes_slice(&suba_img) {
                        //Image needed conversion or tiling
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
                    if let Some(bt_slice) = Self::image_bytes_slice(&image) {
                        //Use input image directly
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
                    texture_vec.push(t);
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

        let boundary_pixels_bytes: [u8; 4] = [0, 0, 0, 255];
        let texture_boundary: Result<Texture, String> = gfx
            .create_texture()
            .from_bytes(&boundary_pixels_bytes, 1, 1)
            .with_format(notan::app::TextureFormat::Rgba32)
            .build();

        if fine {
            let texture_count = texture_vec.len();
            let (uniforms, uniforms2) =
                Self::gen_uniform_buffer_swizzle_mask(gfx, swizzle_mask, add_vec);
            Some(TexWrap {
                texture_boundary: texture_boundary.unwrap(),
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
                uniform_swizzle_mask: uniforms,
                uniform_offset_vec: uniforms2,
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
                draw.image(&self.texture_array[tex_idx])
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
        //print!("Start x: {}, y: {}\n",xy_tex_center.0 - xy_tex_size.0,y_coordinate);

        while y_coordinate <= y_coordinate_end {
            let mut y_coordinate_new = i32::MAX; //increment for y coordinate after x loop
            let mut x_coordinate = xy_tex_center.0 - xy_tex_size.0;
            curr_ui_curs.x = base_ui_curs.x;
            let mut last_display_size_y = f64::MAX;
            while x_coordinate <= x_coordinate_end {
                //get texture tile
                let curr_tex_response =
                    self.get_texture_at_xy(x_coordinate as i32, y_coordinate as i32);

                //print!("x: {} y: {} ", x_coordinate, y_coordinate);
                //print!("top: {} left: {} ", curr_tex_response.y_tex_top_global, curr_tex_response.x_tex_left_global);
                //print!("bottom: {} right: {} \n", curr_tex_response.y_tex_bottom_global, curr_tex_response.x_tex_right_global);

                //Handling last texture in a row or col
                let curr_tex_end = nalgebra::Vector2::new(
                    i32::min(curr_tex_response.x_tex_right_global, x_coordinate_end),
                    i32::min(curr_tex_response.y_tex_bottom_global, y_coordinate_end),
                );

                //Usable tile size, depending on offsets
                let tile_size = nalgebra::Vector2::new(
                    curr_tex_end.x - x_coordinate + 1,
                    curr_tex_end.y - y_coordinate + 1,
                );

                //Display size - tile size scaled
                let display_size = nalgebra::Vector2::new(
                    ((tile_size.x) as f64 / (2 * width_tex + 1) as f64) * width as f64,
                    ((tile_size.y) as f64 / (2 * width_tex + 1) as f64) * width as f64,
                );

                draw.image(&curr_tex_response.texture)
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

                x_coordinate = curr_tex_response.x_tex_right_global + 1;
                y_coordinate_new = y_coordinate_new.min(curr_tex_response.y_tex_bottom_global + 1);
                //Update display cursor
                curr_ui_curs.x += display_size.x;
                last_display_size_y = last_display_size_y.min(display_size.y);
            }
            //Update y coordinates
            //print!("new y: {}, old y: {} \n", y_coordinate_new, y_coordinate);
            y_coordinate = y_coordinate_new;
            curr_ui_curs.y += last_display_size_y;
        }
        //print!("\n");
        self.remove_draw_shader(draw);

        //Draw crosshair
        //let stroke_width = 0.5;
        let half_width = scale/4.0/*-stroke_width*/;

        draw.rect(
            (translation_x + width / 2.0 - half_width, translation_y),
            (2.0 * half_width, width),
        )
        /*.fill()
        .stroke(stroke_width)
        .stroke_color(notan::app::Color { r: (0.0), g: (0.0), b: (0.0), a: (1.0) }) */;
        draw.rect(
            (translation_x, translation_y + width / 2.0 - half_width),
            (width, 2.0 * half_width),
        )
        /* .fill()
        .stroke(stroke_width)
        .stroke_color(notan::app::Color { r: (0.0), g: (0.0), b: (0.0), a: (1.0) })*/;
    }

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
                            .update_texture(&mut self.texture_array[tex_index])
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
                            .update_texture(&mut self.texture_array[tex_index])
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

    pub fn get_dummy_texture_at_xy(&self, xa: i32, ya: i32) -> TextureResponse {
        let tex_width_int = self.width() as i32;
        let tex_height_int = self.height() as i32;

        let width: i32;
        let height: i32;
        width = if xa < 0 { xa.abs() } else { tex_width_int };
        height = if ya < 0 { ya.abs() } else { tex_height_int };

        TextureResponse {
            texture: &self.texture_boundary,
            x_offset_texture: 0,
            y_offset_texture: 0,
            x_tex_left_global: xa,
            y_tex_top_global: ya,
            x_tex_right_global: xa + width - 1,
            y_tex_bottom_global: ya + height - 1,
        }
    }

    pub fn get_texture_at_xy(&self, xa: i32, ya: i32) -> TextureResponse {
        let width_int = self.width() as i32;
        let height_int = self.height() as i32;

        //Dummy texture for outside of boundary
        if xa < 0 || ya < 0 || xa >= width_int || ya >= height_int {
            return self.get_dummy_texture_at_xy(xa, ya);
        }

        let x = xa.max(0).min(self.width() as i32 - 1);
        let y = ya.max(0).min(self.height() as i32 - 1);

        //Div by zero possible, never allow zero sized textures!
        let x_idx = x / self.col_translation as i32;
        let y_idx = y / self.row_translation as i32;
        let tex_idx =
            (y_idx * self.col_count as i32 + x_idx).min(self.texture_array.len() as i32 - 1);
        let my_tex_pair = &self.texture_array[tex_idx as usize];
        let my_tex = &my_tex_pair;
        let width = my_tex.width() as i32;
        let height = my_tex.height() as i32;

        let tex_left = x_idx * self.col_translation as i32;
        let tex_top = y_idx * self.row_translation as i32;
        let tex_right = tex_left + width - 1;
        let tex_bottom = tex_top + height - 1;

        let x_offset_texture = xa - tex_left;
        let y_offset_texture = ya - tex_top;

        TextureResponse {
            texture: my_tex_pair,
            x_offset_texture: x_offset_texture,
            y_offset_texture: y_offset_texture,
            x_tex_left_global: tex_left,
            y_tex_top_global: tex_top,
            x_tex_right_global: tex_right,
            y_tex_bottom_global: tex_bottom,
        }
    }

    fn add_draw_shader(&self, draw: &mut Draw) {
        if let Some(pip) = &self.pipeline {
            draw.image_pipeline()
                .pipeline(pip)
                .uniform_buffer(&self.uniform_swizzle_mask)
                .uniform_buffer(&self.uniform_offset_vec);
        }
    }

    fn remove_draw_shader(&self, draw: &mut Draw) {
        if self.pipeline.is_some() {
            draw.image_pipeline().remove();
        }
    }

    pub fn size(&self) -> (f32, f32) {
        self.size_vec
    }

    pub fn width(&self) -> f32 {
        self.size_vec.0
    }

    pub fn height(&self) -> f32 {
        self.size_vec.1
    }
}
