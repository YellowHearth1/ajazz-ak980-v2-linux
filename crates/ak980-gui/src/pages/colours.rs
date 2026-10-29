//! Colours: per-key colours stored in the keyboard, painted on the drawing.

use std::collections::BTreeSet;

use ak980_core::config::LightSource;
use ak980_core::device::WiredKeyboard;
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
}

impl Default for State {
    fn default() -> Self {
        Self { brush: [0xFF, 0xD2, 0x00] }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Key(KeyEvent),
    FillAll,
    Wasd,
    Clear,
    Brightness(u8),
    Send,
}

fn wrap(m: Msg) -> Message {
    Message::Colours(m)
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let brush = app.colours.brush;
    let p = app.cfg.profile_mut();
    match msg {
        Msg::Key(KeyEvent::Painted(i) | KeyEvent::Pressed(i)) => {
            p.custom_colors.insert(i, brush);
        }
        Msg::Key(KeyEvent::Erased(i)) => {
            p.custom_colors.remove(&i);
        }
        Msg::FillAll => p.custom_colors = layout::keys().iter().map(|k| (k.index, brush)).collect(),
        Msg::Wasd => {
            for name in ["W", "A", "S", "D"] {
                if let Ok(k) = layout::find_key(name) {
                    p.custom_colors.insert(k.index, brush);
                }
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
    let tools = row![
        widgets::secondary_button(tr("Fill every key"), Some(wrap(Msg::FillAll))),
        widgets::secondary_button(tr("WASD"), Some(wrap(Msg::Wasd))),
        widgets::danger_button(tr("Clear"), (n > 0).then(|| wrap(Msg::Clear))),
    ]
    .spacing(8);
    let rows = vec![
        setting(tr("Brush"), Some(tr("Click or drag over the keys; the right button clears them.")),
                color_picker("color:paint:brush".into(), brush, app.drafts.get("color:paint:brush"),
                             |c| Message::Draft("color:paint:brush".into(), widgets::html(c)))),
        setting(tr("Paint a group"), None, tools),
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
