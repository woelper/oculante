/// eframe-based application shell.
///
/// This implements `eframe::App` and replaces notan's init/update/draw callbacks.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{Align, FontData, FontDefinitions, FontFamily, FontTweak, Id};
use image::GenericImageView;
use log::{debug, error, info, warn};
use nalgebra::Vector2;

use crate::appstate::*;
use crate::filebrowser::BrowserDir;
use crate::glow_renderer::{self, GlowRenderer, GlowTexture, TexFormat};
use crate::shortcuts::{self, key_pressed};
use crate::ui::*;
use crate::utils::*;
use crate::{BOLD_FONT, FONT};

#[cfg(feature = "file_open")]
use crate::filebrowser::browse_for_image_path;
#[cfg(feature = "turbo")]
use crate::image_editing::lossless_tx;

/// Render state shared between update() and paint callbacks
pub struct RenderState {
    pub renderer: GlowRenderer,
    /// Current image as glow textures (tiled for large images)
    pub textures: Vec<GlowTexture>,
    pub col_count: u32,
    pub row_count: u32,
    pub col_translation: u32,
    pub row_translation: u32,
    pub image_width: f32,
    pub image_height: f32,
    /// Swizzle matrix (color channel selection)
    pub swizzle_mat: [f32; 16],
    pub offset_vec: [f32; 4],
    /// Transform
    pub offset: [f32; 2],
    pub scale: f32,
    pub viewport: [f32; 2],
    /// Background
    pub bg_color: [f32; 3],
    pub tiling: usize,
    /// Checker texture
    pub checker_texture: Option<GlowTexture>,
    pub show_checker: bool,
}

pub struct OculanteApp {
    pub state: OculanteState,
    pub render_state: Option<Arc<Mutex<RenderState>>>,
    first_frame: bool,
    /// Track if image needs re-upload
    image_dirty: bool,
}

impl OculanteApp {
    pub fn new(state: OculanteState) -> Self {
        Self {
            state,
            render_state: None,
            first_frame: true,
            image_dirty: false,
        }
    }

    fn ensure_render_state(&mut self, gl: &glow::Context) {
        if self.render_state.is_none() {
            let renderer = GlowRenderer::new(gl);

            // Create checker texture (8x8 checkerboard)
            let mut checker_data = vec![0u8; 8 * 8 * 4];
            for y in 0..8 {
                for x in 0..8 {
                    let idx = (y * 8 + x) * 4;
                    let bright = ((x + y) % 2 == 0) as u8 * 40 + 20;
                    checker_data[idx] = bright;
                    checker_data[idx + 1] = bright;
                    checker_data[idx + 2] = bright;
                    checker_data[idx + 3] = 255;
                }
            }
            let checker_texture =
                renderer.create_texture(gl, &checker_data, 8, 8, TexFormat::Rgba8, false, false, false);

            self.render_state = Some(Arc::new(Mutex::new(RenderState {
                renderer,
                textures: Vec::new(),
                col_count: 0,
                row_count: 0,
                col_translation: 0,
                row_translation: 0,
                image_width: 0.0,
                image_height: 0.0,
                swizzle_mat: [
                    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
                    0.0, 1.0,
                ],
                offset_vec: [0.0; 4],
                offset: [0.0, 0.0],
                scale: 1.0,
                viewport: [1.0, 1.0],
                bg_color: [0.0, 0.12, 0.12],
                tiling: 1,
                checker_texture: Some(checker_texture),
                show_checker: false,
            })));
        }
    }

