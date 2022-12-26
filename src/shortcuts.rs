use std::collections::{HashMap, HashSet};

// use hashbrown::{HashMap, HashSet};
use log::{debug, error, info};
// use std::collections::HashMap;

use crate::utils::OculanteState;
use notan::prelude::{App, KeyCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum InputEvent {
    AlwaysOnTop,
    Fullscreen,
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



pub type SimultaneousKeypresses = HashSet<KeyCode>;

pub trait ShortcutExt {
    fn default_keys() -> Self
    where
        Self: Sized,
    {
        unimplemented!()
    }

    #[allow(unused_variables)]
    fn add_key(mut self, function: InputEvent, key: KeyCode) -> Self
    where
        Self: Sized,
    {
        unimplemented!()
    }

    #[allow(unused_variables)]
    fn add_keys(mut self, function: InputEvent, keys: &[KeyCode]) -> Self
    where
        Self: Sized,
    {
        unimplemented!()
    }

    #[allow(unused_variables)]
    fn all_down(&self, event_name: &InputEvent, down_keys: &HashMap<KeyCode, f32>) -> bool {
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
            .add_key(InputEvent::AlwaysOnTop, KeyCode::T)
            .add_key(InputEvent::Fullscreen, KeyCode::F)
            .add_key(InputEvent::ResetView, KeyCode::V)
            .add_key(InputEvent::Quit, KeyCode::Q)
            .add_key(InputEvent::InfoMode, KeyCode::I)
            .add_key(InputEvent::EditMode, KeyCode::E)
            .add_key(InputEvent::RedChannel, KeyCode::R)
            .add_key(InputEvent::GreenChannel, KeyCode::G)
            .add_key(InputEvent::BlueChannel, KeyCode::B)
            .add_key(InputEvent::AlphaChannel, KeyCode::A)
            .add_key(InputEvent::RGBChannel, KeyCode::U)
            .add_key(InputEvent::RGBAChannel, KeyCode::C)
            .add_keys(InputEvent::PanRight, &[KeyCode::LShift, KeyCode::Right])
            .add_keys(InputEvent::PanLeft, &[KeyCode::LShift, KeyCode::Left])
            .add_keys(InputEvent::PanDown, &[KeyCode::LShift, KeyCode::Down])
            .add_keys(InputEvent::PanUp, &[KeyCode::LShift, KeyCode::Up])
            .add_keys(InputEvent::Paste, &[KeyCode::LControl, KeyCode::V])
            .add_keys(InputEvent::Copy, &[KeyCode::LControl, KeyCode::C])
    }
    fn add_key(mut self, function: InputEvent, key: KeyCode) -> Self {
        self.insert(function, vec![key].into_iter().collect());
        self
    }
    fn add_keys(mut self, function: InputEvent, keys: &[KeyCode]) -> Self
    where
        Self: Sized,
    {
        self.insert(function, keys.into_iter().map(|k| k.clone()).collect());
        self
    }
    fn all_down(&self, event_name: &InputEvent, down_keys: &HashMap<KeyCode, f32>) -> bool {
        if down_keys.is_empty() {
            return false;
        }
        info!("dn {:?}", down_keys);

        if !down_keys.values().any(|delta| *delta < 0.0001) {
            info!("No key pressed was in the last 10ms");
            return false;
        }
        let down_keys: HashSet<KeyCode> = down_keys.keys().map(|k| k.clone()).collect();
        // debug!("Got keypress: {:?}", down_keys);
        self.get(event_name)
            // .map(|keys| keys.)
            .map(|keys| keys == &down_keys)
            == Some(true)
    }
}

pub fn key_pressed(app: &mut App, state: &mut OculanteState, command: InputEvent) -> bool {
    if state.key_grab {
        return false;
    }

    if let Some(keys) = state.shortcuts.get(&command) {
        // make sure all modifiers are down
        for m in keys.modifiers() {
            if !app.keyboard.is_down(m) {
                return false;
            }
        }
        for key in keys.alphanumeric() {
            debug!("Received {:?} / {:?}", command, keys);
            // Workaround macos fullscreen double press bug
            if command == InputEvent::Fullscreen {
                if app.keyboard.was_released(key) {
                    return true;
                }
            } else {
                if app.keyboard.was_pressed(key) {
                    return true;
                }
            }
        }
    } else {
        error!("Command not registered! {:?}", command)
    }
    false
}

pub fn lookup_as_string(
    shortcuts: &HashMap<InputEvent, HashSet<KeyCode>>,
    command: &InputEvent,
) -> String {
    if let Some(keys) = shortcuts.get(&command) {
        return keys
            .iter()
            .map(|k| format!("{:?}", k))
            .collect::<Vec<_>>()
            .join(" ");
    }
    "None".into()
}

fn is_key_modifier(key: &KeyCode) -> bool {
    match key {
        KeyCode::LShift
        | KeyCode::LControl
        | KeyCode::LAlt
        | KeyCode::RAlt
        | KeyCode::RControl
        | KeyCode::RShift => true,
        _ => false,
    }
}


#[derive(Serialize, Deserialize)]
#[serde(remote = "KeyCode")]
pub enum KeyCodeSer {
       /// The '1' key over the letters.
       Key1,
       /// The '2' key over the letters.
       Key2,
       /// The '3' key over the letters.
       Key3,
       /// The '4' key over the letters.
       Key4,
       /// The '5' key over the letters.
       Key5,
       /// The '6' key over the letters.
       Key6,
       /// The '7' key over the letters.
       Key7,
       /// The '8' key over the letters.
       Key8,
       /// The '9' key over the letters.
       Key9,
       /// The '0' key over the 'O' and 'P' keys.
       Key0,
   
       A,
       B,
       C,
       D,
       E,
       F,
       G,
       H,
       I,
       J,
       K,
       L,
       M,
       N,
       O,
       P,
       Q,
       R,
       S,
       T,
       U,
       V,
       W,
       X,
       Y,
       Z,
   
       /// The Escape key, next to F1.
       Escape,
   
       F1,
       F2,
       F3,
       F4,
       F5,
       F6,
       F7,
       F8,
       F9,
       F10,
       F11,
       F12,
       F13,
       F14,
       F15,
       F16,
       F17,
       F18,
       F19,
       F20,
       F21,
       F22,
       F23,
       F24,
   
       /// Print Screen/SysRq.
       Snapshot,
       /// Scroll Lock.
       Scroll,
       /// Pause/Break key, next to Scroll lock.
       Pause,
   
       /// `Insert`, next to Backspace.
       Insert,
       Home,
       Delete,
       End,
       PageDown,
       PageUp,
   
       Left,
       Up,
       Right,
       Down,
   
       /// The Backspace key, right over Enter.
       Back,
       /// The Enter key.
       Return,
       /// The space bar.
       Space,
   
       /// The "Compose" key on Linux.
       Compose,
   
       Caret,
   
       Numlock,
       Numpad0,
       Numpad1,
       Numpad2,
       Numpad3,
       Numpad4,
       Numpad5,
       Numpad6,
       Numpad7,
       Numpad8,
       Numpad9,
       Add,
       Divide,
       Decimal,
       NumpadComma,
       NumpadEnter,
       NumpadEquals,
       Multiply,
       Subtract,
   
       AbntC1,
       AbntC2,
       Apostrophe,
       Apps,
       Asterisk,
       At,
       Ax,
       Backslash,
       Calculator,
       Capital,
       Colon,
       Comma,
       Convert,
       Equals,
       Grave,
       Kana,
       Kanji,
       LAlt,
       LBracket,
       LControl,
       LShift,
       LWin,
       Mail,
       MediaSelect,
       MediaStop,
       Minus,
       Mute,
       MyComputer,
       // also called "Next"
       NavigateForward,
       // also called "Prior"
       NavigateBackward,
       NextTrack,
       NoConvert,
       OEM102,
       Period,
       PlayPause,
       Plus,
       Power,
       PrevTrack,
       RAlt,
       RBracket,
       RControl,
       RShift,
       RWin,
       Semicolon,
       Slash,
       Sleep,
       Stop,
       Sysrq,
       Tab,
       Underline,
       Unlabeled,
       VolumeDown,
       VolumeUp,
       Wake,
       WebBack,
       WebFavorites,
       WebForward,
       WebHome,
       WebRefresh,
       WebSearch,
       WebStop,
       Yen,
       Copy,
       Paste,
       Cut,
   
       Unknown,
}