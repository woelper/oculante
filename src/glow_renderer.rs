/// Custom glow (OpenGL) renderer for image display.
///
/// The image is kept on the GPU as a grid of texture tiles, which are drawn
/// from egui paint callbacks: the main view and the zoom preview.
use crate::utils::ColorChannel;
use glam::{Mat4, Vec4};
use glow::HasContext;

/// Texture formats we support
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TexFormat {
    Rgba8,
    Rgb8,
    /// Gray with alpha: gray in the red, alpha in the green channel
    Rg8,
    R8,
    Rgba32F,
    SRgba8,
}

/// A GPU texture handle with metadata
pub struct GlowTexture {
    pub texture: glow::Texture,
    pub width: u32,
    pub height: u32,
    pub format: TexFormat,
}

/// How a texture is sampled when it is scaled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TexFilter {
    /// Interpolate when zooming out
    pub linear_min: bool,
    /// Interpolate when zooming in
    pub linear_mag: bool,
    pub mipmaps: bool,
}

/// The GL handles needed to draw with the image shader. Cheap to copy into
/// egui paint callbacks, where `&GlowRenderer` is not available.
#[derive(Debug, Clone, Copy)]
pub struct ImageShader {
    program: glow::Program,
    vao: glow::VertexArray,
}

/// Target tile edge length. Effective tile size is `min(TILE_TARGET, max_texture_size)`.
/// 4096 keeps each RGBA8 tile at ~64 MB so allocation is safe on every modern GPU,
/// and lets typical images (<= 4096 in both dims) collapse to exactly one tile.
pub const TILE_TARGET: u32 = 4096;

/// One tile of an image. The tile owns its own GL texture and remembers
/// its position within the parent image in image-pixel coordinates.
pub struct GlowTile {
    pub texture: GlowTexture,
    pub x: u32,
    pub y: u32,
}

/// The glow-based renderer. Holds compiled shaders and shared GL state.
pub struct GlowRenderer {
    /// Shader program for textured quads with swizzle/offset uniforms
    image_program: glow::Program,
    /// Fullscreen quad VAO (two triangles covering clip space, UVs computed from position)
    quad_vao: glow::VertexArray,
    /// Max texture size for this GPU
    pub max_texture_size: u32,
}

// The `#version` line is prepended in `compile_program`, depending on the GL flavour.

// Vertex shader of the image program
const VERTEX_SHADER: &str = r#"
uniform vec2 u_offset;
uniform vec2 u_scale;
uniform vec2 u_viewport;
// For crop: uv offset and scale
uniform vec2 u_uv_offset;
uniform vec2 u_uv_scale;
// Quad size in pixels
uniform vec2 u_size;

out vec2 v_uv;

void main() {
    // Triangle strip: 0,1,2,3 -> quad corners
    vec2 pos = vec2(gl_VertexID & 1, (gl_VertexID >> 1) & 1);
    v_uv = u_uv_offset + pos * u_uv_scale;

    // Position in pixels
    vec2 pixel_pos = u_offset + pos * u_size * u_scale;
    // Convert to clip space: [-1, 1]
    vec2 clip = (pixel_pos / u_viewport) * 2.0 - 1.0;
    clip.y = -clip.y; // flip Y (screen coords -> GL)
    gl_Position = vec4(clip, 0.0, 1.0);
}
"#;

const IMAGE_FRAGMENT_SHADER: &str = r#"
precision highp float;
in vec2 v_uv;
uniform sampler2D u_texture;
uniform mat4 u_swizzle_mat;
uniform vec4 u_offset_vec;
// Sample texel centers of the base level, so the result is unfiltered
// regardless of the texture's filter settings (used by the zoom preview)
uniform bool u_snap_texels;
out vec4 color;
void main() {
    vec4 tex_col;
    if (u_snap_texels) {
        vec2 tex_size = vec2(textureSize(u_texture, 0));
        tex_col = textureLod(u_texture, (floor(v_uv * tex_size) + 0.5) / tex_size, 0.0);
    } else {
        tex_col = texture(u_texture, v_uv);
    }
    color = (u_swizzle_mat * tex_col) + u_offset_vec;
}
"#;

impl GlowRenderer {
    /// Create a new renderer. Call this once with the GL context.
    pub fn new(gl: &glow::Context) -> Self {
        let image_program = compile_program(gl, VERTEX_SHADER, IMAGE_FRAGMENT_SHADER);

        // Empty VAO for attribute-less rendering (we compute positions from gl_VertexID)
        let quad_vao = unsafe { gl.create_vertex_array().expect("Failed to create VAO") };
        let max_texture_size = unsafe { gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE) } as u32;

