//! Lighting: the keyboard's own effects, or an effect computed on the PC, with the result shown
//! on the drawing. Firmware effects go to the keyboard as they are changed.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use ak980_core::audio::AudioMeter;
use ak980_core::config::LightSource;
use ak980_core::device::WiredKeyboard;
use ak980_core::effects::{self, Effect, Inputs, ParamKind, ParamValue, EFFECTS};
use ak980_core::engine::{LightingEngine, Overlays};
use ak980_core::i18n::tr;
use ak980_core::lighting::{self, CAP_BRIGHTNESS, CAP_COLOR, CAP_DIRECTION2, CAP_DIRECTION4, CAP_RAINBOW, CAP_SPEED,
                            DIRECTIONS, LIGHT_MODES, MAX_LEVEL};
use ak980_core::sysinfo::SystemReadings;
use ak980_core::{layout, paths, time::LocalTime};
use iced::widget::{canvas, column, container, pick_list, toggler};
use iced::{Color, Element, Fill, Task};

use crate::app::{blocking, App, Message, PREVIEW_HEIGHT};
use crate::keyboard::{KeyEvent, Keyboard, Mode};
use crate::pages::Page;
use crate::theme;
use crate::widgets::{self, caption, card, color_picker, level, section, segmented, setting, Choice};

