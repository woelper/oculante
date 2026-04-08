pub mod app;
pub mod appstate;
pub mod cache;
pub mod comparelist;
pub mod glow_renderer;
pub mod image_editing;
pub mod input;
pub mod image_loader;
pub mod ktx2_loader;
pub mod settings;
pub mod shortcuts;
pub mod utils;
pub const FONT: &[u8; 309828] = include_bytes!("../res/fonts/Inter-Regular.ttf");
pub const BOLD_FONT: &[u8; 344152] = include_bytes!("../res/fonts/Inter-Bold.ttf");
pub mod file_encoder;
pub mod filebrowser;
pub mod icons;
pub mod net;
pub mod paint;
pub mod scrubber;
// texture_wrapper removed — replaced by glow_renderer
pub mod thumbnails;
pub mod ui;
#[cfg(feature = "update")]
pub mod update;
pub mod window_config;

// mod events;
#[cfg(target_os = "macos")]
pub mod mac;

#[cfg(test)]
mod tests;
