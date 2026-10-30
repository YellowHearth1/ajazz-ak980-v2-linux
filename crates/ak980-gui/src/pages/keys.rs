//! Keys: what each key does on the base layer and while Fn is held.

use std::collections::BTreeSet;

use ak980_core::device::WiredKeyboard;
use ak980_core::i18n::tr;
use ak980_core::keymap::{hid_keys, ActionKind, KeyAction, MacroMode, MouseFn, MEDIA_KEYS, MOD_ALT, MOD_CTRL, MOD_SHIFT,
                         MOD_WIN};
use ak980_core::{layout, tr};
use iced::widget::{button, checkbox, column, pick_list, row, text};
use iced::{Alignment, Element, Fill, Task};

use crate::app::{App, Message};
use crate::keyboard::{KeyEvent, Mode};
use crate::pages::lighting::drawing;
use crate::theme;
use crate::widgets::{self, caption, card, section, segmented, setting, Choice};

#[derive(Default)]
pub struct State {
    pub selected: Option<u8>,
    pub fn_layer: bool,
    pub draft: KeyAction,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Key(KeyEvent),
    Layer(bool),
    Kind(ActionKind),
    Modifier(u8, bool),
    Code(Choice<u8>),
    Mouse(Choice<MouseFn>),
    Media(Choice<u16>),
    MacroSlot(Choice<u8>),
    MacroMode(Choice<MacroMode>),
    MacroCount(u8),
    Multi(usize, Choice<u8>),
    Apply,
    ResetKey(u8),
    ResetLayer,
    Write,
}

fn wrap(m: Msg) -> Message {
    Message::Keys(m)
}

fn key_name(index: u8) -> String {
    layout::by_index(index).map(|k| k.label.clone()).unwrap_or_else(|| index.to_string())
}

fn load_draft(app: &mut App) {
    let fn_layer = app.keys.fn_layer;
    app.keys.draft = app
        .keys
        .selected
        .and_then(|k| app.cfg.profile().layer(fn_layer).get(&k).cloned())
        .unwrap_or_default();
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let d = &mut app.keys.draft;
    match msg {
        Msg::Key(KeyEvent::Pressed(i) | KeyEvent::Painted(i)) => {
            app.keys.selected = Some(i);
            load_draft(app);
        }
        Msg::Key(KeyEvent::Erased(_)) => {}
        Msg::Layer(fn_layer) => {
            app.keys.fn_layer = fn_layer;
            load_draft(app);
        }
        Msg::Kind(kind) => {
            d.kind = kind;
            if kind == ActionKind::Multi && d.keys.is_empty() {
                d.keys = vec![0x04];
            }
        }
        Msg::Modifier(bit, on) => {
            if on { d.mods |= bit } else { d.mods &= !bit }
        }
        Msg::Code(c) => d.code = c.value,
        Msg::Mouse(m) => d.mouse = m.value,
        Msg::Media(m) => d.usage = m.value,
        Msg::MacroSlot(m) => d.macro_slot = m.value,
        Msg::MacroMode(m) => d.macro_mode = m.value,
        Msg::MacroCount(n) => d.macro_count = n.max(1),
        Msg::Multi(i, c) => {
            if c.value == 0 {
                if i < d.keys.len() {
                    d.keys.remove(i);
                }
            } else if i < d.keys.len() {
                d.keys[i] = c.value;
            } else {
                d.keys.push(c.value);
            }
        }
        Msg::Apply => {
            let Some(key) = app.keys.selected else { return Task::none() };
            let action = app.keys.draft.clone();
            let fn_layer = app.keys.fn_layer;
            let layer = app.cfg.profile_mut().layer_mut(fn_layer);
            if action.is_default() { layer.remove(&key); } else { layer.insert(key, action); }
            app.save();
        }
        Msg::ResetKey(key) => {
            let fn_layer = app.keys.fn_layer;
            app.cfg.profile_mut().layer_mut(fn_layer).remove(&key);
            app.save();
            load_draft(app);
        }
        Msg::ResetLayer => {
            let fn_layer = app.keys.fn_layer;
            app.checkpoint(if fn_layer { tr("Fn layer reset") } else { tr("Base layer reset") });
            app.cfg.profile_mut().layer_mut(fn_layer).clear();
            app.save();
            load_draft(app);
        }
        Msg::Write => {
            let fn_layer = app.keys.fn_layer;
            let layer = app.cfg.profile().layer(fn_layer).clone();
            return app.device_task(tr("Writing keys…"), move || {
                WiredKeyboard::open()?.set_keymap(fn_layer, &layer)?;
                Ok(if fn_layer { tr("Fn layer written") } else { tr("Base layer written") })
            });
        }
    }
    Task::none()
}

