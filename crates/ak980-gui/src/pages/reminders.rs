//! Reminders, on the Screen page: at a set time the background agent shows a notification once,
//! makes the keys call for attention for a while and, if asked, puts the text on the screen and
//! then the last animation back.

use std::time::Duration;

use ak980_core::config::Reminder;
use ak980_core::device::WiredKeyboard;
use ak980_core::effects::ATTENTION;
use ak980_core::i18n::tr;
use ak980_core::{agent, engine, profiles, tr};
use iced::widget::{button, column, container, pick_list, row, space, text, text_input, toggler};
use iced::{Alignment, Element, Fill, Task};

use crate::app::{App, Message};
use crate::fonts;
use crate::theme;
use crate::widgets::{self, caption, card, section, setting, Choice, SWATCHES};

/// The reminder being written, before it is added.
pub struct State {
    time: String,
    text: String,
    minutes: i32,
    effect: &'static str,
    color: [u8; 3],
    days: Vec<u8>,
    screen: bool,
    trying: bool,
}

impl Default for State {
    fn default() -> Self {
        let r = Reminder::default();
        Self { time: r.time, text: String::new(), minutes: r.minutes as i32, effect: "flash", color: r.color,
               days: Vec::new(), screen: false, trying: false }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Time(String),
    Text(String),
    Minutes(i32),
    Effect(Choice<&'static str>),
    Color([u8; 3]),
    Day(u8),
    Screen(bool),
    Add,
    Remove(usize),
    Enable(usize, bool),
    Try,
    Tried(Result<(), String>),
}

fn wrap(m: Msg) -> Message {
    Message::Reminders(m)
}

/// The week from Monday, as `Reminder::days` counts it (Sunday = 0).
const WEEK: [(u8, &str); 7] = [(1, "Mon"), (2, "Tue"), (3, "Wed"), (4, "Thu"), (5, "Fri"), (6, "Sat"), (0, "Sun")];

fn draft(st: &State) -> Option<Reminder> {
    let r = Reminder { time: st.time.trim().to_string(), days: st.days.clone(), text: st.text.trim().to_string(),
                       minutes: st.minutes.clamp(1, 60) as u32, effect: st.effect.to_string(), color: st.color,
                       screen: st.screen, enabled: true };
    r.hour_minute().map(|(h, m)| Reminder { time: format!("{h:02}:{m:02}"), ..r })
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let st = &mut app.reminders;
    match msg {
        Msg::Time(t) => st.time = t,
        Msg::Text(t) => st.text = t,
        Msg::Minutes(m) => st.minutes = m,
        Msg::Effect(e) => st.effect = e.value,
        Msg::Color(c) => st.color = c,
        Msg::Day(d) => {
            if let Some(i) = st.days.iter().position(|x| *x == d) {
                st.days.remove(i);
            } else {
                st.days.push(d);
            }
        }
        Msg::Screen(on) => st.screen = on,
        Msg::Add => {
            let Some(r) = draft(st) else { return Task::none() };
            st.text.clear();
            app.cfg.settings.reminders.push(r);
            app.cfg.settings.reminders.sort_by(|a, b| a.time.cmp(&b.time));
            app.save();
        }
        Msg::Remove(i) => {
            if i < app.cfg.settings.reminders.len() {
                app.checkpoint(tr("Reminder deleted"));
                app.cfg.settings.reminders.remove(i);
                app.save();
            }
        }
        Msg::Enable(i, on) => {
            if let Some(r) = app.cfg.settings.reminders.get_mut(i) {
                r.enabled = on;
                app.save();
            }
        }
        Msg::Try => {
            // The notification and the keys for a few seconds; the screen is left alone
            let (effect, color, text) = (st.effect, st.color, st.text.clone());
            st.trying = true;
            agent::notify(&agent::Notice {
                summary: tr("Reminder"),
                body: if text.trim().is_empty() { tr("It is time") } else { text },
                urgent: true,
            });
            let profile = app.cfg.profile().clone();
            return Task::perform(
                crate::app::blocking(move || -> Result<(), String> {
                    let mut kb = WiredKeyboard::open().map_err(|e| e.to_string())?;
                    let played = engine::play_attention(&mut kb, effect, color, Duration::from_secs(6));
                    let back = profiles::apply_lighting(&mut kb, &profile);
                    played.and(back).map_err(|e| e.to_string())
                }),
                |r| wrap(Msg::Tried(r.unwrap_or_else(|| Err("the worker stopped".into())))),
            );
        }
        Msg::Tried(result) => {
            st.trying = false;
            if let Err(e) = result {
                app.notify(e, true);
            }
        }
    }
    Task::none()
}

fn days_text(days: &[u8]) -> String {
    if days.is_empty() || days.len() == 7 {
        return tr("every day");
    }
    WEEK.iter().filter(|(d, _)| days.contains(d)).map(|(_, n)| tr(n)).collect::<Vec<_>>().join(", ")
}

fn effect_name(id: &str) -> String {
    ATTENTION.iter().find(|(e, _)| *e == id).map(|(_, n)| tr(n)).unwrap_or_else(|| id.to_string())
}

pub fn view(app: &App) -> Element<'_, Message> {
    let st = &app.reminders;
    let mut rows: Vec<Element<'_, Message>> = vec![];
    for (i, r) in app.cfg.settings.reminders.iter().enumerate() {
        let swatch = container(space().width(14).height(14)).style(theme::swatch(widgets::to_color(r.color), false));
        let mut what = format!("{} · {} · {}", days_text(&r.days), effect_name(&r.effect),
                               tr!("{n} min", n = r.minutes));
        if r.screen {
            what = format!("{what} · {}", tr("on the screen too"));
        }
        rows.push(
            row![
                text(r.time.clone()).font(fonts::MEDIUM).size(theme::TEXT).width(56),
                swatch,
                column![text(if r.text.is_empty() { tr("It is time") } else { r.text.clone() }).size(theme::TEXT),
                        caption(what)].spacing(2).width(Fill),
                toggler(r.enabled).on_toggle(move |v| wrap(Msg::Enable(i, v))).style(theme::switch),
                button(text(tr("Remove")).size(theme::SMALL)).style(theme::flat).padding([4, 8]).on_press(wrap(Msg::Remove(i))),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into(),
        );
    }
    if rows.is_empty() {
        rows.push(caption(tr("No reminders yet.")).into());
    }

    let effects: Vec<Choice<&'static str>> = ATTENTION.iter().map(|(id, n)| Choice::new(*id, tr(n))).collect();
    let current = effects.iter().find(|c| c.value == st.effect).cloned();
    let mut days = row![].spacing(4);
    for (d, name) in WEEK {
        let on = st.days.contains(&d);
        days = days.push(button(text(tr(name)).size(theme::SMALL)).padding([5, 9]).style(theme::segment(on))
            .on_press(wrap(Msg::Day(d))));
    }
    let mut colors = row![].spacing(6);
    for c in SWATCHES {
        let tile = container(space().width(20).height(20)).style(theme::swatch(widgets::to_color(c), c == st.color));
        colors = colors.push(button(tile).padding(0).style(theme::flat).on_press(wrap(Msg::Color(c))));
    }
    let ready = draft(st).is_some();
    let form = vec![
        setting(tr("At"), Some(tr("Hours and minutes, e.g. 14:30")),
                text_input("12:00", &st.time).on_input(|t| wrap(Msg::Time(t))).width(90).padding(6).style(theme::input)
                    .font(fonts::MONO)),
        setting(tr("Days"), Some(tr("None picked: every day")), days.wrap().vertical_spacing(4)),
        setting(tr("Text"), None, text_input(&tr("e.g. Stand up and stretch"), &st.text)
            .on_input(|t| wrap(Msg::Text(t))).on_submit(wrap(Msg::Add)).padding(6).style(theme::input).width(320)),
        setting(tr("For"), None, widgets::ranged(st.minutes, 1, 60, &tr(" min"), |v| wrap(Msg::Minutes(v)))),
        setting(tr("Keys"), None, pick_list(effects, current, |c| wrap(Msg::Effect(c))).style(theme::pick)
            .menu_style(theme::menu).width(240)),
        setting(tr("Colour"), None, colors.wrap().vertical_spacing(6)),
        setting(tr("Show it on the screen too"),
                Some(tr("The text blinks on the screen, then your last animation comes back. Each time that writes \
                         the screen's memory twice, about 5 s for the text and as long as your animation took \
                         (about a minute for 28 frames), so keep it for the reminders that matter.")),
                toggler(st.screen).on_toggle(|v| wrap(Msg::Screen(v))).style(theme::switch)),
        row![
            widgets::primary_button(tr("Add reminder"), ready.then(|| wrap(Msg::Add))),
            widgets::secondary_button(tr("Try it"), (app.wired() && !st.trying).then(|| wrap(Msg::Try))),
            caption(tr("Try it: the notification and the keys for a few seconds.")),
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into(),
    ];
    column![
        section(tr("Reminders"), card(rows)),
        card(form),
        caption(tr("The background agent keeps the time, so reminders come while it runs. The keys need the cable; \
                    over 2.4 GHz only the notification comes.")),
    ]
    .spacing(10)
    .into()
}
