//! Sending a profile to the keyboard.

use crate::config::{LightSource, Profile};
use crate::device::WiredKeyboard;
use crate::error::Result;
use crate::i18n::tr;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Lighting,
    Options,
    Macros,
    Keys,
}

pub const ALL_PARTS: [Part; 4] = [Part::Lighting, Part::Options, Part::Macros, Part::Keys];

/// What the keyboard shows on its own: the firmware effect or the stored per-key colours.
///
/// For a PC effect this is what remains when the stream stops.
pub fn apply_lighting(kb: &mut WiredKeyboard, p: &Profile) -> Result<()> {
    if p.light_source == LightSource::Custom {
        kb.set_custom_colors(&p.custom_colors, p.custom_brightness)
    } else {
        kb.set_lighting(&p.lighting)
    }
}

pub fn apply_profile(kb: &mut WiredKeyboard, p: &Profile, parts: &[Part], step: &mut dyn FnMut(String)) -> Result<()> {
    kb.locked(|kb| {
        if parts.contains(&Part::Lighting) {
            step(tr("Lighting"));
            apply_lighting(kb, p)?;
        }
        if parts.contains(&Part::Options) {
            step(tr("Keyboard options"));
            kb.set_options(&p.options)?;
        }
        if parts.contains(&Part::Macros) {
            step(tr("Macros"));
            kb.set_macros(&p.macros)?;
        }
        if parts.contains(&Part::Keys) {
            step(tr("Key layers"));
            kb.set_keymap(false, &p.keymap_top)?;
            kb.set_keymap(true, &p.keymap_fn)?;
        }
        Ok(())
    })
}
