//! Key remapping (opcode 0x11 for the base layer, 0x27 for the Fn layer).
//!
//! Every key is a 4-byte entry at its key index (= the LED index in layout.json):
//!
//! | Entry | Meaning |
//! |---|---|
//! | `00 00 00 00` | factory default |
//! | `05 03 00 00` | disabled |
//! | `02 MODS CODE 00` | HID key with modifiers (1 Ctrl, 2 Shift, 4 Alt, 8 Win) |
//! | `01 01 BTN 00` | mouse button (1 left, 2 right, 4 middle, 3 double click, 10 button 4, 08 button 5) |
//! | `01 03 DIR 00` | wheel (01 up, FF down) |
//! | `03 LO HI 00` | consumer usage (media and launch keys) |
//! | `06 SLOT MODE COUNT` | macro |
//! | `07 K1 K2 K3` | up to three keys at once |

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::i18n::tr;
use crate::tr;

pub const TABLE_BYTES: usize = 576;
pub const TABLE_PACKETS: usize = 9;
pub const MAX_INDEX: u8 = 142;

pub const MOD_CTRL: u8 = 0x01;
pub const MOD_SHIFT: u8 = 0x02;
pub const MOD_ALT: u8 = 0x04;
pub const MOD_WIN: u8 = 0x08;
const MOD_NAMES: [(u8, &str); 4] = [(MOD_CTRL, "Ctrl"), (MOD_SHIFT, "Shift"), (MOD_ALT, "Alt"), (MOD_WIN, "Win")];

/// HID usages by name, in the order the app lists them.
pub fn hid_keys() -> &'static [(String, u8)] {
    static KEYS: OnceLock<Vec<(String, u8)>> = OnceLock::new();
    KEYS.get_or_init(|| {
        let mut k: Vec<(String, u8)> = (0..26u8).map(|i| (((b'A' + i) as char).to_string(), 0x04 + i)).collect();
        k.extend((1..10u8).map(|i| (i.to_string(), 0x1D + i)));
        k.push(("0".into(), 0x27));
        for (n, c) in [
            ("Enter", 0x28), ("Esc", 0x29), ("Backspace", 0x2A), ("Tab", 0x2B), ("Space", 0x2C), ("-", 0x2D),
            ("=", 0x2E), ("[", 0x2F), ("]", 0x30), ("\\", 0x31), (";", 0x33), ("'", 0x34), ("`", 0x35),
            (",", 0x36), (".", 0x37), ("/", 0x38), ("CapsLock", 0x39),
        ] {
            k.push((n.into(), c));
        }
        k.extend((1..=12u8).map(|i| (format!("F{i}"), 0x39 + i)));
        for (n, c) in [
            ("PrintScreen", 0x46), ("ScrollLock", 0x47), ("Pause", 0x48), ("Insert", 0x49), ("Home", 0x4A),
            ("PageUp", 0x4B), ("Delete", 0x4C), ("End", 0x4D), ("PageDown", 0x4E), ("Right", 0x4F),
            ("Left", 0x50), ("Down", 0x51), ("Up", 0x52), ("NumLock", 0x53), ("Num/", 0x54), ("Num*", 0x55),
            ("Num-", 0x56), ("Num+", 0x57), ("NumEnter", 0x58),
        ] {
            k.push((n.into(), c));
        }
        k.extend((1..10u8).map(|i| (format!("Num{i}"), 0x58 + i)));
        k.extend([("Num0".into(), 0x62), ("Num.".into(), 0x63), ("Menu".into(), 0x65)]);
        k.extend((13..=24u8).map(|i| (format!("F{i}"), 0x68 + i - 13)));
        for (n, c) in [
            ("LeftCtrl", 0xE0), ("LeftShift", 0xE1), ("LeftAlt", 0xE2), ("LeftWin", 0xE3), ("RightCtrl", 0xE4),
            ("RightShift", 0xE5), ("RightAlt", 0xE6), ("RightWin", 0xE7), ("Fn", 0xAF),
        ] {
            k.push((n.into(), c));
        }
        k
    })
}

