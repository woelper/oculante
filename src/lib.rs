pub mod appstate;
pub mod cache;
pub mod image_editing;
pub mod image_loader;
pub mod ktx2_loader;
pub mod settings;
pub mod shortcuts;
pub mod utils;
pub const FONT: &[u8; 309828] = include_bytes!("../res/fonts/Inter-Regular.ttf");
pub mod filebrowser;
pub mod icons;
pub mod paint;
pub mod scrubber;
pub mod texture_wrapper;
pub mod ui;
use utils::*;
#[cfg(feature = "update")]
pub mod update;
