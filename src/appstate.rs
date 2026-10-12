use crate::{
    comparelist::CompareList,
    filebrowser::BrowserDir,
    image_editing::EditState,
    scrubber::Scrubber,
    sequence::Sequence,
    settings::{PersistentSettings, VolatileSettings},
    thumbnails::Thumbnails,
    toasts::{Anchor, Toasts},
    utils::{ExtendedImageInfo, Frame, Player},
};

use image::DynamicImage;
use nalgebra::Vector2;
use std::{
    path::{Path, PathBuf},
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
pub struct OculanteState {
    pub image_geometry: ImageGeometry,
    pub compare_list: CompareList,
    /// The images of a file that holds several. Only meant for the file whose
    /// path it has, use `sequence()`.
    pub sequence: Option<Sequence>,
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
    pub extended_info_channel: crate::utils::ExtendedInfoChannel,
    /// Changes whenever the pixels of the current image change: a new image,
    /// or edits applied to it. The info panel computes its numbers once for
    /// every version.
    pub image_version: u64,
    /// The Player, responsible for loading and sending Frames
    pub player: Player,
    pub current_path: Option<PathBuf>,
    /// Shared with the cache and the threads that look at the image
    pub current_image: Option<std::sync::Arc<DynamicImage>>,
    pub settings_enabled: bool,
    pub image_metadata: Option<ExtendedImageInfo>,
    pub tiling: usize,
    pub mouse_grab: bool,
    pub key_grab: bool,
    pub edit_state: EditState,
    pub pointer_over_ui: bool,
    /// Things that persist between launches
    pub persistent_settings: PersistentSettings,
    pub volatile_settings: VolatileSettings,
    pub always_on_top: bool,
    pub network_mode: bool,
    /// File names piped in at startup, they arrive from a background thread
    pub piped_paths: Option<std::sync::mpsc::Receiver<Vec<std::path::PathBuf>>>,
    /// Where the window's content was on the screen while fullscreen is switched
    /// on or off, and when it last moved, see `follow_fullscreen`
    pub fullscreen_origin: Option<(egui::Pos2, std::time::Instant)>,
    /// List of images to cycle through. Usually the current dir or dropped files
    pub scrubber: Scrubber,
    pub toasts: Toasts,
    pub filebrowser_id: Option<String>,
    pub filebrowser_last_dir: BrowserDir,
    pub thumbnails: Thumbnails,
    pub new_image_loaded: bool,
}

impl OculanteState {
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

    /// Evaluate the base path for the file browser.
    pub fn filebrowser_path(&self) -> PathBuf {
        match self.filebrowser_last_dir {
            BrowserDir::LastOpenDir => self.volatile_settings.last_open_directory.to_owned(),
            BrowserDir::CurrentImageDir => self
                .current_path
                .as_deref()
                .and_then(|path| path.parent())
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.volatile_settings.last_open_directory.clone()),
        }
    }

    /// Start loading an image, it is shown when it arrives
    pub fn load(&mut self, path: &Path) {
        self.is_loaded = false;
        self.current_path = Some(path.to_owned());
        if let Some(sequence) = &mut self.sequence
            && sequence.path == path
        {
            sequence.current = 0;
        }
        self.player.load(path);
    }

    /// Read the current image from its file again, for example after it was changed there
    pub fn reload(&mut self) {
        if let Some(path) = self.current_path.clone() {
            self.is_loaded = false;
            self.player.cache.clear();
            self.player.load_item(&path, self.current_item());
        }
    }

    /// The images of the file shown, when it holds more than one
    pub fn sequence(&self) -> Option<&Sequence> {
        self.sequence
            .as_ref()
            .filter(|s| s.count > 1 && self.current_path.as_deref() == Some(&s.path))
    }

    /// Which image of the file is shown, 0 for a file that holds one
    pub fn current_item(&self) -> usize {
        self.sequence().map_or(0, |s| s.current)
    }

    /// Show another image of the file
    pub fn load_item(&mut self, item: usize) {
        if let Some(path) = self.current_path.clone() {
            self.is_loaded = false;
            self.set_item(&path, item, None);
            self.player.load_item(&path, item);
        }
    }

    /// Remember which image of a file is shown, and how many it holds when that
    /// is known
    pub fn set_item(&mut self, path: &Path, item: usize, count: Option<usize>) {
        match &mut self.sequence {
            Some(sequence) if sequence.path == path => {
                sequence.current = item;
                sequence.count = count.unwrap_or(sequence.count);
            }
            _ => self.sequence = Some(Sequence::new(path, item, count.unwrap_or(item + 1))),
        }
    }
}

impl Default for OculanteState {
    fn default() -> OculanteState {
        let persistent_settings = PersistentSettings::load().unwrap_or_default();

        let mut volatile_settings = VolatileSettings::load().unwrap_or_default();
        volatile_settings.remove_missing_recents();

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
            sequence: None,
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
            image_version: 0,
            mouse_delta: Default::default(),
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
            volatile_settings,
            always_on_top: Default::default(),
            network_mode: Default::default(),
            piped_paths: None,
            window_size: Default::default(),
            fullscreen_origin: Default::default(),
            scrubber: Default::default(),
            toasts: Toasts::default().with_anchor(Anchor::BottomLeft),
            filebrowser_id: None,
            filebrowser_last_dir: Default::default(),
            thumbnails: Default::default(),
            new_image_loaded: false,
        }
    }
}
