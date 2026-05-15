use log::{debug, error, info, warn};
use std::collections::{BTreeMap, BTreeSet};

use crate::appstate::OculanteState;
use egui::{Key, Modifiers};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize, PartialOrd, Ord)]
pub enum InputEvent {
    AlwaysOnTop,
    Fullscreen,
    InfoMode,
    EditMode,
    NextImage,
    FirstImage,
    LastImage,
    PreviousImage,
    RedChannel,
    GreenChannel,
    BlueChannel,
    AlphaChannel,
    RGBChannel,
    RGBAChannel,
    ResetView,
    ZoomOut,
    ZoomIn,
    ZoomActualSize,
    ZoomDouble,
    ZoomThree,
    ZoomFour,
    ZoomFive,
    CompareNext,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    DeleteFile,
    ClearImage,
    LosslessRotateRight,
    LosslessRotateLeft,
    Copy,
    Paste,
    Browse,
    Quit,
    ZenMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Shortcut {
    pub keys: BTreeSet<Key>,
    pub modifiers: Modifiers,
}

impl Shortcut {
    fn key(k: Key) -> Self {
        Self {
            keys: BTreeSet::from([k]),
            modifiers: Modifiers::NONE,
        }
    }

    fn ctrl_key(k: Key) -> Self {
        #[cfg(not(target_os = "macos"))]
        let modifiers = Modifiers::CTRL;
        #[cfg(target_os = "macos")]
        let modifiers = Modifiers::MAC_CMD;
        Self {
            keys: BTreeSet::from([k]),
            modifiers,
        }
    }

    fn shift_key(k: Key) -> Self {
        Self {
            keys: BTreeSet::from([k]),
            modifiers: Modifiers::SHIFT,
        }
    }

    /// Human-readable display string
    pub fn to_string_pretty(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.modifiers.mac_cmd || self.modifiers.command {
            parts.push("Cmd".to_string());
        }
        if self.modifiers.alt {
            parts.push("Alt".to_string());
        }
        if self.modifiers.shift {
            parts.push("Shift".to_string());
        }
        for k in &self.keys {
            parts.push(format!("{k:?}"));
        }
        parts.join(" + ")
    }

