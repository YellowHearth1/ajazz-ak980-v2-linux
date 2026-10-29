//! The AK980 V2 key layout (from the vendor's layouts/rgb-keyboard.xml, via ak980-studio).

use std::sync::OnceLock;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::keymap::{key_code, key_name};
use crate::tr;

/// Where the 1.14" screen sits (above the numpad), in layout units: x, y, w, h of the bezel.
pub const SCREEN_RECT: (f32, f32, f32, f32) = (648.0, 36.0, 72.0, 41.0);
pub const KNOB_CENTER: (f32, f32) = (757.5, 56.5);
pub const KNOB_RADIUS: f32 = 17.0;
pub const CASE_RECT: (f32, f32, f32, f32) = (8.0, 20.0, 785.0, 268.0);

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct LayoutKey {
    #[serde(rename = "name")]
    pub label: String,
    /// LED index = slot in the remap and colour tables.
    #[serde(rename = "led")]
    pub index: u8,
    /// Default HID usage.
    pub code: u8,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl LayoutKey {
    /// Unique name for the command line (from the HID usage: LeftShift, Num7, ...).
    pub fn id(&self) -> String {
        key_name(self.code)
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x as f32 + self.w as f32 / 2.0, self.y as f32 + self.h as f32 / 2.0)
    }
}

#[derive(Deserialize)]
struct LayoutFile {
    width: i32,
    height: i32,
    keys: Vec<LayoutKey>,
}

fn file() -> &'static LayoutFile {
    static LAYOUT: OnceLock<LayoutFile> = OnceLock::new();
    LAYOUT.get_or_init(|| {
        serde_json::from_str(include_str!("../../../ak980/data/layout.json")).expect("valid layout.json")
    })
}

/// Width and height of the layout in its own units.
pub fn size() -> (i32, i32) {
    (file().width, file().height)
}

pub fn keys() -> &'static [LayoutKey] {
    &file().keys
}

pub fn by_index(index: u8) -> Option<&'static LayoutKey> {
    keys().iter().find(|k| k.index == index)
}

pub fn by_code(code: u8) -> Option<&'static LayoutKey> {
    keys().iter().find(|k| k.code == code)
}

/// By LED index, id ("LeftShift"), printed label ("~`") or key name ("esc").
pub fn find_key(name: &str) -> Result<&'static LayoutKey> {
    if let Ok(i) = name.parse::<u8>()
        && let Some(k) = by_index(i) {
            return Ok(k);
        }
    let low = name.to_lowercase();
    if let Some(k) = keys().iter().find(|k| k.id().to_lowercase() == low) {
        return Ok(k);
    }
    if let Some(k) = keys().iter().find(|k| k.label.to_lowercase() == low) {
        return Ok(k);
    }
    if let Ok(code) = key_code(name)
        && let Some(k) = by_code(code) {
            return Ok(k);
        }
    Err(Error::invalid(tr!("There is no key “{name}” on the AK980 (see `ak980 keys`)", name = name)))
}

/// LED indices and key centres normalised to 0..1, for the effects.
pub fn positions() -> &'static (Vec<u8>, Vec<[f32; 2]>) {
    static POS: OnceLock<(Vec<u8>, Vec<[f32; 2]>)> = OnceLock::new();
    POS.get_or_init(|| {
        let ks = keys();
        let xs: Vec<f32> = ks.iter().map(|k| k.center().0).collect();
        let ys: Vec<f32> = ks.iter().map(|k| k.center().1).collect();
        let (xmin, xmax) = xs.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        let (ymin, ymax) = ys.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        let pos = xs.iter().zip(&ys).map(|(x, y)| [(x - xmin) / (xmax - xmin), (y - ymin) / (ymax - ymin)]).collect();
        (ks.iter().map(|k| k.index).collect(), pos)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_complete() {
        let ks = keys();
        assert_eq!(ks.len(), 97);
        let mut idx: Vec<u8> = ks.iter().map(|k| k.index).collect();
        idx.sort();
        idx.dedup();
        assert_eq!(idx.len(), 97, "LED indices are unique");
        assert!(ks.iter().all(|k| k.index >= 1 && k.index <= crate::keymap::MAX_INDEX));
        assert_eq!(find_key("esc").unwrap().index, 1);
        assert_eq!(find_key("Delete").unwrap().index, 119);
        let (leds, pos) = positions();
        assert_eq!(leds.len(), pos.len());
        assert!(pos.iter().all(|p| (0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1])));
    }
}
