use crate::{shortcuts::*, utils::ColorChannel};
use anyhow::{anyhow, Result};
use log::{info, trace};
use notan::egui::{Context, Visuals};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{create_dir_all, File},
    path::PathBuf,
};

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
    /// The UI accent color
    pub accent_color: [u8; 3],
    /// The BG color
    pub background_color: [u8; 3],
    /// Should we sync to monitor rate? This makes the app snappier, but also more resource intensive.
    pub vsync: bool,
    pub force_redraw: bool,
    /// Keyboard map to actions
    pub shortcuts: Shortcuts,
    /// Do not reset view when receiving a new image
    pub keep_view: bool,
    /// How many images to keep in cache
    pub max_cache: usize,
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
    pub current_channel: ColorChannel,
    /// How much to scale SVG images when rendering
    pub svg_scale: f32,
    pub zen_mode: bool,
    pub theme: ColorTheme,
    pub linear_mag_filter: bool,
    pub linear_min_filter: bool,
    pub use_mipmaps: bool,
    pub fit_image_on_window_resize: bool,
    pub zoom_multiplier: f32,
    pub borderless: bool,
    pub min_window_size: (u32, u32),
}

impl Default for PersistentSettings {
    fn default() -> Self {
        PersistentSettings {
            accent_color: [255, 0, 75],
            background_color: [51, 51, 51],
            vsync: true,
            force_redraw: false,
            shortcuts: Shortcuts::default_keys(),
            keep_view: Default::default(),
            max_cache: 30,
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
            svg_scale: 1.0,
            zen_mode: false,
            theme: ColorTheme::Dark,
            linear_mag_filter: false,
            linear_min_filter: true,
            use_mipmaps: true,
            fit_image_on_window_resize: false,
            zoom_multiplier: 1.0,
            borderless: false,
            min_window_size: (100, 100),
        }
    }
}

impl PersistentSettings {
    pub fn load() -> Result<Self> {
        //data_local_dir
        let config_path = dirs::config_local_dir()
            .ok_or(anyhow!("Can't get config_local dir"))?
            .join("oculante")
            .join("config.json")
            .canonicalize()
            // migrate old config
            .unwrap_or(
                dirs::data_local_dir()
                    .ok_or(anyhow!("Can't get data_local dir"))?
                    .join(".oculante"),
            );

        Ok(serde_json::from_reader::<_, PersistentSettings>(
            File::open(config_path)?,
        )?)
    }

    // save settings in a thread so we don't block
    pub fn save_threaded(&self) {
        let settings = self.clone();
        std::thread::spawn(move || {
            _ = save(&settings);
        });
    }

    pub fn save_blocking(&self) {
        _ = save(&self);
    }
}

fn save(ps: &PersistentSettings) -> Result<()> {
    let local_dir = dirs::config_local_dir()
        .ok_or(anyhow!("Can't get local dir"))?
        .join("oculante");
    if !local_dir.exists() {
        _ = create_dir_all(&local_dir);
    }
    let f = File::create(local_dir.join("config.json"))?;
    _ = serde_json::to_writer_pretty(f, ps)?;
    trace!("Saved to {}", local_dir.display());
    Ok(())
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct VolatileSettings {
    pub favourite_images: HashSet<PathBuf>,
    pub recent_images: Vec<PathBuf>,
    pub window_geometry: ((u32, u32), (u32, u32)),
    pub last_open_directory: PathBuf,
}

impl VolatileSettings {
    pub fn load() -> Result<Self> {
        let config_path = dirs::config_local_dir()
            .ok_or(anyhow!("Can't get config_local dir"))?
            .join("oculante")
            .join("config_volatile.json")
            .canonicalize()
            // migrate old config
            ?;

        let s = serde_json::from_reader::<_, VolatileSettings>(File::open(config_path)?)?;
        info!("Loaded volatile settings.");
        Ok(s)
    }

    pub fn save_blocking(&self) -> Result<()> {
        let local_dir = dirs::config_local_dir()
            .ok_or(anyhow!("Can't get local dir"))?
            .join("oculante");
        if !local_dir.exists() {
            _ = create_dir_all(&local_dir);
        }

        let f = File::create(local_dir.join("config_volatile.json"))?;
        let _res = serde_json::to_writer_pretty(f, self)?;
        trace!("Saved volatile settings");
        Ok(())
    }
}

pub fn set_system_theme(ctx: &Context) {
    match dark_light::detect() {
        dark_light::Mode::Dark => ctx.set_visuals(Visuals::dark()),
        dark_light::Mode::Light => ctx.set_visuals(Visuals::light()),
        dark_light::Mode::Default => ctx.set_visuals(Visuals::dark()),
    }
}