const KEY_ALIASES: [(&str, u8); 17] = [
    ("return", 0x28), ("escape", 0x29), ("bksp", 0x2A), ("del", 0x4C), ("ins", 0x49), ("pgup", 0x4B),
    ("pgdn", 0x4E), ("prtsc", 0x46), ("scrlk", 0x47), ("caps", 0x39), ("app", 0x65), ("ctrl", 0xE0),
    ("shift", 0xE1), ("alt", 0xE2), ("win", 0xE3), ("gui", 0xE3), ("super", 0xE3),
];

pub fn key_code(name: &str) -> Result<u8> {
    let n = name.trim().to_lowercase();
    if let Some((_, c)) = hid_keys().iter().find(|(k, _)| k.to_lowercase() == n) {
        return Ok(*c);
    }
    if let Some((_, c)) = KEY_ALIASES.iter().find(|(k, _)| *k == n) {
        return Ok(*c);
    }
    if let Some(hex) = n.strip_prefix("0x")
        && let Ok(v) = u8::from_str_radix(hex, 16) {
            return Ok(v);
        }
    Err(Error::invalid(tr!("Unknown key: {name}", name = name)))
}

pub fn key_name(code: u8) -> String {
    hid_keys().iter().find(|(_, c)| *c == code).map(|(n, _)| n.clone()).unwrap_or_else(|| format!("0x{code:02X}"))
}

/// Linux input keycode (linux/input-event-codes.h) to HID usage.
pub fn linux_to_hid(code: u16) -> Option<u8> {
    const TABLE: &[(u16, u8)] = &[
        (1, 0x29), (2, 0x1E), (3, 0x1F), (4, 0x20), (5, 0x21), (6, 0x22), (7, 0x23), (8, 0x24), (9, 0x25),
        (10, 0x26), (11, 0x27), (12, 0x2D), (13, 0x2E), (14, 0x2A), (15, 0x2B), (16, 0x14), (17, 0x1A),
        (18, 0x08), (19, 0x15), (20, 0x17), (21, 0x1C), (22, 0x18), (23, 0x0C), (24, 0x12), (25, 0x13),
        (26, 0x2F), (27, 0x30), (28, 0x28), (29, 0xE0), (30, 0x04), (31, 0x16), (32, 0x07), (33, 0x09),
        (34, 0x0A), (35, 0x0B), (36, 0x0D), (37, 0x0E), (38, 0x0F), (39, 0x33), (40, 0x34), (41, 0x35),
        (42, 0xE1), (43, 0x31), (44, 0x1D), (45, 0x1B), (46, 0x06), (47, 0x19), (48, 0x05), (49, 0x11),
        (50, 0x10), (51, 0x36), (52, 0x37), (53, 0x38), (54, 0xE5), (55, 0x55), (56, 0xE2), (57, 0x2C),
        (58, 0x39), (59, 0x3A), (60, 0x3B), (61, 0x3C), (62, 0x3D), (63, 0x3E), (64, 0x3F), (65, 0x40),
        (66, 0x41), (67, 0x42), (68, 0x43), (69, 0x53), (70, 0x47), (71, 0x5F), (72, 0x60), (73, 0x61),
        (74, 0x56), (75, 0x5C), (76, 0x5D), (77, 0x5E), (78, 0x57), (79, 0x59), (80, 0x5A), (81, 0x5B),
        (82, 0x62), (83, 0x63), (86, 0x64), (87, 0x44), (88, 0x45), (96, 0x58), (97, 0xE4), (98, 0x54),
        (99, 0x46), (100, 0xE6), (102, 0x4A), (103, 0x52), (104, 0x4B), (105, 0x50), (106, 0x4F), (107, 0x4D),
        (108, 0x51), (109, 0x4E), (110, 0x49), (111, 0x4C), (119, 0x48), (125, 0xE3), (126, 0xE7),
    ];
    match code {
        127 => Some(0x65),
        183..=194 => Some(0x68 + (code - 183) as u8), // F13..F24
        _ => TABLE.iter().find(|(c, _)| *c == code).map(|(_, h)| *h),
    }
}

