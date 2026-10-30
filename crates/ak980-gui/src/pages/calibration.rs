//! Calibration: how colours come out on the keyboard's LEDs. Per channel a strength (the white
//! balance) and a curve (how mid levels come out), tried live on every key in a test colour. It
//! applies to every colour the app, the agent and the CLI send.

use std::time::Duration;

use ak980_core::device::{self, Calibration, WiredKeyboard};
use ak980_core::effects;
use ak980_core::i18n::tr;
use ak980_core::{layout, profiles};
use iced::widget::{column, row, text, toggler};
use iced::{Alignment, Element, Task};

use crate::app::{blocking, App, Message};
use crate::theme;
use crate::widgets::{self, caption, card, color_picker, section, setting};

pub struct State {
    /// Every key shows the test colour.
    pub showing: bool,
    pub test: [u8; 3],
}

impl Default for State {
    fn default() -> Self {
        Self { showing: false, test: [255, 255, 255] }
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// A channel (0 red, 1 green, 2 blue) and its strength in percent.
    Gain(usize, i32),
    /// A channel and its curve in hundredths.
    Curve(usize, i32),
    Reset,
    Show(bool),
    Test([u8; 3]),
    /// The keyboard showed the colour or got its lighting back; an error, if any.
    Shown(Option<String>),
}

fn wrap(m: Msg) -> Message {
    Message::Calibration(m)
}

const GAIN_MIN: i32 = 40;
const CURVE: (i32, i32) = (50, 250);

/// Colours to compare with: white for the balance, grey for the curves, the rest for mixes.
const TESTS: [([u8; 3], &str); 9] = [
    ([255, 255, 255], "White"), ([128, 128, 128], "Grey"), ([255, 0, 0], "Red"), ([0, 255, 0], "Green"),
    ([0, 0, 255], "Blue"), ([255, 255, 0], "Yellow"), ([0, 255, 255], "Cyan"), ([255, 0, 255], "Magenta"),
    ([255, 128, 0], "Orange"),
];

/// The test colour on every key as one real-time frame: it stays until the keyboard gets another
/// command, and nothing is written to its memory.
fn show(app: &mut App) -> Task<Message> {
    if !app.wired() {
        app.calibration.showing = false;
        return Task::none();
    }
    let (c, calibration) = (app.calibration.test, app.cfg.settings.calibration());
    Task::perform(
        blocking(move || {
            let (leds, _) = layout::positions();
            let colour = c.map(|v| v as f32 / 255.0);
            let table = effects::to_bytes(&vec![colour; leds.len()], leds, 1.0);
            WiredKeyboard::open()
                .and_then(|mut kb| {
                    kb.calibration = calibration;
                    kb.send_realtime(&table, Duration::from_millis(2))
                })
                .err()
                .map(|e| e.to_string())
        }),
        |r| wrap(Msg::Shown(r.flatten())),
    )
}

/// The profile's own lighting back on the keyboard after the test colour.
pub fn give_lighting_back(app: &mut App) -> Task<Message> {
    app.calibration.showing = false;
    if !app.wired() {
        return Task::none();
    }
    let profile = app.cfg.profile().clone();
    Task::perform(
        blocking(move || {
            WiredKeyboard::open()
                .and_then(|mut kb| profiles::apply_lighting(&mut kb, &profile))
                .err()
                .map(|e| e.to_string())
        }),
        |r| wrap(Msg::Shown(r.flatten())),
    )
}

/// A test colour picked on the colour wheel.
pub fn set_test_color(app: &mut App, c: [u8; 3]) -> Task<Message> {
    app.calibration.test = c;
    if app.calibration.showing { show(app) } else { Task::none() }
}

/// New settings: kept, used for whatever is sent from now on, and shown at once.
fn changed(app: &mut App) -> Task<Message> {
    device::set_calibration(app.cfg.settings.calibration());
    app.save();
    if app.calibration.showing { show(app) } else { Task::none() }
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    match msg {
        Msg::Gain(i, v) => {
            if let Some(g) = app.cfg.settings.white_balance.get_mut(i) {
                *g = v.clamp(GAIN_MIN, 100) as u8;
            }
            changed(app)
        }
        Msg::Curve(i, v) => {
            if let Some(c) = app.cfg.settings.led_curves.get_mut(i) {
                *c = v.clamp(CURVE.0, CURVE.1) as u16;
            }
            changed(app)
        }
        Msg::Reset => {
            app.checkpoint(tr("Calibration reset"));
            let none = Calibration::NONE;
            (app.cfg.settings.white_balance, app.cfg.settings.led_curves) = (none.gains, none.curves);
            changed(app)
        }
        Msg::Show(on) => {
            if on {
                app.calibration.showing = true;
                show(app)
            } else {
                give_lighting_back(app)
            }
        }
        Msg::Test(c) => set_test_color(app, c),
        Msg::Shown(error) => {
            if let Some(e) = error {
                app.calibration.showing = false;
                app.notify(e, true);
            }
            Task::none()
        }
    }
}

pub fn view(app: &App) -> Element<'_, Message> {
    let st = &app.calibration;
    let s = &app.cfg.settings;
    let mut tests = row![].spacing(6);
    for (c, name) in TESTS {
        tests = tests.push(widgets::hint(
            iced::widget::button(iced::widget::container(iced::widget::space().width(26).height(26))
                .style(theme::swatch(widgets::to_color(c), c == st.test)))
                .padding(0)
                .style(theme::flat)
                .on_press(wrap(Msg::Test(c))),
            tr(name),
        ));
    }
    let test_rows = vec![
        setting(tr("Show it on the keyboard"),
                Some(tr("Every key in the test colour, while you move the sliders; nothing is written to the keyboard. \
                         Turning it off gives the keyboard its lighting back.")),
                toggler(st.showing).on_toggle_maybe(app.wired().then_some(|v| wrap(Msg::Show(v)))).style(theme::switch)),
        setting(tr("Compare with"), Some(tr("White for the strengths, grey for the curves, the others for mixes")),
                tests.wrap().vertical_spacing(6)),
        setting(tr("Test colour"), None,
                color_picker("color:calib:test".into(), st.test, app.drafts.get("color:calib:test"),
                             |c| Message::Draft("color:calib:test".into(), widgets::html(c)))),
    ];
    let mut channels = vec![];
    for (i, name) in [tr("Red"), tr("Green"), tr("Blue")].into_iter().enumerate() {
        let curve = s.led_curves[i] as i32;
        channels.push(setting(
            name,
            None,
            column![
                row![caption(tr("Strength")).width(70),
                     widgets::ranged(s.white_balance[i] as i32, GAIN_MIN, 100, "%", move |v| wrap(Msg::Gain(i, v)))]
                    .spacing(8).align_y(Alignment::Center),
                row![caption(tr("Curve")).width(70),
                     widgets::shown_as(curve, CURVE.0, CURVE.1, format!("{:.2}", curve as f32 / 100.0),
                                       move |v| wrap(Msg::Curve(i, v)))]
                    .spacing(8).align_y(Alignment::Center),
            ]
            .spacing(6),
        ));
    }
    if s.calibration() != Calibration::NONE {
        channels.push(row![widgets::secondary_button(tr("Reset"), Some(wrap(Msg::Reset)))].into());
    }
    column![
        text(tr("The keyboard's LEDs do not show colours exactly as a screen does: its red is the strongest, so \
                 white looks pink, and half the level of a channel looks brighter than half. Tune each channel here \
                 until the test colours on the keys look as they do on the screen. It applies to every colour this \
                 app sends: the effect colour, the key colours (write them again) and PC effects; the keyboard's \
                 own rainbow effects keep their colours."))
            .size(theme::TEXT),
        section(tr("Test on the keyboard"), card(test_rows)),
        section(tr("Channels"), card(channels)),
        caption(tr("Strength: lower the channel that is too strong in white. Curve: more than 1.00 makes the middle \
                    levels of that channel darker, less makes them brighter; judge it on grey and orange.")),
    ]
    .spacing(16)
    .into()
}
