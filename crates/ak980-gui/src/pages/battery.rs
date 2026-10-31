//! Battery calibration: what the background agent has learnt from whole cycles of the keyboard's
//! battery. A full discharge tells how long a charge lasts, a full charge how long charging takes
//! (and the curve that gives the level on the cable, where the keyboard does not tell it), and the
//! latest discharge against the first how worn the battery is.

use ak980_core::battery::{self, Measured, Measurements};
use ak980_core::i18n::tr;
use ak980_core::{paths, tr};
use iced::widget::{column, text, toggler};
use iced::{Element, Task};

use crate::app::{blocking, App, Message};
use crate::theme;
use crate::widgets::{self, caption, card, section, setting};

#[derive(Default)]
pub struct State {
    pub measured: Measurements,
    /// Changes of the battery level recorded so far.
    pub readings: usize,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Measurements, usize),
    Insights(bool),
    StartOver,
}

fn wrap(m: Msg) -> Message {
    Message::Battery(m)
}

/// Reads what the agent has recorded and learnt.
pub fn load() -> Task<Message> {
    Task::perform(
        blocking(|| (Measurements::load(&paths::battery_measurements()), battery::read(&paths::battery_log()).len())),
        |r| {
            let (m, n) = r.unwrap_or_default();
            wrap(Msg::Loaded(m, n))
        },
    )
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    match msg {
        Msg::Loaded(m, n) => {
            (app.battery_page.measured, app.battery_page.readings) = (m, n);
            Task::none()
        }
        Msg::Insights(on) => {
            app.cfg.settings.battery_insights = on;
            app.save();
            Task::none()
        }
        Msg::StartOver => {
            // Kept as a start time rather than deleted, or the agent would learn the old cycles again
            if let Err(e) = Measurements::started_over(battery::now()).save(&paths::battery_measurements()) {
                app.notify(e.to_string(), true);
            }
            load()
        }
    }
}

/// "from 96 % to 18 %, 41 h for the whole battery"
fn cycle_text(m: &Measured) -> String {
    tr!("from {from} to {to}, {time} for the whole battery", from = format!("{}%", m.from), to = format!("{}%", m.to),
        time = battery::duration(m.hours_per_100))
}

pub fn view(app: &App) -> Element<'_, Message> {
    let b = &app.battery_page.measured;
    let s = &app.cfg.settings;
    let done = |ok: bool| if ok { "✓" } else { "…" };
    let steps = vec![
        setting(
            format!("{} 1. {}", done(!b.discharges.is_empty()), tr("Discharge it fully")),
            Some(tr("Use the keyboard over 2.4 GHz from 90 % or more down to 20 % or less, without charging in between. \
                     The agent reads the level every minute.")),
            caption(b.discharges.last().map(cycle_text).unwrap_or_else(|| tr("not measured yet"))),
        ),
        setting(
            format!("{} 2. {}", done(!b.charges.is_empty()), tr("Charge it fully")),
            Some(tr("Then charge it from 20 % or less to full while it stays in 2.4 GHz mode, so it can tell its level. \
                     The red Fn key shows that it charges.")),
            caption(b.charges.last().map(cycle_text).unwrap_or_else(|| tr("not measured yet"))),
        ),
        setting(
            format!("{} 3. {}", done(b.discharges.len() >= 2), tr("Discharge it fully again, now and then")),
            Some(tr("Every further full discharge is compared with the first one: that shows the wear.")),
            caption(tr!("Measured discharges: {n}", n = b.discharges.len())),
        ),
    ];

    let mut results = vec![];
    results.push(match b.full_charge_hours() {
        Some(h) => setting(tr!("A full charge lasts about {time}", time = battery::duration(h)), None, text("")),
        None => setting(tr("How long a charge lasts: after step 1"), None, text("")),
    });
    results.push(match b.charging_hours() {
        Some(h) => setting(tr!("Charging from empty to full takes about {time}", time = battery::duration(h)), None, text("")),
        None => setting(tr("How long charging takes: after step 2"), None, text("")),
    });
    results.push(match b.condition() {
        Some((share, cycles)) => setting(
            tr!("Wear: about {wear}", wear = format!("{}%", ((1.0 - share) * 100.0).round())),
            Some(tr!("The last full charge lasted {share} as long as the first measured one ({cycles} discharges). How \
                      you use the keyboard changes it too, so take it as a rough guide.",
                     share = format!("{}%", (share * 100.0).round()), cycles = cycles)),
            text(""),
        ),
        None => setting(tr("Wear: after the second full discharge"), None, text("")),
    });
    if let Some(e) = app.status.estimate.filter(|_| s.battery_insights) {
        results.push(setting(tr!("Now: {estimate}", estimate = battery::describe(&e)), None, text("")));
    }
    results.push(setting(
        tr("Show the time left"),
        Some(tr("Beside the charge in the sidebar and on the Device page, from what is measured here.")),
        toggler(s.battery_insights).on_toggle(|v| wrap(Msg::Insights(v))).style(theme::switch),
    ));
    let start_over = (!b.discharges.is_empty() || !b.charges.is_empty()).then(|| wrap(Msg::StartOver));
    results.push(setting(
        tr!("Readings so far: {count}", count = app.battery_page.readings),
        Some(tr("Measuring again forgets the cycles above; the readings stay.")),
        widgets::secondary_button(tr("Measure again"), start_over),
    ));

    column![
        text(tr("The keyboard tells its charge only over 2.4 GHz, in steps. While the background agent runs, it \
                 reads it every minute and learns from whole cycles how long a charge lasts, how long charging takes \
                 and how worn the battery is. It keeps doing that by itself: the steps below are only what it needs to \
                 see once."))
            .size(theme::TEXT),
        section(tr("Steps"), card(steps)),
        section(tr("What it has learnt"), card(results)),
    ]
    .spacing(16)
    .into()
}
