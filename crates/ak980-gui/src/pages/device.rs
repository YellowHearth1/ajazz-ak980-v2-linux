//! Device: the connection, access, the clock, the keyboard's own options, the background agent,
//! the 2.4 GHz link record and the app itself.

use ak980_core::device::{self, AnyKeyboard, Link, WiredKeyboard};
use ak980_core::i18n::{self, tr, LANGUAGES};
use ak980_core::lighting::SLEEP_CHOICES;
use ak980_core::service::{self, Method};
use ak980_core::{paths, permissions, tr};
use iced::widget::{button, column, container, pick_list, row, space, text, toggler};
use iced::{Alignment, Element, Fill, Task};

use crate::app::{blocking, App, Message};
use crate::fonts;
use crate::theme;
use crate::widgets::{self, caption, card, section, setting, Choice};

#[derive(Default)]
pub struct State {
    pub methods: Vec<Method>,
    pub link_log: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    AllowAccess,
    Reconnect,
    SyncClock,
    ClockSync(Choice<&'static str>),
    WinLock(bool),
    AltTab(bool),
    AltF4(bool),
    FnToggle(bool),
    Sleep(Choice<u8>),
    Response(u8),
    SendOptions,
    Methods(Vec<Method>, Vec<String>),
    Service(String, bool),
    Theme(&'static str),
    Accent(&'static str),
    Colorway(&'static str),
    /// Which AK980 the drawing shows.
    Model(&'static str),
    /// The theme made from the keyboard's colourway.
    MatchTheme,
    Language(Choice<&'static str>),
    Notifications(bool),
    Open(&'static str),
}

fn wrap(m: Msg) -> Message {
    Message::Device(m)
}

/// Reads what the page shows that takes a moment (service managers, the link record).
pub fn refresh() -> Task<Message> {
    Task::perform(
        blocking(|| {
            let log = std::fs::read_to_string(paths::link_log()).unwrap_or_default();
            let lines: Vec<String> = log.lines().rev().take(8).map(str::to_string).collect();
            (service::methods(), lines)
        }),
        |r| {
            let (m, l) = r.unwrap_or_default();
            wrap(Msg::Methods(m, l))
        },
    )
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let o = &mut app.cfg.profile_mut().options;
    match msg {
        Msg::AllowAccess => {
            return app.device_task(tr("Waiting for authorisation…"), || {
                permissions::install_rule()?;
                Ok(tr("Access allowed"))
            });
        }
        Msg::Reconnect => {
            return app.device_task(tr("Waiting for authorisation…"), || {
                let dir = device::receiver_usb_dir()
                    .ok_or_else(|| ak980_core::Error::NotConnected(tr("AK980 2.4 GHz dongle not found")))?;
                service::run_as_root(&device::reconnect_script(&dir))?;
                Ok(tr("The receiver was reconnected. Give the keyboard a few seconds to find it again."))
            });
        }
        Msg::SyncClock => {
            let slot = app.cfg.settings.clock_slot;
            return app.device_task(tr("Syncing the clock…"), move || {
                let mut kb = AnyKeyboard::open()?;
                let link = if kb.link() == Link::Usb { tr("USB cable") } else { tr("2.4 GHz") };
                if !kb.sync_clock(None, slot)? {
                    return Err(ak980_core::Error::NotConnected(tr(
                        "The keyboard did not answer through the dongle (is it in 2.4 GHz mode?)",
                    )));
                }
                Ok(tr!("Clock synced over {link}", link = link))
            });
        }
        Msg::ClockSync(c) => {
            app.cfg.settings.clock_sync = c.value.to_string();
            app.cfg.settings.clock_auto = c.value != "manual";
        }
        Msg::WinLock(v) => o.disable_win = v,
        Msg::AltTab(v) => o.disable_alt_tab = v,
        Msg::AltF4(v) => o.disable_alt_f4 = v,
        Msg::FnToggle(v) => o.fn_switch = v,
        Msg::Sleep(c) => o.sleep_time = c.value,
        Msg::Response(v) => o.key_response = v.clamp(1, 5),
        Msg::SendOptions => {
            let o = o.clone();
            return app.device_task(tr("Sending options…"), move || {
                WiredKeyboard::open()?.set_options(&o)?;
                Ok(tr("Keyboard options sent"))
            });
        }
        Msg::Methods(m, l) => {
            app.device.methods = m;
            app.device.link_log = l;
            return Task::none();
        }
        Msg::Service(id, on) => {
            let task = app.device_task(tr("Changing how the agent starts…"), move || {
                if on { service::enable(&id)? } else { service::disable(&id)? }
                Ok(tr("Background agent settings saved"))
            });
            // Read the switches again once the service manager is done, not while it works
            return task.chain(refresh());
        }
        Msg::Theme(t) => {
            app.cfg.settings.theme = t.to_string();
            app.apply_theme();
        }
        Msg::Accent(a) => {
            app.cfg.settings.accent = a.to_string();
            app.apply_theme();
        }
        Msg::Colorway(c) => {
            app.cfg.settings.colorway = c.to_string();
            app.apply_theme();
        }
        Msg::Model(m) => {
            app.cfg.settings.keyboard_model = m.to_string();
            app.apply_theme();
        }
        Msg::MatchTheme => {
            if let Some(t) = theme::theme_for_colorway(&app.cfg.settings.colorway) {
                app.cfg.settings.theme = t.to_string();
                app.apply_theme();
            }
        }
        Msg::Language(c) => {
            app.cfg.settings.language = c.value.to_string();
            i18n::set_language(c.value);
        }
        Msg::Notifications(v) => app.cfg.settings.notifications = v,
        Msg::Open(url) => {
            if let Ok(mut child) = std::process::Command::new("xdg-open").arg(url).spawn() {
                std::thread::spawn(move || child.wait()); // reaped, so no zombie stays behind
            }
            return Task::none();
        }
    }
    app.save();
    Task::none()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let st = &app.status;
    let (state, detail) = match st.link {
        Some(Link::Usb) => (tr("USB cable"), tr("Every setting is available")),
        Some(Link::Dongle) => (
            tr("2.4 GHz"),
            match app.battery {
                Some(b) => tr!("Battery: {level}", level = crate::app::battery_text(app, b)),
                None => tr("Through the dongle the keyboard only takes the clock and reports its battery."),
            },
        ),
        None => (tr("Not connected"), tr("Connect the AK980 with the USB cable or its 2.4 GHz dongle")),
    };
    let mut connection = vec![setting(state, Some(detail), text(""))];
    connection.push(match st.access {
        Some(false) => setting(tr("No access to the keyboard yet"), None,
                               widgets::primary_button(tr("Allow access"), Some(wrap(Msg::AllowAccess)))),
        Some(true) => setting(tr("Your user can use the keyboard"), None, text("")),
        None => setting(tr("Keyboard not connected"), None, text("")),
    });
    if st.link == Some(Link::Dongle) || st.receiver {
        connection.push(setting(
            tr("Reconnect the receiver"),
            Some(tr("If keys stop arriving over 2.4 GHz: the PC drops the receiver and finds it again, as if it had been plugged in anew. Asks for your password.")),
            widgets::secondary_button(tr("Reconnect"), Some(wrap(Msg::Reconnect))),
        ));
    }
    if !app.device.link_log.is_empty() {
        let mut log = column![caption(tr("2.4 GHz link record (newest first)"))].spacing(2);
        for line in &app.device.link_log {
            log = log.push(text(line.clone()).font(fonts::MONO).size(theme::SMALL));
        }
        connection.push(log.into());
    }

    let s = &app.cfg.settings;
    let clock = vec![
        setting(tr("Sync clock"), Some(tr("Set the screen clock to the system time")),
                widgets::secondary_button(tr("Sync clock now"), st.link.is_some().then(|| wrap(Msg::SyncClock)))),
        setting(tr("Set the clock by itself"),
                Some(tr("Setting the clock also switches the keyboard's screen from your animation to its clock page, so \
                         the rarer, the longer the animation stays.")),
                pick_list(clock_choices(), current_clock(s), |c| wrap(Msg::ClockSync(c))).style(theme::pick)
                    .menu_style(theme::menu).width(260)),
    ];


    let o = &app.cfg.profile().options;
    let sleeps: Vec<Choice<u8>> = SLEEP_CHOICES.iter().enumerate().map(|(i, c)| Choice::new(i as u8, tr(c))).collect();
    let current_sleep = sleeps.iter().find(|c| c.value == o.sleep_time).cloned();
    let options = vec![
        setting(tr("Lock the Win key"), None, toggler(o.disable_win).on_toggle(|v| wrap(Msg::WinLock(v))).style(theme::switch)),
        setting(tr("Lock Alt+Tab"), None, toggler(o.disable_alt_tab).on_toggle(|v| wrap(Msg::AltTab(v))).style(theme::switch)),
        setting(tr("Lock Alt+F4"), None, toggler(o.disable_alt_f4).on_toggle(|v| wrap(Msg::AltF4(v))).style(theme::switch)),
        setting(tr("Fn works as a switch"), None, toggler(o.fn_switch).on_toggle(|v| wrap(Msg::FnToggle(v))).style(theme::switch)),
        setting(tr("Sleep after"), None, pick_list(sleeps, current_sleep, |c| wrap(Msg::Sleep(c))).style(theme::pick)
            .menu_style(theme::menu).width(160)),
        setting(tr("Key response"), Some(tr("Lower is faster; raise it if keys double-type")),
                widgets::ranged(o.key_response as i32, 1, 5, "", |v| wrap(Msg::Response(v as u8)))),
        row![widgets::primary_button(tr("Send to keyboard"), app.wired().then(|| wrap(Msg::SendOptions)))].into(),
    ];

    let mut agent = vec![setting(
        tr("Background agent"),
        Some(tr("Keeps the clock in sync, streams PC effects, switches profiles and runs launcher keys.")),
        caption(if st.agent { tr("Running") } else { tr("Not running") }),
    )];
    for m in &app.device.methods {
        let id = m.id.clone();
        let init = m.id.split_once('-').map(|(_, i)| i.to_string()).unwrap_or_default();
        let title = i18n::format(&tr(m.title), &[("init", &init)]);
        let control: Element<'_, Message> = if m.available {
            toggler(m.enabled).on_toggle(move |v| wrap(Msg::Service(id.clone(), v))).style(theme::switch).into()
        } else {
            caption(tr("(not available here)")).into()
        };
        agent.push(setting(title, Some(tr(m.detail)), control));
    }

    let mut langs: Vec<Choice<&'static str>> = vec![Choice::new("auto", tr("System language"))];
    langs.extend(LANGUAGES.iter().map(|(c, n)| Choice::new(*c, n.to_string())));
    let current_lang = langs.iter().find(|c| c.value == s.language).cloned();
    let mut appearance: Vec<Element<'_, Message>> = vec![];
    appearance.extend([
        setting(tr("Language"), None, pick_list(langs, current_lang, |c| wrap(Msg::Language(c))).style(theme::pick)
            .menu_style(theme::menu).width(200)),
        setting(tr("Notifications"), Some(tr("From the agent: low battery, a lost 2.4 GHz link.")),
                toggler(s.notifications).on_toggle(|v| wrap(Msg::Notifications(v))).style(theme::switch)),
    ]);
    let about = vec![
        setting(format!("{} {}", ak980_core::APP_TITLE, ak980_core::VERSION),
                Some(tr("Unofficial software, not affiliated with AJAZZ.")),
                row![
                    button(text(tr("Project page")).size(theme::TEXT)).style(theme::flat).padding([4, 8])
                        .on_press(wrap(Msg::Open(ak980_core::HOMEPAGE))),
                    button(text(tr("Report a problem")).size(theme::TEXT)).style(theme::flat).padding([4, 8])
                        .on_press(wrap(Msg::Open(ak980_core::ISSUES))),
                ].spacing(4)),
    ];
    column![
        section(tr("Connection"), card(connection)),
        section(tr("Clock"), card(clock)),
        row![
            caption(tr("How long the battery lasts and how worn it is:")),
            widgets::secondary_button(tr("Battery calibration"), Some(Message::Navigate(crate::pages::Page::Battery))),
        ]
        .spacing(12)
        .align_y(Alignment::Center),
        section(tr("Keyboard options"), card(options)),
        section(tr("Background agent"), card(agent)),
        section(tr("This app"), card(appearance)),
        section(tr("About"), card(about)),
    ]
    .spacing(20)
    .width(Fill)
    .align_x(Alignment::Start)
    .into()
}

/// Catppuccin's accents as swatches, each in the colour it will have in this flavour; they wrap
/// onto as many rows as the sidebar needs.
pub fn accent_swatches(app: &App) -> Element<'_, Message> {
    let s = &app.cfg.settings;
    let mut swatches = row![].spacing(5);
    for (id, name) in theme::ACCENTS {
        let color = theme::roles_for(&s.theme, id, &s.colorway, &s.keyboard_model, app.system_dark).accent;
        let chip = container(space()).width(20).height(20).style(theme::swatch(color, s.accent == id));
        let pick = button(chip).padding(0).style(theme::flat).on_press(wrap(Msg::Accent(id)));
        swatches = swatches.push(widgets::hint(pick, tr(name)));
    }
    swatches.wrap().vertical_spacing(5).into()
}

/// How often the agent sets the clock on its own.
fn clock_choices() -> Vec<Choice<&'static str>> {
    [("hourly", "When the keyboard connects and every hour"), ("daily", "Once a day"), ("weekly", "Once a week"),
     ("manual", "Only when I ask")]
        .into_iter()
        .map(|(id, label)| Choice::new(id, tr(label)))
        .collect()
}

fn current_clock(s: &ak980_core::config::Settings) -> Option<Choice<&'static str>> {
    let id = if s.clock_auto { s.clock_sync.as_str() } else { "manual" };
    clock_choices().into_iter().find(|c| c.value == id).or_else(|| clock_choices().into_iter().nth(2))
}