/// Consumer usages offered for media keys (English labels; translate with `tr`).
pub const MEDIA_KEYS: [(&str, u16); 18] = [
    ("Play/Pause", 0xCD), ("Stop", 0xB7), ("Previous track", 0xB6), ("Next track", 0xB5),
    ("Volume up", 0xE9), ("Volume down", 0xEA), ("Mute", 0xE2),
    ("Calculator", 0x192), ("Mail", 0x18A), ("Media player", 0x183), ("My Computer", 0x194),
    ("Browser home", 0x223), ("Browser refresh", 0x227), ("Browser stop", 0x226), ("Browser back", 0x224),
    ("Browser forward", 0x225), ("Browser favorites", 0x22A), ("Browser search", 0x221),
];
const MEDIA_ALIASES: [(&str, u16); 21] = [
    ("play", 0xCD), ("playpause", 0xCD), ("stop", 0xB7), ("prev", 0xB6), ("previous", 0xB6), ("next", 0xB5),
    ("volup", 0xE9), ("voldown", 0xEA), ("mute", 0xE2), ("calc", 0x192), ("calculator", 0x192),
    ("mail", 0x18A), ("player", 0x183), ("computer", 0x194), ("home", 0x223), ("refresh", 0x227),
    ("back", 0x224), ("forward", 0x225), ("favorites", 0x22A), ("search", 0x221), ("browserstop", 0x226),
];

pub fn media_usage(name: &str) -> Result<u16> {
    let n = name.trim().to_lowercase().replace(' ', "");
    if let Some((_, u)) = MEDIA_ALIASES.iter().find(|(a, _)| *a == n) {
        return Ok(*u);
    }
    if let Some((_, u)) = MEDIA_KEYS.iter().find(|(l, _)| l.to_lowercase().replace(' ', "") == n) {
        return Ok(*u);
    }
    if let Some(hex) = n.strip_prefix("0x")
        && let Ok(v) = u16::from_str_radix(hex, 16) {
            return Ok(v);
        }
    Err(Error::invalid(tr!("Unknown media key: {name}", name = name)))
}

pub fn media_name(usage: u16) -> String {
    MEDIA_KEYS
        .iter()
        .find(|(_, u)| *u == usage)
        .map(|(n, _)| tr(n))
        .unwrap_or_else(|| format!("Consumer 0x{usage:03X}"))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionKind {
    #[default]
    Default,
    Off,
    Key,
    Mouse,
    Media,
    Macro,
    Multi,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseFn {
    #[default]
    Left,
    Right,
    Middle,
    Double,
    Btn4,
    Btn5,
    WheelUp,
    WheelDown,
}

impl MouseFn {
    pub const ALL: [MouseFn; 8] = [
        MouseFn::Left, MouseFn::Right, MouseFn::Middle, MouseFn::Double, MouseFn::Btn4, MouseFn::Btn5,
        MouseFn::WheelUp, MouseFn::WheelDown,
    ];

    pub fn id(self) -> &'static str {
        ["left", "right", "middle", "double", "btn4", "btn5", "wheelup", "wheeldown"][self as usize]
    }

    /// English label; translate with `tr`.
    pub fn label(self) -> &'static str {
        ["Left button", "Right button", "Middle button", "Double click", "Button 4", "Button 5", "Wheel up",
         "Wheel down"][self as usize]
    }

    fn bytes(self) -> (u8, u8) {
        [(0x01, 0x01), (0x01, 0x02), (0x01, 0x04), (0x01, 0x03), (0x01, 0x10), (0x01, 0x08), (0x03, 0x01),
         (0x03, 0xFF)][self as usize]
    }

    pub fn by_id(id: &str) -> Option<MouseFn> {
        Self::ALL.into_iter().find(|m| m.id() == id.to_lowercase())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MacroMode {
    #[default]
    Once,
    Repeat,
    Toggle,
    Hold,
    Click,
}

impl MacroMode {
    pub const ALL: [MacroMode; 5] =
        [MacroMode::Once, MacroMode::Repeat, MacroMode::Toggle, MacroMode::Hold, MacroMode::Click];

    pub fn id(self) -> &'static str {
        ["once", "repeat", "toggle", "hold", "click"][self as usize]
    }

    /// English label; translate with `tr`.
    pub fn label(self) -> &'static str {
        ["Play once", "Play N times", "Until pressed again", "While held", "Click to play"][self as usize]
    }

    pub fn by_id(id: &str) -> Option<MacroMode> {
        Self::ALL.into_iter().find(|m| m.id() == id.to_lowercase())
    }
}