        Self {
            image_program,
            quad_vao,
            max_texture_size,
        }
    }

    /// Handles for drawing with `paint_quads`.
    pub fn image_shader(&self) -> ImageShader {
        ImageShader {
            program: self.image_program,
            vao: self.quad_vao,
        }
    }

    /// Upload a new texture from raw bytes.
    pub fn create_texture(
        &self,
        gl: &glow::Context,
        bytes: &[u8],
        width: u32,
        height: u32,
        format: TexFormat,
        filter: TexFilter,
    ) -> GlowTexture {
        unsafe {
            let texture = gl.create_texture().expect("Failed to create texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));

            let (internal, fmt, typ) = gl_format(format);
            // Rows of one or three bytes per pixel do not end on four byte boundaries
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                internal as i32,
                width as i32,
                height as i32,
                0,
                fmt,
                typ,
                glow::PixelUnpackData::Slice(Some(bytes)),
            );

            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_S,
                glow::CLAMP_TO_EDGE as i32,
            );
            gl.tex_parameter_i32(
                glow::TEXTURE_2D,
                glow::TEXTURE_WRAP_T,
                glow::CLAMP_TO_EDGE as i32,
            );
            apply_filter(gl, filter);

            gl.bind_texture(glow::TEXTURE_2D, None);

            GlowTexture {
                texture,
                width,
                height,
                format,
            }
        }
    }

    /// Update an existing texture's data (must be same dimensions and format).
    /// Mipmaps are not regenerated, follow up with `set_filter` for that.
    pub fn update_texture(&self, gl: &glow::Context, tex: &GlowTexture, bytes: &[u8]) {
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(tex.texture));
            let (_internal, fmt, typ) = gl_format(tex.format);
            gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 1);
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                tex.width as i32,
                tex.height as i32,
                fmt,
                typ,
                glow::PixelUnpackData::Slice(Some(bytes)),
            );
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    /// Change the filtering of an existing texture. Regenerates mipmaps if they are enabled.
    pub fn set_filter(&self, gl: &glow::Context, tex: &GlowTexture, filter: TexFilter) {
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(tex.texture));
            apply_filter(gl, filter);
            gl.bind_texture(glow::TEXTURE_2D, None);
        }
    }

    /// Delete a texture.
    pub fn delete_texture(&self, gl: &glow::Context, tex: GlowTexture) {
        unsafe {
            gl.delete_texture(tex.texture);
        }
    }

    /// Clean up GL resources.
    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.image_program);
            gl.delete_vertex_array(self.quad_vao);
        }
    }

    /// Effective tile edge length for this renderer (clamped to GPU limit).
    pub fn tile_size(&self) -> u32 {
        TILE_TARGET.min(self.max_texture_size).max(1)
    }

    /// Slice a full-image buffer into tiles, uploading each as its own GPU texture.
    /// For images that fit in a single tile this is a single allocation/upload — no per-tile copy.
    pub fn create_tiles(
        &self,
        gl: &glow::Context,
        bytes: &[u8],
        width: u32,
        height: u32,
        format: TexFormat,
        filter: TexFilter,
    ) -> Vec<GlowTile> {
        if width == 0 || height == 0 {
            return Vec::new();
        }

        let cap = self.tile_size();
        let cols = width.div_ceil(cap);
        let rows = height.div_ceil(cap);
        let mut tiles = Vec::with_capacity((cols * rows) as usize);

        // Single-tile fast path: forward the original byte slice with no copy.
        if cols == 1 && rows == 1 {
            let texture = self.create_texture(gl, bytes, width, height, format, filter);
            tiles.push(GlowTile {
                texture,
                x: 0,
                y: 0,
            });
            return tiles;
        }

        let bpp = bytes_per_pixel(format);
        let stride = width as usize * bpp;

        for row in 0..rows {
            let ty = row * cap;
            let th = cap.min(height - ty);
            for col in 0..cols {
                let tx = col * cap;
                let tw = cap.min(width - tx);

                let mut tile_bytes = vec![0u8; tw as usize * th as usize * bpp];
                for y in 0..th {
                    let src_off = (ty + y) as usize * stride + tx as usize * bpp;
                    let dst_off = y as usize * tw as usize * bpp;
                    let len = tw as usize * bpp;
                    tile_bytes[dst_off..dst_off + len]
                        .copy_from_slice(&bytes[src_off..src_off + len]);
                }

                let texture = self.create_texture(gl, &tile_bytes, tw, th, format, filter);
                tiles.push(GlowTile {
                    texture,
                    x: tx,
                    y: ty,
                });
            }
        }

        tiles
    }

    /// Update existing tiles in place when the image dimensions and the format match the
    /// current grid. Returns `false` if they differ (caller must delete and re-create).
    #[allow(clippy::too_many_arguments)]
    pub fn update_tiles(
        &self,
        gl: &glow::Context,
        tiles: &[GlowTile],
        bytes: &[u8],
        width: u32,
        height: u32,
        format: TexFormat,
        filter: TexFilter,
    ) -> bool {
        if tiles.is_empty() || width == 0 || height == 0 || tiles[0].texture.format != format {
            return false;
        }

        let cap = self.tile_size();
        let cols = width.div_ceil(cap);
        let rows = height.div_ceil(cap);
        if tiles.len() != (cols * rows) as usize {
            return false;
        }

        // Verify each tile sits where the current grid would place it.
        for (i, tile) in tiles.iter().enumerate() {
            let row = i as u32 / cols;
            let col = i as u32 % cols;
            let ex = col * cap;
            let ey = row * cap;
            let ew = cap.min(width - ex);
            let eh = cap.min(height - ey);
            if tile.x != ex || tile.y != ey || tile.texture.width != ew || tile.texture.height != eh
            {
                return false;
            }
        }

        // Single-tile fast path: no copy.
        if tiles.len() == 1 {
            self.update_texture(gl, &tiles[0].texture, bytes);
            self.set_filter(gl, &tiles[0].texture, filter);
            return true;
        }

        let bpp = bytes_per_pixel(tiles[0].texture.format);
        let stride = width as usize * bpp;

        for tile in tiles {
            let tw = tile.texture.width;
            let th = tile.texture.height;
            let mut sub = vec![0u8; tw as usize * th as usize * bpp];
            for y in 0..th {
                let src_off = (tile.y + y) as usize * stride + tile.x as usize * bpp;
                let dst_off = y as usize * tw as usize * bpp;
                let len = tw as usize * bpp;
                sub[dst_off..dst_off + len].copy_from_slice(&bytes[src_off..src_off + len]);
            }
            self.update_texture(gl, &tile.texture, &sub);
            self.set_filter(gl, &tile.texture, filter);
        }

        true
    }

    /// Delete all tiles' GPU textures.
    pub fn delete_tiles(&self, gl: &glow::Context, tiles: Vec<GlowTile>) {
        for tile in tiles {
            self.delete_texture(gl, tile.texture);
        }
    }
}

