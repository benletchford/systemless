//! Guest keyboard translation and focus lifecycle shared by GPUI views.

use gpui_kit::{Keystroke, Modifiers};
use std::collections::HashMap;
use systemless::systems::macintosh::session::MacintoshInput;

#[derive(Default)]
pub(crate) struct KeyboardState {
    host_modifiers: Modifiers,
    caps_lock_on: bool,
    held_keys: HashMap<u8, u8>,
}

impl KeyboardState {
    pub(crate) fn sync_caps_lock(&mut self, on: bool) -> Vec<MacintoshInput> {
        if self.caps_lock_on == on {
            return Vec::new();
        }
        self.caps_lock_on = on;
        // Guest Caps Lock latches on key-down and survives physical release.
        vec![
            MacintoshInput::KeyDown { mac_key: 0x39, character: 0 },
            MacintoshInput::KeyUp { mac_key: 0x39, character: 0 },
        ]
    }

    pub(crate) fn sync_host_modifiers(&mut self, modifiers: Modifiers) -> Vec<MacintoshInput> {
        let mut events = Vec::new();
        // Event Manager exposes Command, Shift, Option and Control in
        // EventRecord.modifiers (Macintosh Toolbox Essentials, chapter 2).
        for (mac_key, previous, down) in [
            (0x37, self.host_modifiers.platform, modifiers.platform),
            (0x38, self.host_modifiers.shift, modifiers.shift),
            (0x3a, self.host_modifiers.alt, modifiers.alt),
            (0x3b, self.host_modifiers.control, modifiers.control),
        ] {
            if previous != down {
                let input = if down {
                    MacintoshInput::KeyDown { mac_key, character: 0 }
                } else {
                    MacintoshInput::KeyUp { mac_key, character: 0 }
                };
                events.push(input);
            }
        }
        self.host_modifiers = modifiers;
        events
    }

    pub(crate) fn press_host_key(&mut self, mac_key: u8, character: u8) -> Option<MacintoshInput> {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.held_keys.entry(mac_key) {
            entry.insert(character);
            return Some(MacintoshInput::KeyDown { mac_key, character });
        }
        None
    }

    pub(crate) fn release_host_key(&mut self, mac_key: u8) -> Option<MacintoshInput> {
        self.held_keys.remove(&mac_key).map(|character| MacintoshInput::KeyUp { mac_key, character })
    }

    // Focus loss releases physical keys but preserves the guest Caps Lock latch.
    pub(crate) fn release_all(&mut self) -> Vec<MacintoshInput> {
        let mut keys: Vec<u8> = self.held_keys.keys().copied().collect();
        keys.sort_unstable();
        let mut events: Vec<_> = keys.into_iter().filter_map(|key| self.release_host_key(key)).collect();
        events.extend(self.sync_host_modifiers(Modifiers::default()));
        events
    }
}

// GPUI reports the printed key separately from its typed character.
// Keep the Macintosh virtual code tied to the key and pass the typed
// character through the existing guest event queue. Inside Macintosh
// Volume V (1986), V-191, key-code assignments; Text (1993), pp. 2-32--2-37.
pub(crate) fn guest_virtual_key(key: &str) -> Option<u8> {
    Some(match key {
        "enter" => 0x24,
        "escape" => 0x35,
        "space" => 0x31,
        "tab" => 0x30,
        "backspace" => 0x33,
        "left" | "arrowleft" => 0x7b,
        "right" | "arrowright" => 0x7c,
        "down" | "arrowdown" => 0x7d,
        "up" | "arrowup" => 0x7e,
        "a" => 0x00,
        "s" => 0x01,
        "d" => 0x02,
        "f" => 0x03,
        "h" => 0x04,
        "g" => 0x05,
        "z" => 0x06,
        "x" => 0x07,
        "c" => 0x08,
        "v" => 0x09,
        "b" => 0x0b,
        "q" => 0x0c,
        "w" => 0x0d,
        "e" => 0x0e,
        "r" => 0x0f,
        "y" => 0x10,
        "t" => 0x11,
        "1" => 0x12,
        "2" => 0x13,
        "3" => 0x14,
        "4" => 0x15,
        "6" => 0x16,
        "5" => 0x17,
        "=" => 0x18,
        "9" => 0x19,
        "7" => 0x1a,
        "-" => 0x1b,
        "8" => 0x1c,
        "0" => 0x1d,
        "]" => 0x1e,
        "o" => 0x1f,
        "u" => 0x20,
        "[" => 0x21,
        "i" => 0x22,
        "p" => 0x23,
        "l" => 0x25,
        "j" => 0x26,
        "'" => 0x27,
        "k" => 0x28,
        ";" => 0x29,
        "\\" => 0x2a,
        "," => 0x2b,
        "/" => 0x2c,
        "n" => 0x2d,
        "m" => 0x2e,
        "." => 0x2f,
        "`" => 0x32,
        _ => return None,
    })
}

pub(crate) fn guest_key(keystroke: &Keystroke) -> Option<(u8, u8)> {
    let key = keystroke.key.to_ascii_lowercase();
    let control = match key.as_str() {
        "enter" => Some((0x24, 13)),
        "escape" => Some((0x35, 27)),
        "space" => Some((0x31, 32)),
        "tab" => Some((0x30, 9)),
        "backspace" => Some((0x33, 8)),
        "left" | "arrowleft" => Some((0x7b, 28)),
        "right" | "arrowright" => Some((0x7c, 29)),
        "down" | "arrowdown" => Some((0x7d, 31)),
        "up" | "arrowup" => Some((0x7e, 30)),
        _ => None,
    };
    if control.is_some() {
        return control;
    }
    let virtual_key = guest_virtual_key(&key)?;
    let character = if let Some(text) = keystroke.key_char.as_deref() {
        let mut chars = text.chars();
        let character = chars.next()?;
        if chars.next().is_some() {
            return None;
        }
        systemless::systems::macintosh::mac_roman::encode_mac_roman_char(character)?
    } else {
        let character = key.as_bytes()[0];
        if keystroke.modifiers.shift && character.is_ascii_alphabetic() {
            character.to_ascii_uppercase()
        } else {
            character
        }
    };
    Some((virtual_key, character))
}

