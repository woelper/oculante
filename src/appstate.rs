use crate::{
    comparelist::CompareList,
    image_editing::EditState,
    scrubber::Scrubber,
    settings::{PersistentSettings, VolatileSettings},
    texture_wrapper::TextureWrapperManager,
    thumbnails::Thumbnails,
    utils::{ExtendedImageInfo, Frame, Player},
};

use egui_notify::Toasts;
use image::DynamicImage;
use nalgebra::Vector2;
use notan::{prelude::Texture, AppState};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageGeometry {
    /// The scale of the displayed image
    pub scale: f32,
    /// Image offset on canvas
    pub offset: Vector2<f32>,
    pub dimensions: (u32, u32),
}

#[derive(Debug, Clone)]
pub enum Message {
    Info(String),
    Warning(String),
    Error(String),
    LoadError(String),
    Saved(PathBuf),
}

impl Message {
    pub fn info(m: &str) -> Self {
        Self::Info(m.into())
    }
    pub fn warn(m: &str) -> Self {
        Self::Warning(m.into())
    }
    pub fn err(m: &str) -> Self {
        Self::Error(m.into())
    }
}

/// The state of the application
#[derive(AppState)]
pub struct OculanteState {
    pub image_geometry: ImageGeometry,
    pub compare_list: CompareList,
    pub drag_enabled: bool,
    pub reset_image: bool,
    /// Is the image fully loaded?
    pub is_loaded: bool,
    pub window_size: Vector2<f32>,
    pub cursor: Vector2<f32>,
    pub cursor_relative: Vector2<f32>,
    pub sampled_color: [f32; 4],
    pub mouse_delta: Vector2<f32>,
    pub texture_channel: (Sender<Frame>, Receiver<Frame>),
    pub message_channel: (Sender<Message>, Receiver<Message>),
    /// Channel to load images from
    pub load_channel: (Sender<PathBuf>, Receiver<PathBuf>),
    pub extended_info_channel: (Sender<ExtendedImageInfo>, Receiver<ExtendedImageInfo>),
    /// The Player, responsible for loading and sending Frames
    pub player: Player,
    //pub current_texture: Option<TexWrap>,
    pub current_texture: TextureWrapperManager,
    pub current_path: Option<PathBuf>,
    pub current_image: Option<DynamicImage>,
    pub settings_enabled: bool,
    pub image_metadata: Option<ExtendedImageInfo>,
    pub tiling: usize,
    pub mouse_grab: bool,
    pub key_grab: bool,
    pub edit_state: EditState,
    pub pointer_over_ui: bool,
    /// Things that perisist between launches
    pub persistent_settings: PersistentSettings,
    pub volatile_settings: VolatileSettings,
    pub always_on_top: bool,
    pub network_mode: bool,
    /// how long the toast message appears
    /// data to transform image once fullscreen is entered/left
    pub fullscreen_offset: Option<(i32, i32)>,
    /// List of images to cycle through. Usually the current dir or dropped files
    pub scrubber: Scrubber,
    pub checker_texture: Option<Texture>,
    pub redraw: bool,
    pub first_start: bool,
    pub toasts: Toasts,
    pub filebrowser_id: Option<String>,
    pub thumbnails: Thumbnails,
    pub fitted_window: bool,
}

impl<'b> OculanteState {
    pub fn send_message_info(&self, msg: &str) {
        _ = self.message_channel.0.send(Message::info(msg));
    }

    pub fn send_message_err(&self, msg: &str) {
        _ = self.message_channel.0.send(Message::err(msg));
    }

    pub fn send_message_warn(&self, msg: &str) {
        _ = self.message_channel.0.send(Message::warn(msg));
    }

    pub fn send_frame(&self, frame: Frame) {
        let _ = self.texture_channel.0.send(frame);
    }
}

impl<'b> Default for OculanteState {
    fn default() -> OculanteState {
        let persistent_settings = PersistentSettings::load().unwrap_or_default();

        let tx_channel = mpsc::channel();
        let msg_channel = mpsc::channel();
        let meta_channel = mpsc::channel();
        OculanteState {
            image_geometry: ImageGeometry {
                scale: 1.0,
                offset: Default::default(),
                dimensions: Default::default(),
            },
            compare_list: Default::default(),
            drag_enabled: Default::default(),
            reset_image: Default::default(),
            is_loaded: Default::default(),
            cursor: Default::default(),
            cursor_relative: Default::default(),
            sampled_color: [0., 0., 0., 0.],
            player: Player::new(
                tx_channel.0.clone(),
                20,
                msg_channel.0.clone(),
                persistent_settings.decoders,
            ),
            texture_channel: tx_channel,
            message_channel: msg_channel,
            load_channel: mpsc::channel(),
            extended_info_channel: meta_channel,
            mouse_delta: Default::default(),
            current_texture: Default::default(),
            current_image: Default::default(),
            current_path: Default::default(),
            settings_enabled: Default::default(),
            image_metadata: Default::default(),
            tiling: 1,
            mouse_grab: Default::default(),
            key_grab: Default::default(),
            edit_state: Default::default(),
            pointer_over_ui: Default::default(),
            persistent_settings: PersistentSettings::load().unwrap_or_default(),
            volatile_settings: VolatileSettings::load().unwrap_or_default(),
            always_on_top: Default::default(),
            network_mode: Default::default(),
            window_size: Default::default(),
            fullscreen_offset: Default::default(),
            scrubber: Default::default(),
            checker_texture: Default::default(),
            redraw: Default::default(),
            first_start: true,
            toasts: Toasts::default().with_anchor(egui_notify::Anchor::BottomLeft),
            filebrowser_id: None,
            thumbnails: Default::default(),
            fitted_window: false,
        }
    }
}
