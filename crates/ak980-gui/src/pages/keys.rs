//! Keys: what each key does on the base layer and while Fn is held.

use std::collections::BTreeSet;

use ak980_core::config::{Config, Launcher};
use ak980_core::device::WiredKeyboard;
use ak980_core::i18n::tr;
use ak980_core::keymap::{hid_keys, key_code, ActionKind, KeyAction, MacroMode, MouseFn, LAUNCHER_KEYS, MEDIA_KEYS,
                         MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_WIN};
use ak980_core::{layout, tr};
use iced::widget::{button, checkbox, column, pick_list, row, text, text_input};
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
    /// The command of a "Program or link" key being edited; `None` for every other kind.
    pub launch: Option<String>,
}

/// The kinds offered: the keyboard's own, and a program or link, which is a spare F13–F24 key
/// that the agent turns into the program.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Action(ActionKind),
    Launch,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Key(KeyEvent),
    Layer(bool),
    Kind(Kind),
    Command(String),
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
    app.keys.launch = launcher_for(&app.cfg, &app.keys.draft).map(|l| l.command.clone());
}

/// The launcher a key action stands for: a spare F13–F24 key, pressed alone, that has a command.
fn launcher_for<'a>(cfg: &'a Config, a: &KeyAction) -> Option<&'a Launcher> {
    if a.kind != ActionKind::Key || a.mods != 0 {
        return None;
    }
    let name = hid_keys().iter().find(|(_, c)| *c == a.code).map(|(n, _)| n)?;
    cfg.settings.launchers.iter().find(|l| l.key == *name)
}

/// What a key does, as the list of changed keys shows it.
fn describe(app: &App, a: &KeyAction, names: &[String]) -> String {
    match launcher_for(&app.cfg, a) {
        Some(l) => tr!("Run: {cmd}", cmd = l.command),
        None => a.describe(names),
    }
}

/// The launcher key for `command`: the one this key had, or the first free one of F13–F24.
fn launcher_key(app: &mut App, key: u8, command: String) -> Option<String> {
    let old = app.cfg.profile().layer(app.keys.fn_layer).get(&key).cloned().unwrap_or_default();
    let launchers = &mut app.cfg.settings.launchers;
    let reused = hid_keys().iter().find(|(_, c)| *c == old.code && old.kind == ActionKind::Key && old.mods == 0)
        .and_then(|(n, _)| launchers.iter_mut().find(|l| l.key == *n));
    if let Some(l) = reused {
        l.command = command;
        return Some(l.key.clone());
    }
    let free = LAUNCHER_KEYS.iter().find(|k| !launchers.iter().any(|l| l.key == **k))?.to_string();
    launchers.push(Launcher { key: free.clone(), command, label: String::new() });
    Some(free)
}

