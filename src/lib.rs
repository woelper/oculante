pub mod app;
pub mod appstate;
pub(crate) mod cache;
pub(crate) mod comparelist;
pub(crate) mod desktop_theme;
pub(crate) mod glow_renderer;
pub mod image_editing;
pub mod image_loader;
pub(crate) mod ktx2_loader;
pub(crate) mod settings;
pub(crate) mod shortcuts;
pub mod utils;
pub const FONT: &[u8; 309828] = include_bytes!("../res/fonts/Inter-Regular.ttf");
pub const BOLD_FONT: &[u8; 344152] = include_bytes!("../res/fonts/Inter-Bold.ttf");
/// The icons of the UI, and the symbols of the stamps in annotations
pub const ICON_FONT: &[u8] = include_bytes!("../res/fonts/icons.ttf");
pub(crate) mod file_encoder;
pub(crate) mod file_sort;
// With file_open the system file dialog replaces the built-in browser, what only
// the browser uses is unused then
pub(crate) mod annotate;
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
pub(crate) mod tone;
pub(crate) mod ui;
pub mod window_config;

// mod events;
#[cfg(target_os = "macos")]
pub mod mac;

#[cfg(test)]
mod tests;
