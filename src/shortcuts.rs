use log::{debug, error, info, trace, warn};
use std::collections::{BTreeMap, BTreeSet};

use crate::appstate::OculanteState;
use notan::prelude::{App, KeyCode};
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
            .cloned()
            .collect()
    }
    fn alphanumeric(&self) -> SimultaneousKeypresses {
        self.iter()
            .filter(|k| !is_key_modifier(k))
            .cloned()
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
            .add_keys(InputEvent::ClearImage, &["LShift", "Delete"])
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
        self.insert(function, keys.iter().map(|k| k.to_string()).collect());
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

    if !app.keyboard.down.is_empty() {
        trace!("Keyboard down: {:?}", app.keyboard.down);
    }

    // if nothing is down, just return
    if app.keyboard.down.is_empty() && app.keyboard.released.is_empty() {
        return false;
    }

    // early out if just one key is pressed, and it's a modifier
    if (app.keyboard.alt() || app.keyboard.shift() || app.keyboard.ctrl())
        && app.keyboard.down.len() == 1
    {
        trace!("alt/shift/ctrl modifier down");
        return false;
    }

    if let Some(keys) = state.persistent_settings.shortcuts.get(&command) {
        // make sure the appropriate number of keys are down
        if app.keyboard.down.len() != keys.len() && command != InputEvent::Fullscreen {
            return false;
        }

        // make sure all modifiers are down
        for m in keys.modifiers() {
            if m.contains("Shift") && !app.keyboard.shift() {
                return false;
            }
            if m.contains("Alt") && !app.keyboard.alt() {
                return false;
            }
            if m.contains("Control") && !app.keyboard.ctrl() {
                return false;
            }
            if m.contains("Win") && !app.keyboard.logo() {
                return false;
            }
        }

        // debug!("Down {:?}", app.keyboard.down);

        for key in keys.alphanumeric() {
            // Workaround macos fullscreen double press bug
            if command == InputEvent::Fullscreen {
                for pressed in &app.keyboard.released {
                    if key_matches(pressed, &key) {
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
                        if key_matches(dn, &key) {
                            debug!("REPEAT: Number of keys down: {}", app.keyboard.down.len());
                            debug!("Matched {:?} / {:?}", command, key);
                            return true;
                        }
                    }
                }

                for pressed in &app.keyboard.pressed {
                    // debug!("{:?}", pressed);
                    if key_matches(pressed, &key) {
                        debug!("Number of keys pressed: {}", app.keyboard.down.len());
                        debug!("Matched {:?} / {:?}", command, key);
                        return true;
                    }
                }
            }
        }
    } else {
        warn!("Command not registered: '{:?}'", command);
        // update missing shortcut
        if let Some(default_shortcut) = Shortcuts::default_keys().get(&command) {
            info!("Inserted command: {:?}", default_shortcut);
            state
                .persistent_settings
                .shortcuts
                .insert(command, default_shortcut.clone());
        } else {
            error!("Failed to insert command. Please report this as a bug.")
        }
    }
    false
}

/// The name of a key as it is stored in the shortcut settings.
///
/// Shortcuts are saved as key names. Notan 0.14 renamed its key codes (`F`
/// became `KeyF`, `LShift` became `ShiftLeft`), which made every saved and
/// default shortcut stop matching. The names from before that change are kept
/// here, so existing settings files continue to work.
pub fn key_name(key: &KeyCode) -> String {
    let name = format!("{key:?}");
    let legacy = match name.as_str() {
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ShiftLeft" => "LShift",
        "ShiftRight" => "RShift",
        "ControlLeft" => "LControl",
        "ControlRight" => "RControl",
        "AltLeft" => "LAlt",
        "AltRight" => "RAlt",
        "SuperLeft" => "LWin",
        "SuperRight" => "RWin",
        "Equal" => "Equals",
        "BracketLeft" => "LBracket",
        "BracketRight" => "RBracket",
        "Enter" => "Return",
        "Backspace" => "Back",
        "Backquote" => "Grave",
        "Quote" => "Apostrophe",
        "CapsLock" => "Capital",
        "ContextMenu" => "Apps",
        "PrintScreen" => "Snapshot",
        "ScrollLock" => "Scroll",
        "NumLock" => "Numlock",
        "NumpadAdd" => "Add",
        "NumpadSubtract" => "Subtract",
        "NumpadMultiply" => "Multiply",
        "NumpadDivide" => "Divide",
        "NumpadDecimal" => "Decimal",
        "NumpadEqual" => "NumpadEquals",
        _ => {
            // KeyA..KeyZ were A..Z, Digit0..Digit9 were Key0..Key9
            if let Some(letter) = name.strip_prefix("Key") {
                if letter.len() == 1 {
                    return letter.to_string();
                }
            }
            if let Some(digit) = name.strip_prefix("Digit") {
                return format!("Key{digit}");
            }
            return name;
        }
    };
    legacy.to_string()
}

