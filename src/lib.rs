
pub mod ktx2_loader;
pub mod utils;
pub mod image_loader;
pub mod cache;
pub mod settings;
pub mod appstate;
pub mod image_editing;
pub mod shortcuts;
pub const FONT: &[u8; 309828] = include_bytes!("../res/fonts/Inter-Regular.ttf");
pub mod scrubber;
pub mod icons;
pub mod paint;
pub mod ui;
pub mod filebrowser;
pub mod texture_wrapper;
use utils::*;
#[cfg(feature = "update")]
pub mod update;


