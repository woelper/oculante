/// Custom glow (OpenGL) renderer for image display.
///
/// This replaces notan's Draw API for rendering the main image,
/// checker background, zoom preview, and overlays.
use crate::utils::ColorChannel;
use glam::{Mat4, Vec4};
use glow::HasContext;

/// Texture formats we support
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TexFormat {
    Rgba8,
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

/// The glow-based renderer. Holds compiled shaders and shared GL state.
pub struct GlowRenderer {
    /// Shader program for textured quads with swizzle/offset uniforms
    image_program: glow::Program,
    /// Simple shader for solid-color rectangles
    rect_program: glow::Program,
    /// Fullscreen quad VAO (two triangles covering clip space, UVs computed from position)
    quad_vao: glow::VertexArray,
    /// Max texture size for this GPU
    pub max_texture_size: u32,
}

// Vertex shader shared by image and rect programs
const VERTEX_SHADER: &str = r#"#version 330
uniform vec2 u_offset;
uniform vec2 u_scale;
uniform vec2 u_viewport;
// For crop: uv offset and scale
uniform vec2 u_uv_offset;
uniform vec2 u_uv_scale;
// Quad size in pixels (for image: texture size, for rect: rect size)
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

const IMAGE_FRAGMENT_SHADER: &str = r#"#version 330
precision mediump float;
in vec2 v_uv;
uniform sampler2D u_texture;
uniform mat4 u_swizzle_mat;
uniform vec4 u_offset_vec;
out vec4 color;
void main() {
    vec4 tex_col = texture(u_texture, v_uv);
    color = (u_swizzle_mat * tex_col) + u_offset_vec;
}
"#;

const RECT_FRAGMENT_SHADER: &str = r#"#version 330
precision mediump float;
uniform vec4 u_color;
out vec4 color;
void main() {
    color = u_color;
}
"#;

impl GlowRenderer {
    /// Create a new renderer. Call this once with the GL context.
    pub fn new(gl: &glow::Context) -> Self {
        unsafe {
            let image_program = compile_program(gl, VERTEX_SHADER, IMAGE_FRAGMENT_SHADER);
            let rect_program = compile_program(gl, VERTEX_SHADER, RECT_FRAGMENT_SHADER);

            // Empty VAO for attribute-less rendering (we compute positions from gl_VertexID)
            let quad_vao = gl.create_vertex_array().expect("Failed to create VAO");

            let max_texture_size = gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE) as u32;

            Self {
                image_program,
                rect_program,
                quad_vao,
                max_texture_size,
            }
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
        linear_min: bool,
        linear_mag: bool,
        mipmaps: bool,
    ) -> GlowTexture {
        unsafe {
            let texture = gl.create_texture().expect("Failed to create texture");
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));

            let (internal, fmt, typ) = gl_format(format);
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

            let min_filter = if mipmaps {
                if linear_min {
                    glow::LINEAR_MIPMAP_LINEAR
                } else {
                    glow::NEAREST_MIPMAP_NEAREST
                }
            } else if linear_min {
                glow::LINEAR
            } else {
                glow::NEAREST
            };
            let mag_filter = if linear_mag {
                glow::LINEAR
            } else {
                glow::NEAREST
            };

            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, min_filter as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, mag_filter as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
            gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);

            if mipmaps {
                gl.generate_mipmap(glow::TEXTURE_2D);
            }

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
    pub fn update_texture(
        &self,
        gl: &glow::Context,
        tex: &GlowTexture,
        bytes: &[u8],
    ) {
        unsafe {
            gl.bind_texture(glow::TEXTURE_2D, Some(tex.texture));
            let (_internal, fmt, typ) = gl_format(tex.format);
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

    /// Delete a texture.
    pub fn delete_texture(&self, gl: &glow::Context, tex: GlowTexture) {
        unsafe {
            gl.delete_texture(tex.texture);
        }
    }

    /// Draw a textured quad with the image swizzle shader.
    ///
    /// `offset`: top-left position in screen pixels
    /// `scale`: uniform scale factor
    /// `viewport`: (width, height) of the window in pixels
    /// `swizzle_mat`: 4x4 color channel selection matrix
    /// `offset_vec`: additive color offset
    /// `uv_offset`, `uv_scale`: for cropping (default: (0,0), (1,1))
    pub fn draw_image(
        &self,
        gl: &glow::Context,
        tex: &GlowTexture,
        offset: [f32; 2],
        scale: [f32; 2],
        viewport: [f32; 2],
        swizzle_mat: &[f32; 16],
        offset_vec: &[f32; 4],
        uv_offset: [f32; 2],
        uv_scale: [f32; 2],
        size_override: Option<[f32; 2]>,
    ) {
        unsafe {
            gl.use_program(Some(self.image_program));
            gl.bind_vertex_array(Some(self.quad_vao));

            let size = size_override.unwrap_or([tex.width as f32, tex.height as f32]);

            set_uniform_2f(gl, self.image_program, "u_offset", offset);
            set_uniform_2f(gl, self.image_program, "u_scale", scale);
            set_uniform_2f(gl, self.image_program, "u_viewport", viewport);
            set_uniform_2f(gl, self.image_program, "u_size", size);
            set_uniform_2f(gl, self.image_program, "u_uv_offset", uv_offset);
            set_uniform_2f(gl, self.image_program, "u_uv_scale", uv_scale);

            let loc = gl.get_uniform_location(self.image_program, "u_swizzle_mat");
            gl.uniform_matrix_4_f32_slice(loc.as_ref(), false, swizzle_mat);

            let loc = gl.get_uniform_location(self.image_program, "u_offset_vec");
            gl.uniform_4_f32_slice(loc.as_ref(), offset_vec);

            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(tex.texture));
            let loc = gl.get_uniform_location(self.image_program, "u_texture");
            gl.uniform_1_i32(loc.as_ref(), 0);

            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);

            gl.bind_texture(glow::TEXTURE_2D, None);
            gl.use_program(None);
            gl.bind_vertex_array(None);
        }
    }

    /// Draw a solid-color rectangle.
    pub fn draw_rect(
        &self,
        gl: &glow::Context,
        pos: [f32; 2],
        size: [f32; 2],
        color: [f32; 4],
        viewport: [f32; 2],
    ) {
        unsafe {
            gl.use_program(Some(self.rect_program));
            gl.bind_vertex_array(Some(self.quad_vao));

            set_uniform_2f(gl, self.rect_program, "u_offset", pos);
            set_uniform_2f(gl, self.rect_program, "u_scale", [1.0, 1.0]);
            set_uniform_2f(gl, self.rect_program, "u_viewport", viewport);
            set_uniform_2f(gl, self.rect_program, "u_size", size);
            set_uniform_2f(gl, self.rect_program, "u_uv_offset", [0.0, 0.0]);
            set_uniform_2f(gl, self.rect_program, "u_uv_scale", [1.0, 1.0]);

            let loc = gl.get_uniform_location(self.rect_program, "u_color");
            gl.uniform_4_f32_slice(loc.as_ref(), &color);

            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);

            gl.use_program(None);
            gl.bind_vertex_array(None);
        }
    }

    /// Clear the framebuffer with a color.
    pub fn clear(&self, gl: &glow::Context, r: f32, g: f32, b: f32) {
        unsafe {
            gl.clear_color(r, g, b, 1.0);
            gl.clear(glow::COLOR_BUFFER_BIT);
        }
    }

    /// Clean up GL resources.
    pub fn destroy(&self, gl: &glow::Context) {
        unsafe {
            gl.delete_program(self.image_program);
            gl.delete_program(self.rect_program);
            gl.delete_vertex_array(self.quad_vao);
        }
    }
}