impl GlowTile {
    /// The quad covering this tile, for an image drawn at `offset` with `scale`.
    pub fn quad(&self, offset: [f32; 2], scale: f32) -> Quad {
        Quad {
            texture: self.texture.texture,
            pos: [
                offset[0] + self.x as f32 * scale,
                offset[1] + self.y as f32 * scale,
            ],
            size: [
                self.texture.width as f32 * scale,
                self.texture.height as f32 * scale,
            ],
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
        }
    }
}

/// A textured rectangle on screen: where to draw (in egui points) and which
/// part of the texture to show.
#[derive(Debug, Clone, Copy)]
pub struct Quad {
    pub texture: glow::Texture,
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
}

/// Draw textured quads from within an egui paint callback, with one
/// program/state setup and one draw call per quad.
///
/// With `snap_texels` the quads are drawn unfiltered, whatever filter their
/// textures use.
pub fn paint_quads(
    gl: &glow::Context,
    shader: ImageShader,
    info: &egui::PaintCallbackInfo,
    swizzle_mat: &[f32; 16],
    offset_vec: &[f32; 4],
    snap_texels: bool,
    quads: &[Quad],
) {
    if quads.is_empty() {
        return;
    }
    let ImageShader { program, vao } = shader;
    let screen_size_px = info.screen_size_px;
    // Quads are positioned in egui points, relative to the whole window
    let viewport = [
        screen_size_px[0] as f32 / info.pixels_per_point,
        screen_size_px[1] as f32 / info.pixels_per_point,
    ];
    unsafe {
        gl.viewport(0, 0, screen_size_px[0] as i32, screen_size_px[1] as i32);
        gl.enable(glow::BLEND);
        // Blend the color, but leave the alpha of the window untouched. Otherwise half
        // transparent pixels make the window itself translucent on Wayland, and
        // whatever is behind it shines through (#342).
        gl.blend_func_separate(
            glow::SRC_ALPHA,
            glow::ONE_MINUS_SRC_ALPHA,
            glow::ZERO,
            glow::ONE,
        );

        gl.use_program(Some(program));
        gl.bind_vertex_array(Some(vao));

        set_uniform_2f(gl, program, "u_scale", [1.0, 1.0]);
        set_uniform_2f(gl, program, "u_viewport", viewport);

        let mat_loc = gl.get_uniform_location(program, "u_swizzle_mat");
        gl.uniform_matrix_4_f32_slice(mat_loc.as_ref(), false, swizzle_mat);
        let off_loc = gl.get_uniform_location(program, "u_offset_vec");
        gl.uniform_4_f32_slice(off_loc.as_ref(), offset_vec);
        let snap_loc = gl.get_uniform_location(program, "u_snap_texels");
        gl.uniform_1_i32(snap_loc.as_ref(), snap_texels as i32);

        gl.active_texture(glow::TEXTURE0);
        let tex_loc = gl.get_uniform_location(program, "u_texture");
        gl.uniform_1_i32(tex_loc.as_ref(), 0);

        for quad in quads {
            set_uniform_2f(gl, program, "u_offset", quad.pos);
            set_uniform_2f(gl, program, "u_size", quad.size);
            set_uniform_2f(gl, program, "u_uv_offset", quad.uv_offset);
            set_uniform_2f(gl, program, "u_uv_scale", quad.uv_scale);
            gl.bind_texture(glow::TEXTURE_2D, Some(quad.texture));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }

        // the next use of the program expects filtered sampling unless it asks otherwise
        gl.uniform_1_i32(snap_loc.as_ref(), 0);
        gl.bind_texture(glow::TEXTURE_2D, None);
        gl.use_program(None);
        gl.bind_vertex_array(None);
    }
}