/// Spare keys the firmware can send but no key produces; the agent turns them into "launch a
/// program or open a link".
pub const LAUNCHER_KEYS: [&str; 12] = ["F13", "F14", "F15", "F16", "F17", "F18", "F19", "F20", "F21", "F22", "F23",
                                       "F24"];

/// What one key does. Stored in config.json with the same fields as the Python version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyAction {
    pub kind: ActionKind,
    pub mods: u8,
    pub code: u8,
    pub mouse: MouseFn,
    pub usage: u16,
    #[serde(rename = "macro")]
    pub macro_slot: u8,
    pub macro_mode: MacroMode,
    pub macro_count: u8,
    pub keys: Vec<u8>,
}

impl Default for KeyAction {
    fn default() -> Self {
        Self { kind: ActionKind::Default, mods: 0, code: 0, mouse: MouseFn::Left, usage: 0, macro_slot: 0,
               macro_mode: MacroMode::Once, macro_count: 1, keys: Vec::new() }
    }
}

impl KeyAction {
    pub fn off() -> Self {
        Self { kind: ActionKind::Off, ..Self::default() }
    }

    pub fn key(mods: u8, code: u8) -> Self {
        Self { kind: ActionKind::Key, mods, code, ..Self::default() }
    }

    pub fn mouse(f: MouseFn) -> Self {
        Self { kind: ActionKind::Mouse, mouse: f, ..Self::default() }
    }

    pub fn media(usage: u16) -> Self {
        Self { kind: ActionKind::Media, usage, ..Self::default() }
    }

    pub fn macro_(slot: u8, mode: MacroMode, count: u8) -> Self {
        Self { kind: ActionKind::Macro, macro_slot: slot, macro_mode: mode, macro_count: count, ..Self::default() }
    }

    pub fn multi(keys: Vec<u8>) -> Self {
        Self { kind: ActionKind::Multi, keys, ..Self::default() }
    }

    pub fn is_default(&self) -> bool {
        self.kind == ActionKind::Default
    }

    pub fn to_entry(&self) -> [u8; 4] {
        let mut e = [0u8; 4];
        match self.kind {
            ActionKind::Default => {}
            ActionKind::Off => e[..2].copy_from_slice(&[0x05, 0x03]),
            ActionKind::Key => {
                e[0] = 0x02;
                if self.mods == 0 && (0xE0..=0xE7).contains(&self.code) {
                    e[1] = 1 << (self.code - 0xE0); // a bare modifier goes out as its bit, like the vendor driver
                } else {
                    e[1] = self.mods;
                    e[2] = self.code;
                }
            }
            ActionKind::Mouse => {
                let (a, b) = self.mouse.bytes();
                e[0] = 0x01;
                e[1] = a;
                e[2] = b;
            }
            ActionKind::Media => {
                e[0] = 0x03;
                e[1] = (self.usage & 0xFF) as u8;
                e[2] = (self.usage >> 8) as u8;
            }
            ActionKind::Macro => {
                e[0] = 0x06;
                e[1] = self.macro_slot.min(99);
                e[2] = self.macro_mode as u8;
                e[3] = if self.macro_mode == MacroMode::Repeat { self.macro_count.max(1) } else { 0 };
            }
            ActionKind::Multi => {
                e[0] = 0x07;
                for (i, k) in self.keys.iter().take(3).enumerate() {
                    e[1 + i] = *k;
                }
            }
        }
        e
    }

