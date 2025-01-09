pub mod appstate;
pub mod cache;
pub mod image_editing;
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
pub mod paint;
pub mod net;
pub mod scrubber;
pub mod texture_wrapper;
pub mod thumbnails;
pub mod ui;
#[cfg(feature = "update")]
pub mod update;

// mod events;
#[cfg(target_os = "macos")]
pub mod mac;

#[cfg(test)]
mod tests;