/// Set min/mag filter of the bound texture and (re)generate mipmaps if enabled.
unsafe fn apply_filter(gl: &glow::Context, filter: TexFilter) {
    let min_filter = if filter.mipmaps {
        if filter.linear_min {
            glow::LINEAR_MIPMAP_LINEAR
        } else {
            glow::NEAREST_MIPMAP_NEAREST
        }
    } else if filter.linear_min {
        glow::LINEAR
    } else {
        glow::NEAREST
    };
    let mag_filter = if filter.linear_mag {
        glow::LINEAR
    } else {
        glow::NEAREST
    };
    unsafe {
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            min_filter as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            mag_filter as i32,
        );
        if filter.mipmaps {
            gl.generate_mipmap(glow::TEXTURE_2D);
        }
    }
}

fn bytes_per_pixel(format: TexFormat) -> usize {
    match format {
        TexFormat::Rgba8 | TexFormat::SRgba8 => 4,
        TexFormat::Rgb8 => 3,
        TexFormat::Rg8 => 2,
        TexFormat::R8 => 1,
        TexFormat::Rgba32F => 16,
    }
}

fn gl_format(format: TexFormat) -> (u32, u32, u32) {
    match format {
        TexFormat::Rgba8 => (glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE),
        TexFormat::SRgba8 => (glow::SRGB8_ALPHA8, glow::RGBA, glow::UNSIGNED_BYTE),
        TexFormat::Rgb8 => (glow::RGB8, glow::RGB, glow::UNSIGNED_BYTE),
        TexFormat::Rg8 => (glow::RG8, glow::RG, glow::UNSIGNED_BYTE),
        TexFormat::R8 => (glow::R8, glow::RED, glow::UNSIGNED_BYTE),
        TexFormat::Rgba32F => (glow::RGBA32F, glow::RGBA, glow::FLOAT),
    }
}

unsafe fn set_uniform_2f(gl: &glow::Context, program: glow::Program, name: &str, v: [f32; 2]) {
    unsafe {
        let loc = gl.get_uniform_location(program, name);
        gl.uniform_2_f32(loc.as_ref(), v[0], v[1]);
    }
}

