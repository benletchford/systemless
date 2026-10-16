fn logical_arrow_to_mac(key: &Key) -> Option<(u8, u8)> {
    match key {
        Key::Named(NamedKey::ArrowLeft) => Some((0x7B, 28)),
        Key::Named(NamedKey::ArrowRight) => Some((0x7C, 29)),
        Key::Named(NamedKey::ArrowDown) => Some((0x7D, 31)),
        Key::Named(NamedKey::ArrowUp) => Some((0x7E, 30)),
        _ => None,
    }
}

fn physical_numpad_to_mac(key: &PhysicalKey) -> Option<(u8, u8)> {
    match key {
        PhysicalKey::Code(
            KeyCode::NumpadDecimal
            | KeyCode::NumpadMultiply
            | KeyCode::NumpadAdd
            | KeyCode::NumpadDivide
            | KeyCode::NumpadEnter
            | KeyCode::NumpadSubtract
            | KeyCode::NumpadEqual
            | KeyCode::Numpad0
            | KeyCode::Numpad1
            | KeyCode::Numpad2
            | KeyCode::Numpad3
            | KeyCode::Numpad4
            | KeyCode::Numpad5
            | KeyCode::Numpad6
            | KeyCode::Numpad7
            | KeyCode::Numpad8
            | KeyCode::Numpad9,
        ) => Some((keycode_to_mac(key), keycode_to_mac_char(key))),
        _ => None,
    }
}

fn host_keyboard_event_to_mac(
    logical_key: &Key,
    physical_key: &PhysicalKey,
    text: Option<&str>,
) -> Option<(u8, u8)> {
    // Laptop media actions can share a physical function-key position. The
    // logical volume action belongs to the host and has no classic key event.
    if matches!(
        logical_key,
        Key::Named(NamedKey::AudioVolumeDown | NamedKey::AudioVolumeUp | NamedKey::AudioVolumeMute)
    ) {
        return None;
    }
    Some(host_key_to_mac(logical_key, physical_key, text))
}

fn host_key_to_mac(logical_key: &Key, physical_key: &PhysicalKey, text: Option<&str>) -> (u8, u8) {
    let (mac_key, mac_char_fallback) = physical_numpad_to_mac(physical_key)
        .or_else(|| logical_arrow_to_mac(logical_key))
        .unwrap_or_else(|| {
            (
                keycode_to_mac(physical_key),
                keycode_to_mac_char(physical_key),
            )
        });

    // Control keys (Enter / Tab / Escape / arrows / Space / Backspace)
    // have canonical Mac char codes (CR = 13 for Enter, not LF = 10).
    // winit's `event.text` reports the platform's text-input view (often
    // "\n" for Enter on Linux / wayland), which is wrong for classic Mac.
    // Use `keycode_to_mac_char` first; it returns the correct Mac code for
    // every control key we handle, and 0 for printable keys.
    let char_code = if mac_char_fallback != 0 {
        mac_char_fallback
    } else {
        text.and_then(|t| t.bytes().next())
            .unwrap_or_else(|| keycode_to_mac_printable_char(physical_key))
    };

    (mac_key, char_code)
}