fn gl_format(format: TexFormat) -> (u32, u32, u32) {
    match format {
        TexFormat::Rgba8 => (glow::RGBA8, glow::RGBA, glow::UNSIGNED_BYTE),
        TexFormat::SRgba8 => (glow::SRGB8_ALPHA8, glow::RGBA, glow::UNSIGNED_BYTE),
        TexFormat::R8 => (glow::R8, glow::RED, glow::UNSIGNED_BYTE),
        TexFormat::Rgba32F => (glow::RGBA32F, glow::RGBA, glow::FLOAT),
    }
}

unsafe fn set_uniform_2f(gl: &glow::Context, program: glow::Program, name: &str, v: [f32; 2]) {
    let loc = gl.get_uniform_location(program, name);
    gl.uniform_2_f32(loc.as_ref(), v[0], v[1]);
}

unsafe fn compile_program(
    gl: &glow::Context,
    vertex_src: &str,
    fragment_src: &str,
) -> glow::Program {
    let program = gl.create_program().expect("Failed to create program");

    let vs = gl.create_shader(glow::VERTEX_SHADER).expect("Failed to create VS");
    gl.shader_source(vs, vertex_src);
    gl.compile_shader(vs);
    if !gl.get_shader_compile_status(vs) {
        panic!("Vertex shader error: {}", gl.get_shader_info_log(vs));
    }

    let fs = gl.create_shader(glow::FRAGMENT_SHADER).expect("Failed to create FS");
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

/// Compute the swizzle matrix and offset vector for a given color channel selection.
pub fn get_swizzle_mat_vec(
    channel: ColorChannel,
    image_color: image::ColorType,
) -> (Mat4, Vec4) {
    if image_color == image::ColorType::L8 || image_color == image::ColorType::L16 {
        get_swizzle_gray(channel)
    } else {
        get_swizzle_rgba(channel)
    }
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