    fn upload_image(&mut self, gl: &glow::Context) {
        let img = match &self.state.current_image {
            Some(img) => img,
            None => return,
        };

        let rs = self.render_state.as_ref().unwrap();
        let mut rs = rs.lock().unwrap();

        // Clear old textures
        let old_textures: Vec<GlowTexture> = rs.textures.drain(..).collect();
        for tex in old_textures {
            rs.renderer.delete_texture(gl, tex);
        }

        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        let max_size = rs.renderer.max_texture_size;
        let col_count = ((w as f32) / max_size as f32).ceil() as u32;
        let row_count = ((h as f32) / max_size as f32).ceil() as u32;
        let col_inc = w.min(max_size);
        let row_inc = h.min(max_size);

        let settings = &self.state.persistent_settings;
        let allow_mipmap = (w as usize * h as usize) < (8192 * 8192);

        for row in 0..row_count {
            let ty = row * row_inc;
            let th = row_inc.min(h - ty);
            for col in 0..col_count {
                let tx = col * col_inc;
                let tw = col_inc.min(w - tx);

                // Extract tile
                let tile: Vec<u8> = if col_count == 1 && row_count == 1 {
                    rgba.as_raw().clone()
                } else {
                    let mut tile_data = vec![0u8; (tw * th * 4) as usize];
                    for y in 0..th {
                        let src_start = ((ty + y) * w + tx) as usize * 4;
                        let dst_start = (y * tw) as usize * 4;
                        let len = tw as usize * 4;
                        tile_data[dst_start..dst_start + len]
                            .copy_from_slice(&rgba.as_raw()[src_start..src_start + len]);
                    }
                    tile_data
                };

                let tex = rs.renderer.create_texture(
                    gl,
                    &tile,
                    tw,
                    th,
                    TexFormat::Rgba8,
                    settings.linear_min_filter,
                    settings.linear_mag_filter,
                    settings.use_mipmaps && allow_mipmap,
                );
                rs.textures.push(tex);
            }
        }

        rs.col_count = col_count;
        rs.row_count = row_count;
        rs.col_translation = col_inc;
        rs.row_translation = row_inc;
        rs.image_width = w as f32;
        rs.image_height = h as f32;

        self.image_dirty = false;
    }

