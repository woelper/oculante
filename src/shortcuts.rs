use std::collections::{BTreeMap, BTreeSet};
// use hashbrown::{HashMap, HashSet};
use log::{debug, error};
// use std::collections::HashMap;

use crate::{next_image, prev_image, zoom_image, OculanteState};
use notan::{input::keyboard::Keyboard, prelude::App};
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
    LosslessRotateRight,
    LosslessRotateLeft,
    Copy,
    Paste,
    Browse,
    Quit,
    ZenMode,
}

pub type Shortcuts = BTreeMap<InputEvent, SimultaneousKeypresses>;

pub type SimultaneousKeypresses = BTreeSet<String>;

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
        #[allow(unused_mut)]
        let mut s = Shortcuts::default()
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
            .add_keys(InputEvent::CompareNext, &["LShift", "C"])
            .add_key(InputEvent::PreviousImage, "Left")
            .add_key(InputEvent::FirstImage, "Home")
            .add_key(InputEvent::LastImage, "End")
            .add_key(InputEvent::NextImage, "Right")
            .add_key(InputEvent::ZoomIn, "Equals")
            .add_key(InputEvent::ZoomOut, "Minus")
            .add_key(InputEvent::ZoomActualSize, "Key1")
            .add_key(InputEvent::ZoomDouble, "Key2")
            .add_key(InputEvent::ZoomThree, "Key3")
            .add_key(InputEvent::ZoomFour, "Key4")
            .add_key(InputEvent::ZoomFive, "Key5")
            .add_key(InputEvent::LosslessRotateLeft, "LBracket")
            .add_key(InputEvent::LosslessRotateRight, "RBracket")
            .add_key(InputEvent::ZenMode, "Z")
            .add_key(InputEvent::DeleteFile, "Delete")
            // .add_key(InputEvent::Browse, "F1") // FIXME: As Shortcuts is a HashMap, only the newer key-sequence will be registered
            .add_keys(InputEvent::Browse, &["LControl", "O"])
            .add_keys(InputEvent::PanRight, &["LShift", "Right"])
            .add_keys(InputEvent::PanLeft, &["LShift", "Left"])
            .add_keys(InputEvent::PanDown, &["LShift", "Down"])
            .add_keys(InputEvent::PanUp, &["LShift", "Up"])
            .add_keys(InputEvent::Paste, &["LControl", "V"])
            .add_keys(InputEvent::Copy, &["LControl", "C"]);
        #[cfg(target_os = "macos")]
        {
            for (_, keys) in s.iter_mut() {
                *keys = keys.iter().map(|k| k.replace("LControl", "LWin")).collect();
            }
        }
        s
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

    // don't do anything if keyboard is grabbed (typing in textbox etc)
    if state.key_grab {
        return false;
    }

    // if nothing is down, just return
    if app.keyboard.down.is_empty() && app.keyboard.released.is_empty() {
        return false;
    }

    // early out if just one key is pressed, and it's a modifier
    if app.keyboard.alt() || app.keyboard.shift() || app.keyboard.ctrl() {
        if app.keyboard.down.len() == 1 {
            debug!("just modifier down");
            return false;
        }
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
            if m.contains("Win") {
                if !app.keyboard.logo() {
                    return false;
                }
            }
        }

        // debug!("Down {:?}", app.keyboard.down);

        for key in keys.alphanumeric() {
            // Workaround macos fullscreen double press bug
            if command == InputEvent::Fullscreen {
                for pressed in &app.keyboard.released {
                    if format!("{:?}", pressed) == key {
                        debug!("Fullscreen received");
                        debug!("Matched {:?} / {:?}", command, key);
                        return true;
                    }
                }
            } else {
                // List of "repeating" keys. Basically "early out" before checking if there were pressed keys
                if [
                    InputEvent::NextImage,
                    InputEvent::PreviousImage,
                    InputEvent::PanRight,
                    InputEvent::PanLeft,
                    InputEvent::PanDown,
                    InputEvent::PanUp,
                    InputEvent::ZoomIn,
                    InputEvent::ZoomOut,
                ]
                .contains(&command)
                {
                    for (dn, _) in &app.keyboard.down {
                        if format!("{:?}", dn) == key {
                            debug!("REPEAT: Number of keys down: {}", app.keyboard.down.len());
                            debug!("Matched {:?} / {:?}", command, key);
                            debug!("d {}", app.system_timer.delta_f32());
                            return true;
                        }
                    }
                }

                for pressed in &app.keyboard.pressed {
                    // debug!("{:?}", pressed);
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

pub fn keypresses_as_markdown(keys: &SimultaneousKeypresses) -> String {
    let mut modifiers = keys.modifiers().into_iter().collect::<Vec<_>>();
    let mut alpha = keys.alphanumeric().into_iter().collect::<Vec<_>>();
    modifiers.sort();
    alpha.sort();
    modifiers.extend(alpha);
    modifiers = modifiers
        .into_iter()
        .map(|k| format!("<kbd>{}</kbd>", k))
        .collect();
    modifiers.join(" + ")
}

fn is_key_modifier(key: &str) -> bool {
    match key {
        "LShift" | "LControl" | "LAlt" | "RAlt" | "RControl" | "RShift" | "LWin" | "Rwin" => true,
        _ => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseWheelDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MouseWheelEvent {
    pub direction: MouseWheelDirection,
    pub ctrl: bool,
    pub shift: bool,
}

impl Default for MouseWheelEvent {
    fn default() -> Self {
        Self {
            direction: MouseWheelDirection::Up,
            ctrl: false,
            shift: false,
        }
    }
}

impl MouseWheelEvent {
    pub fn new(delta_y: f32, keyboard: &Keyboard) -> Self {
        let direction = if delta_y > 0.0 {
            MouseWheelDirection::Up
        } else {
            MouseWheelDirection::Down
        };
        let ctrl = keyboard.ctrl();
        let shift = keyboard.shift();
        Self {
            direction,
            ctrl,
            shift,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize, PartialOrd, Ord)]
pub enum MouseWheelAction {
    ZoomOut,
    ZoomIn,
    NextImage,
    PrevImage,
    // TODO: implement other actions
    // NextChannel,
    // PrevChannel,
    // RotateRight,
    // RotateLeft,
}

impl MouseWheelAction {
    pub fn perform(&self, state: &mut OculanteState, delta_y: f32) {
        match self {
            MouseWheelAction::ZoomOut => zoom_image(state, delta_y),
            MouseWheelAction::ZoomIn => zoom_image(state, delta_y),
            MouseWheelAction::NextImage => next_image(state),
            MouseWheelAction::PrevImage => prev_image(state),
            // MouseWheelAction::NextChannel => todo!(),
            // MouseWheelAction::PrevChannel => todo!(),
            // MouseWheelAction::RotateRight => rotate_right(state),
            // MouseWheelAction::RotateLeft => rotate_left(state),
        }
    }
}

pub type MouseWheelSettings = Vec<(MouseWheelAction, Option<MouseWheelEvent>)>;

pub fn default_wheel_settings() -> MouseWheelSettings {
    MouseWheelSettings::from([
        (
            MouseWheelAction::ZoomOut,
            Some(MouseWheelEvent {
                direction: MouseWheelDirection::Down,
                ctrl: false,
                shift: false,
            }),
        ),
        (
            MouseWheelAction::ZoomIn,
            Some(MouseWheelEvent {
                direction: MouseWheelDirection::Up,
                ctrl: false,
                shift: false,
            }),
        ),
        (
            MouseWheelAction::NextImage,
            Some(MouseWheelEvent {
                direction: MouseWheelDirection::Down,
                ctrl: true,
                shift: false,
            }),
        ),
        (
            MouseWheelAction::PrevImage,
            Some(MouseWheelEvent {
                direction: MouseWheelDirection::Up,
                ctrl: true,
                shift: false,
            }),
        ),
        // (MouseWheelAction::NextChannel, None),
        // (MouseWheelAction::PrevChannel, None),
        // (
        //     MouseWheelAction::RotateRight,
        //     Some(MouseWheelEvent {
        //         direction: MouseWheelDirection::Down,
        //         ctrl: true,
        //         shift: true,
        //     }),
        // ),
        // (
        //     MouseWheelAction::RotateLeft,
        //     Some(MouseWheelEvent {
        //         direction: MouseWheelDirection::Up,
        //         ctrl: true,
        //         shift: true,
        //     }),
        // ),
    ])
}