#[derive(Default)]
pub struct State {
    fx: Option<(String, String, Box<dyn Effect>)>,
    t0: Option<Instant>,
    last: Option<Instant>,
    pub preview: BTreeMap<u8, Color>,
    presses: Vec<usize>,
    sending: bool,
    dirty: bool,
    engine: Option<LightingEngine>,
    audio: Option<AudioMeter>,
    system: SystemReadings,
    overlays: Overlays,
    /// Seconds into the typing shown for "Single on" and "Single off".
    typing: f32,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Source(bool),
    Mode(Choice<u8>),
    Rainbow(bool),
    Brightness(u8),
    Speed(u8),
    Direction(u8),
    Effect(Choice<&'static str>),
    Param(&'static str, ParamValue),
    FxBrightness(u8),
    OverlayLocks(bool),
    OverlayNotifications(bool),
    Key(KeyEvent),
    Sent(Option<String>),
}

fn wrap(m: Msg) -> Message {
    Message::Lighting(m)
}

/// Sends the profile's firmware effect; changes made meanwhile are sent once it is done.
fn send(app: &mut App) -> Task<Message> {
    if !app.wired() {
        return Task::none();
    }
    if app.lighting.sending {
        app.lighting.dirty = true;
        return Task::none();
    }
    app.lighting.sending = true;
    let s = app.cfg.profile().lighting.clone();
    Task::perform(
        blocking(move || WiredKeyboard::open().and_then(|mut kb| kb.set_lighting(&s)).err().map(|e| e.to_string())),
        |r| wrap(Msg::Sent(r.flatten())),
    )
}

pub fn set_color(app: &mut App, what: &str, c: [u8; 3]) -> Task<Message> {
    if what == "firmware" {
        let p = app.cfg.profile_mut();
        p.lighting.set_rgb(c);
        p.lighting.rainbow = false;
        p.light_source = LightSource::Firmware;
        app.save();
        return send(app);
    }
    // The colours drawn over every PC effect: "overlay/locks", "overlay/notify"
    match what {
        "overlay/locks" => app.cfg.settings.overlay_lock_color = c,
        "overlay/notify" => app.cfg.settings.overlay_notify_color = c,
        _ => {}
    }
    if what.starts_with("overlay/") {
        app.save();
        return Task::none();
    }
    // A PC effect's colour setting: "fx/<param>"
    if let Some(param) = what.strip_prefix("fx/") {
        let p = app.cfg.profile_mut();
        let effect = p.host_effect.clone();
        p.host_values.entry(effect).or_default().insert(param.to_string(), ParamValue::Color(c).to_json());
        app.save();
    }
    Task::none()
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let firmware_change = |app: &mut App, f: &dyn Fn(&mut lighting::LightSettings)| {
        let p = app.cfg.profile_mut();
        f(&mut p.lighting);
        p.light_source = LightSource::Firmware;
        app.save();
        send(app)
    };
    match msg {
        Msg::Source(host) => {
            let p = app.cfg.profile_mut();
            p.light_source = if host { LightSource::Host } else { LightSource::Firmware };
            app.save();
            let engine = keep_engine(app);
            if host { engine } else { Task::batch([engine, send(app)]) }
        }
        Msg::Mode(m) => firmware_change(app, &|l| l.mode = m.value),
        Msg::Rainbow(on) => firmware_change(app, &|l| l.rainbow = on),
        Msg::Brightness(v) => firmware_change(app, &|l| l.brightness = v),
        Msg::Speed(v) => firmware_change(app, &|l| l.speed = v),
        Msg::Direction(v) => firmware_change(app, &|l| l.direction = v),
        Msg::Effect(e) => {
            let p = app.cfg.profile_mut();
            p.host_effect = e.value.to_string();
            p.light_source = LightSource::Host;
            app.save();
            keep_engine(app)
        }
        Msg::Param(id, value) => {
            let p = app.cfg.profile_mut();
            let effect = p.host_effect.clone();
            p.host_values.entry(effect).or_default().insert(id.to_string(), value.to_json());
            app.save();
            Task::none()
        }
        Msg::FxBrightness(v) => {
            app.cfg.profile_mut().host_brightness = v;
            app.save();
            Task::none()
        }
        Msg::OverlayLocks(on) => {
            app.cfg.settings.overlay_locks = on;
            app.save();
            Task::none()
        }
        Msg::OverlayNotifications(on) => {
            app.cfg.settings.overlay_notifications = on;
            app.save();
            Task::none()
        }
        Msg::Key(KeyEvent::Pressed(led) | KeyEvent::Painted(led)) => {
            let (leds, pos) = layout::positions();
            if let Some(row) = leds.iter().position(|l| *l == led) {
                app.lighting.presses.push(row);
                // An effect that spreads from a centre takes the clicked key as its centre
                if let Some(centre) = movable_centre(app) {
                    let p = app.cfg.profile_mut();
                    let values = p.host_values.entry(centre.to_string()).or_default();
                    for (axis, v) in ["centre_x", "centre_y"].iter().zip(pos[row]) {
                        values.insert(axis.to_string(), serde_json::Value::from((v * 100.0).round() as i64));
                    }
                    app.save();
                }
            }
            if let Some(engine) = &app.lighting.engine {
                engine.inject_press(led);
            }
            Task::none()
        }
        Msg::Key(KeyEvent::Erased(_) | KeyEvent::Picked(_)) => Task::none(),
        Msg::Sent(error) => {
            app.lighting.sending = false;
            if let Some(e) = error {
                app.lighting.dirty = false;
                app.notify(e, true);
                return Task::none();
            }
            if std::mem::take(&mut app.lighting.dirty) { send(app) } else { Task::none() }
        }
    }
}

/// Streams the PC effect from the window while no background agent does it.
pub fn keep_engine(app: &mut App) -> Task<Message> {
    let wanted = app.cfg.profile().light_source == LightSource::Host && !app.status.agent && app.wired();
    match (wanted, app.lighting.engine.is_some()) {
        (true, false) => {
            let mut engine = LightingEngine::new(true, None, paths::config_path());
            engine.start();
            app.lighting.engine = Some(engine);
        }
        (false, true) => {
            if let Some(mut engine) = app.lighting.engine.take() {
                engine.stop();
            }
        }
        _ => {}
    }
    Task::none()
}

/// The PC effect whose waves start from a centre that can be moved, while it is shown.
fn movable_centre(app: &App) -> Option<&'static str> {
    let p = app.cfg.profile();
    let info = effects::info(&p.host_effect)?;
    let values = p.host_values.get(info.id).cloned().unwrap_or_default();
    let centre = info.params.iter().find(|prm| prm.id == "centre_x")?;
    (p.light_source == LightSource::Host && !info.needs_keys && centre.shown(&values, info)).then_some(info.id)
}

pub fn animating(app: &App) -> bool {
    let p = app.cfg.profile();
    app.page == Page::Lighting
        && (p.light_source == LightSource::Host || (p.light_source == LightSource::Firmware && reacts_to_keys(p.lighting.mode)))
}

/// "Single on" lights the key being pressed, "Single off" darkens it: the drawing shows them typing.
fn reacts_to_keys(mode: u8) -> bool {
    matches!(mode, 2 | 3)
}

/// The keys typed in the preview of the effects that react to keys, one every `TYPING_STEP` seconds.
const TYPING: [&str; 12] = ["H", "E", "L", "L", "O", "Space", "W", "O", "R", "L", "D", "Enter"];
const TYPING_STEP: f32 = 0.24;
/// How long a key stays lit after its press, as the firmware lets it fade.
const TYPING_FADE: f32 = 0.7;

/// How strongly each key is pressed `t` seconds into the typing, 1 just pressed to 0 faded.
fn typing(t: f32) -> BTreeMap<u8, f32> {
    let cycle = TYPING.len() as f32 * TYPING_STEP + 1.2; // a pause before it starts again
    let t = t % cycle;
    let mut lit = BTreeMap::new();
    for (i, name) in TYPING.iter().enumerate() {
        let since = t - i as f32 * TYPING_STEP;
        if (0.0..TYPING_FADE).contains(&since)
            && let Ok(k) = layout::find_key(name)
        {
            let v = 1.0 - since / TYPING_FADE;
            let e = lit.entry(k.index).or_insert(0.0_f32);
            *e = e.max(v * v);
        }
    }
    lit
}

/// The next preview frame of the PC effect.
pub fn tick(app: &mut App, now: Instant) {
    let host = app.cfg.profile().light_source == LightSource::Host;
    if !animating(app) || !host {
        if let Some(mut audio) = app.lighting.audio.take() {
            audio.stop();
        }
        if animating(app) {
            let t0 = *app.lighting.t0.get_or_insert(now);
            app.lighting.typing = now.duration_since(t0).as_secs_f32();
        }
        return;
    }
    let p = app.cfg.profile();
    let values = p.host_values.get(&p.host_effect).cloned().unwrap_or_default();
    let key = serde_json::to_string(&values).unwrap_or_default();
    let (leds, pos) = layout::positions();
    let st = &mut app.lighting;
    if st.fx.as_ref().is_none_or(|(id, k, _)| *id != p.host_effect || *k != key) {
        st.fx = Some((p.host_effect.clone(), key, effects::create(&p.host_effect, pos, &values)));
    }
    let Some((_, _, fx)) = st.fx.as_mut() else { return };
    let needs_audio = fx.info().needs_audio;
    if needs_audio && st.audio.is_none() {
        let mut meter = AudioMeter::new();
        meter.start();
        st.audio = Some(meter);
    } else if !needs_audio
        && let Some(mut audio) = st.audio.take() {
            audio.stop();
        }
    let t0 = *st.t0.get_or_insert(now);
    let dt = st.last.map(|l| now.duration_since(l).as_secs_f32()).unwrap_or(1.0 / 30.0).clamp(0.001, 0.2);
    st.last = Some(now);
    let mut inp = Inputs { presses: std::mem::take(&mut st.presses), ..Inputs::default() };
    if let Some(audio) = &st.audio {
        let (bands, level, beat) = audio.snapshot();
        inp.bands = bands;
        inp.level = level;
        inp.beat = beat;
    }
    if fx.info().needs_system {
        inp.system = st.system.get();
    }
    let mut colors = fx.render(now.duration_since(t0).as_secs_f32(), dt, &inp);
    st.overlays.apply(&mut colors, &app.cfg.settings);
    let k = p.host_brightness as f32 / 100.0;
    st.preview = leds.iter().zip(colors).map(|(l, c)| (*l, Color::from_rgb(c[0] * k, c[1] * k, c[2] * k))).collect();
}

/// What the keyboard shows for a firmware effect, without its motion.
fn firmware_preview(app: &App) -> BTreeMap<u8, Color> {
    let l = &app.cfg.profile().lighting;
    let m = lighting::mode_by_id(l.mode);
    if l.mode == 0 {
        return BTreeMap::new();
    }
    let k = if m.has(CAP_BRIGHTNESS) { l.brightness as f32 / MAX_LEVEL as f32 } else { 1.0 };
    let (leds, pos) = layout::positions();
    let rainbow = l.rainbow || !m.has(CAP_COLOR);
    // The effects that react to keys show them typing: a pressed key lights up ("Single on") or
    // goes dark ("Single off"), then fades back
    let pressed = reacts_to_keys(l.mode).then(|| typing(app.lighting.typing));
    leds.iter()
        .zip(pos)
        .filter_map(|(led, p)| {
            let c = if rainbow {
                effects::hsv(p[0] * 0.9, 1.0, 1.0)
            } else {
                [l.r as f32 / 255.0, l.g as f32 / 255.0, l.b as f32 / 255.0]
            };
            let lit = match (&pressed, l.mode) {
                (Some(keys), 2) => keys.get(led).copied().unwrap_or(0.0),
                (Some(keys), _) => 1.0 - keys.get(led).copied().unwrap_or(0.0),
                (None, _) => 1.0,
            };
            (lit > 0.01).then(|| (*led, Color::from_rgb(c[0] * k * lit, c[1] * k * lit, c[2] * k * lit)))
        })
        .collect()
}

pub fn custom_preview(app: &App) -> BTreeMap<u8, Color> {
    let p = app.cfg.profile();
    let k = p.custom_brightness as f32 / MAX_LEVEL as f32;
    p.custom_colors
        .iter()
        .map(|(i, c)| (*i, Color::from_rgb(c[0] as f32 / 255.0 * k, c[1] as f32 / 255.0 * k, c[2] as f32 / 255.0 * k)))
        .collect()
}

pub fn clock() -> String {
    let t = LocalTime::now();
    format!("{:02}:{:02}", t.hour, t.minute)
}

pub fn drawing<'a>(leds: BTreeMap<u8, Color>, selected: BTreeSet<u8>, marked: BTreeSet<u8>, mode: Mode,
                   on: impl Fn(KeyEvent) -> Message + 'a) -> Element<'a, Message> {
    let kb = Keyboard { leds, selected, marked, mode, on_event: Some(Box::new(on)), clock: clock() };
    container(canvas(kb).width(Fill).height(PREVIEW_HEIGHT)).padding(12).style(theme::stage).into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let p = app.cfg.profile();
    let source = p.light_source;
    let leds = match source {
        LightSource::Host => app.lighting.preview.clone(),
        LightSource::Custom => custom_preview(app),
        LightSource::Firmware => firmware_preview(app),
    };
    let mode = if source == LightSource::Host { Mode::Select } else { Mode::View };
    let mut page = column![drawing(leds, BTreeSet::new(), BTreeSet::new(), mode, |e| wrap(Msg::Key(e)))].spacing(20);

    page = page.push(segmented(
        vec![(false, tr("On the keyboard itself")), (true, tr("PC effects"))],
        source == LightSource::Host,
        |host| wrap(Msg::Source(host)),
    ));
    match source {
        LightSource::Host => page = page.push(pc_effect(app)),
        LightSource::Custom => {
            page = page.push(card(vec![setting(
                tr("This profile shows its own key colours"),
                Some(tr("Change them on the Colours page, or pick an effect here to use it instead.")),
                widgets::secondary_button(tr("Colours"), Some(Message::Navigate(Page::Colours))),
            )]));
            page = page.push(firmware_effect(app));
        }
        LightSource::Firmware => page = page.push(firmware_effect(app)),
    }
    page.into()
}

fn firmware_effect(app: &App) -> Element<'_, Message> {
    let l = &app.cfg.profile().lighting;
    let m = lighting::mode_by_id(l.mode);
    let modes: Vec<Choice<u8>> = LIGHT_MODES.iter().map(|m| Choice::new(m.id, tr(m.name))).collect();
    let current = modes.iter().find(|c| c.value == l.mode).cloned();
    let mut rows = vec![setting(
        tr("Effect"),
        Some(tr("Stored in the keyboard; it runs without this app.")),
        pick_list(modes, current, |c| wrap(Msg::Mode(c))).style(theme::pick).menu_style(theme::menu).width(220),
    )];
    if m.has(CAP_RAINBOW) {
        rows.push(setting(tr("Rainbow instead of one colour"), None,
                          toggler(l.rainbow).on_toggle(|v| wrap(Msg::Rainbow(v))).style(theme::switch)));
    }
    if m.has(CAP_COLOR) && !l.rainbow {
        rows.push(setting(tr("Colour"), None, color_picker("color:light:firmware".into(), l.rgb(),
                                                            app.drafts.get("color:light:firmware"),
                                                            |c| Message::Draft("color:light:firmware".into(), widgets::html(c)))));
    }
    if m.has(CAP_BRIGHTNESS) {
        rows.push(setting(tr("Brightness"), None, level(l.brightness, MAX_LEVEL, |v| wrap(Msg::Brightness(v)))));
    }
    if m.has(CAP_SPEED) {
        rows.push(setting(tr("Speed"), None, level(l.speed, MAX_LEVEL, |v| wrap(Msg::Speed(v)))));
    }
    if m.has(CAP_DIRECTION4) || m.has(CAP_DIRECTION2) {
        let dirs: Vec<(u8, String)> = DIRECTIONS
            .iter()
            .enumerate()
            .filter(|(i, _)| m.has(CAP_DIRECTION4) || *i == 0 || *i == 3)
            .map(|(i, d)| (i as u8, tr(d)))
            .collect();
        rows.push(setting(tr("Direction"), None, segmented(dirs, l.direction, |d| wrap(Msg::Direction(d)))));
    }
    let hint = if app.wired() { None } else { Some(caption(tr("Saved in the profile; it goes to the keyboard when the cable is connected."))) };
    let mut col = column![section(tr("Keyboard effects"), card(rows))].spacing(8);
    if let Some(h) = hint {
        col = col.push(h);
    }
    col.into()
}

