//! Firmware lighting effects (opcode 0x13) and keyboard options (opcode 0x17).

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::i18n::tr;
use crate::tr;

pub const MAX_LEVEL: u8 = 5;

// What an effect responds to (the config_func column in the vendor driver's database)
pub const CAP_BRIGHTNESS: u8 = 0x01;
pub const CAP_SPEED: u8 = 0x02;
pub const CAP_DIRECTION2: u8 = 0x04;
pub const CAP_DIRECTION4: u8 = 0x08;
pub const CAP_COLOR: u8 = 0x10;
pub const CAP_RAINBOW: u8 = 0x20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightMode {
    pub id: u8,
    /// English name from the vendor's language/1033.lan; translate with `tr`.
    pub name: &'static str,
    pub caps: u8,
}

impl LightMode {
    pub fn has(&self, cap: u8) -> bool {
        self.caps & cap != 0
    }
}

/// Firmware order, then "Off".
pub const LIGHT_MODES: [LightMode; 20] = [
    LightMode { id: 1, name: "Static", caps: 0x31 },
    LightMode { id: 2, name: "Single on", caps: 0x33 },
    LightMode { id: 3, name: "Single off", caps: 0x33 },
    LightMode { id: 4, name: "Glittering", caps: 0x33 },
    LightMode { id: 5, name: "Fluttering", caps: 0x33 },
    LightMode { id: 6, name: "Colourful", caps: 0x03 },
    LightMode { id: 7, name: "Breath", caps: 0x33 },
    LightMode { id: 8, name: "Spectrum", caps: 0x03 },
    LightMode { id: 9, name: "Outward", caps: 0x33 },
    LightMode { id: 10, name: "Scrolling", caps: 0x3B },
    LightMode { id: 11, name: "Rolling", caps: 0x37 },
    LightMode { id: 12, name: "Rotating", caps: 0x37 },
    LightMode { id: 13, name: "Explode", caps: 0x33 },
    LightMode { id: 14, name: "Launch", caps: 0x33 },
    LightMode { id: 15, name: "Ripples", caps: 0x33 },
    LightMode { id: 16, name: "Flowing", caps: 0x37 },
    LightMode { id: 17, name: "Pulsating", caps: 0x33 },
    LightMode { id: 18, name: "Tilt", caps: 0x37 },
    LightMode { id: 19, name: "Shuttle", caps: 0x33 },
    LightMode { id: 0, name: "Off", caps: 0 },
];

pub const DIRECTIONS: [&str; 4] = ["Left", "Down", "Up", "Right"];

/// The "side light" modes of the vendor driver (hidden in its UI, no visible effect on the AK980).
pub const SIDE_LIGHT_MODES: [(&str, u8); 10] = [
    ("flowing", 0x20), ("red", 0x21), ("yellow", 0x22), ("green", 0x23), ("iceblue", 0x24),
    ("blue", 0x25), ("pink", 0x26), ("white", 0x27), ("neon", 0x28), ("off", 0x29),
];

/// Factory default from the vendor's config.xml (`<light default_mode="11" .../>`).
pub const FACTORY_MODE: u8 = 11;

pub fn mode_by_id(id: u8) -> LightMode {
    LIGHT_MODES.iter().copied().find(|m| m.id == id).unwrap_or(LIGHT_MODES[0])
}

/// By number, English name or translated name ("single on", "2", "Одна натиснута").
pub fn mode_by_name(name: &str) -> Result<LightMode> {
    let squash = |s: &str| s.to_lowercase().replace([' ', '-'], "");
    let key = squash(name.trim());
    LIGHT_MODES
        .iter()
        .copied()
        .find(|m| key == m.id.to_string() || key == squash(m.name) || key == squash(&tr(m.name)))
        .ok_or_else(|| Error::invalid(tr!("Unknown lighting effect: {name}", name = name)))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LightSettings {
    pub mode: u8,
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub rainbow: bool,
    pub brightness: u8,
    pub speed: u8,
    pub direction: u8,
}

impl Default for LightSettings {
    fn default() -> Self {
        Self { mode: FACTORY_MODE, r: 0xFF, g: 0x8F, b: 0x1F, rainbow: false, brightness: MAX_LEVEL, speed: 3,
               direction: 0 }
    }
}

impl LightSettings {
    /// `MODE R G B .. .. .. .. RAINBOW BRIGHT SPEED DIR .. .. AA 55`
    pub fn to_packet(&self) -> [u8; 64] {
        let mut p = [0u8; 64];
        p[0] = self.mode;
        if self.mode != 0 {
            p[1] = self.r;
            p[2] = self.g;
            p[3] = self.b;
            p[8] = self.rainbow as u8;
            p[9] = self.brightness.min(MAX_LEVEL);
            p[10] = self.speed.min(MAX_LEVEL);
            p[11] = self.direction;
        }
        p[14] = 0xAA;
        p[15] = 0x55;
        p
    }

    pub fn rgb(&self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }

    pub fn set_rgb(&mut self, c: [u8; 3]) {
        [self.r, self.g, self.b] = c;
    }
}

pub const SLEEP_CHOICES: [&str; 4] = ["Never", "1 min", "5 min", "30 min"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyboardOptions {
    pub disable_alt_tab: bool,
    pub disable_alt_f4: bool,
    pub disable_win: bool,
    pub fn_switch: bool,
    /// 0 never, 1 = 1 min, 2 = 5 min, 3 = 30 min
    pub sleep_time: u8,
    /// debounce level 1..5
    pub key_response: u8,
}

impl Default for KeyboardOptions {
    fn default() -> Self {
        Self { disable_alt_tab: false, disable_alt_f4: false, disable_win: false, fn_switch: false, sleep_time: 2,
               key_response: 2 }
    }
}

impl KeyboardOptions {
    /// `00 01 ALTTAB ALTF4 WIN FNSWAP SLEEP 00 RESPONSE … [62]=AA [63]=55`
    pub fn to_packet(&self) -> [u8; 64] {
        let mut p = [0u8; 64];
        p[1] = 0x01;
        p[2] = self.disable_alt_tab as u8;
        p[3] = self.disable_alt_f4 as u8;
        p[4] = self.disable_win as u8;
        p[5] = self.fn_switch as u8;
        p[6] = self.sleep_time;
        p[8] = self.key_response;
        p[62] = 0xAA;
        p[63] = 0x55;
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighting_packet() {
        let s = LightSettings { mode: 7, r: 1, g: 2, b: 3, brightness: 4, speed: 2, direction: 3, rainbow: false };
        let p = s.to_packet();
        assert_eq!(&p[..4], &[7, 1, 2, 3]);
        assert_eq!(&p[9..12], &[4, 2, 3]);
        assert_eq!(&p[14..16], &[0xAA, 0x55]);
        let off = LightSettings { mode: 0, ..s }.to_packet();
        assert_eq!(&off[..12], &[0; 12]);
    }

    #[test]
    fn options_packet() {
        let o = KeyboardOptions { disable_win: true, sleep_time: 3, key_response: 4, ..Default::default() };
        let p = o.to_packet();
        assert_eq!((p[1], p[4], p[6], p[8]), (1, 1, 3, 4));
        assert_eq!(&p[62..], &[0xAA, 0x55]);
    }

    #[test]
    fn modes_by_name() {
        assert_eq!(mode_by_name("single on").unwrap().id, 2);
        assert_eq!(mode_by_name("19").unwrap().name, "Shuttle");
        assert!(mode_by_name("nonsense").is_err());
    }
}