fn compile_program(gl: &glow::Context, vertex_src: &str, fragment_src: &str) -> glow::Program {
    // Desktop GL and GLES need different headers, the shader bodies work for both
    let header = if gl.version().is_embedded {
        "#version 300 es"
    } else {
        "#version 330"
    };
    let vertex_src = &format!("{header}{vertex_src}");
    let fragment_src = &format!("{header}{fragment_src}");
    unsafe {
        let program = gl.create_program().expect("Failed to create program");

        let vs = gl
            .create_shader(glow::VERTEX_SHADER)
            .expect("Failed to create VS");
        gl.shader_source(vs, vertex_src);
        gl.compile_shader(vs);
        if !gl.get_shader_compile_status(vs) {
            panic!("Vertex shader error: {}", gl.get_shader_info_log(vs));
        }

        let fs = gl
            .create_shader(glow::FRAGMENT_SHADER)
            .expect("Failed to create FS");
        gl.shader_source(fs, fragment_src);
        gl.compile_shader(fs);
        if !gl.get_shader_compile_status(fs) {
            panic!("Fragment shader error: {}", gl.get_shader_info_log(fs));
        }

        gl.attach_shader(program, vs);
        gl.attach_shader(program, fs);
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            panic!("Program link error: {}", gl.get_program_info_log(program));
        }

        gl.detach_shader(program, vs);
        gl.detach_shader(program, fs);
        gl.delete_shader(vs);
        gl.delete_shader(fs);

        program
    }
}

/// The texture format an image is uploaded with, and the image in that layout.
///
/// Images keep the channels they have, at 8 bit per channel. A gray image takes a
/// quarter of the memory of an RGBA one that way. More than 8 bit would not be
/// visible on screen.
pub fn texture_layout(
    img: &image::DynamicImage,
) -> (TexFormat, std::borrow::Cow<'_, image::DynamicImage>) {
    use image::ColorType::*;
    use image::DynamicImage;
    use std::borrow::Cow;
    let img = match img.color() {
        L8 | La8 | Rgb8 | Rgba8 => Cow::Borrowed(img),
        L16 => Cow::Owned(DynamicImage::ImageLuma8(img.to_luma8())),
        La16 => Cow::Owned(DynamicImage::ImageLumaA8(img.to_luma_alpha8())),
        Rgb16 | Rgb32F => Cow::Owned(DynamicImage::ImageRgb8(img.to_rgb8())),
        _ => Cow::Owned(DynamicImage::ImageRgba8(img.to_rgba8())),
    };
    let format = match img.color() {
        L8 => TexFormat::R8,
        La8 => TexFormat::Rg8,
        Rgb8 => TexFormat::Rgb8,
        _ => TexFormat::Rgba8,
    };
    (format, img)
}

/// Compute the swizzle matrix and offset vector for a given color channel selection.
pub fn get_swizzle_mat_vec(channel: ColorChannel, format: TexFormat) -> (Mat4, Vec4) {
    match format {
        TexFormat::R8 => get_swizzle_gray(channel),
        TexFormat::Rg8 => get_swizzle_gray_alpha(channel),
        // A texture without alpha is sampled with an alpha of one
        TexFormat::Rgb8 | TexFormat::Rgba8 | TexFormat::SRgba8 | TexFormat::Rgba32F => {
            get_swizzle_rgba(channel)
        }
    }
}

/// Gray is in the red channel of the texture, alpha in the green one
fn get_swizzle_gray_alpha(channel: ColorChannel) -> (Mat4, Vec4) {
    let mut mat = Mat4::ZERO;
    let mut vec = Vec4::ZERO;
    let one = Vec4::new(1.0, 1.0, 1.0, 0.0);
    match channel {
        ColorChannel::Alpha => {
            mat.y_axis = one;
            vec.w = 1.0;
        }
        ColorChannel::Rgba => {
            mat.x_axis = one;
            mat.y_axis = Vec4::new(0.0, 0.0, 0.0, 1.0);
        }
        _ => {
            mat.x_axis = one;
            vec.w = 1.0;
        }
    }
    (mat, vec)
}

fn get_swizzle_gray(channel: ColorChannel) -> (Mat4, Vec4) {
    let mut mat = Mat4::ZERO;
    let mut vec = Vec4::ZERO;
    match channel {
        ColorChannel::Alpha => {
            vec = Vec4::ONE;
        }
        _ => {
            mat.x_axis = Vec4::new(1.0, 1.0, 1.0, 0.0);
            vec.w = 1.0;
        }
    }
    (mat, vec)
}