/// What is drawn over every PC effect: the lock keys and a flash for notifications.
fn overlays(app: &App) -> Element<'_, Message> {
    let s = &app.cfg.settings;
    let colour = |key: &'static str, c: [u8; 3]| -> Element<'_, Message> {
        let key = format!("color:light:overlay/{key}");
        color_picker(key.clone(), c, app.drafts.get(&key), move |c| Message::Draft(key.clone(), widgets::html(c)))
    };
    let mut rows = vec![setting(
        tr("Lock keys"),
        Some(tr("Caps Lock and Num Lock light up in their own colour while they are on.")),
        toggler(s.overlay_locks).on_toggle(|v| wrap(Msg::OverlayLocks(v))).style(theme::switch),
    )];
    if s.overlay_locks {
        rows.push(setting(tr("Their colour"), None, colour("locks", s.overlay_lock_color)));
    }
    rows.push(setting(
        tr("Notifications"),
        Some(tr("The keyboard flashes twice when a desktop notification comes. The app only sees that one came, never its text.")),
        toggler(s.overlay_notifications).on_toggle(|v| wrap(Msg::OverlayNotifications(v))).style(theme::switch),
    ));
    if s.overlay_notifications {
        rows.push(setting(tr("Flash colour"), None, colour("notify", s.overlay_notify_color)));
    }
    section(tr("Over every PC effect"), card(rows))
}