/// Map a winit PhysicalKey to a classic Mac virtual key code.
/// Inside Macintosh Volume V, V-191 (key code assignments)
fn keycode_to_mac(key: &PhysicalKey) -> u8 {
    match key {
        PhysicalKey::Code(code) => match code {
            KeyCode::KeyA => 0x00,
            KeyCode::KeyS => 0x01,
            KeyCode::KeyD => 0x02,
            KeyCode::KeyF => 0x03,
            KeyCode::KeyH => 0x04,
            KeyCode::KeyG => 0x05,
            KeyCode::KeyZ => 0x06,
            KeyCode::KeyX => 0x07,
            KeyCode::KeyC => 0x08,
            KeyCode::KeyV => 0x09,
            KeyCode::KeyB => 0x0B,
            KeyCode::KeyQ => 0x0C,
            KeyCode::KeyW => 0x0D,
            KeyCode::KeyE => 0x0E,
            KeyCode::KeyR => 0x0F,
            KeyCode::KeyY => 0x10,
            KeyCode::KeyT => 0x11,
            KeyCode::Digit1 => 0x12,
            KeyCode::Digit2 => 0x13,
            KeyCode::Digit3 => 0x14,
            KeyCode::Digit4 => 0x15,
            KeyCode::Digit6 => 0x16,
            KeyCode::Digit5 => 0x17,
            KeyCode::Equal => 0x18,
            KeyCode::Digit9 => 0x19,
            KeyCode::Digit7 => 0x1A,
            KeyCode::Minus => 0x1B,
            KeyCode::Digit8 => 0x1C,
            KeyCode::Digit0 => 0x1D,
            KeyCode::BracketRight => 0x1E,
            KeyCode::KeyO => 0x1F,
            KeyCode::KeyU => 0x20,
            KeyCode::BracketLeft => 0x21,
            KeyCode::KeyI => 0x22,
            KeyCode::KeyP => 0x23,
            KeyCode::Enter => 0x24,
            KeyCode::KeyL => 0x25,
            KeyCode::KeyJ => 0x26,
            KeyCode::Quote => 0x27,
            KeyCode::KeyK => 0x28,
            KeyCode::Semicolon => 0x29,
            KeyCode::Backslash => 0x2A,
            KeyCode::Comma => 0x2B,
            KeyCode::Slash => 0x2C,
            KeyCode::KeyN => 0x2D,
            KeyCode::KeyM => 0x2E,
            KeyCode::Period => 0x2F,
            KeyCode::Tab => 0x30,
            KeyCode::Space => 0x31,
            KeyCode::Backquote => 0x32,
            KeyCode::Backspace => 0x33,
            KeyCode::Escape => 0x35,
            KeyCode::SuperLeft => 0x37,
            KeyCode::ShiftLeft => 0x38,
            KeyCode::CapsLock => 0x39,
            KeyCode::AltLeft => 0x3A,
            KeyCode::ControlLeft => 0x3B,
            KeyCode::ShiftRight => 0x3C,
            KeyCode::AltRight => 0x3D,
            KeyCode::ControlRight => 0x3E,
            KeyCode::NumpadDecimal => 0x41,
            KeyCode::NumpadMultiply => 0x43,
            KeyCode::NumpadAdd => 0x45,
            KeyCode::NumLock => 0x47,
            KeyCode::NumpadDivide => 0x4B,
            KeyCode::NumpadEnter => 0x4C,
            KeyCode::NumpadSubtract => 0x4E,
            KeyCode::NumpadEqual => 0x51,
            KeyCode::Numpad0 => 0x52,
            KeyCode::Numpad1 => 0x53,
            KeyCode::Numpad2 => 0x54,
            KeyCode::Numpad3 => 0x55,
            KeyCode::Numpad4 => 0x56,
            KeyCode::Numpad5 => 0x57,
            KeyCode::Numpad6 => 0x58,
            KeyCode::Numpad7 => 0x59,
            KeyCode::Numpad8 => 0x5B,
            KeyCode::Numpad9 => 0x5C,
            KeyCode::ArrowLeft => 0x7B,
            KeyCode::ArrowRight => 0x7C,
            KeyCode::ArrowDown => 0x7D,
            KeyCode::ArrowUp => 0x7E,
            KeyCode::F1 => 0x7A,
            KeyCode::F2 => 0x78,
            KeyCode::F3 => 0x63,
            KeyCode::F4 => 0x76,
            KeyCode::F5 => 0x60,
            KeyCode::F6 => 0x61,
            KeyCode::F7 => 0x62,
            KeyCode::F8 => 0x64,
            KeyCode::F9 => 0x65,
            KeyCode::F10 => 0x6D,
            KeyCode::F11 => 0x67,
            KeyCode::F12 => 0x6F,
            KeyCode::F13 => 0x69,
            KeyCode::F14 => 0x6B,
            KeyCode::F15 => 0x71,
            _ => 0xFF,
        },
        _ => 0xFF,
    }
}

