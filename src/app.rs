/// eframe-based application shell.
///
/// This implements `eframe::App` and replaces notan's init/update/draw callbacks.
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Align, FontData, FontDefinitions, FontFamily, FontTweak, Id};
use image::GenericImageView;
use log::{debug, info};
use nalgebra::Vector2;

use crate::appstate::*;
use crate::filebrowser::BrowserDir;
use crate::glow_renderer::{self, GlowRenderer, GlowTile, TexFilter, TexFormat};
use crate::settings::ColorTheme;
use crate::shortcuts::{self, key_pressed};
use crate::ui::*;
use crate::utils::*;
use crate::{BOLD_FONT, FONT};

#[cfg(feature = "file_open")]
use crate::filebrowser::browse_for_image_path;
#[cfg(feature = "turbo")]
use crate::image_editing::lossless_tx;

pub struct OculanteApp {
    pub state: OculanteState,
    first_frame: bool,
    /// Track if image needs re-upload to the GPU
    texture_dirty: bool,
    /// Glow renderer for direct GL rendering (created on the first frame)
    renderer: Option<GlowRenderer>,
    /// Tiles covering the current image. One tile for images that fit in the
    /// renderer's tile size; a grid for anything larger.
    image_tiles: Vec<GlowTile>,
    /// Format of the uploaded image (selects the channel swizzle)
    image_format: TexFormat,
    /// True while an animation is playing (keeps repainting)
    animation_playing: bool,
    /// Checker texture for transparency grid
    checker_texture: Option<egui::TextureHandle>,
    /// Set when a new image arrives; cleared after upload resets the view
    reset_after_upload: bool,
    /// The position stored for an image of the compare list that is being shown
    compare_geometry: Option<ImageGeometry>,
    /// True if the most recent frame is from an image in the compare menu
    last_frame_was_compared_image: bool,
    /// True if egui owned the pointer when the current press started.
    /// Prevents image drag for the entire press duration.
    egui_started_press: bool,
    /// Updates system theme during runtime
    last_system_theme: Option<egui::Theme>,
    /// When the current file was last checked for changes on disk
    last_file_check: Instant,
    /// Fonts of the system for scripts the built-in fonts do not cover
    system_fonts: SystemFonts,
}

enum SystemFonts {
    /// Holds the built-in fonts, the system fonts are added to them
    NotLoaded(Box<FontDefinitions>),
    Loading(std::sync::mpsc::Receiver<FontDefinitions>),
    Loaded,
}

impl OculanteApp {
    pub fn new(state: OculanteState) -> Self {
        Self {
            state,
            first_frame: true,
            texture_dirty: false,
            renderer: None,
            image_tiles: Vec::new(),
            image_format: TexFormat::Rgba8,
            animation_playing: false,
            checker_texture: None,
            reset_after_upload: false,
            compare_geometry: None,
            last_frame_was_compared_image: false,
            egui_started_press: false,
            last_system_theme: None,
            last_file_check: Instant::now(),
            system_fonts: SystemFonts::Loaded,
        }
    }

