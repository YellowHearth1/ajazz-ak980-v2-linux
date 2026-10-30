//! The window: state, messages, the frame around the pages (sidebar, header, notices) and the
//! work that talks to the keyboard, which always runs on its own thread.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

use ak980_core::config::{self, Config};
use ak980_core::device::{self, Dongle, Link};
use ak980_core::i18n::tr;
use ak980_core::{paths, permissions, runtime, tr};
use iced::futures::channel::oneshot;
use iced::widget::{button, column, container, pick_list, row, scrollable, space, stack, text};
use iced::{Alignment, Element, Fill, Length, Subscription, Task, Theme};

use crate::pages::{self, Page};
use crate::theme::{self, roles};
use crate::widgets::{self, caption};
use crate::fonts;

#[derive(Debug, Clone)]
pub enum Message {
    Navigate(Page),
    Poll,
    Polled(Status),
    SystemTheme(iced::theme::Mode),
    Tick(Instant),
    /// The window is about to close: what is not saved yet is written first.
    CloseRequested,
    /// Puts back what the last deletion or reset took away.
    Undo,
    /// A keyboard operation finished: the notice to show, or the error.
    Done(Result<String, String>),
    SwitchProfile(String),
    /// Text typed into a field whose value is checked before it is used (hex colours, numbers).
    Draft(String, String),
    DraftSubmit(String),
    CloseNotice,
    Lighting(pages::lighting::Msg),
    Colours(pages::colours::Msg),
    Keys(pages::keys::Msg),
    Macros(pages::macros::Msg),
    Screen(pages::screen::Msg),
    Profiles(pages::profiles::Msg),
    Device(pages::device::Msg),
}

/// What the periodic check found out.
#[derive(Debug, Clone, Default)]
pub struct Status {
    pub link: Option<Link>,
    /// Read over the dongle at most once a minute; `None` when not read this time.
    pub battery: Option<Option<u8>>,
    pub access: Option<bool>,
    pub agent: bool,
    /// The 2.4 GHz receiver is plugged in (also while the keyboard is on the cable).
    pub receiver: bool,
    pub config_mtime: Option<SystemTime>,
}

pub struct Toast {
    pub text: String,
    pub error: bool,
    pub at: Instant,
}

/// The settings as they were before a deletion or a reset, offered back until the next change.
pub struct Undo {
    /// What was done, shown next to the button.
    pub what: String,
    cfg: Config,
    /// Saves since the checkpoint: the first is the deletion itself, a second one is a new change.
    saves: u8,
}

pub struct App {
    pub cfg: Config,
    pub page: Page,
    pub system_dark: bool,
    pub theme: Theme,
    pub status: Status,
    pub battery: Option<u8>,
    battery_at: Option<Instant>,
    pub toasts: Vec<Toast>,
    /// What runs on the keyboard right now, shown in the header.
    pub busy: Option<String>,
    pub drafts: HashMap<String, String>,
    config_mtime: Option<SystemTime>,
    saved_at: Option<Instant>,
    /// Changes made since the last write of the settings file.
    unsaved: bool,
    pub undo: Option<Undo>,
    pub lighting: pages::lighting::State,
    pub colours: pages::colours::State,
    pub keys: pages::keys::State,
    pub macros: pages::macros::State,
    pub screen: pages::screen::State,
    pub profiles: pages::profiles::State,
    pub device: pages::device::State,
}

/// Runs `f` on a thread of its own and hands its result back to the window.
pub fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> impl Future<Output = Option<T>> {
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    async move { rx.await.ok() }
}

fn poll_status(read_battery: bool) -> Status {
    let link = device::connection_state();
    let battery = (read_battery && link == Some(Link::Dongle)).then(|| {
        Dongle::find().and_then(|info| Dongle::open_found(&info).ok()).and_then(|mut d| d.read_battery().ok().flatten())
    });
    Status {
        link,
        battery,
        access: permissions::accessible(),
        agent: runtime::agent_running(),
        receiver: device::receiver_usb_dir().is_some(),
        config_mtime: config::mtime(&paths::config_path()),
    }
}

impl App {
    pub fn boot() -> (Self, Task<Message>) {
        let cfg = Config::load();
        let system_dark = true;
        let s = &cfg.settings;
        let theme = theme::activate(theme::roles_for(&s.theme, &s.accent, &s.colorway, system_dark));
        let app = Self {
            config_mtime: config::mtime(&cfg.path),
            saved_at: None,
            unsaved: false,
            undo: None,
            cfg,
            page: Page::Lighting,
            system_dark,
            theme,
            status: Status::default(),
            battery: None,
            battery_at: None,
            toasts: Vec::new(),
            busy: None,
            drafts: HashMap::new(),
            lighting: Default::default(),
            colours: Default::default(),
            keys: Default::default(),
            macros: Default::default(),
            screen: Default::default(),
            profiles: Default::default(),
            device: Default::default(),
        };
        let tasks = Task::batch([
            iced::system::theme().map(Message::SystemTheme),
            Task::done(Message::Poll),
            pages::screen::load_history(),
        ]);
        (app, tasks)
    }