    /// For people: "Ctrl+Shift+A", "Volume up", "Macro 2 ×5".
    pub fn describe(&self, macro_names: &[String]) -> String {
        match self.kind {
            ActionKind::Default => tr("Default"),
            ActionKind::Off => tr("Disabled"),
            ActionKind::Key => {
                let mut parts: Vec<String> =
                    MOD_NAMES.iter().filter(|(b, _)| self.mods & b != 0).map(|(_, n)| n.to_string()).collect();
                if self.code != 0 {
                    parts.push(key_name(self.code));
                }
                if parts.is_empty() { tr("(nothing)") } else { parts.join("+") }
            }
            ActionKind::Mouse => tr(self.mouse.label()),
            ActionKind::Media => media_name(self.usage),
            ActionKind::Macro => {
                let name = macro_names
                    .get(self.macro_slot as usize)
                    .cloned()
                    .unwrap_or_else(|| tr!("Macro {n}", n = self.macro_slot));
                if self.macro_mode == MacroMode::Repeat { format!("{name} ×{}", self.macro_count) } else { name }
            }
            ActionKind::Multi => self.keys.iter().map(|k| key_name(*k)).collect::<Vec<_>>().join("+"),
        }
    }

    /// The command-line form, which [`KeyAction::parse`] reads back.
    pub fn to_spec(&self) -> String {
        match self.kind {
            ActionKind::Default => "default".into(),
            ActionKind::Off => "off".into(),
            ActionKind::Key => {
                let mut parts: Vec<String> = MOD_NAMES
                    .iter()
                    .filter(|(b, _)| self.mods & b != 0)
                    .map(|(_, n)| n.to_lowercase())
                    .collect();
                if self.code != 0 {
                    parts.push(key_name(self.code));
                }
                format!("key:{}", parts.join("+"))
            }
            ActionKind::Mouse => format!("mouse:{}", self.mouse.id()),
            ActionKind::Media => format!("media:0x{:x}", self.usage),
            ActionKind::Macro => {
                let s = format!("macro:{}:{}", self.macro_slot, self.macro_mode.id());
                if self.macro_mode == MacroMode::Repeat { format!("{s}:{}", self.macro_count) } else { s }
            }
            ActionKind::Multi => {
                format!("multi:{}", self.keys.iter().map(|k| key_name(*k)).collect::<Vec<_>>().join(","))
            }
        }
    }

    /// `default | off | key:[ctrl+][shift+][alt+][win+]NAME | mouse:left|right|middle|double|btn4|btn5|
    /// wheelup|wheeldown | media:NAME | macro:N[:once|repeat:COUNT|toggle|hold|click] | multi:A,B,C`
    pub fn parse(spec: &str) -> Result<Self> {
        let s = spec.trim();
        match s.to_lowercase().as_str() {
            "default" | "def" => return Ok(Self::default()),
            "off" | "disabled" | "none" => return Ok(Self::off()),
            _ => {}
        }
        let (kind, rest) = s.split_once(':').unwrap_or((s, ""));
        match kind.to_lowercase().as_str() {
            "key" => {
                // Everything before the last "+" is a modifier; "key:ctrl" on its own is the Ctrl key
                let parts: Vec<&str> = rest.split('+').map(str::trim).filter(|p| !p.is_empty()).collect();
                let bit = |p: &str| match p.to_lowercase().as_str() {
                    "ctrl" => Some(MOD_CTRL),
                    "shift" => Some(MOD_SHIFT),
                    "alt" => Some(MOD_ALT),
                    "win" | "super" => Some(MOD_WIN),
                    _ => None,
                };
                let mut mods = 0;
                for p in parts.iter().take(parts.len().saturating_sub(1)) {
                    mods |= bit(p).ok_or_else(|| Error::invalid(tr!("Unknown modifier: {name}", name = p)))?;
                }
                if parts.len() > 1
                    && let Some(b) = bit(parts[parts.len() - 1]) {
                        return Ok(Self::key(mods | b, 0));
                    }
                let code = match parts.last() {
                    Some(name) => key_code(name)?,
                    None => 0,
                };
                Ok(Self::key(mods, code))
            }
            "mouse" => MouseFn::by_id(rest)
                .map(Self::mouse)
                .ok_or_else(|| Error::invalid(tr!("Unknown mouse action: {name}", name = rest))),
            "media" => Ok(Self::media(media_usage(rest)?)),
            "macro" => {
                let parts: Vec<&str> = rest.split(':').collect();
                let slot: u8 = parts[0].trim().parse().map_err(|_| {
                    Error::invalid(tr!("Unknown key action: {spec}", spec = spec))
                })?;
                let mut a = Self::macro_(slot, MacroMode::Once, 1);
                if let Some(mode) = parts.get(1) {
                    a.macro_mode = MacroMode::by_id(mode)
                        .ok_or_else(|| Error::invalid(tr!("Unknown macro mode: {name}", name = mode)))?;
                    if a.macro_mode == MacroMode::Repeat {
                        a.macro_count = parts.get(2).and_then(|c| c.trim().parse().ok()).unwrap_or(2);
                    }
                }
                Ok(a)
            }
            "multi" => {
                let keys = rest
                    .split(',')
                    .map(str::trim)
                    .filter(|k| !k.is_empty())
                    .take(3)
                    .map(key_code)
                    .collect::<Result<Vec<u8>>>()?;
                Ok(Self::multi(keys))
            }
            _ => Err(Error::invalid(tr!("Unknown key action: {spec}", spec = spec))),
        }
    }
}

