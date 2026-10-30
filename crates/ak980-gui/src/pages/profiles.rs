//! Profiles: switching, copies, export and import, a profile per program, launcher keys.

use ak980_core::config::{self, Launcher, Profile, Rule};
use ak980_core::device::WiredKeyboard;
use ak980_core::i18n::tr;
use ak980_core::keymap::LAUNCHER_KEYS;
use ak980_core::profiles::{apply_profile, ALL_PARTS};
use ak980_core::tr;
use iced::widget::{button, column, pick_list, row, text, text_input, toggler};
use iced::{Alignment, Element, Fill, Task};

use crate::app::{App, Message};
use crate::fonts;
use crate::theme;
use crate::widgets::{self, caption, card, section, setting, Choice};

#[derive(Default)]
pub struct State {
    pub new_name: String,
    pub rename: String,
    pub process: String,
    pub rule_profile: Option<String>,
    pub launcher_key: Option<String>,
    pub command: String,
}

#[derive(Debug, Clone)]
pub enum Msg {
    NewName(String),
    Create(bool),
    Rename(String),
    DoRename,
    Delete,
    Export,
    Exported(Option<std::path::PathBuf>),
    Import,
    Imported(Option<std::path::PathBuf>),
    Apply,
    AutoSwitch(bool),
    Process(String),
    RuleProfile(String),
    AddRule,
    RemoveRule(usize),
    DefaultProfile(Choice<String>),
    LauncherKey(String),
    Command(String),
    AddLauncher,
    RemoveLauncher(usize),
}

fn wrap(m: Msg) -> Message {
    Message::Profiles(m)
}

fn send_profile(app: &mut App) -> Task<Message> {
    if !app.wired() {
        return Task::none();
    }
    let p = app.cfg.profile().clone();
    app.device_task(tr("Sending the profile…"), move || {
        let mut kb = WiredKeyboard::open()?;
        apply_profile(&mut kb, &p, &ALL_PARTS, &mut |_| {})?;
        Ok(tr!("Profile “{name}” is on the keyboard", name = p.name))
    })
}

