//! Macros: the keyboard's macro memory, edited step by step.

use ak980_core::device::WiredKeyboard;
use ak980_core::i18n::tr;
use ak980_core::keymap::hid_keys;
use ak980_core::macros::{self, Macro, MacroStep, StepAction, BUDGET_BYTES, MAX_MACROS, MIN_DELAY, MOUSE_BUTTONS};
use ak980_core::tr;
use iced::widget::{button, column, container, pick_list, progress_bar, row, text, text_input};
use iced::{Alignment, Element, Fill, Task};

use crate::app::{App, Message};
use crate::fonts;
use crate::theme;
use crate::widgets::{self, caption, card, section, setting, Choice};

#[derive(Default)]
pub struct State {
    pub selected: Option<usize>,
    pub key: Option<Choice<u8>>,
    pub button: Option<Choice<u8>>,
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Select(usize),
    New,
    Delete,
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

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let sel = app.macros.selected;
    match msg {
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
            if let Some(i) = sel {
                let p = app.cfg.profile_mut();
                if i < p.macros.len() {
                    p.macros.remove(i);
                }
                app.macros.selected = None;
            }
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
                button(text("↑")).style(theme::flat).padding([2, 8]).on_press_maybe((n > 0).then(|| wrap(Msg::Up(n)))),
                button(text("↓")).style(theme::flat).padding([2, 8])
                    .on_press_maybe((n + 1 < m.steps.len()).then(|| wrap(Msg::Down(n)))),
                button(text("×")).style(theme::flat).padding([2, 8]).on_press(wrap(Msg::Remove(n))),
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
    let add = column![
        row![
            pick_list(keys, app.macros.key.clone(), |k| wrap(Msg::PickKey(k))).placeholder(tr("Key")).style(theme::pick)
                .menu_style(theme::menu).width(160),
            widgets::secondary_button(tr("Add key press"), app.macros.key.is_some().then(|| wrap(Msg::AddKey))),
            pick_list(buttons, app.macros.button.clone(), |b| wrap(Msg::PickButton(b))).placeholder(tr("Mouse"))
                .style(theme::pick).menu_style(theme::menu).width(160),
            widgets::secondary_button(tr("Add mouse click"), app.macros.button.is_some().then(|| wrap(Msg::AddClick))),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
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
        caption(tr("A key plays a macro once you assign it on the Keys page.")),
    ]
    .spacing(16)
    .into()
}