/// Fallback char code for non-text keys (arrows, return, etc.).
fn keycode_to_mac_char(key: &PhysicalKey) -> u8 {
    match key {
        PhysicalKey::Code(code) => match code {
            KeyCode::Enter => 13,
            KeyCode::NumpadEnter => 0x03,
            KeyCode::Tab => 9,
            KeyCode::Space => 32,
            KeyCode::Backspace => 8,
            KeyCode::Escape => 27,
            KeyCode::ArrowLeft => 28,
            KeyCode::ArrowRight => 29,
            KeyCode::ArrowUp => 30,
            KeyCode::ArrowDown => 31,
            // Apple Events.h: kFunctionKeyCharCode = 16. Preserve the
            // virtual key separately, as described by Inside Macintosh:
            // Macintosh Toolbox Essentials, "Handling Low-Level Events".
            KeyCode::F1
            | KeyCode::F2
            | KeyCode::F3
            | KeyCode::F4
            | KeyCode::F5
            | KeyCode::F6
            | KeyCode::F7
            | KeyCode::F8
            | KeyCode::F9
            | KeyCode::F10
            | KeyCode::F11
            | KeyCode::F12
            | KeyCode::F13
            | KeyCode::F14
            | KeyCode::F15 => 0x10,
            _ => 0,
        },
        _ => 0,
    }
}

/// Last-resort printable fallback when the windowing layer reports a physical
/// key event without text. This preserves menu hotkeys and EventRecord readers;
/// when text is available, the platform's layout-aware character still wins.
fn keycode_to_mac_printable_char(key: &PhysicalKey) -> u8 {
    match key {
        PhysicalKey::Code(code) => match code {
            KeyCode::KeyA => b'a',
            KeyCode::KeyB => b'b',
            KeyCode::KeyC => b'c',
            KeyCode::KeyD => b'd',
            KeyCode::KeyE => b'e',
            KeyCode::KeyF => b'f',
            KeyCode::KeyG => b'g',
            KeyCode::KeyH => b'h',
            KeyCode::KeyI => b'i',
            KeyCode::KeyJ => b'j',
            KeyCode::KeyK => b'k',
            KeyCode::KeyL => b'l',
            KeyCode::KeyM => b'm',
            KeyCode::KeyN => b'n',
            KeyCode::KeyO => b'o',
            KeyCode::KeyP => b'p',
            KeyCode::KeyQ => b'q',
            KeyCode::KeyR => b'r',
            KeyCode::KeyS => b's',
            KeyCode::KeyT => b't',
            KeyCode::KeyU => b'u',
            KeyCode::KeyV => b'v',
            KeyCode::KeyW => b'w',
            KeyCode::KeyX => b'x',
            KeyCode::KeyY => b'y',
            KeyCode::KeyZ => b'z',
            KeyCode::Digit0 | KeyCode::Numpad0 => b'0',
            KeyCode::Digit1 | KeyCode::Numpad1 => b'1',
            KeyCode::Digit2 | KeyCode::Numpad2 => b'2',
            KeyCode::Digit3 | KeyCode::Numpad3 => b'3',
            KeyCode::Digit4 | KeyCode::Numpad4 => b'4',
            KeyCode::Digit5 | KeyCode::Numpad5 => b'5',
            KeyCode::Digit6 | KeyCode::Numpad6 => b'6',
            KeyCode::Digit7 | KeyCode::Numpad7 => b'7',
            KeyCode::Digit8 | KeyCode::Numpad8 => b'8',
            KeyCode::Digit9 | KeyCode::Numpad9 => b'9',
            KeyCode::Minus | KeyCode::NumpadSubtract => b'-',
            KeyCode::Equal | KeyCode::NumpadEqual => b'=',
            KeyCode::BracketLeft => b'[',
            KeyCode::BracketRight => b']',
            KeyCode::Backslash => b'\\',
            KeyCode::Semicolon => b';',
            KeyCode::Quote => b'\'',
            KeyCode::Comma => b',',
            KeyCode::Period | KeyCode::NumpadDecimal => b'.',
            KeyCode::Slash | KeyCode::NumpadDivide => b'/',
            KeyCode::NumpadMultiply => b'*',
            KeyCode::NumpadAdd => b'+',
            KeyCode::Backquote => b'`',
            _ => 0,
        },
        _ => 0,
    }
}
