pub mod app;
pub mod appstate;
pub(crate) mod cache;
pub(crate) mod comparelist;
pub(crate) mod glow_renderer;
pub mod image_editing;
pub mod image_loader;
pub(crate) mod ktx2_loader;
pub(crate) mod settings;
pub(crate) mod shortcuts;
pub mod utils;
pub const FONT: &[u8; 309828] = include_bytes!("../res/fonts/Inter-Regular.ttf");
pub const BOLD_FONT: &[u8; 344152] = include_bytes!("../res/fonts/Inter-Bold.ttf");
pub(crate) mod file_encoder;
// With file_open the system file dialog replaces the built-in browser, what only
// the browser uses is unused then
#[cfg_attr(feature = "file_open", allow(dead_code))]
pub(crate) mod filebrowser;
#[cfg_attr(feature = "file_open", allow(dead_code))]
pub(crate) mod icons;
pub mod net;
pub(crate) mod paint;
pub(crate) mod scrubber;
pub(crate) mod sequence;
#[cfg_attr(feature = "file_open", allow(dead_code))]
pub(crate) mod thumbnails;
pub(crate) mod toasts;
pub(crate) mod ui;
pub mod window_config;

// mod events;
#[cfg(target_os = "macos")]
pub mod mac;

#[cfg(test)]
mod tests;
