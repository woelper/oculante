/// eframe-based application shell.
///
/// This implements `eframe::App` and replaces notan's init/update/draw callbacks.
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use egui::{Align, FontData, FontDefinitions, FontFamily, FontTweak, Id};
use image::GenericImageView;
use log::{debug, error, info, warn};
use nalgebra::Vector2;

use crate::appstate::*;
use crate::filebrowser::BrowserDir;
use crate::glow_renderer;
use crate::shortcuts::{self, key_pressed};
use crate::ui::*;
use crate::utils::*;
use crate::{BOLD_FONT, FONT};

#[cfg(feature = "file_open")]
use crate::filebrowser::browse_for_image_path;
#[cfg(feature = "turbo")]
use crate::image_editing::lossless_tx;

/// A tile of the displayed image
/// A tile of the displayed image (public so info_ui can use it for zoom preview)
pub struct ImageTile {
    pub texture: egui::TextureHandle,
    /// Offset in image pixels from top-left
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

pub struct OculanteApp {
    pub state: OculanteState,
    first_frame: bool,
    /// Track if image needs re-upload to egui
    texture_dirty: bool,
    /// The current image as one or more tiles
    image_tiles: Vec<ImageTile>,
    /// Last channel setting used for swizzle (to detect changes)
    last_channel: ColorChannel,
    /// Max texture size (queried once from GL)
    max_texture_size: u32,
}

impl OculanteApp {
    pub fn new(state: OculanteState) -> Self {
        let last_channel = state.persistent_settings.current_channel;
        Self {
            state,
            first_frame: true,
            texture_dirty: false,
            image_tiles: Vec::new(),
            last_channel,
            max_texture_size: 8192, // conservative default, updated on first frame
        }
    }

    /// Upload or re-upload the current image as egui texture tile(s).
    /// Splits into tiles if the image exceeds max_texture_size.
    /// Applies the color channel swizzle at upload time.
    fn upload_image_to_egui(&mut self, ctx: &egui::Context) {
        let img = match &self.state.current_image {
            Some(img) => img,
            None => return,
        };

        let channel = self.state.persistent_settings.current_channel;
        let (mat, offset_vec) = glow_renderer::get_swizzle_mat_vec(channel, img.color());

        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());

        // Apply swizzle on CPU if not RGBA passthrough
        let mut pixels = rgba.into_raw();
        if channel != ColorChannel::Rgba {
            let mat_arr = mat.to_cols_array_2d();
            let off = [offset_vec.x, offset_vec.y, offset_vec.z, offset_vec.w];
            for chunk in pixels.chunks_exact_mut(4) {
                let r = chunk[0] as f32 / 255.0;
                let g = chunk[1] as f32 / 255.0;
                let b = chunk[2] as f32 / 255.0;
                let a = chunk[3] as f32 / 255.0;
                let nr = (mat_arr[0][0] * r + mat_arr[1][0] * g + mat_arr[2][0] * b + mat_arr[3][0] * a + off[0]).clamp(0.0, 1.0);
                let ng = (mat_arr[0][1] * r + mat_arr[1][1] * g + mat_arr[2][1] * b + mat_arr[3][1] * a + off[1]).clamp(0.0, 1.0);
                let nb = (mat_arr[0][2] * r + mat_arr[1][2] * g + mat_arr[2][2] * b + mat_arr[3][2] * a + off[2]).clamp(0.0, 1.0);
                let na = (mat_arr[0][3] * r + mat_arr[1][3] * g + mat_arr[2][3] * b + mat_arr[3][3] * a + off[3]).clamp(0.0, 1.0);
                chunk[0] = (nr * 255.0) as u8;
                chunk[1] = (ng * 255.0) as u8;
                chunk[2] = (nb * 255.0) as u8;
                chunk[3] = (na * 255.0) as u8;
            }
        }

        let filter = if self.state.persistent_settings.linear_mag_filter {
            egui::TextureFilter::Linear
        } else {
            egui::TextureFilter::Nearest
        };
        let tex_options = egui::TextureOptions {
            magnification: filter,
            minification: filter,
            ..Default::default()
        };

        // Clear old tiles
        self.image_tiles.clear();

        let max = self.max_texture_size;
        let cols = (w + max - 1) / max;
        let rows = (h + max - 1) / max;

        for row in 0..rows {
            let ty = row * max;
            let th = max.min(h - ty);
            for col in 0..cols {
                let tx = col * max;
                let tw = max.min(w - tx);

                // Extract tile pixels
                let tile_pixels: Vec<u8> = if cols == 1 && rows == 1 {
                    pixels.clone()
                } else {
                    let mut tile = vec![0u8; (tw * th * 4) as usize];
                    for y in 0..th {
                        let src = ((ty + y) * w + tx) as usize * 4;
                        let dst = (y * tw) as usize * 4;
                        let len = tw as usize * 4;
                        tile[dst..dst + len].copy_from_slice(&pixels[src..src + len]);
                    }
                    tile
                };

                let color_image = egui::ColorImage::from_rgba_unmultiplied(
                    [tw as usize, th as usize],
                    &tile_pixels,
                );
                let name = format!("tile_{}_{}", col, row);
                let texture = ctx.load_texture(&name, color_image, tex_options);

                self.image_tiles.push(ImageTile {
                    texture,
                    x: tx,
                    y: ty,
                    w: tw,
                    h: th,
                });
            }
        }