    /// Markdown version for documentation
    pub fn to_markdown(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.ctrl {
            parts.push("<kbd>Ctrl</kbd>".to_string());
        }
        if self.modifiers.mac_cmd || self.modifiers.command {
            parts.push("<kbd>Cmd</kbd>".to_string());
        }
        if self.modifiers.alt {
            parts.push("<kbd>Alt</kbd>".to_string());
        }
        if self.modifiers.shift {
            parts.push("<kbd>Shift</kbd>".to_string());
        }
        for k in &self.keys {
            parts.push(format!("<kbd>{k:?}</kbd>"));
        }
        parts.join(" + ")
    }
}

pub type Shortcuts = BTreeMap<InputEvent, Shortcut>;

pub fn default_shortcuts() -> Shortcuts {
    use Key::*;
    let mut s = Shortcuts::new();
    s.insert(InputEvent::AlwaysOnTop, Shortcut::key(T));
    s.insert(InputEvent::Fullscreen, Shortcut::key(F));
    s.insert(InputEvent::ResetView, Shortcut::key(V));
    s.insert(InputEvent::Quit, Shortcut::key(Q));
    s.insert(InputEvent::InfoMode, Shortcut::key(I));
    s.insert(InputEvent::EditMode, Shortcut::key(E));
    s.insert(InputEvent::RedChannel, Shortcut::key(R));
    s.insert(InputEvent::GreenChannel, Shortcut::key(G));
    s.insert(InputEvent::BlueChannel, Shortcut::key(B));
    s.insert(InputEvent::AlphaChannel, Shortcut::key(A));
    s.insert(InputEvent::RGBChannel, Shortcut::key(U));
    s.insert(InputEvent::RGBAChannel, Shortcut::key(C));
    s.insert(InputEvent::CompareNext, Shortcut::shift_key(C));
    s.insert(InputEvent::PreviousImage, Shortcut::key(ArrowLeft));
    s.insert(InputEvent::FirstImage, Shortcut::key(Home));
    s.insert(InputEvent::LastImage, Shortcut::key(End));
    s.insert(InputEvent::NextImage, Shortcut::key(ArrowRight));
    s.insert(InputEvent::ZoomIn, Shortcut::key(Equals));
    s.insert(InputEvent::ZoomOut, Shortcut::key(Minus));
    s.insert(InputEvent::ZoomActualSize, Shortcut::key(Num1));
    s.insert(InputEvent::ZoomDouble, Shortcut::key(Num2));
    s.insert(InputEvent::ZoomThree, Shortcut::key(Num3));
    s.insert(InputEvent::ZoomFour, Shortcut::key(Num4));
    s.insert(InputEvent::ZoomFive, Shortcut::key(Num5));
    s.insert(InputEvent::LosslessRotateLeft, Shortcut::key(OpenBracket));
    s.insert(
        InputEvent::LosslessRotateRight,
        Shortcut::key(CloseBracket),
    );
    s.insert(InputEvent::ZenMode, Shortcut::key(Z));
    s.insert(InputEvent::DeleteFile, Shortcut::key(Delete));
    s.insert(InputEvent::ClearImage, Shortcut::shift_key(Delete));
    s.insert(InputEvent::Browse, Shortcut::ctrl_key(O));
    s.insert(InputEvent::PanRight, Shortcut::shift_key(ArrowRight));
    s.insert(InputEvent::PanLeft, Shortcut::shift_key(ArrowLeft));
    s.insert(InputEvent::PanDown, Shortcut::shift_key(ArrowDown));
    s.insert(InputEvent::PanUp, Shortcut::shift_key(ArrowUp));
    s.insert(InputEvent::Paste, Shortcut::ctrl_key(V));
    s.insert(InputEvent::Copy, Shortcut::ctrl_key(C));
    s
}

/// Check if a shortcut's command is currently triggered, reading from egui input.
pub fn key_pressed(ctx: &egui::Context, state: &mut OculanteState, command: InputEvent) -> bool {
    if state.key_grab {
        return false;
    }

    let shortcut = match state.persistent_settings.shortcuts.get(&command) {
        Some(s) => s,
        None => {
            warn!("Command not registered: '{:?}'", command);
            if let Some(default) = default_shortcuts().get(&command) {
                info!("Inserted default shortcut for: {:?}", command);
                state
                    .persistent_settings
                    .shortcuts
                    .insert(command, default.clone());
            } else {
                error!(
                    "No default shortcut for {:?}. Please report this as a bug.",
                    command
                );
            }
            return false;
        }
    };

    let shortcut = shortcut.clone();

    ctx.input(|input| {
        let is_release = command == InputEvent::Fullscreen;

        for key in &shortcut.keys {
            let matched = input.events.iter().any(|event| {
                if let egui::Event::Key {
                    key: k,
                    pressed,
                    modifiers,
                    ..
                } = event
                {
                    *k == *key
                        && *pressed != is_release
                        && modifiers.shift == shortcut.modifiers.shift
                        && modifiers.alt == shortcut.modifiers.alt
                        && modifiers.ctrl == shortcut.modifiers.ctrl
                        && (modifiers.mac_cmd || modifiers.command)
                            == (shortcut.modifiers.mac_cmd || shortcut.modifiers.command)
                } else {
                    false
                }
            });
            if !matched {
                return false;
            }
        }

        debug!("Matched shortcut {:?}", command);
        true
    })
}

pub fn lookup(shortcuts: &Shortcuts, command: &InputEvent) -> String {
    if let Some(shortcut) = shortcuts.get(command) {
        return shortcut.to_string_pretty();
    }
    "None".into()
}

pub fn keypresses_as_string(shortcut: &Shortcut) -> String {
    shortcut.to_string_pretty()
}

pub fn keypresses_as_markdown(shortcut: &Shortcut) -> String {
    shortcut.to_markdown()
}
