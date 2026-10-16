//! Stateful host delivery; guest KeyMap stays canonical and queryable by both CPUs.
use crate::{
    events::{self, Input},
    keyboard::KEY_MAP,
    memory::{GuestPtr, Memory},
    Error, Result,
};
use winit::keyboard::KeyCode;
pub struct Keyboard {
    held: [bool; 128],
    characters: [u8; 128],
}
impl Default for Keyboard {
    fn default() -> Self {
        Self {
            held: [false; 128],
            characters: [0; 128],
        }
    }
}
fn get(map: &[u8; 16], code: u8) -> bool {
    map[usize::from(code / 8)] & (1 << (code % 8)) != 0
}
fn set(map: &mut [u8; 16], code: u8, down: bool) {
    let bit = 1 << (code % 8);
    let p = &mut map[usize::from(code / 8)];
    if down {
        *p |= bit
    } else {
        *p &= !bit
    }
}
const GROUPS: [(u16, u8, u8); 4] = [
    (0x100, 55, 54),
    (0x200, 56, 60),
    (0x800, 58, 61),
    (0x1000, 59, 62),
];
fn flags(map: &[u8; 16], old: u16) -> u16 {
    let mut result = old & !0x1f00;
    for (bit, left, right) in GROUPS {
        if get(map, left) || get(map, right) {
            result |= bit;
        }
    }
    if get(map, 57) {
        result |= 0x400;
    }
    result
}
fn modifier(code: u8) -> bool {
    (54..=63).contains(&code)
}
fn character(code: u8, text: Option<&str>, modifiers: u16) -> Option<u8> {
    let special = match code {
        36 => 13,
        48 => 9,
        49 => 32,
        51 => 8,
        53 => 27,
        76 => 3,
        114 => 5,
        115 => 1,
        116 => 11,
        117 => 127,
        119 => 4,
        121 => 12,
        123 => 28,
        124 => 29,
        125 => 31,
        126 => 30,
        96 | 97 | 98 | 99 | 100 | 101 | 103 | 105 | 106 | 107 | 109 | 111 | 113 | 118 | 120
        | 122 => 16,
        _ => 0,
    };
    if special != 0 {
        return Some(special);
    }
    if let Some(text) = text.filter(|s| !s.is_empty()) {
        let mut chars = text.chars();
        let c = chars.next()?;
        return if chars.next().is_none() {
            crate::mac_roman::encode(c)
        } else {
            None
        };
    }
    let shift = modifiers & 0x200 != 0;
    let caps = modifiers & 0x400 != 0;
    let letter = match code {
        0..=9 | 11..=17 => b"asdfhgzxcv\0bqweryt"[usize::from(code)],
        31 => b'o',
        32 => b'u',
        34 => b'i',
        35 => b'p',
        37 => b'l',
        38 => b'j',
        40 => b'k',
        45 => b'n',
        46 => b'm',
        _ => 0,
    };
    if letter != 0 {
        return Some(if shift ^ caps {
            letter.to_ascii_uppercase()
        } else {
            letter
        });
    }
    let pair = match code {
        18 => (b'1', b'!'),
        19 => (b'2', b'@'),
        20 => (b'3', b'#'),
        21 => (b'4', b'$'),
        22 => (b'6', b'^'),
        23 => (b'5', b'%'),
        24 => (b'=', b'+'),
        25 => (b'9', b'('),
        26 => (b'7', b'&'),
        27 => (b'-', b'_'),
        28 => (b'8', b'*'),
        29 => (b'0', b')'),
        30 => (b']', b'}'),
        33 => (b'[', b'{'),
        39 => (b'\'', b'"'),
        41 => (b';', b':'),
        42 => (b'\\', b'|'),
        43 => (b',', b'<'),
        44 => (b'/', b'?'),
        47 => (b'.', b'>'),
        50 => (b'`', b'~'),
        65 => (b'.', b'.'),
        67 => (b'*', b'*'),
        69 => (b'+', b'+'),
        75 => (b'/', b'/'),
        78 => (b'-', b'-'),
        81 => (b'=', b'='),
        82 => (b'0', b'0'),
        83..=89 => {
            let c = b'1' + code - 83;
            (c, c)
        }
        91 => (b'8', b'8'),
        92 => (b'9', b'9'),
        _ => return None,
    };
    Some(if shift { pair.1 } else { pair.0 })
}
impl Keyboard {
    pub fn key(
        &mut self,
        memory: &mut Memory,
        input: &mut Input,
        code: u8,
        text: Option<&str>,
        down: bool,
        repeat: bool,
    ) -> Result<()> {
        if code >= 128 {
            return Err(Error::Cpu("virtual key outside 0..127".into()));
        }
        let i = usize::from(code);
        let was = self.held[i];
        if down && was && !repeat {
            return Ok(());
        }
        let mut map: [u8; 16] = memory.read(KEY_MAP, 16)?.try_into().unwrap();
        let mut held = self.held;
        held[i] = down;
        if code == 57 {
            if down && !was {
                let active = !get(&map, 57);
                set(&mut map, 57, active);
            }
        } else if let Some((_, left, right)) = GROUPS
            .iter()
            .copied()
            .find(|(_, l, r)| *l == code || *r == code)
        {
            set(&mut map, left, held[usize::from(left)]);
            set(&mut map, right, held[usize::from(right)]);
        } else {
            set(&mut map, code, down);
        }
        let mut next = *input;
        next.modifiers = flags(&map, input.modifiers);
        let value = if modifier(code) {
            None
        } else if down {
            character(code, text, next.modifiers)
        } else if was && self.characters[i] != 0 {
            Some(self.characters[i])
        } else {
            None
        }
        .filter(|&byte| byte != 0);
        memory.validate_write(KEY_MAP, 16)?;
        if let Some(c) = value {
            let ticks = memory.u32(GuestPtr(0x16a))?;
            events::post(
                memory,
                if down {
                    if was && repeat {
                        5
                    } else {
                        3
                    }
                } else {
                    4
                },
                u32::from(code) << 8 | u32::from(c),
                ticks,
                next,
            )?;
        }
        memory.write(KEY_MAP, &map)?;
        self.held = held;
        self.characters[i] = if down { value.unwrap_or(0) } else { 0 };
        *input = next;
        Ok(())
    }
    /// Aggregate platform modifiers fill missing left/right events; Caps Lock is latched separately.
    pub fn modifiers(&mut self, memory: &mut Memory, input: &mut Input, mask: u16) -> Result<()> {
        let mut map: [u8; 16] = memory.read(KEY_MAP, 16)?.try_into().unwrap();
        let mut held = self.held;
        for (flag, left, right) in GROUPS {
            if mask & flag == 0 {
                set(&mut map, left, false);
                set(&mut map, right, false);
                held[usize::from(left)] = false;
                held[usize::from(right)] = false;
            } else if !get(&map, left) && !get(&map, right) {
                set(&mut map, left, true);
            }
        }
        memory.write(KEY_MAP, &map)?;
        self.held = held;
        input.modifiers = flags(&map, input.modifiers);
        Ok(())
    }
    /// Focus loss releases physical keys without synthesizing character events; keep the lock latch.
    pub fn unfocus(&mut self, memory: &mut Memory, input: &mut Input) -> Result<()> {
        let caps = memory.read(KEY_MAP, 16)?[7] & 2;
        let mut map = [0; 16];
        map[7] = caps;
        memory.write(KEY_MAP, &map)?;
        self.held = [false; 128];
        self.characters = [0; 128];
        input.modifiers = flags(&map, input.modifiers);
        Ok(())
    }
}
/// Classic Apple/ADB physical assignments, compared with preserved Systemless.
/// Unmapped host keys are ignored rather than becoming the A key.
pub fn virtual_key(key: KeyCode) -> Option<u8> {
    use KeyCode::*;
    Some(match key {
        KeyA => 0,
        KeyS => 1,
        KeyD => 2,
        KeyF => 3,
        KeyH => 4,
        KeyG => 5,
        KeyZ => 6,
        KeyX => 7,
        KeyC => 8,
        KeyV => 9,
        IntlBackslash => 10,
        KeyB => 11,
        KeyQ => 12,
        KeyW => 13,
        KeyE => 14,
        KeyR => 15,
        KeyY => 16,
        KeyT => 17,
        Digit1 => 18,
        Digit2 => 19,
        Digit3 => 20,
        Digit4 => 21,
        Digit6 => 22,
        Digit5 => 23,
        Equal => 24,
        Digit9 => 25,
        Digit7 => 26,
        Minus => 27,
        Digit8 => 28,
        Digit0 => 29,
        BracketRight => 30,
        KeyO => 31,
        KeyU => 32,
        BracketLeft => 33,
        KeyI => 34,
        KeyP => 35,
        Enter => 36,
        KeyL => 37,
        KeyJ => 38,
        Quote => 39,
        KeyK => 40,
        Semicolon => 41,
        Backslash => 42,
        Comma => 43,
        Slash => 44,
        KeyN => 45,
        KeyM => 46,
        Period => 47,
        Tab => 48,
        Space => 49,
        Backquote => 50,
        Backspace => 51,
        Escape => 53,
        SuperRight => 54,
        SuperLeft => 55,
        ShiftLeft => 56,
        CapsLock => 57,
        AltLeft => 58,
        ControlLeft => 59,
        ShiftRight => 60,
        AltRight => 61,
        ControlRight => 62,
        Fn => 63,
        NumpadDecimal => 65,
        NumpadMultiply => 67,
        NumpadAdd => 69,
        NumLock | NumpadClear => 71,
        NumpadDivide => 75,
        NumpadEnter => 76,
        NumpadSubtract => 78,
        NumpadEqual => 81,
        Numpad0 => 82,
        Numpad1 => 83,
        Numpad2 => 84,
        Numpad3 => 85,
        Numpad4 => 86,
        Numpad5 => 87,
        Numpad6 => 88,
        Numpad7 => 89,
        Numpad8 => 91,
        Numpad9 => 92,
        F5 => 96,
        F6 => 97,
        F7 => 98,
        F3 => 99,
        F8 => 100,
        F9 => 101,
        F11 => 103,
        F13 => 105,
        F16 => 106,
        F14 => 107,
        F10 => 109,
        F12 => 111,
        F15 => 113,
        Help | Insert => 114,
        Home => 115,
        PageUp => 116,
        Delete => 117,
        F4 => 118,
        End => 119,
        F2 => 120,
        PageDown => 121,
        F1 => 122,
        ArrowLeft => 123,
        ArrowRight => 124,
        ArrowDown => 125,
        ArrowUp => 126,
        _ => return None,
    })
}