/// Whether `key` is the key stored as `name` in a shortcut. Names written by
/// the 0.9.3 and 0.9.4 pre-releases use notan's new naming and match as well.
fn key_matches(key: &KeyCode, name: &str) -> bool {
    key_name(key) == name || format!("{key:?}") == name
}

pub fn lookup(shortcuts: &Shortcuts, command: &InputEvent) -> String {
    if let Some(keys) = shortcuts.get(command) {
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
    matches!(
        key,
        "LShift" | "LControl" | "LAlt" | "RAlt" | "RControl" | "RShift" | "LWin" | "Rwin"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every key notan can report for the keys we use in shortcuts.
    fn keys() -> Vec<KeyCode> {
        use KeyCode::*;
        vec![
            KeyA,
            KeyB,
            KeyC,
            KeyD,
            KeyE,
            KeyF,
            KeyG,
            KeyH,
            KeyI,
            KeyJ,
            KeyK,
            KeyL,
            KeyM,
            KeyN,
            KeyO,
            KeyP,
            KeyQ,
            KeyR,
            KeyS,
            KeyT,
            KeyU,
            KeyV,
            KeyW,
            KeyX,
            KeyY,
            KeyZ,
            Digit0,
            Digit1,
            Digit2,
            Digit3,
            Digit4,
            Digit5,
            Digit6,
            Digit7,
            Digit8,
            Digit9,
            ArrowLeft,
            ArrowRight,
            ArrowUp,
            ArrowDown,
            ShiftLeft,
            ShiftRight,
            ControlLeft,
            ControlRight,
            AltLeft,
            AltRight,
            SuperLeft,
            SuperRight,
            Equal,
            Minus,
            BracketLeft,
            BracketRight,
            Delete,
            Home,
            End,
            PageUp,
            PageDown,
            Enter,
            Backspace,
            Space,
            Tab,
            Escape,
            F1,
            F11,
            F12,
        ]
    }

    #[test]
    fn key_names_match_the_names_used_before_notan_014() {
        let expected = [
            (KeyCode::KeyF, "F"),
            (KeyCode::KeyZ, "Z"),
            (KeyCode::Digit1, "Key1"),
            (KeyCode::Digit0, "Key0"),
            (KeyCode::ArrowLeft, "Left"),
            (KeyCode::ArrowDown, "Down"),
            (KeyCode::ShiftLeft, "LShift"),
            (KeyCode::ControlLeft, "LControl"),
            (KeyCode::ControlRight, "RControl"),
            (KeyCode::AltRight, "RAlt"),
            (KeyCode::SuperLeft, "LWin"),
            (KeyCode::Equal, "Equals"),
            (KeyCode::Minus, "Minus"),
            (KeyCode::BracketLeft, "LBracket"),
            (KeyCode::BracketRight, "RBracket"),
            (KeyCode::Delete, "Delete"),
            (KeyCode::Home, "Home"),
            (KeyCode::End, "End"),
            (KeyCode::Enter, "Return"),
            (KeyCode::Backspace, "Back"),
            (KeyCode::F11, "F11"),
        ];
        for (key, name) in expected {
            assert_eq!(key_name(&key), name);
        }
    }

    /// The regression in 0.9.3 and 0.9.4: no default shortcut could be triggered,
    /// because none of its key names was ever produced by a key press.
    #[test]
    fn every_default_shortcut_can_be_triggered() {
        let producible: BTreeSet<String> = keys().iter().map(key_name).collect();
        for (command, shortcut) in Shortcuts::default_keys() {
            for key in shortcut {
                // on mac, control is replaced by the command key
                assert!(
                    producible.contains(&key),
                    "{command:?} uses the key {key:?}, which no key press produces"
                );
            }
        }
    }

    #[test]
    fn default_modifiers_are_recognized() {
        for name in [
            "LShift", "RShift", "LControl", "RControl", "LAlt", "RAlt", "LWin",
        ] {
            assert!(is_key_modifier(name), "{name} should be a modifier");
        }
        assert!(!is_key_modifier("F"));
    }

    #[test]
    fn shortcuts_saved_by_the_prereleases_still_match() {
        assert!(key_matches(&KeyCode::KeyF, "F"));
        assert!(key_matches(&KeyCode::KeyF, "KeyF"));
        assert!(!key_matches(&KeyCode::KeyF, "G"));
    }
}