fn pc_effect(app: &App) -> Element<'_, Message> {
    let p = app.cfg.profile();
    let info = effects::info(&p.host_effect).unwrap_or(&EFFECTS[0]);
    let values = p.host_values.get(info.id).cloned().unwrap_or_default();
    let list: Vec<Choice<&'static str>> = EFFECTS.iter().map(|e| Choice::new(e.id, tr(e.name))).collect();
    let current = list.iter().find(|c| c.value == info.id).cloned();
    let mut rows = vec![setting(
        tr("Effect"),
        Some(tr(info.description)),
        pick_list(list, current, |c| wrap(Msg::Effect(c))).style(theme::pick).menu_style(theme::menu).width(220),
    )];
    for prm in info.params {
        if !prm.shown(&values, info) {
            continue;
        }
        let id = prm.id;
        let control: Element<'_, Message> = match (prm.kind, effects::value(prm, &values)) {
            (ParamKind::Color(_), ParamValue::Color(c)) => {
                let key = format!("color:light:fx/{id}");
                color_picker(key.clone(), c, app.drafts.get(&key), move |c| Message::Draft(key.clone(), widgets::html(c)))
            }
            (ParamKind::Range { lo, hi, .. }, ParamValue::Int(v)) => {
                widgets::ranged(v, lo, hi, "", move |x| wrap(Msg::Param(id, ParamValue::Int(x))))
            }
            (ParamKind::Choice { choices, .. }, ParamValue::Text(t)) => {
                let items: Vec<Choice<&'static str>> = choices.iter().map(|(v, l)| Choice::new(*v, tr(l))).collect();
                let current = items.iter().find(|c| c.value == t).cloned();
                pick_list(items, current, move |c| wrap(Msg::Param(id, ParamValue::Text(c.value.to_string()))))
                    .style(theme::pick)
                    .menu_style(theme::menu)
                    .width(220)
                    .into()
            }
            (ParamKind::Toggle(_), ParamValue::Bool(b)) => {
                toggler(b).on_toggle(move |v| wrap(Msg::Param(id, ParamValue::Bool(v)))).style(theme::switch).into()
            }
            _ => continue,
        };
        rows.push(setting(tr(prm.label), None, control));
    }
    rows.push(setting(tr("Brightness"), None,
                      widgets::ranged(p.host_brightness as i32, 0, 100, "%", |v| wrap(Msg::FxBrightness(v as u8)))));
    let who = if app.status.agent {
        tr("The background agent streams it to the keyboard, also when this window is closed.")
    } else {
        tr("Runs while this window is open. Turn on the background agent (Device page) to keep it running.")
    };
    let mut col = column![section(tr("PC effects"), card(rows)), caption(who)].spacing(8);
    if info.needs_keys {
        col = col.push(caption(tr("Click keys on the drawing to try it.")));
    } else if movable_centre(app).is_some() {
        col = col.push(caption(tr("Click a key on the drawing to move the centre there.")));
    }
    col.push(overlays(app)).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_presses_one_key_after_another_and_lets_them_fade() {
        let h = layout::find_key("H").unwrap().index;
        let e = layout::find_key("E").unwrap().index;
        assert_eq!(typing(0.0).get(&h), Some(&1.0));
        let later = typing(TYPING_STEP + 0.01);
        assert!(later[&e] > 0.9 && later[&h] < 1.0, "E pressed, H fading");
        assert!(!typing(TYPING_FADE + 0.01).contains_key(&h), "H faded out");
    }
}