    fn first_frame_setup(&mut self, ctx: &egui::Context) {
        let mut fonts = FontDefinitions::default();
        egui_extras::install_image_loaders(ctx);

        let dpi = ctx.pixels_per_point();
        // Apply user UI scale on top of system DPI
        ctx.set_pixels_per_point(dpi * self.state.persistent_settings.ui_scale);
        ctx.options_mut(|o| o.zoom_with_keyboard = false);

        let offset = if dpi > 1.0 { 0.0 } else { -1.4 };

        fonts.font_data.insert(
            "inter".to_owned(),
            Arc::new(FontData::from_static(FONT).tweak(FontTweak {
                scale: 1.0,
                y_offset_factor: 0.0,
                y_offset: offset,
                baseline_offset_factor: 0.0,
            })),
        );
        fonts.font_data.insert(
            "inter-bold".to_owned(),
            Arc::new(FontData::from_static(BOLD_FONT).tweak(FontTweak {
                scale: 1.0,
                y_offset_factor: 0.0,
                y_offset: offset,
                baseline_offset_factor: 0.0,
            })),
        );

        // Icon font
        fonts.font_data.insert(
            "icons".to_owned(),
            Arc::new(
                FontData::from_static(include_bytes!("../res/fonts/icons.ttf")).tweak(FontTweak {
                    scale: 1.0,
                    y_offset_factor: 0.0,
                    y_offset: 1.0,
                    baseline_offset_factor: 0.0,
                }),
            ),
        );

        // Font families: icons first, then inter (so icon codepoints resolve to icon font)
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "icons".to_owned());
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "inter".to_owned());
        fonts.families.insert(
            FontFamily::Name("bold".to_owned().into()),
            vec!["inter-bold".into()],
        );

        let fonts = load_system_fonts(fonts);

        apply_theme(&mut self.state, ctx);
        ctx.set_fonts(fonts);
        self.first_frame = false;
    }

    fn process_load_channel(&mut self) {
        if let Ok(p) = self.state.load_channel.1.try_recv() {
            self.state.is_loaded = false;
            self.state.current_image = None;
            self.state.player.load(&p);
            if let Some(dir) = p.parent() {
                self.state.volatile_settings.last_open_directory = dir.to_path_buf();
            }
            self.state.current_path = Some(p);
            self.state.scrubber.fixed_paths = false;
        }
    }

    fn process_texture_channel(&mut self, ctx: &egui::Context) {
        // Drain to get latest frame (prevents animation speedup on focus loss)
        let latest_frame = self.state.texture_channel.1.try_iter().last();

        if let Some(frame) = latest_frame {
            self.state.is_loaded = true;

            // Update scrubber on new images
            if matches!(
                &frame,
                Frame::AnimationStart(_) | Frame::Still(_) | Frame::ImageCollectionMember(_)
            ) {
                if let Some(path) = &self.state.current_path {
                    if self.state.scrubber.has_folder_changed(path)
                        && !self.state.scrubber.fixed_paths
                    {
                        self.state.scrubber =
                            crate::scrubber::Scrubber::new(path);
                        self.state.scrubber.wrap = self.state.persistent_settings.wrap_folder;
                    } else {
                        let index = self
                            .state
                            .scrubber
                            .entries
                            .iter()
                            .position(|p| p == path)
                            .unwrap_or_default();
                        if index < self.state.scrubber.entries.len() {
                            self.state.scrubber.index = index;
                        }
                    }
                }

                // Update recent images
                if let Some(path) = &self.state.current_path {
                    if self.state.persistent_settings.max_recents > 0
                        && !self.state.volatile_settings.recent_images.contains(path)
                    {
                        self.state
                            .volatile_settings
                            .recent_images
                            .push_back(path.clone());
                        self.state
                            .volatile_settings
                            .recent_images
                            .truncate(self.state.persistent_settings.max_recents.into());
                    }
                }
            }

            match frame {
                Frame::Still(img)
                | Frame::CompareResult(img, _)
                | Frame::ImageCollectionMember(img)
                | Frame::AnimationStart(img) => {
                    debug!("Received image {}x{}", img.width(), img.height());
                    self.state.current_image = Some(img);
                    self.state.new_image_loaded = true;
                    self.state.reset_image = true;
                    self.image_dirty = true;
                    ctx.request_repaint();
                }
                Frame::EditResult(img) => {
                    self.state.current_image = Some(img);
                    self.image_dirty = true;
                    ctx.request_repaint();
                }
                Frame::Animation(img, delay_ms) => {
                    self.state.current_image = Some(img);
                    self.image_dirty = true;
                    ctx.request_repaint_after(Duration::from_millis(delay_ms as u64));
                }
                Frame::UpdateTexture => {
                    self.image_dirty = true;
                    ctx.request_repaint();
                }
                _ => {}
            }
        }
    }

    fn process_messages(&mut self) {
        while let Ok(msg) = self.state.message_channel.1.try_recv() {
            match msg {
                Message::LoadError(e) => {
                    self.state.toasts.error(e);
                    self.state.current_image = None;
                    self.state.is_loaded = true;
                }
                Message::Info(m) => {
                    self.state
                        .toasts
                        .info(m)
                        .duration(Some(Duration::from_secs(1)));
                }
                Message::Warning(m) => {
                    self.state.toasts.warning(m);
                }
                Message::Error(m) => {
                    self.state.toasts.error(m);
                }
                Message::Saved(_) => {
                    self.state.toasts.info("Saved");
                }
            }
        }
    }

    /// Update the swizzle matrix from current channel settings
    fn update_swizzle(&self, rs: &mut RenderState) {
        let (mat, vec) = glow_renderer::get_swizzle_mat_vec(
            self.state.persistent_settings.current_channel,
            self.state
                .current_image
                .as_ref()
                .map(|i| i.color())
                .unwrap_or(image::ColorType::Rgba8),
        );
        rs.swizzle_mat = mat.to_cols_array();
        rs.offset_vec = vec.into();
    }
}