        self.texture_dirty = false;
        self.last_channel = channel;
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

            // Clear metadata for non-animation frames
            if !matches!(frame, Frame::Animation(_, _)) {
                self.state.image_metadata = None;
            }

            match frame {
                Frame::Still(img)
                | Frame::CompareResult(img, _)
                | Frame::ImageCollectionMember(img)
                | Frame::AnimationStart(img) => {
                    debug!("Received image {}x{}", img.width(), img.height());
                    self.state.image_geometry.dimensions = img.dimensions();
                    self.state.current_image = Some(img);
                    self.state.new_image_loaded = true;
                    self.state.reset_image = true;
                    self.texture_dirty = true;
                    ctx.request_repaint();
                }
                Frame::EditResult(img) => {
                    self.state.current_image = Some(img);
                    self.texture_dirty = true;
                    ctx.request_repaint();
                }
                Frame::Animation(img, delay_ms) => {
                    self.state.image_geometry.dimensions = img.dimensions();
                    self.state.current_image = Some(img);
                    self.texture_dirty = true;
                    ctx.request_repaint_after(Duration::from_millis(delay_ms as u64));
                }
                Frame::UpdateTexture => {
                    self.texture_dirty = true;
                    ctx.request_repaint();
                }
                _ => {}
            }

            // Send extended info (histogram, exif, etc.) in background thread
            send_extended_info(
                &self.state.current_image,
                &self.state.current_path,
                &self.state.extended_info_channel,
            );

            // Update window title
            set_title(ctx, &mut self.state);
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
}

impl eframe::App for OculanteApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Initialize on first frame
        if self.first_frame {
            self.first_frame_setup(ctx);
            // Query max texture size from GL
            if let Some(gl) = frame.gl() {
                self.max_texture_size =
                    unsafe { glow::HasContext::get_parameter_i32(gl.as_ref(), glow::MAX_TEXTURE_SIZE) as u32 };
                debug!("Max texture size: {}", self.max_texture_size);
            }
        }

        // Upload image to egui texture if needed
        if self.texture_dirty {
            self.upload_image_to_egui(ctx);
        }
        // Re-upload if channel changed
        if self.state.persistent_settings.current_channel != self.last_channel
            && self.state.current_image.is_some()
        {
            self.upload_image_to_egui(ctx);
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
            let (_bbox_tl, _bbox_br) = info_ui(ctx, state, &self.image_tiles);
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

        // ===== IMAGE RENDERING =====
        // Render image via egui's CentralPanel (behind side panels, below UI)
        let bg = self.state.persistent_settings.background_color;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(bg[0], bg[1], bg[2])))
            .show(ctx, |ui| {
                if !self.image_tiles.is_empty() {
                    let offset = self.state.image_geometry.offset;
                    let scale = self.state.image_geometry.scale;
                    let img_w = self.state.image_geometry.dimensions.0 as f32;
                    let img_h = self.state.image_geometry.dimensions.1 as f32;
                    let tiling = self.state.tiling.max(1);
                    let uv = egui::Rect::from_min_max(
                        egui::pos2(0.0, 0.0),
                        egui::pos2(1.0, 1.0),
                    );

                    for rep_y in 0..tiling {
                        for rep_x in 0..tiling {
                            let base_x = offset.x + rep_x as f32 * img_w * scale;
                            let base_y = offset.y + rep_y as f32 * img_h * scale;

                            for tile in &self.image_tiles {
                                let pos = egui::pos2(
                                    base_x + tile.x as f32 * scale,
                                    base_y + tile.y as f32 * scale,
                                );
                                let size = egui::vec2(
                                    tile.w as f32 * scale,
                                    tile.h as f32 * scale,
                                );
                                let rect = egui::Rect::from_min_size(pos, size);
                                ui.painter().image(
                                    tile.texture.id(),
                                    rect,
                                    uv,
                                    egui::Color32::WHITE,
                                );
                            }
                        }
                    }
                }
            });

        // Repaint if needed
        if self.state.network_mode {
            ctx.request_repaint();
        }
        if self.state.new_image_loaded {
            self.state.new_image_loaded = false;
        }

        // Save window geometry into volatile settings for persistence
        if let Some(outer) = ctx.input(|i| i.viewport().outer_rect) {
            self.state.volatile_settings.window_geometry = (
                (outer.left() as u32, outer.top() as u32),
                (outer.width() as u32, outer.height() as u32),
            );
        }
    }

    fn on_exit(&mut self, _gl: Option<&glow::Context>) {
        info!("Saving settings on exit");
        _ = self.state.persistent_settings.save_blocking();
        _ = self.state.volatile_settings.save_blocking();
    }
}