/// Launchers that no key of any profile sends any more go, so their F-keys are free again.
fn drop_unused_launchers(cfg: &mut Config) {
    let used: Vec<u8> = cfg.profiles.iter()
        .flat_map(|p| p.keymap_top.values().chain(p.keymap_fn.values()))
        .filter(|a| a.kind == ActionKind::Key && a.mods == 0)
        .map(|a| a.code)
        .collect();
    cfg.settings.launchers.retain(|l| key_code(&l.key).is_ok_and(|c| used.contains(&c)));
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let d = &mut app.keys.draft;
    match msg {
        Msg::Key(KeyEvent::Pressed(i) | KeyEvent::Painted(i)) => {
            app.keys.selected = Some(i);
            load_draft(app);
        }
        Msg::Key(KeyEvent::Erased(_) | KeyEvent::Picked(_)) => {}
        Msg::Layer(fn_layer) => {
            app.keys.fn_layer = fn_layer;
            load_draft(app);
        }
        Msg::Kind(Kind::Launch) => {
            app.keys.launch.get_or_insert_default();
        }
        Msg::Kind(Kind::Action(kind)) => {
            app.keys.launch = None;
            d.kind = kind;
            if kind == ActionKind::Multi && d.keys.is_empty() {
                d.keys = vec![0x04];
            }
        }
        Msg::Command(c) => app.keys.launch = Some(c),
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
            let mut action = app.keys.draft.clone();
            if let Some(command) = app.keys.launch.clone() {
                let command = command.trim().to_string();
                if command.is_empty() {
                    return Task::none();
                }
                let Some(fkey) = launcher_key(app, key, command) else {
                    app.notify(tr("All twelve launcher keys (F13–F24) are in use."), true);
                    return Task::none();
                };
                action = KeyAction::key(0, key_code(&fkey).unwrap_or_default());
            }
            let fn_layer = app.keys.fn_layer;
            let layer = app.cfg.profile_mut().layer_mut(fn_layer);
            if action.is_default() { layer.remove(&key); } else { layer.insert(key, action); }
            drop_unused_launchers(&mut app.cfg);
            app.save();
            load_draft(app);
        }
        Msg::ResetKey(key) => {
            let fn_layer = app.keys.fn_layer;
            app.cfg.profile_mut().layer_mut(fn_layer).remove(&key);
            drop_unused_launchers(&mut app.cfg);
            app.save();
            load_draft(app);
        }
        Msg::ResetLayer => {
            let fn_layer = app.keys.fn_layer;
            app.checkpoint(if fn_layer { tr("Fn layer reset") } else { tr("Base layer reset") });
            app.cfg.profile_mut().layer_mut(fn_layer).clear();
            drop_unused_launchers(&mut app.cfg);
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
        (Kind::Action(ActionKind::Default), tr("Default")),
        (Kind::Action(ActionKind::Key), tr("Key or shortcut")),
        (Kind::Action(ActionKind::Mouse), tr("Mouse")),
        (Kind::Action(ActionKind::Media), tr("Media")),
        (Kind::Action(ActionKind::Macro), tr("Macro")),
        (Kind::Action(ActionKind::Multi), tr("Several keys")),
        (Kind::Launch, tr("Program or link")),
        (Kind::Action(ActionKind::Off), tr("Disabled")),
    ];
    let kind = if app.keys.launch.is_some() { Kind::Launch } else { Kind::Action(d.kind) };
    let mut rows = vec![
        column![widgets::heading(tr!("Key {name}", name = key_name(key))), segmented(kinds, kind, |k| wrap(Msg::Kind(k)))]
            .spacing(10)
            .into(),
    ];
    let pick = |items: Vec<Choice<u8>>, current: u8, f: fn(Choice<u8>) -> Msg| {
        let cur = items.iter().find(|c| c.value == current).cloned();
        pick_list(items, cur, move |c| wrap(f(c))).style(theme::pick).menu_style(theme::menu).width(200)
    };
    match d.kind {
        _ if kind == Kind::Launch => {
            let command = app.keys.launch.clone().unwrap_or_default();
            rows.push(setting(tr("Run"), None, text_input(&tr("e.g. steam, obs or https://…"), &command)
                .on_input(|t| wrap(Msg::Command(t))).on_submit(wrap(Msg::Apply)).padding(6).style(theme::input).width(320)));
            rows.push(caption(tr("The key sends a spare F13–F24 code, and the background agent runs the program or opens the link. It works while the agent runs.")).into());
        }
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
    // A program's F-key is only picked when it is applied, so its bytes are not known before
    let (sent, what, ready) = match &app.keys.launch {
        Some(c) => (String::new(), tr!("Run: {cmd}", cmd = c.trim()), !c.trim().is_empty()),
        None => (tr!("Sent as {hex}", hex = d.to_entry().iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")),
                 d.describe(&names), true),
    };
    rows.push(
        row![
            caption(sent),
            iced::widget::space().width(Fill),
            caption(what),
            widgets::primary_button(tr!("Apply to {key}", key = key_name(key)), ready.then(|| wrap(Msg::Apply))),
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
                text(describe(app, action, &names)).size(theme::TEXT).width(Fill),
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