fn key_choices() -> Vec<Choice<u8>> {
    hid_keys().iter().map(|(n, c)| Choice::new(*c, n.clone())).collect()
}

fn editor(app: &App) -> Element<'_, Message> {
    let Some(key) = app.keys.selected else {
        return card(vec![setting(tr("No key selected"), Some(tr("Click a key on the keyboard above.")), text(""))]);
    };
    let d = &app.keys.draft;
    let kinds = vec![
        (ActionKind::Default, tr("Default")),
        (ActionKind::Key, tr("Key or shortcut")),
        (ActionKind::Mouse, tr("Mouse")),
        (ActionKind::Media, tr("Media")),
        (ActionKind::Macro, tr("Macro")),
        (ActionKind::Multi, tr("Several keys")),
        (ActionKind::Off, tr("Disabled")),
    ];
    let mut rows = vec![
        column![widgets::heading(tr!("Key {name}", name = key_name(key))), segmented(kinds, d.kind, |k| wrap(Msg::Kind(k)))]
            .spacing(10)
            .into(),
    ];
    let pick = |items: Vec<Choice<u8>>, current: u8, f: fn(Choice<u8>) -> Msg| {
        let cur = items.iter().find(|c| c.value == current).cloned();
        pick_list(items, cur, move |c| wrap(f(c))).style(theme::pick).menu_style(theme::menu).width(200)
    };
    match d.kind {
        ActionKind::Default => rows.push(caption(tr("The key does what is printed on it.")).into()),
        ActionKind::Off => rows.push(caption(tr("The key does nothing.")).into()),
        ActionKind::Key => {
            let mut mods = row![].spacing(14);
            for (bit, name) in [(MOD_CTRL, "Ctrl"), (MOD_SHIFT, "Shift"), (MOD_ALT, "Alt"), (MOD_WIN, "Win")] {
                mods = mods.push(checkbox(d.mods & bit != 0).label(name).on_toggle(move |on| wrap(Msg::Modifier(bit, on)))
                    .style(theme::check));
            }
            let mut keys = key_choices();
            keys.insert(0, Choice::new(0, tr("(only the modifiers)")));
            rows.push(setting(tr("Modifiers"), None, mods));
            rows.push(setting(tr("Key"), None, pick(keys, d.code, Msg::Code)));
        }
        ActionKind::Mouse => {
            let items: Vec<Choice<MouseFn>> = MouseFn::ALL.iter().map(|m| Choice::new(*m, tr(m.label()))).collect();
            let cur = items.iter().find(|c| c.value == d.mouse).cloned();
            rows.push(setting(tr("Mouse"), None, pick_list(items, cur, |c| wrap(Msg::Mouse(c))).style(theme::pick)
                .menu_style(theme::menu).width(200)));
        }
        ActionKind::Media => {
            let items: Vec<Choice<u16>> = MEDIA_KEYS.iter().map(|(n, u)| Choice::new(*u, tr(n))).collect();
            let cur = items.iter().find(|c| c.value == d.usage).cloned();
            rows.push(setting(tr("Media"), None, pick_list(items, cur, |c| wrap(Msg::Media(c))).style(theme::pick)
                .menu_style(theme::menu).width(200)));
        }
        ActionKind::Macro => {
            let names = app.cfg.profile().macro_names();
            if names.is_empty() {
                rows.push(caption(tr("No macros yet: record one on the Macros page.")).into());
            } else {
                let items: Vec<Choice<u8>> = names.iter().enumerate().map(|(i, n)| Choice::new(i as u8, n.clone())).collect();
                rows.push(setting(tr("Macro"), None, pick(items, d.macro_slot, Msg::MacroSlot)));
                let modes: Vec<Choice<MacroMode>> = MacroMode::ALL.iter().map(|m| Choice::new(*m, tr(m.label()))).collect();
                let cur = modes.iter().find(|c| c.value == d.macro_mode).cloned();
                rows.push(setting(tr("Play"), None, pick_list(modes, cur, |c| wrap(Msg::MacroMode(c))).style(theme::pick)
                    .menu_style(theme::menu).width(200)));
                if d.macro_mode == MacroMode::Repeat {
                    rows.push(setting(tr("Times"), None, widgets::ranged(d.macro_count as i32, 1, 99, "",
                                                                          |v| wrap(Msg::MacroCount(v as u8)))));
                }
            }
        }
        ActionKind::Multi => {
            let mut r = row![].spacing(8);
            for i in 0..3 {
                let mut items = key_choices();
                items.insert(0, Choice::new(0, tr("(nothing)")));
                let current = d.keys.get(i).copied().unwrap_or(0);
                let cur = items.iter().find(|c| c.value == current).cloned();
                r = r.push(pick_list(items, cur, move |c| wrap(Msg::Multi(i, c))).style(theme::pick)
                    .menu_style(theme::menu).width(130));
            }
            rows.push(setting(tr("Several keys"), Some(tr("Pressed together, up to three")), r));
        }
    }
    let names = app.cfg.profile().macro_names();
    rows.push(
        row![
            caption(tr!("Sent as {hex}", hex = d.to_entry().iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "))),
            iced::widget::space().width(Fill),
            caption(d.describe(&names)),
            widgets::primary_button(tr!("Apply to {key}", key = key_name(key)), Some(wrap(Msg::Apply))),
        ]
        .spacing(12)
        .align_y(Alignment::Center)
        .into(),
    );
    card(rows)
}

pub fn view(app: &App) -> Element<'_, Message> {
    let fn_layer = app.keys.fn_layer;
    let p = app.cfg.profile();
    let layer = p.layer(fn_layer);
    let marked: BTreeSet<u8> = layer.keys().copied().collect();
    let selected: BTreeSet<u8> = app.keys.selected.into_iter().collect();
    let names = p.macro_names();
    let mut changed = vec![];
    for (key, action) in layer {
        changed.push(
            row![
                text(key_name(*key)).size(theme::TEXT).width(120),
                text(action.describe(&names)).size(theme::TEXT).width(Fill),
                button(text(tr("Reset key")).size(theme::SMALL)).style(theme::flat).padding([4, 8])
                    .on_press(wrap(Msg::ResetKey(*key))),
            ]
            .align_y(Alignment::Center)
            .into(),
        );
    }
    if changed.is_empty() {
        changed.push(caption(tr("Every key does what is printed on it")).into());
    }
    let actions = row![
        widgets::primary_button(tr("Write layer to keyboard"), app.wired().then(|| wrap(Msg::Write))),
        widgets::danger_button(tr("Reset layer"), (!layer.is_empty()).then(|| wrap(Msg::ResetLayer))),
    ]
    .spacing(10);
    column![
        drawing(Default::default(), selected, marked, Mode::Select, |e| wrap(Msg::Key(e))),
        row![
            segmented(vec![(false, tr("Base layer")), (true, tr("Fn layer"))], fn_layer, |f| wrap(Msg::Layer(f))),
            caption(tr("The Fn layer is what a key does while Fn is held")),
        ]
        .spacing(14)
        .align_y(Alignment::Center),
        editor(app),
        section(tr("Changed keys"), card(changed)),
        actions,
    ]
    .spacing(20)
    .into()
}