    /// Load the fonts of the system once text needs them. Reading them takes a
    /// moment and tens of megabytes, so it happens on demand and in the background.
    fn update_system_fonts(&mut self, ctx: &egui::Context) {
        if matches!(self.system_fonts, SystemFonts::Loaded) {
            return;
        }
        if !system_fonts_wanted() {
            // Text that comes from outside: paths and whatever is typed or pasted
            if let Some(path) = &self.state.current_path {
                want_system_fonts_for(&path.to_string_lossy());
            }
            for path in &self.state.volatile_settings.recent_images {
                want_system_fonts_for(&path.to_string_lossy());
            }
            ctx.input(|i| {
                for event in &i.events {
                    match event {
                        egui::Event::Text(text) | egui::Event::Paste(text) => {
                            want_system_fonts_for(text)
                        }
                        egui::Event::Ime(_) => want_system_fonts_for_ime(),
                        _ => {}
                    }
                }
            });
            if !system_fonts_wanted() {
                return;
            }
        }
        match std::mem::replace(&mut self.system_fonts, SystemFonts::Loaded) {
            SystemFonts::NotLoaded(fonts) => {
                let (sender, receiver) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    _ = sender.send(load_system_fonts(*fonts));
                    request_repaint();
                });
                self.system_fonts = SystemFonts::Loading(receiver);
            }
            SystemFonts::Loading(receiver) => match receiver.try_recv() {
                Ok(fonts) => ctx.set_fonts(fonts),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.system_fonts = SystemFonts::Loading(receiver)
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
            },
            SystemFonts::Loaded => {}
        }
    }

    /// Upload or re-upload the current image as GL texture tile(s).
    /// Channel selection happens in the shader, so this is only needed when pixels change.
    fn upload_image_to_glow(&mut self, gl: &glow::Context) {
        let Some(renderer) = &self.renderer else {
            return;
        };

        // Prefer the edit result if present. Animations show their frames unedited.
        let edit_result = &self.state.edit_state.result_pixel_op;
        let img = if edit_result.width() > 0 && !self.animation_playing {
            edit_result
        } else {
            match &self.state.current_image {
                Some(img) => img,
                None => return,
            }
        };

        let (w, h) = (img.width(), img.height());

        // 8 bit images are uploaded as they are, everything else is converted first
        let (format, layout) = glow_renderer::texture_layout(img);
        debug!("Uploading {:?} as {:?}", img.color(), format);
        let bytes = layout.as_bytes();

        let filter = TexFilter {
            linear_min: self.state.persistent_settings.linear_min_filter,
            linear_mag: self.state.persistent_settings.linear_mag_filter,
            mipmaps: self.state.persistent_settings.use_mipmaps,
        };

        // Update in place if the tile layout is unchanged, otherwise start over
        if !renderer.update_tiles(gl, &self.image_tiles, bytes, w, h, format, filter) {
            let old = std::mem::take(&mut self.image_tiles);
            renderer.delete_tiles(gl, old);
            self.image_tiles = renderer.create_tiles(gl, bytes, w, h, format, filter);
        }
        self.image_format = format;

        // Now that the texture is ready, update geometry and reset view
        self.state.image_geometry.dimensions = (w, h);
        if self.last_frame_was_compared_image {
            // A compared image goes to the position stored for it, unless the view
            // is to be kept across the compared images. It is never fitted anew.
            if let Some(geometry) = self.compare_geometry.take()
                && !self.state.persistent_settings.compare_keep_view
            {
                self.state.image_geometry.scale = geometry.scale;
                self.state.image_geometry.offset = geometry.offset;
            }
        } else if !self.state.persistent_settings.keep_view && self.reset_after_upload {
            self.state.reset_image = true;
        }
        self.reset_after_upload = false;

        self.texture_dirty = false;
    }

    fn first_frame_setup(&mut self, ctx: &egui::Context) {
        set_repaint_context(ctx);
        let mut fonts = FontDefinitions::default();
        egui_extras::install_image_loaders(ctx);

        let dpi = ctx.pixels_per_point();
        // Apply user UI scale on top of system DPI
        ctx.set_zoom_factor(self.state.persistent_settings.ui_scale);
        ctx.options_mut(|o| o.zoom_with_keyboard = false);

        let offset = if dpi > 1.0 { 0.0 } else { -1.4 };

        fonts.font_data.insert(
            "inter".to_owned(),
            Arc::new(FontData::from_static(FONT).tweak(FontTweak {
                scale: 1.0,
                y_offset_factor: 0.0,
                y_offset: offset,
                ..Default::default()
            })),
        );
        fonts.font_data.insert(
            "inter-bold".to_owned(),
            Arc::new(FontData::from_static(BOLD_FONT).tweak(FontTweak {
                scale: 1.0,
                y_offset_factor: 0.0,
                y_offset: offset,
                ..Default::default()
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
                    ..Default::default()
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

        // Fonts of the system for other scripts are loaded once they are needed
        want_system_fonts_for_locale();
        self.system_fonts = SystemFonts::NotLoaded(Box::new(fonts.clone()));

        apply_theme(&mut self.state, ctx);
        self.last_system_theme = ctx.system_theme();
        ctx.set_fonts(fonts);

        // Load checker texture for transparency grid (once)
        let checker_data = include_bytes!("../res/checker.png");
        if let Ok(checker_img) = image::load_from_memory(checker_data) {
            let rgba = checker_img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
            self.checker_texture = Some(ctx.load_texture(
                "checker",
                color_image,
                egui::TextureOptions {
                    magnification: egui::TextureFilter::Nearest,
                    minification: egui::TextureFilter::Nearest,
                    wrap_mode: egui::TextureWrapMode::Repeat,
                    ..Default::default()
                },
            ));
        }

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
        // If an AnimationStart is encountered during drain, preserve its reset flag
        let latest_frame = self
            .state
            .texture_channel
            .1
            .try_iter()
            .inspect(|f| {
                if matches!(f, Frame::AnimationStart(_)) {
                    self.reset_after_upload = true;
                }
            })
            .last();

        if let Some(frame) = latest_frame {
            self.state.is_loaded = true;
            self.last_frame_was_compared_image = matches!(frame, Frame::CompareResult(_, _));
            if let Frame::CompareResult(_, geometry) = &frame {
                self.compare_geometry = Some(*geometry);
            }

            // Update scrubber on new images
            // Also match Animation if an AnimationStart was drained
            if matches!(
                &frame,
                Frame::AnimationStart(_) | Frame::Still(_) | Frame::ImageCollectionMember(_)
            ) || (self.reset_after_upload && matches!(&frame, Frame::Animation(_, _)))
            {
                if let Some(path) = &self.state.current_path {
                    if self.state.scrubber.has_folder_changed(path)
                        && !self.state.scrubber.fixed_paths
                    {
                        self.state.scrubber = crate::scrubber::Scrubber::new(path);
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
                if let Some(path) = &self.state.current_path
                    && self.state.persistent_settings.max_recents > 0
                    && !self.state.volatile_settings.recent_images.contains(path)
                {
                    self.state
                        .volatile_settings
                        .recent_images
                        .push_front(path.clone());
                    self.state
                        .volatile_settings
                        .recent_images
                        .truncate(self.state.persistent_settings.max_recents.into());
                }
            }

            // Clear metadata and edit state for non-animation frames
            if !matches!(frame, Frame::Animation(_, _)) {
                self.state.image_metadata = None;
            }
            if !matches!(
                frame,
                Frame::Animation(_, _) | Frame::EditResult(_) | Frame::UpdateTexture
            ) {
                self.state.edit_state.result_pixel_op = Default::default();
                self.state.edit_state.result_image_op = Default::default();
                if !self.state.persistent_settings.keep_edits {
                    self.state.edit_state = Default::default();
                }
            }

            match frame {
                Frame::Still(img)
                | Frame::CompareResult(img, _)
                | Frame::ImageCollectionMember(img) => {
                    debug!("Received image {}x{}", img.width(), img.height());

                    // Insert into cache for fast back/forth navigation
                    if self.state.persistent_settings.max_cache != 0
                        && let Some(p) = self.state.current_path.clone()
                    {
                        self.state.player.cache.insert(&p, img.clone());
                    }

                    self.state.current_image = Some(img);
                    self.state.new_image_loaded = true;
                    self.texture_dirty = true;
                    self.animation_playing = false;
                    self.reset_after_upload = true;
                    ctx.request_repaint();
                }
                Frame::AnimationStart(img) => {
                    debug!("Animation start {}x{}", img.width(), img.height());
                    self.state.current_image = Some(img);
                    self.state.new_image_loaded = true;
                    self.reset_after_upload = true;
                    self.texture_dirty = true;
                    self.animation_playing = true;
                    ctx.request_repaint();
                }
                Frame::EditResult(img) => {
                    self.state.current_image = Some(img);
                    self.texture_dirty = true;
                    ctx.request_repaint();
                }
                Frame::Animation(img, _delay) => {
                    // delay is not used since the sender delays (sleeps) and we repaint on every frame when anim is playing
                    self.state.current_image = Some(img);
                    self.texture_dirty = true;
                    self.animation_playing = true;
                }
                Frame::UpdateTexture => {
                    debug!("received UpdateTexture");
                    self.texture_dirty = true;
                    ctx.request_repaint();
                }
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
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx_owned = ui.ctx().clone();
        let ctx = &ctx_owned;

        // Initialize on first frame
        if self.first_frame {
            self.first_frame_setup(ctx);
            if let Some(gl) = frame.gl() {
                let renderer = GlowRenderer::new(gl);
                debug!("Max texture size: {}", renderer.max_texture_size);
                self.renderer = Some(renderer);
            }
        } else if self.state.persistent_settings.theme == ColorTheme::System {
            let current_system_theme = ctx.system_theme();
            if current_system_theme != self.last_system_theme {
                self.last_system_theme = current_system_theme;
                apply_theme(&mut self.state, ctx);
            }
        }

        self.update_system_fonts(ctx);

        // The setting "Redraw every frame" turns off drawing on demand
        if self.state.persistent_settings.force_redraw {
            ctx.request_repaint();
        }

        // File names piped in at startup arrive from a background thread
        if let Some(receiver) = &self.state.piped_paths {
            match receiver.try_recv() {
                Ok(paths) => {
                    self.state.piped_paths = None;
                    open_paths(&mut self.state, paths);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.state.piped_paths = None,
            }
        }

        // Reload the image if its file changed on disk
        if self.last_file_check.elapsed() > Duration::from_millis(800) {
            self.last_file_check = Instant::now();
            if let Some(path) = &self.state.current_path {
                self.state.player.check_modified(path);
            }
        }

        // Upload image to the GPU if needed
        if self.texture_dirty
            && let Some(gl) = frame.gl()
        {
            debug!("Texture was dirty. uploading");
            self.upload_image_to_glow(gl);
        }

        // Update window size
        let screen_rect = ctx.content_rect();
        let window_size = Vector2::new(screen_rect.width(), screen_rect.height());
        // By resetting the image, we make it fill the window on resize
        if self.state.persistent_settings.fit_image_on_window_resize
            && window_size != self.state.window_size
            && self.state.window_size != Vector2::zeros()
        {
            self.state.reset_image = true;
        }
        self.state.window_size = window_size;

        // Process channels
        self.process_load_channel();
        self.process_texture_channel(ctx);
        self.process_messages();

        if let Ok(info) = self.state.extended_info_channel.1.try_recv() {
            for value in info.exif.values() {
                want_system_fonts_for(value);
            }
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
        let middle_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
        let any_down = primary_down || middle_down;

        // Track whether egui owned the pointer at press start.
        // This persists for the entire press so that slider drags
        // leaving the window don't turn into image pans.
        if !any_down {
            self.egui_started_press = false;
            self.state.drag_enabled = false;
        } else if !self.state.drag_enabled && !self.egui_started_press {
            // Button just went down — check who owns it
            if ctx.egui_is_using_pointer() || self.state.pointer_over_ui || self.state.mouse_grab {
                self.egui_started_press = true;
            }
        }

        if middle_down && !self.egui_started_press {
            self.state.drag_enabled = true;
            self.state.image_geometry.offset += self.state.mouse_delta;
        } else if primary_down && !self.egui_started_press && !self.state.mouse_grab {
            self.state.drag_enabled = true;
        }
        if self.state.drag_enabled && !self.state.mouse_grab {
            self.state.image_geometry.offset += self.state.mouse_delta;
        }

        // Scroll zoom
        let scroll_delta = ctx.input(|i| i.smooth_scroll_delta.y);
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
                let path = file.path();
                if let Some(ext) = path.extension()
                    && SUPPORTED_EXTENSIONS.contains(&ext.to_string_lossy().to_lowercase().as_str())
                {
                    self.state.is_loaded = false;
                    self.state.current_image = None;
                    self.state.player.load(path);
                    self.state.current_path = Some(path.to_path_buf());
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
            crate::ui::open_popup(ctx, Id::new(&id));
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
            if crate::ui::is_popup_open(ctx, Id::new("OPEN")) {
                crate::filebrowser::browse_modal(
                    false,
                    SUPPORTED_EXTENSIONS,
                    &mut state.volatile_settings,
                    |p| {
                        let _ = state.load_channel.0.clone().send(p.to_path_buf());
                    },
                    ctx,
                    Id::new("OPEN"),
                );
            }
        }

        // Top menu bar
        if !state.persistent_settings.zen_mode {
            egui::Panel::top("menu")
                .exact_size(36.0)
                .show_separator_line(false)
                .show(ui, |ui| {
                    main_menu(ui, state);
                });
        }
        if state.persistent_settings.zen_mode && state.persistent_settings.borderless {
            egui::Panel::top("menu_zen")
                .min_size(40.)
                .default_size(40.)
                .show_separator_line(false)
                .frame(egui::containers::Frame::NONE)
                .show(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        drag_area(ui, state);
                        ui.add_space(15.);
                        draw_hamburger_menu(ui, state);
                    });
                });
        }

        show_delete_confirmation_modal(ctx, state);

        // Scrub bar
        if state.persistent_settings.show_scrub_bar {
            egui::Panel::bottom("scrubber")
                .exact_size(22.)
                .show(ui, |ui| {
                    scrubber_ui(state, ui);
                });
        }

        // Edit panel
        if state.persistent_settings.edit_enabled
            && !state.settings_enabled
            && !state.persistent_settings.zen_mode
            && state.current_image.is_some()
        {
            edit_ui(ui, state);
        }

        // Info panel
        if state.persistent_settings.info_enabled
            && !state.settings_enabled
            && !state.persistent_settings.zen_mode
            && state.current_image.is_some()
        {
            let (_bbox_tl, _bbox_br) = info_ui(
                ui,
                state,
                self.renderer.as_ref(),
                &self.image_tiles,
                self.image_format,
            );
        }

        let canvas_rect = ui.available_rect_before_wrap();
        // Tools that draw over the image from inside a panel clip to this
        ctx.data_mut(|data| {
            data.insert_temp(Id::new(crate::image_editing::CANVAS_RECT), canvas_rect)
        });
        let pointer_pos = ctx.input(|i| i.pointer.interact_pos());
        let over_floating_layer = pointer_pos
            .and_then(|p| ctx.layer_id_at(p))
            .is_some_and(|layer| layer.order != egui::Order::Background);
        state.pointer_over_ui =
            over_floating_layer || pointer_pos.is_none_or(|p| !canvas_rect.contains(p));
        state.mouse_grab = ctx.egui_is_using_pointer()
            || state.edit_state.painting
            || state.pointer_over_ui
            || state.edit_state.block_panning;
        state.key_grab = ctx.egui_wants_keyboard_input();

        // Reset image to fit window
        if state.reset_image
            && let Some(current_image) = &state.current_image
        {
            let draw_area = ctx.content_rect();
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
            if key_pressed(ctx, state, ScrubBar) {
                state.persistent_settings.show_scrub_bar =
                    !state.persistent_settings.show_scrub_bar;
            }
            if key_pressed(ctx, state, AlwaysOnTop) {
                state.always_on_top = !state.always_on_top;
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if state.always_on_top {
                    egui::WindowLevel::AlwaysOnTop
                } else {
                    egui::WindowLevel::Normal
                }));
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
            if key_pressed(ctx, state, CompareNext) {
                compare_next(state);
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
                    let center = Vector2::new(state.window_size.x / 2., state.window_size.y / 2.);
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
                    let center = Vector2::new(state.window_size.x / 2., state.window_size.y / 2.);
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
            if key_pressed(ctx, state, Copy)
                && let Some(img) = effective_image(state)
            {
                clipboard_copy(img);
                state.send_message_info("Image copied");
            }
            if key_pressed(ctx, state, CopyPath)
                && let Some(path) = &state.current_path
            {
                clipboard_copy_path(path);
                state.send_message_info("Path copied");
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
                // Shared with trash button in top bar. Both trigger the same confirmation modal.
                request_delete_current_file(ctx, state);
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
            if key_pressed(ctx, state, LosslessRotateRight)
                && let Some(p) = &state.current_path
                && lossless_tx(p, turbojpeg::Transform::op(turbojpeg::TransformOp::Rot90)).is_ok()
            {
                state.is_loaded = false;
                state.player.cache.clear();
                state.player.load(p);
            }
            #[cfg(feature = "turbo")]
            if key_pressed(ctx, state, LosslessRotateLeft)
                && let Some(p) = &state.current_path
                && lossless_tx(p, turbojpeg::Transform::op(turbojpeg::TransformOp::Rot270)).is_ok()
            {
                state.is_loaded = false;
                state.player.cache.clear();
                state.player.load(p);
            }
        }

        limit_offset(&mut self.state);

        // ===== IMAGE RENDERING =====
        // Render image in egui's CentralPanel (behind side panels, below UI)
        let bg = self.state.persistent_settings.background_color;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(bg[0], bg[1], bg[2])))
            .show(ui, |ui| {
                if let Some(renderer) = &self.renderer
                    && !self.image_tiles.is_empty()
                {
                    let offset = self.state.image_geometry.offset;
                    let scale = self.state.image_geometry.scale;
                    let img_w = self.state.image_geometry.dimensions.0 as f32;
                    let img_h = self.state.image_geometry.dimensions.1 as f32;
                    let tiling = self.state.tiling.max(1);

                    // Draw checker background for transparency (single textured quad per tile)
                    if self.state.persistent_settings.show_checker_background
                        && let Some(checker) = &self.checker_texture
                    {
                        // The checker texture tiles via wrap_mode = Repeat.
                        // UV is scaled so the pattern stays a fixed screen size.
                        let checker_px = checker.size()[0] as f32;

                        for rep_y in 0..tiling {
                            for rep_x in 0..tiling {
                                let base_x = offset.x + rep_x as f32 * img_w * scale;
                                let base_y = offset.y + rep_y as f32 * img_h * scale;
                                let img_rect = egui::Rect::from_min_size(
                                    egui::pos2(base_x, base_y),
                                    egui::vec2(img_w * scale, img_h * scale),
                                );
                                // UV repeats = image screen size / checker texture size
                                let repeats_x = (img_w * scale) / checker_px;
                                let repeats_y = (img_h * scale) / checker_px;
                                let checker_uv = egui::Rect::from_min_max(
                                    egui::pos2(0.0, 0.0),
                                    egui::pos2(repeats_x, repeats_y),
                                );
                                ui.painter().image(
                                    checker.id(),
                                    img_rect,
                                    checker_uv,
                                    egui::Color32::WHITE,
                                );
                            }
                        }
                    }

                    // The image itself: all tiles are drawn directly with GL in a paint
                    // callback, the channel selection is done by the shader.
                    let mut quads = Vec::with_capacity(self.image_tiles.len() * tiling * tiling);
                    for rep_y in 0..tiling {
                        for rep_x in 0..tiling {
                            let base = [
                                offset.x + rep_x as f32 * img_w * scale,
                                offset.y + rep_y as f32 * img_h * scale,
                            ];
                            quads.extend(self.image_tiles.iter().map(|t| t.quad(base, scale)));
                        }
                    }
                    let shader = renderer.image_shader();
                    let (swizzle_mat, color_offset) = glow_renderer::get_swizzle_mat_vec(
                        self.state.persistent_settings.current_channel,
                        self.image_format,
                    );
                    let swizzle_mat = swizzle_mat.to_cols_array();
                    let color_offset = color_offset.to_array();
                    let cb = egui_glow::CallbackFn::new(move |info, painter| {
                        glow_renderer::paint_quads(
                            painter.gl(),
                            shader,
                            &info,
                            &swizzle_mat,
                            &color_offset,
                            false,
                            &quads,
                        );
                    });
                    ui.painter().add(egui::PaintCallback {
                        rect: ui.max_rect(),
                        callback: Arc::new(cb),
                    });

                    // The outline of the brush while painting
                    if self.state.edit_state.painting
                        && !self.state.pointer_over_ui
                        && let Some(stroke) = self.state.edit_state.paint_strokes.last()
                    {
                        // The brush is as wide as this fraction of the smaller image side
                        let radius = stroke.width * img_w.min(img_h) * scale / 2.;
                        ui.painter().circle_stroke(
                            egui::pos2(self.state.cursor.x, self.state.cursor.y),
                            radius,
                            egui::Stroke::new(1.5, egui::Color32::from_white_alpha(128)),
                        );
                    }

                    // Draw frame around image if enabled
                    if self.state.persistent_settings.show_frame {
                        for rep_y in 0..tiling {
                            for rep_x in 0..tiling {
                                let frame_rect = egui::Rect::from_min_size(
                                    egui::pos2(
                                        offset.x + rep_x as f32 * img_w * scale,
                                        offset.y + rep_y as f32 * img_h * scale,
                                    ),
                                    egui::vec2(img_w * scale, img_h * scale),
                                );
                                ui.painter().rect_stroke(
                                    frame_rect,
                                    0.0,
                                    egui::Stroke::new(1.0, egui::Color32::GRAY),
                                    egui::StrokeKind::Inside,
                                );
                            }
                        }
                    }
                }
            });

        // Automatically hide cursor after a period of inactivity in zen mode
        let hide_delay = self.state.persistent_settings.zen_mode_cursor_timeout;
        if self.state.persistent_settings.zen_mode
            && !self.state.settings_enabled
            && hide_delay > 0.0
        {
            let idle_time = ctx.input(|i| i.pointer.time_since_last_movement());
            if idle_time >= hide_delay && !ctx.is_pointer_over_egui() {
                ctx.set_cursor_icon(egui::CursorIcon::None);
            } else if idle_time < hide_delay {
                ctx.request_repaint_after(Duration::from_secs_f32(hide_delay - idle_time));
            }
        }

        // Repaint if needed
        if self.state.network_mode || self.animation_playing {
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

    fn on_exit(&mut self, gl: Option<&glow::Context>) {
        if let (Some(gl), Some(renderer)) = (gl, self.renderer.take()) {
            let tiles = std::mem::take(&mut self.image_tiles);
            renderer.delete_tiles(gl, tiles);
            renderer.destroy(gl);
        }
        info!("Saving settings on exit");
        _ = self.state.persistent_settings.save_blocking();
        _ = self.state.volatile_settings.save_blocking();
    }
}

/// Keep the image from being moved out of the window completely
fn limit_offset(state: &mut OculanteState) {
    let geometry = &mut state.image_geometry;
    let scaled_width = geometry.dimensions.0 as f32 * geometry.scale;
    let scaled_height = geometry.dimensions.1 as f32 * geometry.scale;
    geometry.offset.x = geometry
        .offset
        .x
        .min(state.window_size.x)
        .max(-scaled_width);
    geometry.offset.y = geometry
        .offset
        .y
        .min(state.window_size.y)
        .max(-scaled_height);
}
