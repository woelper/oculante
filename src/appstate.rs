use crate::{
    image_editing::EditState,
    scrubber::Scrubber,
    settings::PersistentSettings,
    utils::{ColorChannel, ExtendedImageInfo, Frame, Player},
};
use image::{RgbaImage, DynamicImage};
use nalgebra::Vector2;
use notan::{egui::epaint::ahash::HashMap, prelude::Texture, AppState};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
};

#[derive(Debug, Clone)]
pub struct ImageGeometry {
    /// The scale of the displayed image
    pub scale: f32,
    /// Image offset on canvas
    pub offset: Vector2<f32>,
}

/// The state of the application
#[derive(Debug, AppState)]
pub struct OculanteState {
    pub image_geometry: ImageGeometry,
    pub compare_list: HashMap<PathBuf, ImageGeometry>,
    pub drag_enabled: bool,
    pub reset_image: bool,
    pub message: Option<String>,
    /// Is the image fully loaded?
    pub is_loaded: bool,
    pub window_size: Vector2<f32>,
    pub cursor: Vector2<f32>,
    pub cursor_relative: Vector2<f32>,
    pub image_dimension: (u32, u32),
    pub sampled_color: [f32; 4],
    pub mouse_delta: Vector2<f32>,
    pub texture_channel: (Sender<Frame>, Receiver<Frame>),
    pub message_channel: (Sender<String>, Receiver<String>),
    pub extended_info_channel: (Sender<ExtendedImageInfo>, Receiver<ExtendedImageInfo>),
    pub extended_info_loading: bool,
    /// The Player, responsible for loading and sending Frames
    pub player: Player,
    pub current_texture: Option<Texture>,
    pub current_path: Option<PathBuf>,
    pub current_image: Option<DynamicImage>,
    pub current_channel: ColorChannel,
    pub settings_enabled: bool,
    pub image_info: Option<ExtendedImageInfo>,
    pub tiling: usize,
    pub mouse_grab: bool,
    pub key_grab: bool,
    pub edit_state: EditState,
    pub pointer_over_ui: bool,
    /// Things that perisist between launches
    pub persistent_settings: PersistentSettings,
    pub always_on_top: bool,
    pub network_mode: bool,
    /// how long the toast message appears
    pub toast_cooldown: f32,
    /// data to transform image once fullscreen is entered/left
    pub fullscreen_offset: Option<(i32, i32)>,
    /// List of images to cycle through. Usually the current dir or dropped files
    pub scrubber: Scrubber,
    pub checker_texture: Option<Texture>,
}

impl OculanteState {
    pub fn send_message(&self, msg: &str) {
        _ = self.message_channel.0.send(msg.into());
    }
}

impl Default for OculanteState {
    fn default() -> OculanteState {
        let tx_channel = mpsc::channel();
        OculanteState {
            image_geometry: ImageGeometry {
                scale: 1.0,
                offset: Default::default(),
            },
            compare_list: Default::default(),
            drag_enabled: Default::default(),
            reset_image: Default::default(),
            message: Default::default(),
            is_loaded: Default::default(),
            cursor: Default::default(),
            cursor_relative: Default::default(),
            image_dimension: (0, 0),
            sampled_color: [0., 0., 0., 0.],
            player: Player::new(tx_channel.0.clone(), 20, 16384),
            texture_channel: tx_channel,
            message_channel: mpsc::channel(),
            extended_info_channel: mpsc::channel(),
            extended_info_loading: Default::default(),
            mouse_delta: Default::default(),
            current_texture: Default::default(),
            current_image: Default::default(),
            current_path: Default::default(),
            current_channel: ColorChannel::Rgba,
            settings_enabled: Default::default(),
            image_info: Default::default(),
            tiling: 1,
            mouse_grab: Default::default(),
            key_grab: Default::default(),
            edit_state: Default::default(),
            pointer_over_ui: Default::default(),
            persistent_settings: Default::default(),
            always_on_top: Default::default(),
            network_mode: Default::default(),
            window_size: Default::default(),
            toast_cooldown: Default::default(),
            fullscreen_offset: Default::default(),
            scrubber: Default::default(),
            checker_texture: Default::default(),
        }
    }
}