    pub fn title(&self) -> String {
        ak980_core::APP_TITLE.to_string()
    }

    pub fn theme(&self) -> Theme {
        self.theme.clone()
    }

    pub fn wired(&self) -> bool {
        self.status.link == Some(Link::Usb)
    }

    pub fn notify(&mut self, text: String, error: bool) {
        self.toasts.retain(|t| t.text != text);
        self.toasts.push(Toast { text, error, at: Instant::now() });
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
    }

    /// Saves the settings, at most twice a second: a drag over the keys or a slider sends a message
    /// for every step. The rest is written on the next tick or poll, and when the window closes.
    pub fn save(&mut self) {
        if let Some(u) = &mut self.undo {
            u.saves += 1;
            if u.saves > 1 {
                self.undo = None; // undoing now would take a later change away too
            }
        }
        if self.saved_at.is_some_and(|t| t.elapsed() < SAVE_GAP) {
            self.unsaved = true;
        } else {
            self.save_now();
        }
    }

    /// Remembers the settings before a deletion or a reset (`what` says which), for Undo.
    pub fn checkpoint(&mut self, what: String) {
        self.undo = Some(Undo { what, cfg: self.cfg.clone(), saves: 0 });
    }

    /// Writes the settings file now; a failure becomes a notice.
    fn save_now(&mut self) {
        self.unsaved = false;
        self.saved_at = Some(Instant::now());
        if let Err(e) = self.cfg.save() {
            self.notify(tr!("Error: {message}", message = e), true);
        }
        self.config_mtime = config::mtime(&self.cfg.path);
    }

    fn save_if_due(&mut self) {
        if self.unsaved && self.saved_at.is_none_or(|t| t.elapsed() >= SAVE_GAP) {
            self.save_now();
        }
    }

    /// Runs a keyboard operation on its own thread; `busy` is shown while it runs.
    pub fn device_task(&mut self, busy: String, f: impl FnOnce() -> ak980_core::Result<String> + Send + 'static) -> Task<Message> {
        self.busy = Some(busy);
        Task::perform(blocking(move || f().map_err(|e| e.to_string())), |r| {
            Message::Done(r.unwrap_or_else(|| Err("the worker stopped".into())))
        })
    }

