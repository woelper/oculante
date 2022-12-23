use hashbrown::HashSet;
use log::debug;
use std::collections::HashMap;

use notan::{prelude::KeyCode, Event};

#[derive(Debug, PartialEq, Eq, Hash)]
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
    RBGChannel,
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

pub trait ShortcutExt {
    fn default_keys() -> Self
    where
        Self: Sized,
    {
        unimplemented!()
    }
    fn add_key(mut self, _function: InputEvent, _key: KeyCode) -> Self
    where
        Self: Sized,
    {
        unimplemented!()
    }
    fn add_keys(mut self, _function: InputEvent, _keys: &[KeyCode]) -> Self
    where
        Self: Sized,
    {
        unimplemented!()
    }
    fn was_pressed(&self, _event_name: &InputEvent, _down_keys: &SimultaneousKeypresses) -> bool {
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

pub type SimultaneousKeypresses = HashSet<KeyCode>;

pub type Shortcuts = HashMap<InputEvent, SimultaneousKeypresses>;

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
        Shortcuts::default().add_key(InputEvent::AlwaysOnTop, KeyCode::T)
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
    fn was_pressed(&self, event_name: &InputEvent, down_keys: &SimultaneousKeypresses) -> bool {
        if down_keys.is_empty() {
            return false;
        }
        debug!("Got keypress: {:?}", down_keys);
        self.get(event_name).map(|keys| keys == down_keys) == Some(true)
    }
}
