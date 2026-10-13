use crate::{file_encoder::FileEncoder, shortcuts::*, utils::ColorChannel};
use anyhow::{Result, anyhow};
use egui::{Context, Theme, Visuals};
use log::{debug, info, trace};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet, VecDeque},
    fs::{File, create_dir_all},
    path::PathBuf,
};

fn get_config_dir() -> Result<PathBuf> {
    Ok(dirs::data_local_dir()
        .ok_or(anyhow!("Can't get local dir"))?
        .join("oculante"))
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ColorTheme {
    /// Light Theme
    Light,
    /// Dark Theme
    Dark,
    /// Same as system
    System,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct PersistentSettings {
    /// UI Scale
    pub ui_scale: f32,
    /// The UI accent color
    pub accent_color: [u8; 3],
    /// Set once the user picks an accent color, so theme switches stop overriding it
    pub accent_color_is_custom: bool,
    /// The BG color
    pub background_color: [u8; 3],
    /// Set once the user picks a background color, so theme switches stop overriding it
    pub background_color_is_custom: bool,
    /// Should we sync to monitor rate? This makes the app snappier, but also more resource intensive.
    pub vsync: bool,
    pub force_redraw: bool,
    /// Keyboard map to actions
    #[serde(deserialize_with = "crate::shortcuts::deserialize_shortcuts")]
    pub shortcuts: Shortcuts,
    /// Do not reset view when receiving a new image
    pub keep_view: bool,
    /// Do not reset view when switching between images in the compare list.  Unlike `keep_view`, this only applies to navigation within the compare list (e.g. clicking a compare list entry), not regular image browsing.
    pub compare_keep_view: bool,
    /// How many images to keep in cache
    pub max_cache: usize,
    /// How many recent images to keep track of
    pub max_recents: u8,
    pub show_scrub_bar: bool,
    pub wrap_folder: bool,
    /// Whether to keep the image edit stack
    pub keep_edits: bool,
    pub title_format: String,
    pub info_enabled: bool,
    pub edit_enabled: bool,
    pub show_checker_background: bool,
    pub show_minimap: bool,
    pub show_frame: bool,
    #[serde(skip)]
    pub current_channel: ColorChannel,
    pub zen_mode: bool,
    pub zen_mode_cursor_timeout: f32,
    pub show_zen_mode_notification: bool,
    pub theme: ColorTheme,
    pub linear_mag_filter: bool,
    pub linear_min_filter: bool,
    pub use_mipmaps: bool,
    pub fit_image_on_window_resize: bool,
    pub zoom_multiplier: f32,
    /// Automatically scale/zoom the image to fit the frame
    pub auto_scale: bool,
    pub borderless: bool,
    pub min_window_size: (u32, u32),
    pub experimental_features: bool,
    /// Tunables for decoders, such as max memory usage
    pub decoders: DecoderSettings,
}

impl Default for PersistentSettings {
    fn default() -> Self {
        PersistentSettings {
            ui_scale: 1.0,
            accent_color: [255, 0, 75],
            accent_color_is_custom: false,
            background_color: [30, 30, 30],
            background_color_is_custom: false,
            vsync: true,
            force_redraw: false,
            shortcuts: default_shortcuts(),
            keep_view: Default::default(),
            compare_keep_view: Default::default(),
            max_cache: 30,
            max_recents: 12,
            show_scrub_bar: Default::default(),
            wrap_folder: true,
            keep_edits: Default::default(),
            title_format: "{APP} | {VERSION} | {FULLPATH}".into(),
            info_enabled: Default::default(),
            edit_enabled: Default::default(),
            show_checker_background: Default::default(),
            show_minimap: Default::default(),
            show_frame: Default::default(),
            current_channel: ColorChannel::Rgba,
            zen_mode: false,
            zen_mode_cursor_timeout: 1.5,
            show_zen_mode_notification: true,
            theme: ColorTheme::Dark,
            linear_mag_filter: false,
            linear_min_filter: true,
            use_mipmaps: true,
            fit_image_on_window_resize: false,
            zoom_multiplier: 1.0,
            auto_scale: false,
            borderless: false,
            min_window_size: (100, 100),
            experimental_features: false,
            decoders: Default::default(),
        }
    }
}

impl PersistentSettings {
    pub fn load() -> Result<Self> {
        let config_path = get_config_dir()?
            .join("config.json")
            .canonicalize()
            // migrate old config
            .unwrap_or(
                dirs::config_local_dir()
                    .ok_or(anyhow!("Can't get config_local_dir"))?
                    .join(".oculante"),
            );
        debug!("Loaded persistent settings: {}", config_path.display());

        Ok(serde_json::from_reader::<_, PersistentSettings>(
            File::open(config_path)?,
        )?)
    }

    pub fn save_blocking(&self) -> Result<()> {
        let config_dir = get_config_dir()?;
        if !config_dir.exists() {
            info!("Created {}", config_dir.display());
            _ = create_dir_all(&config_dir);
        }
        let config_path = config_dir.join("config.json");
        let f = File::create(&config_path)?;
        serde_json::to_writer_pretty(f, self)?;
        debug!("Saved to {}", config_path.display());
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct VolatileSettings {
    pub favourite_images: HashSet<PathBuf>,
    pub recent_images: VecDeque<PathBuf>,
    pub window_geometry: ((u32, u32), (u32, u32)),
    pub last_open_directory: PathBuf,
    pub folder_bookmarks: BTreeSet<PathBuf>,
    pub encoding_options: Vec<FileEncoder>,
}

impl Default for VolatileSettings {
    fn default() -> Self {
        Self {
            favourite_images: Default::default(),
            recent_images: Default::default(),
            window_geometry: Default::default(),
            last_open_directory: Default::default(),
            folder_bookmarks: Default::default(),
            encoding_options: [
                // ("jpg".to_string(), FileEncoder::Jpg { quality: 75 }),
                // ("png".to_string(), FileEncoder::WebP),
                FileEncoder::Jpg { quality: 75 },
                FileEncoder::WebP,
                FileEncoder::Png {
                    compressionlevel: crate::file_encoder::CompressionLevel::Default,
                },
                FileEncoder::Bmp,
                FileEncoder::Avif,
            ]
            .into_iter()
            .collect(),
        }
    }
}

impl VolatileSettings {
    pub fn load() -> Result<Self> {
        let config_path = get_config_dir()?
            .join("config_volatile.json")
            .canonicalize()
            // migrate old config
            ?;

        let s = serde_json::from_reader::<_, VolatileSettings>(File::open(config_path)?)?;
        info!("Loaded volatile settings.");
        Ok(s)
    }

    // Remove any entries from recent images that no longer exist on startup
    pub fn remove_missing_recents(&mut self) {
        let before = self.recent_images.len();
        self.recent_images.retain(|p| p.exists());
        let removed = before - self.recent_images.len();
        if removed > 0 {
            debug!("Removed {removed} recent image(s) that can no longer be found");
        }
    }

    pub fn save_blocking(&self) -> Result<()> {
        let local_dir = get_config_dir()?;
        if !local_dir.exists() {
            _ = create_dir_all(&local_dir);
        }

        let f = File::create(local_dir.join("config_volatile.json"))?;
        serde_json::to_writer_pretty(f, self)?;
        trace!("Saved volatile settings");
        Ok(())
    }
}

pub fn set_system_theme(ctx: &Context) {
    match ctx.system_theme() {
        Some(Theme::Dark) => ctx.set_visuals(Visuals::dark()),
        Some(Theme::Light) => ctx.set_visuals(Visuals::light()),
        None => ctx.set_visuals(Visuals::dark()),
    }
}

/// SVGs are drawn at twice their nominal size, so they stay sharp when zoomed in
pub const DEFAULT_SVG_SCALE: f32 = 2.0;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct DecoderSettings {
    pub svg_scale: f32,
}

impl Default for DecoderSettings {
    fn default() -> Self {
        Self {
            svg_scale: DEFAULT_SVG_SCALE,
        }
    }
}