/// Makes `name` the active profile and sends it to the keyboard over the cable.
pub fn use_profile(app: &mut App, name: String) -> Task<Message> {
    if app.cfg.get(&name).is_none() || app.cfg.active == name {
        return Task::none();
    }
    app.cfg.active = name;
    app.profiles.rename.clear();
    app.save();
    send_profile(app)
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let st = &mut app.profiles;
    match msg {
        Msg::NewName(t) => st.new_name = t,
        Msg::Create(copy) => {
            let name = if st.new_name.trim().is_empty() { tr("Profile") } else { st.new_name.trim().to_string() };
            st.new_name.clear();
            let p = if copy { Profile { name, ..app.cfg.profile().clone() } } else { Profile::named(name) };
            let created = app.cfg.add(p);
            app.notify(tr!("Profile created: {name}", name = created), false);
            app.cfg.active = created;
            app.profiles.rename.clear();
            app.save();
            // The keyboard shows the active profile, as when switching to one
            return send_profile(app);
        }
        Msg::Rename(t) => st.rename = t,
        Msg::DoRename => {
            let old = app.cfg.active.clone();
            let new = if st.rename.is_empty() { old.clone() } else { st.rename.clone() };
            match app.cfg.rename(&old, &new) {
                Ok(_) => {
                    app.profiles.rename.clear();
                    app.save();
                }
                Err(e) => app.notify(e.to_string(), true),
            }
        }
        Msg::Delete => {
            let name = app.cfg.active.clone();
            match app.cfg.remove(&name) {
                Ok(()) => {
                    app.profiles.rename.clear();
                    app.save();
                    return send_profile(app);
                }
                Err(e) => app.notify(e.to_string(), true),
            }
        }
        Msg::Export => {
            let dialog = rfd::AsyncFileDialog::new().set_title(tr("Export profile"))
                .set_file_name(format!("{}.ak980.json", app.cfg.active));
            return Task::perform(async move { dialog.save_file().await.map(|h| h.path().to_path_buf()) },
                                 |p| wrap(Msg::Exported(p)));
        }
        Msg::Exported(Some(path)) => match config::export_profile(app.cfg.profile(), &path) {
            Ok(()) => app.notify(tr("Profile exported"), false),
            Err(e) => app.notify(e.to_string(), true),
        },
        Msg::Import => {
            let dialog = rfd::AsyncFileDialog::new().set_title(tr("Import profile")).add_filter("AK980", &["json"]);
            return Task::perform(async move { dialog.pick_file().await.map(|h| h.path().to_path_buf()) },
                                 |p| wrap(Msg::Imported(p)));
        }
        Msg::Imported(Some(path)) => match config::import_profile(&path) {
            Ok(p) => {
                let name = app.cfg.add(p);
                app.notify(tr!("Profile imported: {name}", name = name), false);
                app.save();
            }
            Err(e) => app.notify(e.to_string(), true),
        },
        Msg::Exported(None) | Msg::Imported(None) => {}
        Msg::Apply => return send_profile(app),
        Msg::AutoSwitch(on) => {
            app.cfg.settings.auto_switch = on;
            app.save();
        }
        Msg::Process(t) => st.process = t,
        Msg::RuleProfile(p) => st.rule_profile = Some(p),
        Msg::AddRule => {
            let process = st.process.trim().to_string();
            if let (false, Some(profile)) = (process.is_empty(), st.rule_profile.clone()) {
                let rules = &mut app.cfg.settings.rules;
                rules.retain(|r| !r.process.eq_ignore_ascii_case(&process));
                rules.push(Rule { process, profile });
                app.profiles.process.clear();
                app.save();
            }
        }
        Msg::RemoveRule(i) => {
            if i < app.cfg.settings.rules.len() {
                app.cfg.settings.rules.remove(i);
                app.save();
            }
        }
        Msg::DefaultProfile(c) => {
            app.cfg.settings.default_profile = c.value;
            app.save();
        }
        Msg::LauncherKey(k) => st.launcher_key = Some(k),
        Msg::Command(t) => st.command = t,
        Msg::AddLauncher => {
            if let (Some(key), false) = (st.launcher_key.clone(), st.command.trim().is_empty()) {
                let command = st.command.trim().to_string();
                let list = &mut app.cfg.settings.launchers;
                list.retain(|l| l.key != key);
                list.push(Launcher { key, command, label: String::new() });
                app.profiles.command.clear();
                app.profiles.launcher_key = None;
                app.save();
            }
        }
        Msg::RemoveLauncher(i) => {
            if i < app.cfg.settings.launchers.len() {
                app.cfg.settings.launchers.remove(i);
                app.save();
            }
        }
    }
    Task::none()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let st = &app.profiles;
    let mut list = column![].spacing(2);
    for p in &app.cfg.profiles {
        let active = p.name == app.cfg.active;
        list = list.push(
            button(row![
                text(p.name.clone()).size(theme::TEXT).width(Fill),
                caption(if active { tr("Active profile") } else { String::new() }),
            ])
            .width(Fill)
            .padding([8, 12])
            .style(theme::list_item(active))
            .on_press(Message::SwitchProfile(p.name.clone())),
        );
    }
    let rename_value = if st.rename.is_empty() { app.cfg.active.clone() } else { st.rename.clone() };
    let profiles = vec![
        list.into(),
        setting(tr("Name"), None, row![
            text_input("", &rename_value).on_input(|t| wrap(Msg::Rename(t))).on_submit(wrap(Msg::DoRename)).padding(6)
                .style(theme::input).width(220),
            widgets::secondary_button(tr("Rename"), Some(wrap(Msg::DoRename))),
        ].spacing(8).align_y(Alignment::Center)),
        row![
            widgets::primary_button(tr("Send everything again"), app.wired().then(|| wrap(Msg::Apply))),
            widgets::secondary_button(tr("Export…"), Some(wrap(Msg::Export))),
            widgets::secondary_button(tr("Import…"), Some(wrap(Msg::Import))),
            iced::widget::space().width(Fill),
            widgets::danger_button(tr("Delete"), (app.cfg.profiles.len() > 1).then(|| wrap(Msg::Delete))),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into(),
        setting(tr("New profile"), None, row![
            text_input(&tr("Name"), &st.new_name).on_input(|t| wrap(Msg::NewName(t))).on_submit(wrap(Msg::Create(false)))
                .padding(6).style(theme::input).width(180),
            widgets::secondary_button(tr("Create"), Some(wrap(Msg::Create(false)))),
            widgets::secondary_button(tr("Duplicate"), Some(wrap(Msg::Create(true)))),
        ].spacing(8).align_y(Alignment::Center)),
    ];

    let s = &app.cfg.settings;
    let names = app.cfg.names();
    let mut rules = vec![setting(tr("Switch profiles when a program starts"),
                                 Some(tr("The background agent watches the running programs.")),
                                 toggler(s.auto_switch).on_toggle(|v| wrap(Msg::AutoSwitch(v))).style(theme::switch))];
    for (i, r) in s.rules.iter().enumerate() {
        rules.push(row![
            text(r.process.clone()).font(fonts::MONO).size(theme::TEXT).width(Fill),
            text(format!("→ {}", r.profile)).size(theme::TEXT).width(Fill),
            button(text(tr("Remove")).size(theme::SMALL)).style(theme::flat).padding([4, 8]).on_press(wrap(Msg::RemoveRule(i))),
        ].align_y(Alignment::Center).into());
    }
    rules.push(row![
        text_input(&tr("Program, e.g. steam"), &st.process).on_input(|t| wrap(Msg::Process(t))).padding(6)
            .style(theme::input).width(Fill),
        pick_list(names.clone(), st.rule_profile.clone(), |p| wrap(Msg::RuleProfile(p))).placeholder(tr("Profile"))
            .style(theme::pick).menu_style(theme::menu).width(180),
        widgets::secondary_button(tr("Add"), (!st.process.trim().is_empty() && st.rule_profile.is_some()).then(|| wrap(Msg::AddRule))),
    ].spacing(8).align_y(Alignment::Center).into());
    let mut back: Vec<Choice<String>> = vec![Choice::new(String::new(), tr("Go back to the previous profile"))];
    back.extend(names.iter().map(|n| Choice::new(n.clone(), n.clone())));
    let current_back = back.iter().find(|c| c.value == s.default_profile).cloned();
    rules.push(setting(tr("After the program exits"), None,
                       pick_list(back, current_back, |c| wrap(Msg::DefaultProfile(c))).style(theme::pick)
                           .menu_style(theme::menu).width(240)));

    let mut launchers = vec![];
    for (i, l) in s.launchers.iter().enumerate() {
        launchers.push(row![
            text(l.key.clone()).font(fonts::MEDIUM).size(theme::TEXT).width(60),
            text(l.command.clone()).font(fonts::MONO).size(theme::TEXT).width(Fill),
            button(text(tr("Remove")).size(theme::SMALL)).style(theme::flat).padding([4, 8]).on_press(wrap(Msg::RemoveLauncher(i))),
        ].align_y(Alignment::Center).into());
    }
    let free: Vec<String> = LAUNCHER_KEYS.iter().filter(|k| !s.launchers.iter().any(|l| l.key == **k)).map(|k| k.to_string()).collect();
    launchers.push(row![
        pick_list(free, st.launcher_key.clone(), |k| wrap(Msg::LauncherKey(k))).placeholder(tr("Key")).style(theme::pick)
            .menu_style(theme::menu).width(100),
        text_input(&tr("e.g. steam, obs or https://…"), &st.command).on_input(|t| wrap(Msg::Command(t))).padding(6)
            .style(theme::input).width(Fill),
        widgets::secondary_button(tr("Add"), (st.launcher_key.is_some() && !st.command.trim().is_empty()).then(|| wrap(Msg::AddLauncher))),
    ].spacing(8).align_y(Alignment::Center).into());
    launchers.push(caption(tr("Then give a key one of these on the Keys page (Key or shortcut → F13…F24); the agent runs the command.")).into());

    column![
        section(tr("Profiles"), card(profiles)),
        section(tr("Games and programs"), card(rules)),
        section(tr("Launcher keys"), card(launchers)),
    ]
    .spacing(20)
    .into()
}
