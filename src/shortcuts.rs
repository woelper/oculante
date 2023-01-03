use std::collections::{HashMap, HashSet};
// use hashbrown::{HashMap, HashSet};
use log::{debug, error};
// use std::collections::HashMap;

use crate::utils::OculanteState;
use notan::prelude::App;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    AlwaysOnTop,
    Fullscreen,
    Browse,
    Copy,
    Paste,
    EditMode,
    InfoMode,
    NextImage,
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
    Quit,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
}

pub type Shortcuts = HashMap<InputEvent, SimultaneousKeypresses>;

pub type SimultaneousKeypresses = HashSet<String>;

pub trait ShortcutExt {
    fn default_keys() -> Self
    where
        Self: Sized,
    {
        unimplemented!()
    }

    #[allow(unused_variables)]
    fn add_key(self, function: InputEvent, key: &str) -> Self
    where
        Self: Sized,
    {
        unimplemented!()
    }

    #[allow(unused_variables)]
    fn add_keys(self, function: InputEvent, keys: &[&str]) -> Self
    where
        Self: Sized,
    {
        unimplemented!()
    }
}

pub trait KeyTrait {
    fn modifiers(&self) -> SimultaneousKeypresses {
        unimplemented!()
    }
    fn alphanumeric(&self) -> SimultaneousKeypresses {
        unimplemented!()
    }
}

impl KeyTrait for SimultaneousKeypresses {
    fn modifiers(&self) -> SimultaneousKeypresses {
        self.iter()
            .filter(|k| is_key_modifier(k))
            .map(|k| k.clone())
            .collect()
    }
    fn alphanumeric(&self) -> SimultaneousKeypresses {
        self.iter()
            .filter(|k| !is_key_modifier(k))
            .map(|k| k.clone())
            .collect()
    }
}

impl ShortcutExt for Shortcuts {
    fn default_keys() -> Self {
        Shortcuts::default()
            .add_key(InputEvent::AlwaysOnTop, "T")
            .add_key(InputEvent::Fullscreen, "F")
            .add_key(InputEvent::ResetView, "V")
            .add_key(InputEvent::Quit, "Q")
            .add_key(InputEvent::InfoMode, "I")
            .add_key(InputEvent::EditMode, "E")
            .add_key(InputEvent::RedChannel, "R")
            .add_key(InputEvent::GreenChannel, "G")
            .add_key(InputEvent::BlueChannel, "B")
            .add_key(InputEvent::AlphaChannel, "A")
            .add_key(InputEvent::RGBChannel, "U")
            .add_key(InputEvent::RGBAChannel, "C")
            .add_key(InputEvent::ZoomIn, "Equals")
            .add_key(InputEvent::PreviousImage, "Left")
            .add_key(InputEvent::NextImage, "Right")
            .add_key(InputEvent::ZoomOut, "Minus")
            .add_key(InputEvent::Browse, "F1") // FIXME: As Shortcuts is a HashMap, only the newer key-sequence will be registered
            .add_keys(InputEvent::Browse, &["LControl", "O"])
            .add_keys(InputEvent::PanRight, &["LShift", "Right"])
            .add_keys(InputEvent::PanLeft, &["LShift", "Left"])
            .add_keys(InputEvent::PanDown, &["LShift", "Down"])
            .add_keys(InputEvent::PanUp, &["LShift", "Up"])
            .add_keys(InputEvent::Paste, &["LControl", "V"])
            .add_keys(InputEvent::Copy, &["LControl", "C"])
    }
    fn add_key(mut self, function: InputEvent, key: &str) -> Self {
        self.insert(
            function,
            vec![key].into_iter().map(|k| k.to_string()).collect(),
        );
        self
    }
    fn add_keys(mut self, function: InputEvent, keys: &[&str]) -> Self
    where
        Self: Sized,
    {
        self.insert(function, keys.into_iter().map(|k| k.to_string()).collect());
        self
    }
}

pub fn key_pressed(app: &mut App, state: &mut OculanteState, command: InputEvent) -> bool {
    // let mut alternates: HashMap<String, String>;
    // alternates.insert("+", v)

    if state.key_grab {
        return false;
    }

    if app.keyboard.down.is_empty() && app.keyboard.released.is_empty() {
        return false;
    }

    if let Some(keys) = state.persistent_settings.shortcuts.get(&command) {
        // make sure the appropriate number of keys are down
        if app.keyboard.down.len() != keys.len() {
            if command != InputEvent::Fullscreen {
                return false;
            }
        }

        // make sure all modifiers are down
        for m in keys.modifiers() {
            if m.contains("Shift") {
                if !app.keyboard.shift() {
                    return false;
                }
            }
            if m.contains("Alt") {
                if !app.keyboard.alt() {
                    return false;
                }
            }
            if m.contains("Control") {
                if !app.keyboard.ctrl() {
                    return false;
                }
            }
        }

        // debug!("Down {:?}", app.keyboard.down);

        for key in keys.alphanumeric() {
            // Workaround macos fullscreen double press bug
            if command == InputEvent::Fullscreen {
                debug!("Fullscreen received");
                for pressed in &app.keyboard.released {
                    if format!("{:?}", pressed) == key {
                        debug!("Matched {:?} / {:?}", command, key);
                        return true;
                    }
                }
            } else {
                for pressed in &app.keyboard.pressed {
                    if format!("{:?}", pressed) == key {
                        debug!("Number of keys pressed: {}", app.keyboard.down.len());
                        debug!("Matched {:?} / {:?}", command, key);
                        return true;
                    }
                }
            }
        }
    } else {
        error!("Command not registered: '{:?}'. Inserting new.", command);
        // update missing shortcut
        if let Some(default_shortcut) = Shortcuts::default_keys().get(&command) {
            state
                .persistent_settings
                .shortcuts
                .insert(command, default_shortcut.clone());
            _ = state.persistent_settings.save();
        }
    }
    false
}

pub fn lookup(shortcuts: &Shortcuts, command: &InputEvent) -> String {
    if let Some(keys) = shortcuts.get(&command) {
        return keypresses_as_string(keys);
    }
    "None".into()
}

pub fn keypresses_as_string(keys: &SimultaneousKeypresses) -> String {
    let mut modifiers = keys.modifiers().into_iter().collect::<Vec<_>>();
    let mut alpha = keys.alphanumeric().into_iter().collect::<Vec<_>>();
    modifiers.sort();
    alpha.sort();
    modifiers.extend(alpha);
    modifiers.join(" + ")
}

fn is_key_modifier(key: &str) -> bool {
    match key {
        "LShift" | "LControl" | "LAlt" | "RAlt" | "RControl" | "RShift" => true,
        _ => false,
    }
}