    pub fn apply_theme(&mut self) {
        let s = &self.cfg.settings;
        self.theme = theme::activate(theme::roles_for(&s.theme, &s.accent, &s.colorway, self.system_dark));
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Navigate(page) => {
                self.page = page;
                if page == Page::Device { pages::device::refresh() } else { Task::none() }
            }
            Message::Poll => {
                self.save_if_due();
                let read = self.battery_at.is_none_or(|t| t.elapsed() > Duration::from_secs(60));
                // Errors stay until they are closed, so one is never missed; the rest go after 6 s
                self.toasts.retain(|t| t.error || t.at.elapsed() < Duration::from_secs(6));
                Task::perform(blocking(move || poll_status(read)), |s| Message::Polled(s.unwrap_or_default()))
            }
            Message::Polled(status) => {
                if status.link != self.status.link {
                    self.battery_at = None; // read the battery again right after a switch
                }
                if let Some(level) = status.battery {
                    self.battery = level;
                    self.battery_at = Some(Instant::now());
                }
                if status.link != Some(Link::Dongle) {
                    self.battery = None;
                }
                // Another program (the CLI, the agent switching profiles) changed the settings. A
                // time older than our own last write is a poll that started before that write.
                if status.config_mtime.is_some() && status.config_mtime > self.config_mtime {
                    self.unsaved = false; // the newer file wins over changes not written yet
                    self.undo = None; // and putting an old copy back would undo that program's change
                    self.config_mtime = status.config_mtime;
                    let active = std::mem::take(&mut self.cfg.active);
                    self.cfg = Config::load();
                    if self.cfg.active != active {
                        self.profiles.rename.clear(); // a name typed for the profile that was active
                    }
                    self.apply_theme();
                }
                self.status = status;
                pages::lighting::keep_engine(self)
            }
            Message::SystemTheme(mode) => {
                self.system_dark = theme::system_is_dark(mode);
                self.apply_theme();
                Task::none()
            }
            Message::Tick(now) => {
                self.save_if_due(); // the PC effect reads its settings from the file
                pages::lighting::tick(self, now);
                pages::screen::tick(self, now);
                Task::none()
            }
            Message::Undo => {
                let Some(u) = self.undo.take() else { return Task::none() };
                let switched = u.cfg.active != self.cfg.active;
                self.cfg = u.cfg;
                self.save_now();
                // A deleted profile was the active one: the keyboard gets it back too
                if switched { pages::profiles::send_profile(self) } else { Task::none() }
            }
            Message::CloseRequested => {
                if self.unsaved {
                    self.save_now();
                }
                iced::exit()
            }
            Message::Done(result) => {
                self.busy = None;
                match result {
                    Ok(text) if !text.is_empty() => self.notify(text, false),
                    Ok(_) => {}
                    Err(e) => self.notify(e, true),
                }
                pages::lighting::after_send(self)
            }
            Message::SwitchProfile(name) => pages::profiles::use_profile(self, name),
            Message::Draft(id, value) => {
                let picked = widgets::parse_html(&value);
                self.drafts.insert(id.clone(), value.clone());
                match picked {
                    Some(c) if id.starts_with("color:") => self.pick_color(&id, c, false),
                    _ => self.number_draft(&id, &value),
                }
            }
            Message::DraftSubmit(id) => {
                self.drafts.remove(&id);
                Task::none()
            }
            Message::CloseNotice => {
                self.toasts.clear();
                Task::none()
            }
            Message::Lighting(m) => pages::lighting::update(self, m),
            Message::Colours(m) => pages::colours::update(self, m),
            Message::Keys(m) => pages::keys::update(self, m),
            Message::Macros(m) => pages::macros::update(self, m),
            Message::Screen(m) => pages::screen::update(self, m),
            Message::Profiles(m) => pages::profiles::update(self, m),
            Message::Device(m) => pages::device::update(self, m),
        }
    }

    /// A colour picked in the picker called `id` ("color:<page>:<what>").
    pub fn pick_color(&mut self, id: &str, c: [u8; 3], clear_draft: bool) -> Task<Message> {
        if clear_draft {
            self.drafts.remove(id);
        }
        let mut parts = id.splitn(3, ':').skip(1);
        let (page, what) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        match page {
            "light" => pages::lighting::set_color(self, what, c),
            "paint" => {
                if what == "to" { self.colours.gradient_to = c } else { self.colours.brush = c }
                Task::none()
            }
            "screen" => pages::screen::set_text_color(self, c),
            _ => Task::none(),
        }
    }

    fn number_draft(&mut self, id: &str, value: &str) -> Task<Message> {
        if let Some(rest) = id.strip_prefix("delay:")
            && let (Ok(i), Ok(ms)) = (rest.parse::<usize>(), value.trim().parse::<u16>()) {
                return pages::macros::set_delay(self, i, ms);
            }
        Task::none()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let mut subs = vec![
            iced::time::every(Duration::from_secs(2)).map(|_| Message::Poll),
            iced::system::theme_changes().map(Message::SystemTheme),
            iced::window::close_requests().map(|_| Message::CloseRequested),
        ];
        if pages::lighting::animating(self) || pages::screen::animating(self) {
            subs.push(iced::time::every(Duration::from_millis(33)).map(Message::Tick));
        }
        Subscription::batch(subs)
    }

    // ------------------------------------------------------------ view

    pub fn view(&self) -> Element<'_, Message> {
        let page = match self.page {
            Page::Screen => pages::screen::view(self),
            Page::Lighting => pages::lighting::view(self),
            Page::Colours => pages::colours::view(self),
            Page::Keys => pages::keys::view(self),
            Page::Macros => pages::macros::view(self),
            Page::Profiles => pages::profiles::view(self),
            Page::Device => pages::device::view(self),
        };
        let main = column![self.header(), self.notices(), scrollable(container(page).padding([4.0, theme::PAGE_PAD]).width(Fill))
            .style(theme::scroll).height(Fill)]
            .spacing(12)
            .width(Fill);
        let body = row![self.sidebar(), main];
        let mut layers = vec![container(body).width(Fill).height(Fill).style(|_| container::Style {
            background: Some(roles().base.into()),
            text_color: Some(roles().text),
            ..Default::default()
        }).into()];
        if let Some(modal) = pages::screen::modal(self) {
            layers.push(modal);
        }
        if !self.toasts.is_empty() {
            layers.push(self.toast_view());
        }
        stack(layers).into()
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let mut nav = column![].spacing(2);
        for page in Page::ALL {
            let selected = self.page == page;
            nav = nav.push(
                button(text(page.title()).size(theme::TEXT).font(if selected { fonts::MEDIUM } else { fonts::REGULAR }))
                    .width(Fill)
                    .padding([8, 12])
                    .style(theme::list_item(selected))
                    .on_press(Message::Navigate(page)),
            );
        }
        let brand = column![
            text("AK980 V2").size(17).font(fonts::BOLD),
            caption(tr("Software for Linux")),
        ]
        .spacing(2);
        container(column![brand, nav, space().height(Fill), self.link_badge()].spacing(18))
            .padding([20, 12])
            .width(212)
            .height(Fill)
            .style(theme::sidebar)
            .into()
    }

    fn link_badge(&self) -> Element<'_, Message> {
        let r = roles();
        let (dot, line1, line2) = match self.status.link {
            Some(Link::Usb) => (r.success, tr("USB cable"), tr("Every setting is available")),
            Some(Link::Dongle) => (
                r.success,
                tr("2.4 GHz"),
                match self.battery {
                    Some(b) => tr!("Battery: {level}", level = format!("{b}%")),
                    None => tr("Clock and battery only"),
                },
            ),
            None => (r.overlay1, tr("Not connected"), tr("Connect the cable or the dongle")),
        };
        let light = container(space().width(8).height(8)).style(move |_| container::Style {
            background: Some(dot.into()),
            border: iced::Border { radius: 4.0.into(), ..Default::default() },
            ..Default::default()
        });
        container(
            row![light, column![text(line1).size(theme::TEXT).font(fonts::MEDIUM), caption(line2)].spacing(2)]
                .spacing(10)
                .align_y(Alignment::Center),
        )
        .padding(12)
        .width(Fill)
        .style(theme::card)
        .into()
    }

    fn header(&self) -> Element<'_, Message> {
        let names = self.cfg.names();
        let profile = pick_list(names, Some(self.cfg.active.clone()), Message::SwitchProfile)
            .style(theme::pick)
            .menu_style(theme::menu)
            .text_size(theme::TEXT)
            .padding([6, 10]);
        let mut right = row![].spacing(12).align_y(Alignment::Center);
        if let Some(b) = &self.busy {
            right = right.push(caption(b.clone()));
        }
        right = right.push(caption(tr("Profile"))).push(profile);
        row![widgets::title(self.page.title()), space().width(Fill), right]
            .align_y(Alignment::Center)
            .padding(iced::Padding { top: 20.0, right: theme::PAGE_PAD, bottom: 0.0, left: theme::PAGE_PAD })
            .into()
    }

    /// Persistent notices: what can be undone, no access, or the cable a page needs.
    fn notices(&self) -> Element<'_, Message> {
        let mut col = column![].spacing(8).padding([0.0, theme::PAGE_PAD]);
        if let Some(u) = &self.undo {
            col = col.push(
                container(
                    row![
                        text(u.what.clone()).size(theme::TEXT).width(Fill),
                        widgets::secondary_button(tr("Undo"), Some(Message::Undo)),
                    ]
                    .spacing(16)
                    .align_y(Alignment::Center),
                )
                .padding([8, 14])
                .style(theme::banner),
            );
        }
        if self.status.access == Some(false) {
            col = col.push(
                container(
                    row![
                        column![
                            text(tr("No access to the keyboard yet")).font(fonts::MEDIUM).size(theme::TEXT),
                            caption(tr("Linux lets programs talk to a keyboard only through a rule. It asks for your password once.")),
                        ]
                        .spacing(2)
                        .width(Fill),
                        widgets::primary_button(tr("Allow access"), Some(Message::Device(pages::device::Msg::AllowAccess))),
                    ]
                    .spacing(16)
                    .align_y(Alignment::Center),
                )
                .padding(14)
                .style(theme::banner),
            );
        }
        if self.page.needs_cable() && self.status.link == Some(Link::Dongle) {
            col = col.push(
                container(
                    column![
                        text(tr("Needs the USB cable")).font(fonts::MEDIUM).size(theme::TEXT),
                        caption(tr("Through the dongle the keyboard only takes the clock and reports its battery. Connect the cable for everything else.")),
                    ]
                    .spacing(2),
                )
                .padding(14)
                .width(Fill)
                .style(theme::banner),
            );
        }
        col.into()
    }

    fn toast_view(&self) -> Element<'_, Message> {
        let mut col = column![].spacing(8).align_x(Alignment::Center);
        for t in &self.toasts {
            col = col.push(
                button(container(text(t.text.clone()).size(theme::TEXT)).padding([10, 16]).style(theme::toast(t.error)))
                    .padding(0)
                    .style(theme::flat)
                    .on_press(Message::CloseNotice),
            );
        }
        container(col)
            .width(Fill)
            .height(Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::End)
            .padding(iced::Padding { bottom: 24.0, left: 212.0, ..Default::default() })
            .into()
    }
}

/// The shortest time between two writes of the settings file.
const SAVE_GAP: Duration = Duration::from_millis(500);

/// `Length` re-export for the pages.
pub const PREVIEW_HEIGHT: Length = Length::Fixed(300.0);
