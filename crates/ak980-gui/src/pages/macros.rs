//! Macros: the keyboard's macro memory, edited step by step.

use std::time::Instant;

use ak980_core::device::WiredKeyboard;
use ak980_core::i18n::{plural, tr};
use ak980_core::keymap::hid_keys;
use ak980_core::macros::{self, Macro, MacroStep, StepAction, BUDGET_BYTES, MAX_MACROS, MIN_DELAY, MOUSE_BUTTONS};
use ak980_core::tr;
use iced::keyboard::{self, key::{Code, Physical}};
use iced::widget::{button, column, container, pick_list, progress_bar, row, text, text_input, toggler};
use iced::{Alignment, Element, Fill, Subscription, Task};

use crate::app::{App, Message};
use crate::fonts;
use crate::theme;
use crate::widgets::{self, caption, card, section, setting, Choice};

pub struct State {
    pub selected: Option<usize>,
    pub key: Option<Choice<u8>>,
    pub button: Option<Choice<u8>>,
    pub text: String,
    pub recording: bool,
    /// Record the pauses between key events as they were, or the shortest one.
    pub real_pauses: bool,
    /// The last key event, added once the next one shows how long it lasted.
    pending: Option<(MacroStep, Instant)>,
}

impl Default for State {
    fn default() -> Self {
        Self { selected: None, key: None, button: None, text: String::new(), recording: false, real_pauses: true,
               pending: None }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Select(usize),
    New,
    Delete,
    Record(bool),
    RealPauses(bool),
    /// A key went down (`true`) or up while recording: its HID code.
    Recorded(bool, u8),
    Name(String),
    PickKey(Choice<u8>),
    AddKey,
    PickButton(Choice<u8>),
    AddClick,
    Text(String),
    AddText,
    Up(usize),
    Down(usize),
    Remove(usize),
    Write,
}

fn wrap(m: Msg) -> Message {
    Message::Macros(m)
}

pub fn set_delay(app: &mut App, step: usize, ms: u16) -> Task<Message> {
    if let Some(i) = app.macros.selected
        && let Some(s) = app.cfg.profile_mut().macros.get_mut(i).and_then(|m| m.steps.get_mut(step)) {
            s.delay = ms.max(MIN_DELAY);
            app.save();
        }
    Task::none()
}

/// Key presses and releases in the window while recording.
pub fn subscription(app: &App) -> Subscription<Message> {
    if !app.macros.recording {
        return Subscription::none();
    }
    keyboard::listen().filter_map(|e| {
        let (down, physical) = match e {
            keyboard::Event::KeyPressed { physical_key, repeat: false, .. } => (true, physical_key),
            keyboard::Event::KeyReleased { physical_key, .. } => (false, physical_key),
            _ => return None,
        };
        let Physical::Code(code) = physical else { return None };
        hid_usage(code).map(|c| wrap(Msg::Recorded(down, c)))
    })
}

/// Adds a recorded step to the selected macro, or to a new one.
fn add_step(app: &mut App, step: MacroStep) {
    let p = app.cfg.profile_mut();
    let i = match app.macros.selected.filter(|i| *i < p.macros.len()) {
        Some(i) => i,
        None if p.macros.len() < MAX_MACROS => {
            p.macros.push(Macro::new(tr!("Macro {n}", n = p.macros.len() + 1), Vec::new()));
            p.macros.len() - 1
        }
        None => return,
    };
    p.macros[i].steps.push(step);
    app.macros.selected = Some(i);
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let sel = app.macros.selected;
    match msg {
        Msg::Record(on) => {
            app.macros.recording = on;
            if !on && let Some((step, _)) = app.macros.pending.take() {
                add_step(app, step);
            } else {
                return Task::none();
            }
        }
        Msg::RealPauses(on) => {
            app.macros.real_pauses = on;
            return Task::none();
        }
        Msg::Recorded(down, code) => {
            let now = Instant::now();
            if let Some((mut step, at)) = app.macros.pending.take() {
                if app.macros.real_pauses {
                    let ms = now.duration_since(at).as_millis().min(u16::MAX as u128) as u16;
                    step.delay = ms.max(MIN_DELAY);
                }
                add_step(app, step);
            }
            let action = if down { StepAction::Down } else { StepAction::Up };
            app.macros.pending = Some((MacroStep::new(action, code), now));
        }
        Msg::Select(i) => {
            app.macros.selected = Some(i);
            app.drafts.retain(|k, _| !k.starts_with("delay:"));
            return Task::none();
        }
        Msg::New => {
            let p = app.cfg.profile_mut();
            if p.macros.len() < MAX_MACROS {
                let name = tr!("Macro {n}", n = p.macros.len() + 1);
                p.macros.push(Macro::new(name, Vec::new()));
                app.macros.selected = Some(p.macros.len() - 1);
            }
        }
        Msg::Delete => {
            let Some(i) = sel else { return Task::none() };
            let Some(name) = app.cfg.profile().macros.get(i).map(|m| m.name.clone()) else { return Task::none() };
            app.checkpoint(tr!("Macro “{name}” deleted", name = name));
            let Some((_, reset)) = app.cfg.profile_mut().remove_macro(i) else { return Task::none() };
            app.macros.selected = None;
            let mut text = tr("The macros after it moved up by one.");
            if reset > 0 {
                text = format!("{text} {}", plural(reset as i64, "{n} key that played it does what is printed on it again.",
                                                   "{n} keys that played it do what is printed on them again."));
            }
            app.notify(text, false);
        }
        Msg::PickKey(k) => {
            app.macros.key = Some(k);
            return Task::none();
        }
        Msg::PickButton(b) => {
            app.macros.button = Some(b);
            return Task::none();
        }
        Msg::Text(t) => {
            app.macros.text = t;
            return Task::none();
        }
        Msg::Write => {
            let list = app.cfg.profile().macros.clone();
            let n = list.len();
            return app.device_task(tr("Writing macros…"), move || {
                WiredKeyboard::open()?.set_macros(&list)?;
                Ok(tr!("Macros written to the keyboard ({n}).", n = n))
            });
        }
        other => {
            let Some(i) = sel else { return Task::none() };
            let (key, button_choice, typed) = (app.macros.key.clone(), app.macros.button.clone(), app.macros.text.clone());
            let Some(m) = app.cfg.profile_mut().macros.get_mut(i) else { return Task::none() };
            match other {
                Msg::Name(n) => m.name = n,
                Msg::AddKey => {
                    if let Some(k) = key {
                        m.steps.push(MacroStep::new(StepAction::Down, k.value));
                        m.steps.push(MacroStep::new(StepAction::Up, k.value));
                    }
                }
                Msg::AddClick => {
                    if let Some(b) = button_choice {
                        m.steps.push(MacroStep::new(StepAction::MouseDown, b.value));
                        m.steps.push(MacroStep::new(StepAction::MouseUp, b.value));
                    }
                }
                Msg::AddText => {
                    m.steps.extend(Macro::text_steps(&typed, MIN_DELAY));
                    app.macros.text.clear();
                }
                Msg::Up(s) if s > 0 && s < m.steps.len() => m.steps.swap(s, s - 1),
                Msg::Down(s) if s + 1 < m.steps.len() => m.steps.swap(s, s + 1),
                Msg::Remove(s) if s < m.steps.len() => {
                    m.steps.remove(s);
                }
                _ => {}
            }
            app.drafts.retain(|k, _| !k.starts_with("delay:"));
        }
    }
    app.save();
    Task::none()
}

fn editor<'a>(app: &'a App, m: &'a Macro) -> Element<'a, Message> {
    let mut steps = column![].spacing(4);
    if !m.steps.is_empty() {
        steps = steps.push(row![iced::widget::space().width(Fill), caption(tr("Then wait, ms")).width(70),
                                iced::widget::space().width(96)].spacing(8));
    }
    for (n, s) in m.steps.iter().enumerate() {
        let id = format!("delay:{n}");
        let value = app.drafts.get(&id).cloned().unwrap_or_else(|| s.delay.to_string());
        let verb = if s.action.pressed() { tr("press") } else { tr("release") };
        steps = steps.push(
            row![
                text(format!("{:>3}", n + 1)).font(fonts::MONO).size(theme::SMALL).width(30),
                text(format!("{verb} {}", s.target())).size(theme::TEXT).width(Fill),
                text_input("10", &value).on_input(move |t| Message::Draft(id.clone(), t)).width(70).padding(4)
                    .style(theme::input).font(fonts::MONO),
                widgets::hint(button(text("↑")).style(theme::flat).padding([2, 8])
                    .on_press_maybe((n > 0).then(|| wrap(Msg::Up(n)))), tr("Move up")),
                widgets::hint(button(text("↓")).style(theme::flat).padding([2, 8])
                    .on_press_maybe((n + 1 < m.steps.len()).then(|| wrap(Msg::Down(n)))), tr("Move down")),
                widgets::hint(button(text("×")).style(theme::flat).padding([2, 8]).on_press(wrap(Msg::Remove(n))),
                              tr("Remove step")),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    if m.steps.is_empty() {
        steps = steps.push(caption(tr("Add keys, text or clicks below.")));
    }
    let keys: Vec<Choice<u8>> = hid_keys().iter().map(|(n, c)| Choice::new(*c, n.clone())).collect();
    let buttons: Vec<Choice<u8>> = MOUSE_BUTTONS.iter().map(|(b, n)| Choice::new(*b, tr(n))).collect();
    let record = row![
        if app.macros.recording {
            widgets::danger_button(tr("Stop recording"), Some(wrap(Msg::Record(false))))
        } else {
            widgets::secondary_button(tr("Record"), Some(wrap(Msg::Record(true))))
        },
        toggler(app.macros.real_pauses).label(tr("Keep my real pauses")).on_toggle(|v| wrap(Msg::RealPauses(v)))
            .style(theme::switch),
    ]
    .spacing(14)
    .align_y(Alignment::Center);
    let add = column![
        record,
        row![
            pick_list(keys, app.macros.key.clone(), |k| wrap(Msg::PickKey(k))).placeholder(tr("Key")).style(theme::pick)
                .menu_style(theme::menu).width(160),
            widgets::secondary_button(tr("Add key press"), app.macros.key.is_some().then(|| wrap(Msg::AddKey))),
            pick_list(buttons, app.macros.button.clone(), |b| wrap(Msg::PickButton(b))).placeholder(tr("Mouse"))
                .style(theme::pick).menu_style(theme::menu).width(160),
            widgets::secondary_button(tr("Add mouse click"), app.macros.button.is_some().then(|| wrap(Msg::AddClick))),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .wrap()
        .vertical_spacing(8),
        row![
            text_input(&tr("Text to type (Latin letters, US layout):"), &app.macros.text).on_input(|t| wrap(Msg::Text(t)))
                .on_submit(wrap(Msg::AddText)).padding(6).style(theme::input).width(Fill),
            widgets::secondary_button(tr("Type text"), (!app.macros.text.is_empty()).then(|| wrap(Msg::AddText))),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    ]
    .spacing(10);
    let rows = vec![
        setting(tr("Name"), None, text_input("", &m.name).on_input(|t| wrap(Msg::Name(t))).padding(6).style(theme::input)
            .width(260)),
        steps.into(),
        add.into(),
        row![
            caption(tr!("Lasts {ms} ms", ms = m.duration_ms())),
            iced::widget::space().width(Fill),
            widgets::danger_button(tr("Delete"), Some(wrap(Msg::Delete))),
        ]
        .align_y(Alignment::Center)
        .into(),
    ];
    section(tr!("Steps of “{name}”", name = m.name), card(rows))
}

pub fn view(app: &App) -> Element<'_, Message> {
    let p = app.cfg.profile();
    let mut list = column![].spacing(2);
    for (i, m) in p.macros.iter().enumerate() {
        let selected = app.macros.selected == Some(i);
        list = list.push(
            button(row![text(m.name.clone()).size(theme::TEXT).width(Fill), caption(format!("{}", m.steps.len()))])
                .width(Fill)
                .padding([8, 12])
                .style(theme::list_item(selected))
                .on_press(wrap(Msg::Select(i))),
        );
    }
    if p.macros.is_empty() {
        list = list.push(container(caption(tr("Create a macro to start"))).padding(12));
    }
    let used = macros::used_bytes(&p.macros);
    let memory = column![
        caption(tr!("Keyboard memory: {used} of {total} bytes", used = used, total = BUDGET_BYTES)),
        progress_bar(0.0..=BUDGET_BYTES as f32, used as f32).style(theme::progress).girth(6),
    ]
    .spacing(6);
    let left = column![
        container(list).padding(6).style(theme::card).width(Fill),
        widgets::secondary_button(tr("New macro"), (p.macros.len() < MAX_MACROS).then(|| wrap(Msg::New))),
        memory,
        widgets::primary_button(tr("Write to keyboard"), app.wired().then(|| wrap(Msg::Write))),
    ]
    .spacing(12)
    .width(260);
    let right: Element<'_, Message> = match app.macros.selected.and_then(|i| p.macros.get(i).map(|m| (i, m))) {
        Some((_, m)) => editor(app, m),
        None => card(vec![setting(tr("Macro"), Some(tr("Pick a macro on the left, or create one.")), text(""))]),
    };
    column![
        row![left, column![right].width(Fill)].spacing(20),
        caption(tr("A key plays a macro once you assign it on the Keys page. Keys refer to macros by number: \
                    after deleting one, write the macros and both key layers again.")),
    ]
    .spacing(16)
    .into()
}

/// The HID usage of a key in the window, for recording; the keys of a full-size keyboard.
fn hid_usage(code: Code) -> Option<u8> {
    use Code::*;
    let letters = [KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ,
                   KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ];
    if let Some(i) = letters.iter().position(|c| *c == code) {
        return Some(0x04 + i as u8);
    }
    // In HID order from 0x1E, with gaps where HID has keys the window never reports
    let runs: [(u8, &[Code]); 4] = [
        (0x1E, &[Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9, Digit0, Enter, Escape,
                 Backspace, Tab, Space, Minus, Equal, BracketLeft, BracketRight, Backslash]),
        (0x33, &[Semicolon, Quote, Backquote, Comma, Period, Slash, CapsLock, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10,
                 F11, F12, PrintScreen, ScrollLock, Pause, Insert, Home, PageUp, Delete, End, PageDown, ArrowRight,
                 ArrowLeft, ArrowDown, ArrowUp, NumLock, NumpadDivide, NumpadMultiply, NumpadSubtract, NumpadAdd,
                 NumpadEnter, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9, Numpad0,
                 NumpadDecimal, IntlBackslash, ContextMenu]),
        (0x68, &[F13, F14, F15, F16, F17, F18, F19, F20, F21, F22, F23, F24]),
        (0xE0, &[ControlLeft, ShiftLeft, AltLeft, SuperLeft, ControlRight, ShiftRight, AltRight, SuperRight]),
    ];
    runs.iter().find_map(|(first, codes)| codes.iter().position(|c| *c == code).map(|i| first + i as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_keys_become_hid_usages() {
        assert_eq!(hid_usage(Code::KeyA), Some(0x04));
        assert_eq!(hid_usage(Code::Digit0), Some(0x27));
        assert_eq!(hid_usage(Code::Backslash), Some(0x31));
        assert_eq!(hid_usage(Code::Semicolon), Some(0x33));
        assert_eq!(hid_usage(Code::F12), Some(0x45));
        assert_eq!(hid_usage(Code::ArrowUp), Some(0x52));
        assert_eq!(hid_usage(Code::NumpadDecimal), Some(0x63));
        assert_eq!(hid_usage(Code::ContextMenu), Some(0x65));
        assert_eq!(hid_usage(Code::F24), Some(0x73));
        assert_eq!(hid_usage(Code::SuperRight), Some(0xE7));
        assert_eq!(hid_usage(Code::Fn), None);
    }
}