impl eframe::App for OculanteApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Initialize on first frame
        if self.first_frame {
            self.first_frame_setup(ctx);
        }

        // Initialize glow renderer
        if let Some(gl) = frame.gl() {
            self.ensure_render_state(gl);
            if self.image_dirty {
                self.upload_image(gl);
            }
        }

        // Update window size
        let screen_rect = ctx.screen_rect();
        self.state.window_size = Vector2::new(screen_rect.width(), screen_rect.height());

        // Process channels
        self.process_load_channel();
        self.process_texture_channel(ctx);
        self.process_messages();

        if let Ok(info) = self.state.extended_info_channel.1.try_recv() {
            self.state.image_metadata = Some(info);
            ctx.request_repaint();
        }

        // Mouse
        let pointer_pos = ctx.input(|i| i.pointer.hover_pos()).unwrap_or_default();
        let new_cursor = Vector2::new(pointer_pos.x, pointer_pos.y);
        self.state.mouse_delta = new_cursor - self.state.cursor;
        self.state.cursor = new_cursor;

        if let Some(dims) = self
            .state
            .current_image
            .as_ref()
            .map(|img| img.dimensions())
        {
            self.state.image_geometry.dimensions = dims;
        }

        // Drag
        let primary_down = ctx.input(|i| i.pointer.primary_down());
        let middle_down = ctx.input(|i| {
            i.pointer.button_down(egui::PointerButton::Middle)
        });

        if middle_down {
            self.state.drag_enabled = true;
            self.state.image_geometry.offset += self.state.mouse_delta;
        } else if primary_down && !self.state.mouse_grab && !self.state.pointer_over_ui {
            self.state.drag_enabled = true;
        }
        if self.state.drag_enabled && !primary_down && !middle_down {
            self.state.drag_enabled = false;
        }
        if self.state.drag_enabled && !self.state.mouse_grab {
            self.state.image_geometry.offset += self.state.mouse_delta;
        }

        // Scroll zoom
        let scroll_delta = ctx.input(|i| i.raw_scroll_delta.y);
        if scroll_delta != 0.0 && !self.state.pointer_over_ui {
            let ctrl = ctx.input(|i| i.modifiers.ctrl || i.modifiers.command);
            if ctrl {
                if scroll_delta > 0.0 {
                    prev_image(&mut self.state)
                } else {
                    next_image(&mut self.state)
                }
            } else {
                let divisor = if cfg!(target_os = "macos") { 1.5 } else { 10. };
                let delta = zoomratio(
                    ((scroll_delta / divisor) * self.state.persistent_settings.zoom_multiplier)
                        .clamp(-5.0, 5.0),
                    self.state.image_geometry.scale,
                );
                let new_scale = self.state.image_geometry.scale + delta;
                if new_scale > 0.01 && new_scale < 40. {
                    self.state.image_geometry.offset -= scale_pt(
                        self.state.image_geometry.offset,
                        self.state.cursor,
                        self.state.image_geometry.scale,
                        delta,
                    );
                    self.state.image_geometry.scale += delta;
                }
            }
        }

        // File drop
        ctx.input(|i| {
            for file in &i.raw.dropped_files {
                if let Some(path) = &file.path {
                    if let Some(ext) = path.extension() {
                        if SUPPORTED_EXTENSIONS
                            .contains(&ext.to_string_lossy().to_lowercase().as_str())
                        {
                            self.state.is_loaded = false;
                            self.state.current_image = None;
                            self.state.player.load(path);
                            self.state.current_path = Some(path.clone());
                        }
                    }
                }
            }
        });

        // Cursor relative
        if self.state.persistent_settings.info_enabled || self.state.edit_state.painting {
            self.state.cursor_relative = pos_from_coord(
                self.state.image_geometry.offset,
                self.state.cursor,
                Vector2::new(
                    self.state.image_geometry.dimensions.0 as f32,
                    self.state.image_geometry.dimensions.1 as f32,
                ),
                self.state.image_geometry.scale,
            );
        }

        // ===== EGUI UI =====
        let state = &mut self.state;

        state.toasts.show(ctx);

        if let Some(id) = state.filebrowser_id.take() {
            ctx.memory_mut(|w| w.open_popup(Id::new(&id)));
        }

        // Double-click fullscreen
        if !state.pointer_over_ui
            && !state.mouse_grab
            && ctx.input(|r| {
                r.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
            })
        {
            toggle_fullscreen(ctx, state);
        }

        if state.new_image_loaded {
            ctx.memory_mut(|m| m.data.remove::<f64>(Id::new("resize_aspect_ratio")));
        }

        // File browser
        #[cfg(not(feature = "file_open"))]
        {
            if ctx.memory(|w| w.is_popup_open(Id::new("OPEN"))) {
                crate::filebrowser::browse_modal(
                    false,
                    SUPPORTED_EXTENSIONS,
                    &mut state.volatile_settings,
                    |p| {
                        let _ = state.load_channel.0.clone().send(p.to_path_buf());
                        ctx.memory_mut(|w| w.close_popup());
                    },
                    ctx,
                );
            }
        }

        // Top menu bar
        if !state.persistent_settings.zen_mode {
            egui::TopBottomPanel::top("menu")
                .exact_height(36.0)
                .show_separator_line(false)
                .show(ctx, |ui| {
                    main_menu(ui, state);
                });
        }
        if state.persistent_settings.zen_mode && state.persistent_settings.borderless {
            egui::TopBottomPanel::top("menu_zen")
                .min_height(40.)
                .default_height(40.)
                .show_separator_line(false)
                .frame(egui::containers::Frame::NONE)
                .show(ctx, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        drag_area(ui, state);
                        ui.add_space(15.);
                        draw_hamburger_menu(ui, state);
                    });
                });
        }

        // Scrub bar
        if state.persistent_settings.show_scrub_bar {
            egui::TopBottomPanel::bottom("scrubber")
                .max_height(22.)
                .min_height(22.)
                .show(ctx, |ui| {
                    scrubber_ui(state, ui);
                });
        }

        // Edit panel
        if state.persistent_settings.edit_enabled
            && !state.settings_enabled
            && !state.persistent_settings.zen_mode
            && state.current_image.is_some()
        {
            edit_ui(ctx, state);
        }

        // Info panel
        if state.persistent_settings.info_enabled
            && !state.settings_enabled
            && !state.persistent_settings.zen_mode
            && state.current_image.is_some()
        {
            let (_bbox_tl, _bbox_br) = info_ui(ctx, state);
        }

        // UI state flags (before keyboard shortcuts, after panels)
        state.pointer_over_ui = ctx.is_pointer_over_area();
        state.mouse_grab = ctx.is_using_pointer()
            || state.edit_state.painting
            || ctx.is_pointer_over_area()
            || state.edit_state.block_panning;
        state.key_grab = ctx.wants_keyboard_input();

        // Reset image to fit window
        if state.reset_image {
            if let Some(current_image) = &state.current_image {
                let draw_area = ctx.available_rect();
                let window_size = Vector2::new(draw_area.width(), draw_area.height());
                let img_size = current_image.size_vec();
                let scaled_to_fit = window_size.component_div(&img_size).amin();
                state.image_geometry.scale = if state.persistent_settings.auto_scale {
                    scaled_to_fit
                } else {
                    scaled_to_fit.min(1.0)
                };
                state.image_geometry.offset =
                    window_size / 2.0 - (img_size * state.image_geometry.scale) / 2.0;
                state.image_geometry.offset.x += draw_area.left();
                state.image_geometry.offset.y += draw_area.top();
                state.reset_image = false;
                ctx.request_repaint();
            }
        }

        // Settings (last — blocks keyboard for hotkey assignment)
        settings_ui(ctx, state);

        // Keyboard shortcuts
        if !state.key_grab {
            use shortcuts::InputEvent::*;

            if key_pressed(ctx, state, Fullscreen) {
                toggle_fullscreen(ctx, state);
            }
            if key_pressed(ctx, state, Quit) {
                _ = state.persistent_settings.save_blocking();
                _ = state.volatile_settings.save_blocking();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            if key_pressed(ctx, state, ResetView) {
                state.reset_image = true;
            }
            if key_pressed(ctx, state, ZenMode) {
                toggle_zen_mode(state, ctx);
            }
            if key_pressed(ctx, state, InfoMode) {
                state.persistent_settings.info_enabled = !state.persistent_settings.info_enabled;
            }
            if key_pressed(ctx, state, EditMode) {
                state.persistent_settings.edit_enabled = !state.persistent_settings.edit_enabled;
            }
            if key_pressed(ctx, state, AlwaysOnTop) {
                state.always_on_top = !state.always_on_top;
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                    if state.always_on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    },
                ));
            }
            if key_pressed(ctx, state, NextImage) {
                next_image(state);
            }
            if key_pressed(ctx, state, PreviousImage) {
                prev_image(state);
            }
            if key_pressed(ctx, state, FirstImage) {
                first_image(state);
            }
            if key_pressed(ctx, state, LastImage) {
                last_image(state);
            }
            if key_pressed(ctx, state, ZoomActualSize) {
                set_zoom(1.0, None, state);
            }
            if key_pressed(ctx, state, ZoomDouble) {
                set_zoom(2.0, None, state);
            }
            if key_pressed(ctx, state, ZoomThree) {
                set_zoom(3.0, None, state);
            }
            if key_pressed(ctx, state, ZoomFour) {
                set_zoom(4.0, None, state);
            }
            if key_pressed(ctx, state, ZoomFive) {
                set_zoom(5.0, None, state);
            }
            if key_pressed(ctx, state, ZoomIn) {
                let delta = zoomratio(3.5, state.image_geometry.scale);
                let new_scale = state.image_geometry.scale + delta;
                if new_scale > 0.05 && new_scale < 40. {
                    let center =
                        Vector2::new(state.window_size.x / 2., state.window_size.y / 2.);
                    state.image_geometry.offset -= scale_pt(
                        state.image_geometry.offset,
                        center,
                        state.image_geometry.scale,
                        delta,
                    );
                    state.image_geometry.scale += delta;
                }
            }
            if key_pressed(ctx, state, ZoomOut) {
                let delta = zoomratio(-3.5, state.image_geometry.scale);
                let new_scale = state.image_geometry.scale + delta;
                if new_scale > 0.05 && new_scale < 40. {
                    let center =
                        Vector2::new(state.window_size.x / 2., state.window_size.y / 2.);
                    state.image_geometry.offset -= scale_pt(
                        state.image_geometry.offset,
                        center,
                        state.image_geometry.scale,
                        delta,
                    );
                    state.image_geometry.scale += delta;
                }
            }
            let pan_delta = 40.;
            if key_pressed(ctx, state, PanRight) {
                state.image_geometry.offset.x -= pan_delta;
            }
            if key_pressed(ctx, state, PanLeft) {
                state.image_geometry.offset.x += pan_delta;
            }
            if key_pressed(ctx, state, PanUp) {
                state.image_geometry.offset.y += pan_delta;
            }
            if key_pressed(ctx, state, PanDown) {
                state.image_geometry.offset.y -= pan_delta;
            }
            if key_pressed(ctx, state, Copy) {
                if let Some(img) = &state.current_image {
                    clipboard_copy(img);
                    state.send_message_info("Image copied");
                }
            }
            if key_pressed(ctx, state, Paste) {
                match clipboard_to_image() {
                    Ok(img) => {
                        state.current_path = None;
                        state.player.stop();
                        _ = state
                            .player
                            .image_sender
                            .send(crate::utils::Frame::new_still(img));
                        state.send_message_info("Image pasted");
                    }
                    Err(e) => state.send_message_err(&e.to_string()),
                }
            }
            if key_pressed(ctx, state, DeleteFile) {
                delete_file(state);
            }
            if key_pressed(ctx, state, ClearImage) {
                clear_image(state);
            }
            if key_pressed(ctx, state, Browse) {
                state.filebrowser_last_dir = if ctx.input(|i| i.modifiers.shift) {
                    BrowserDir::CurrentImageDir
                } else {
                    BrowserDir::LastOpenDir
                };
                state.redraw = true;
                #[cfg(feature = "file_open")]
                browse_for_image_path(state);
                #[cfg(not(feature = "file_open"))]
                {
                    state.filebrowser_id = Some("OPEN".into());
                }
            }
            #[cfg(feature = "turbo")]
            if key_pressed(ctx, state, LosslessRotateRight) {
                if let Some(p) = &state.current_path {
                    if lossless_tx(p, turbojpeg::Transform::op(turbojpeg::TransformOp::Rot90))
                        .is_ok()
                    {
                        state.is_loaded = false;
                        state.player.cache.clear();
                        state.player.load(p);
                    }
                }
            }
            #[cfg(feature = "turbo")]
            if key_pressed(ctx, state, LosslessRotateLeft) {
                if let Some(p) = &state.current_path {
                    if lossless_tx(p, turbojpeg::Transform::op(turbojpeg::TransformOp::Rot270))
                        .is_ok()
                    {
                        state.is_loaded = false;
                        state.player.cache.clear();
                        state.player.load(p);
                    }
                }
            }
        }

        // ===== CUSTOM RENDERING via paint callback =====
        // Update render state for the paint callback
        if let Some(rs) = &self.render_state {
            let mut rs = rs.lock().unwrap();
            self.update_swizzle(&mut rs);
            rs.offset = [
                self.state.image_geometry.offset.x,
                self.state.image_geometry.offset.y,
            ];
            rs.scale = self.state.image_geometry.scale;
            rs.viewport = [self.state.window_size.x, self.state.window_size.y];
            let c = self.state.persistent_settings.background_color;
            rs.bg_color = [c[0] as f32 / 255., c[1] as f32 / 255., c[2] as f32 / 255.];
            rs.bg_color = [1.,1.,1.];
            rs.tiling = self.state.tiling;
            rs.show_checker = self.state.persistent_settings.show_checker_background;
        }

        // CentralPanel required for egui layout (side/top panels need it for sizing)
        // egui::CentralPanel::default()
        //     .frame(egui::Frame::NONE)
        //     .show(ctx, |_ui| {});

        // Image rendering on a background layer (below all egui panels)
        if self.state.current_image.is_some() {
            if let Some(rs_arc) = &self.render_state {
                let rs_arc = rs_arc.clone();
                let callback = egui::PaintCallback {
                    rect: ctx.screen_rect(),
                    callback: Arc::new(egui_glow::CallbackFn::new(move |_info, painter| {
                        let rs = rs_arc.lock().unwrap();
                        let gl = painter.gl().as_ref();

                        if rs.textures.is_empty() {
                            return;
                        }

                        unsafe {
                            glow::HasContext::enable(gl, glow::BLEND);
                            glow::HasContext::blend_func(
                                gl,
                                glow::SRC_ALPHA,
                                glow::ONE_MINUS_SRC_ALPHA,
                            );
                        }

                        let viewport = rs.viewport;
                        let scale = rs.scale;
                        let base_offset = rs.offset;

                        for ty in 0..rs.tiling.max(1) {
                            for tx_i in 0..rs.tiling.max(1) {
                                let tiling_offset = [
                                    base_offset[0] + tx_i as f32 * rs.image_width * scale,
                                    base_offset[1] + ty as f32 * rs.image_height * scale,
                                ];

                                let mut tex_idx = 0;
                                for row in 0..rs.row_count {
                                    let row_off =
                                        row as f32 * rs.row_translation as f32 * scale;
                                    for col in 0..rs.col_count {
                                        if tex_idx >= rs.textures.len() {
                                            break;
                                        }
                                        let col_off =
                                            col as f32 * rs.col_translation as f32 * scale;
                                        let tex = &rs.textures[tex_idx];
                                        rs.renderer.draw_image(
                                            gl,
                                            tex,
                                            [
                                                tiling_offset[0] + col_off,
                                                tiling_offset[1] + row_off,
                                            ],
                                            [scale, scale],
                                            viewport,
                                            &rs.swizzle_mat,
                                            &rs.offset_vec,
                                            [0.0, 0.0],
                                            [1.0, 1.0],
                                            None,
                                        );
                                        tex_idx += 1;
                                    }
                                }
                            }
                        }
                    })),
                };

                ctx.layer_painter(egui::LayerId::background()).add(callback);
            }
        }

        // Repaint if needed
        if self.state.network_mode {
            ctx.request_repaint();
        }
        if self.state.new_image_loaded {
            self.state.new_image_loaded = false;
        }
    }
}