/// The 576-byte table of one layer; keys missing from `actions` keep their default.
pub fn encode_layer(actions: &BTreeMap<u8, KeyAction>) -> [u8; TABLE_BYTES] {
    let mut t = [0u8; TABLE_BYTES];
    for (&idx, action) in actions {
        if (1..=MAX_INDEX).contains(&idx) {
            let i = idx as usize * 4;
            t[i..i + 4].copy_from_slice(&action.to_entry());
        }
    }
    t[574] = 0xAA;
    t[575] = 0x55;
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn key_entries_match_the_vendor_encoding() {
        for (spec, entry) in [
            ("key:ctrl+shift+a", "02 03 04 00"),
            ("key:ctrl", "02 01 00 00"),
            ("key:win+l", "02 08 0f 00"),
            ("mouse:wheeldown", "01 03 ff 00"),
            ("mouse:btn4", "01 01 10 00"),
            ("media:volup", "03 e9 00 00"),
            ("media:calc", "03 92 01 00"),
            ("macro:2:repeat:5", "06 02 01 05"),
            ("macro:1:hold", "06 01 03 00"),
            ("multi:a,b,c", "07 04 05 06"),
            ("off", "05 03 00 00"),
            ("default", "00 00 00 00"),
        ] {
            let a = KeyAction::parse(spec).unwrap();
            assert_eq!(hex(&a.to_entry()), entry, "{spec}");
            assert_eq!(KeyAction::parse(&a.to_spec()).unwrap().to_entry(), a.to_entry(), "{spec}");
        }
    }

    #[test]
    fn layer_table() {
        let mut m = BTreeMap::new();
        m.insert(1, KeyAction::off());
        m.insert(200, KeyAction::off());
        let t = encode_layer(&m);
        assert_eq!(&t[4..8], &[5, 3, 0, 0]);
        assert_eq!(&t[574..], &[0xAA, 0x55]);
        assert_eq!(t.iter().filter(|b| **b != 0).count(), 4);
    }

    #[test]
    fn json_matches_the_python_version() {
        let a = KeyAction::parse("macro:3:repeat:4").unwrap();
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["kind"], "macro");
        assert_eq!(v["macro"], 3);
        assert_eq!(v["macro_mode"], "repeat");
        assert_eq!(v["mouse"], "left");
        let from_python = r#"{"kind": "mouse", "mods": 0, "code": 0, "mouse": "wheelup", "usage": 0,
            "macro": 0, "macro_mode": "once", "macro_count": 1, "keys": []}"#;
        let b: KeyAction = serde_json::from_str(from_python).unwrap();
        assert_eq!(b.to_entry(), [1, 3, 1, 0]);
    }

    #[test]
    fn linux_keycodes() {
        assert_eq!(linux_to_hid(30), Some(0x04)); // KEY_A
        assert_eq!(linux_to_hid(183), Some(0x68)); // KEY_F13
        assert_eq!(linux_to_hid(500), None);
    }
}