fn get_swizzle_rgba(channel: ColorChannel) -> (Mat4, Vec4) {
    let mut mat = Mat4::ZERO;
    let mut vec = Vec4::ZERO;
    let one = Vec4::new(1.0, 1.0, 1.0, 0.0);
    match channel {
        ColorChannel::Red => {
            mat.x_axis = one;
            vec.w = 1.0;
        }
        ColorChannel::Green => {
            mat.y_axis = one;
            vec.w = 1.0;
        }
        ColorChannel::Blue => {
            mat.z_axis = one;
            vec.w = 1.0;
        }
        ColorChannel::Alpha => {
            mat.w_axis = one;
            vec.w = 1.0;
        }
        ColorChannel::Rgb => {
            mat = Mat4::IDENTITY;
            mat.w_axis = Vec4::ZERO;
            vec.w = 1.0;
        }
        ColorChannel::Rgba => {
            mat = Mat4::IDENTITY;
        }
    }
    (mat, vec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;

    fn shown(channel: ColorChannel, format: TexFormat, texel: Vec4) -> Vec4 {
        let (mat, vec) = get_swizzle_mat_vec(channel, format);
        mat * texel + vec
    }

    #[test]
    fn images_keep_their_channels() {
        let layout = |img: DynamicImage| texture_layout(&img).0;
        assert_eq!(layout(DynamicImage::new_luma8(3, 3)), TexFormat::R8);
        assert_eq!(layout(DynamicImage::new_luma16(3, 3)), TexFormat::R8);
        assert_eq!(layout(DynamicImage::new_luma_a8(3, 3)), TexFormat::Rg8);
        assert_eq!(layout(DynamicImage::new_luma_a16(3, 3)), TexFormat::Rg8);
        assert_eq!(layout(DynamicImage::new_rgb8(3, 3)), TexFormat::Rgb8);
        assert_eq!(layout(DynamicImage::new_rgb16(3, 3)), TexFormat::Rgb8);
        assert_eq!(layout(DynamicImage::new_rgb32f(3, 3)), TexFormat::Rgb8);
        assert_eq!(layout(DynamicImage::new_rgba8(3, 3)), TexFormat::Rgba8);
        assert_eq!(layout(DynamicImage::new_rgba16(3, 3)), TexFormat::Rgba8);
        assert_eq!(layout(DynamicImage::new_rgba32f(3, 3)), TexFormat::Rgba8);
    }

    #[test]
    fn layout_has_one_byte_per_channel() {
        for img in [
            DynamicImage::new_luma16(5, 3),
            DynamicImage::new_luma_a16(5, 3),
            DynamicImage::new_rgb32f(5, 3),
            DynamicImage::new_rgba16(5, 3),
        ] {
            let (format, layout) = texture_layout(&img);
            assert_eq!(layout.as_bytes().len(), 5 * 3 * bytes_per_pixel(format));
        }
    }

    #[test]
    fn gray_is_shown_on_all_color_channels() {
        // a gray texture is sampled as (gray, 0, 0, 1)
        let texel = Vec4::new(0.5, 0.0, 0.0, 1.0);
        let gray = Vec4::new(0.5, 0.5, 0.5, 1.0);
        for channel in [ColorChannel::Rgba, ColorChannel::Rgb, ColorChannel::Red] {
            assert_eq!(shown(channel, TexFormat::R8, texel), gray);
        }
        assert_eq!(shown(ColorChannel::Alpha, TexFormat::R8, texel), Vec4::ONE);
    }

    #[test]
    fn gray_with_alpha_keeps_its_alpha() {
        // sampled as (gray, alpha, 0, 1)
        let texel = Vec4::new(0.5, 0.25, 0.0, 1.0);
        assert_eq!(
            shown(ColorChannel::Rgba, TexFormat::Rg8, texel),
            Vec4::new(0.5, 0.5, 0.5, 0.25)
        );
        assert_eq!(
            shown(ColorChannel::Rgb, TexFormat::Rg8, texel),
            Vec4::new(0.5, 0.5, 0.5, 1.0)
        );
        assert_eq!(
            shown(ColorChannel::Green, TexFormat::Rg8, texel),
            Vec4::new(0.5, 0.5, 0.5, 1.0)
        );
        assert_eq!(
            shown(ColorChannel::Alpha, TexFormat::Rg8, texel),
            Vec4::new(0.25, 0.25, 0.25, 1.0)
        );
    }

    #[test]
    fn rgb_without_alpha_is_opaque() {
        // sampled with an alpha of one
        let texel = Vec4::new(0.1, 0.2, 0.3, 1.0);
        assert_eq!(shown(ColorChannel::Rgba, TexFormat::Rgb8, texel), texel);
        assert_eq!(
            shown(ColorChannel::Blue, TexFormat::Rgb8, texel),
            Vec4::new(0.3, 0.3, 0.3, 1.0)
        );
    }
}
