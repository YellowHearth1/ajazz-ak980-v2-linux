//! Colours: per-key colours stored in the keyboard, painted on the drawing.

use std::collections::BTreeSet;

use ak980_core::config::LightSource;
use ak980_core::device::WiredKeyboard;
use ak980_core::effects::hsv;
use ak980_core::i18n::{plural, tr};
use ak980_core::lighting::MAX_LEVEL;
use ak980_core::{layout, tr};
use iced::widget::{column, row};
use iced::{Element, Task};

use crate::app::{App, Message};
use crate::keyboard::{KeyEvent, Mode};
use crate::pages::lighting::{custom_preview, drawing};
use crate::widgets::{self, caption, card, color_picker, level, section, setting};

pub struct State {
    pub brush: [u8; 3],
    /// Where the gradient ends, on the right of the keyboard; the brush is where it starts.
    pub gradient_to: [u8; 3],
}

impl Default for State {
    fn default() -> Self {
        Self { brush: [0xFF, 0xD2, 0x00], gradient_to: [0x3A, 0x7B, 0xFF] }
    }
}

/// Groups of keys painted at once, by their HID codes, as the 0.0.1 window offered them.
const GROUPS: [(&str, &[u8]); 4] = [
    ("WASD", &[0x1A, 0x04, 0x16, 0x07]),
    ("Arrows", &[0x4F, 0x50, 0x51, 0x52]),
    ("F row", &[0x3A, 0x3B, 0x3C, 0x3D, 0x3E, 0x3F, 0x40, 0x41, 0x42, 0x43, 0x44, 0x45]),
    ("Numpad", &[0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x5B, 0x5C, 0x5D, 0x5E, 0x5F, 0x60, 0x61, 0x62,
                 0x63]),
];

#[derive(Debug, Clone)]
pub enum Msg {
    Key(KeyEvent),
    FillAll,
    Group(usize),
    Rainbow,
    Gradient,
    Clear,
    Brightness(u8),
    Send,
}

fn wrap(m: Msg) -> Message {
    Message::Colours(m)
}

/// Where a key sits from the left edge to the right one, 0 to 1.
fn across(k: &layout::LayoutKey) -> f32 {
    let keys = layout::keys();
    let lo = keys.iter().map(|k| k.x).min().unwrap_or(0) as f32;
    let hi = keys.iter().map(|k| k.x).max().unwrap_or(1) as f32;
    (k.x as f32 - lo) / (hi - lo).max(1.0)
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let undoable = match msg {
        Msg::Clear => Some(tr("Key colours cleared")),
        Msg::FillAll | Msg::Rainbow | Msg::Gradient => Some(tr("Every key painted")),
        _ => None,
    };
    if let Some(what) = undoable {
        app.checkpoint(what);
    }
    let (brush, to) = (app.colours.brush, app.colours.gradient_to);
    let p = app.cfg.profile_mut();
    match msg {
        Msg::Key(KeyEvent::Painted(i) | KeyEvent::Pressed(i)) => {
            p.custom_colors.insert(i, brush);
        }
        Msg::Key(KeyEvent::Erased(i)) => {
            p.custom_colors.remove(&i);
        }
        Msg::Key(KeyEvent::Picked(i)) => {
            if let Some(c) = p.custom_colors.get(&i).copied() {
                app.colours.brush = c;
                app.drafts.remove("color:paint:brush");
            }
            return Task::none();
        }
        Msg::FillAll => p.custom_colors = layout::keys().iter().map(|k| (k.index, brush)).collect(),
        Msg::Group(g) => {
            let codes = GROUPS.get(g).map(|(_, c)| *c).unwrap_or_default();
            for k in layout::keys().iter().filter(|k| codes.contains(&k.code)) {
                p.custom_colors.insert(k.index, brush);
            }
        }
        Msg::Rainbow => {
            for k in layout::keys() {
                p.custom_colors.insert(k.index, hsv(0.85 * across(k), 1.0, 1.0).map(|v| (v * 255.0) as u8));
            }
        }
        Msg::Gradient => {
            for k in layout::keys() {
                let t = across(k);
                let mix = |i: usize| (brush[i] as f32 + (to[i] as f32 - brush[i] as f32) * t).round() as u8;
                p.custom_colors.insert(k.index, [mix(0), mix(1), mix(2)]);
            }
        }
        Msg::Clear => p.custom_colors.clear(),
        Msg::Brightness(v) => p.custom_brightness = v,
        Msg::Send => {
            p.light_source = LightSource::Custom;
            let (colors, brightness) = (p.custom_colors.clone(), p.custom_brightness);
            let n = colors.len();
            app.save();
            return app.device_task(tr("Writing colours…"), move || {
                WiredKeyboard::open()?.set_custom_colors(&colors, brightness)?;
                Ok(tr!("Colours of {n} keys written.", n = n))
            });
        }
    }
    app.save();
    Task::none()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let p = app.cfg.profile();
    let n = p.custom_colors.len();
    let brush = app.colours.brush;
    let fills = row![
        widgets::secondary_button(tr("Fill every key"), Some(wrap(Msg::FillAll))),
        widgets::secondary_button(tr("Rainbow"), Some(wrap(Msg::Rainbow))),
        widgets::secondary_button(tr("Gradient"), Some(wrap(Msg::Gradient))),
        widgets::danger_button(tr("Clear"), (n > 0).then(|| wrap(Msg::Clear))),
    ]
    .spacing(8);
    let mut groups = row![].spacing(8);
    for (i, (name, _)) in GROUPS.iter().enumerate() {
        groups = groups.push(widgets::secondary_button(tr(name), Some(wrap(Msg::Group(i)))));
    }
    let rows = vec![
        setting(tr("Brush"), Some(tr("Click or drag over the keys. The right button takes a key's colour, the middle one clears it.")),
                color_picker("color:paint:brush".into(), brush, app.drafts.get("color:paint:brush"),
                             |c| Message::Draft("color:paint:brush".into(), widgets::html(c)))),
        setting(tr("Paint a group"), None, groups),
        setting(tr("Fill"), Some(tr("The gradient runs from the brush on the left to this colour on the right.")), fills),
        setting(tr("Gradient to"), None,
                color_picker("color:paint:to".into(), app.colours.gradient_to, app.drafts.get("color:paint:to"),
                             |c| Message::Draft("color:paint:to".into(), widgets::html(c)))),
        setting(tr("Brightness"), None, level(p.custom_brightness, MAX_LEVEL, |v| wrap(Msg::Brightness(v)))),
    ];
    let send = row![
        widgets::primary_button(tr("Write to keyboard"), app.wired().then(|| wrap(Msg::Send))),
        caption(plural(n as i64, "{n} key coloured", "{n} keys coloured")),
    ]
    .spacing(14)
    .align_y(iced::Alignment::Center);
    column![
        drawing(custom_preview(app), BTreeSet::new(), BTreeSet::new(), Mode::Paint, |e| wrap(Msg::Key(e))),
        section(tr("Colours"), card(rows)),
        send,
        caption(if p.light_source == LightSource::Custom {
            tr("The keyboard shows these colours.")
        } else {
            tr("Writing them makes the keyboard show these colours instead of its effect.")
        }),
    ]
    .spacing(20)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_group_finds_its_keys() {
        for (name, codes) in GROUPS {
            let found = layout::keys().iter().filter(|k| codes.contains(&k.code)).count();
            assert_eq!(found, codes.len(), "{name}");
        }
    }
}
