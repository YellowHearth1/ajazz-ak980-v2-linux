//! Streams the active profile's PC lighting effect to the keyboard.
//!
//! Runs inside the background agent, or inside the app window when no agent is running. It follows
//! the config file: when the profile switches to a firmware effect or per-key colours, the stream
//! stops and the keyboard gets the profile's own lighting back.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use log::warn;

use crate::audio::AudioMeter;
use crate::config::{self, Config, LightSource, Profile, Settings};
use crate::device::WiredKeyboard;
use crate::effects::{Effect, Inputs, Rgbf, attention, create, mix, to_bytes};
use crate::keyinput::KeyListener;
use crate::layout::{by_code, positions};
use crate::profiles::apply_lighting;
use crate::sysinfo::{NotificationWatch, SystemReadings, lock_keys};

/// Pause between the reports of one real-time frame; closer reports make the keyboard stall.
const REALTIME_GAP: Duration = Duration::from_millis(2);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Idle,
    Streaming,
    NoCable,
    Error,
}

/// Receives every frame, already scaled by the profile's brightness, for the on-screen keyboard.
pub type FrameSink = Arc<dyn Fn(Vec<Rgbf>) + Send + Sync>;

/// A call for attention over whatever the keyboard shows, from a reminder.
#[derive(Clone, Debug)]
pub struct Alert {
    /// One of `effects::ATTENTION`.
    pub effect: String,
    pub color: [u8; 3],
    pub started: Instant,
    pub until: Instant,
}

struct Shared {
    stop: AtomicBool,
    status: Mutex<Status>,
    presses: Mutex<Vec<usize>>,
    preview: Mutex<Option<Profile>>,
    audio_error: Mutex<Option<String>>,
    alert: Mutex<Option<Alert>>,
}

pub struct LightingEngine {
    pub drive_device: bool,
    on_frame: Option<FrameSink>,
    config_path: PathBuf,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl LightingEngine {
    pub fn new(drive_device: bool, on_frame: Option<FrameSink>, config_path: PathBuf) -> Self {
        let shared = Shared {
            stop: AtomicBool::new(false),
            status: Mutex::new(Status::Idle),
            presses: Mutex::default(),
            preview: Mutex::default(),
            audio_error: Mutex::default(),
            alert: Mutex::default(),
        };
        Self { drive_device, on_frame, config_path, shared: Arc::new(shared), thread: None }
    }

    pub fn start(&mut self) {
        if self.thread.as_ref().is_some_and(|t| !t.is_finished()) {
            return;
        }
        self.shared.stop.store(false, Ordering::Relaxed);
        let (shared, sink, path, drive) =
            (self.shared.clone(), self.on_frame.clone(), self.config_path.clone(), self.drive_device);
        self.thread =
            std::thread::Builder::new().name("ak980-engine".into()).spawn(move || run(shared, sink, path, drive)).ok();
    }

    pub fn stop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    pub fn status(&self) -> Status {
        *self.shared.status.lock().unwrap()
    }

    pub fn audio_error(&self) -> Option<String> {
        self.shared.audio_error.lock().unwrap().clone()
    }

    /// Lets the app feed clicks on the on-screen keyboard to reactive effects.
    pub fn inject_press(&self, led_index: u8) {
        if let Some(row) = positions().0.iter().position(|&i| i == led_index) {
            self.shared.presses.lock().unwrap().push(row);
        }
    }

    /// Calls for attention with `effect` in `color` for `duration`, over any lighting; the
    /// keyboard's own lighting comes back after it.
    pub fn alert(&self, effect: &str, color: [u8; 3], duration: Duration) {
        let now = Instant::now();
        *self.shared.alert.lock().unwrap() =
            Some(Alert { effect: effect.to_string(), color, started: now, until: now + duration });
    }

    /// The app shows an unsaved profile's effect without touching the config file.
    pub fn set_preview_profile(&self, profile: Option<Profile>) {
        *self.shared.preview.lock().unwrap() = profile;
    }
}

impl Drop for LightingEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Plays a call for attention on `kb` for `duration` (for trying one out); the caller gives the
/// keyboard its lighting back afterwards.
pub fn play_attention(kb: &mut WiredKeyboard, effect: &str, color: [u8; 3], duration: Duration) -> crate::Result<()> {
    let (leds, pos) = positions();
    let start = Instant::now();
    while start.elapsed() < duration {
        let colors = attention(effect, color, start.elapsed().as_secs_f32(), pos);
        kb.send_realtime(&to_bytes(&colors, leds, 1.0), REALTIME_GAP)?;
        std::thread::sleep(Duration::from_millis(40));
    }
    Ok(())
}

/// Sleeps up to `d`, waking early when the engine is asked to stop.
fn wait(stop: &AtomicBool, d: Duration) {
    let end = Instant::now() + d;
    while !stop.load(Ordering::Relaxed) {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        std::thread::sleep(left.min(Duration::from_millis(10)));
    }
}

struct Device {
    drive: bool,
    kb: Option<WiredKeyboard>,
    next_open: Instant,
}

impl Device {
    fn get(&mut self) -> Option<&mut WiredKeyboard> {
        if !self.drive {
            return None;
        }
        if self.kb.is_none() && Instant::now() >= self.next_open {
            self.next_open = Instant::now() + Duration::from_secs(2);
            if let Some((cmd, lcd)) = WiredKeyboard::find() {
                match WiredKeyboard::open_found(&cmd, lcd.as_ref()) {
                    Ok(kb) => self.kb = Some(kb),
                    Err(e) => warn!("cannot open the keyboard: {e}"),
                }
            }
        }
        self.kb.as_mut()
    }

    fn drop_device(&mut self) {
        self.kb = None;
        self.next_open = Instant::now() + Duration::from_secs(2);
    }

    fn restore(&mut self, profile: &Profile) {
        if let Some(kb) = self.get()
            && let Err(e) = apply_lighting(kb, profile)
        {
            warn!("cannot restore lighting: {e}");
            self.drop_device();
        }
    }
}

/// What is drawn over any PC effect, as the settings ask: Caps Lock, Num Lock and Scroll Lock in
/// their own colour while they are on, and a short double flash when a desktop notification comes.
/// Both come from the PC, so they work with every effect and in the app's preview alike.
pub struct Overlays {
    /// Rows (into the key arrays) of Caps Lock, Num Lock and Scroll Lock, where the layout has them.
    lock_rows: [Option<usize>; 3],
    locks: [bool; 3],
    locks_at: Option<Instant>,
    watch: Option<NotificationWatch>,
    seen: u32,
    flash_from: Option<Instant>,
}

/// How long a notification flash lasts.
const FLASH: Duration = Duration::from_millis(900);

impl Default for Overlays {
    fn default() -> Self {
        let row = |code: u8| by_code(code).and_then(|k| positions().0.iter().position(|&i| i == k.index));
        Self { lock_rows: [row(0x39), row(0x53), row(0x47)], locks: [false; 3], locks_at: None, watch: None, seen: 0,
               flash_from: None }
    }
}

impl Overlays {
    pub fn apply(&mut self, colors: &mut [Rgbf], settings: &Settings) {
        if settings.overlay_locks {
            if self.locks_at.is_none_or(|t| t.elapsed() >= Duration::from_millis(150)) {
                self.locks_at = Some(Instant::now());
                self.locks = lock_keys();
            }
            let c = settings.overlay_lock_color.map(|v| v as f32 / 255.0);
            for (row, on) in self.lock_rows.iter().zip(self.locks) {
                if let Some(color) = row.filter(|_| on).and_then(|r| colors.get_mut(r)) {
                    *color = c;
                }
            }
        }
        if settings.overlay_notifications {
            if self.watch.is_none() {
                self.watch = NotificationWatch::start();
                self.seen = self.watch.as_ref().map_or(0, |w| w.count());
            }
            if let Some(count) = self.watch.as_ref().map(|w| w.count()).filter(|c| *c != self.seen) {
                self.seen = count;
                self.flash_from = Some(Instant::now());
            }
            if let Some(a) = self.flash_from.map(|t| flash(t.elapsed())) {
                let c = settings.overlay_notify_color.map(|v| v as f32 / 255.0);
                for color in colors.iter_mut() {
                    *color = mix(*color, c, a);
                }
            }
        } else {
            self.watch = None; // stops dbus-monitor
        }
    }
}

/// How strongly the notification colour covers the keys `elapsed` into a flash: two pulses, the
/// second weaker, then nothing.
fn flash(elapsed: Duration) -> f32 {
    let t = elapsed.as_secs_f32() / FLASH.as_secs_f32();
    if t >= 1.0 {
        return 0.0;
    }
    (1.0 - t) * (0.5 - 0.5 * (t * std::f32::consts::TAU * 2.0).cos())
}

struct Sources {
    keys: Option<KeyListener>,
    audio: AudioMeter,
    system: SystemReadings,
    overlays: Overlays,
}

impl Sources {
    fn set(&mut self, effect: Option<&dyn Effect>, shared: &Arc<Shared>) {
        let (keys, audio) = effect.map_or((false, false), |e| (e.info().needs_keys, e.info().needs_audio));
        if keys && self.keys.is_none() {
            let rows: HashMap<u8, usize> = positions().0.iter().enumerate().map(|(row, &idx)| (idx, row)).collect();
            let shared = shared.clone();
            self.keys = Some(KeyListener::start(move |usage, pressed| {
                if pressed
                    && let Some(row) = by_code(usage).and_then(|k| rows.get(&k.index))
                {
                    shared.presses.lock().unwrap().push(*row);
                }
            }));
        } else if !keys {
            self.keys = None; // dropping the listener stops it
        }
        if audio {
            self.audio.start();
        } else if self.audio.running() {
            self.audio.stop();
        }
        *shared.audio_error.lock().unwrap() = self.audio.error();
    }
}

fn run(shared: Arc<Shared>, sink: Option<FrameSink>, path: PathBuf, drive: bool) {
    let (leds, pos) = positions();
    let mut cfg = Config::load_from(&path);
    crate::device::set_white_balance(cfg.settings.white_balance);
    let mut seen: Option<SystemTime> = config::mtime(&path);
    let mut device = Device { drive, kb: None, next_open: Instant::now() };
    let mut inputs = Sources { keys: None, audio: AudioMeter::new(), system: SystemReadings::default(),
                             overlays: Overlays::default() };
    let mut effect: Option<Box<dyn Effect>> = None;
    let mut effect_key = String::new();
    let mut streaming = false;
    let (mut t0, mut last) = (Instant::now(), Instant::now());
    let mut next_check = Instant::now();
    let set_status = |s: Status| *shared.status.lock().unwrap() = s;

    while !shared.stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        if now >= next_check {
            next_check = now + Duration::from_millis(250);
            let m = config::mtime(&path);
            if m != seen {
                seen = m;
                cfg = Config::load_from(&path);
                if crate::device::white_balance() != cfg.settings.white_balance {
                    crate::device::set_white_balance(cfg.settings.white_balance);
                    device.drop_device(); // opened again with the new balance
                }
            }
        }
        let profile = shared.preview.lock().unwrap().clone().unwrap_or_else(|| cfg.profile().clone());
        let alert = shared.alert.lock().unwrap().clone();
        if let Some(a) = alert.as_ref().filter(|a| now < a.until) {
            let colors = attention(&a.effect, a.color, now.duration_since(a.started).as_secs_f32(), pos);
            if let Some(sink) = &sink {
                sink(colors.clone());
            }
            if let Some(kb) = device.get() {
                match kb.send_realtime(&to_bytes(&colors, leds, 1.0), REALTIME_GAP) {
                    Ok(()) => streaming = true,
                    Err(e) => {
                        warn!("cannot call for attention: {e}");
                        device.drop_device();
                    }
                }
            }
            set_status(Status::Streaming);
            wait(&shared.stop, Duration::from_millis(40));
            continue;
        } else if alert.is_some() {
            *shared.alert.lock().unwrap() = None; // over: the profile's lighting or effect comes back below
        }
        if profile.light_source != LightSource::Host {
            if streaming {
                device.restore(&profile);
                streaming = false;
            }
            inputs.set(None, &shared);
            set_status(Status::Idle);
            effect = None;
            effect_key.clear();
            wait(&shared.stop, Duration::from_millis(250));
            (t0, last) = (Instant::now(), Instant::now());
            continue;
        }

        let values = profile.host_values.get(&profile.host_effect).cloned().unwrap_or_default();
        let key = format!("{}{}", profile.host_effect, serde_json::Value::Object(values.clone()));
        if key != effect_key || effect.is_none() {
            let e = create(&profile.host_effect, pos, &values);
            inputs.set(Some(e.as_ref()), &shared);
            effect = Some(e);
            effect_key = key;
        }
        let Some(e) = effect.as_mut() else { continue };

        let dt = now.duration_since(last).as_secs_f32().max(1e-3);
        last = now;
        let mut inp = Inputs { presses: std::mem::take(&mut *shared.presses.lock().unwrap()), ..Inputs::default() };
        if e.info().needs_audio && inputs.audio.running() {
            (inp.bands, inp.level, inp.beat) = inputs.audio.snapshot();
        }
        if e.info().needs_system {
            inp.system = inputs.system.get();
        }
        let mut colors = e.render(now.duration_since(t0).as_secs_f32(), dt, &inp);
        inputs.overlays.apply(&mut colors, &cfg.settings);
        let brightness = profile.host_brightness.min(100) as f32 / 100.0;

        if let Some(sink) = &sink {
            sink(colors.iter().map(|c| c.map(|v| (v * brightness).clamp(0.0, 1.0))).collect());
        }
        if let Some(kb) = device.get() {
            match kb.send_realtime(&to_bytes(&colors, leds, brightness), REALTIME_GAP) {
                Ok(()) => {
                    streaming = true;
                    set_status(Status::Streaming);
                }
                Err(err) => {
                    warn!("stream interrupted: {err}");
                    device.drop_device();
                    set_status(Status::Error);
                }
            }
        } else if drive {
            set_status(Status::NoCable);
        }
        let fps = cfg.settings.host_fps.clamp(5, 60);
        wait(&shared.stop, Duration::from_secs_f32(1.0 / fps as f32).saturating_sub(now.elapsed()));
    }

    if streaming {
        let profile = shared.preview.lock().unwrap().clone().unwrap_or_else(|| cfg.profile().clone());
        device.restore(&profile);
    }
    inputs.set(None, &shared);
    set_status(Status::Idle);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_config(path: &std::path::Path, source: &str) {
        let json = serde_json::json!({
            "version": 2, "active": "P", "settings": {"host_fps": 60},
            "profiles": [{"name": "P", "light_source": source, "host_effect": "wave", "host_brightness": 50}],
        });
        std::fs::write(path, json.to_string()).unwrap();
    }

    #[test]
    fn notification_flash_pulses_and_ends() {
        assert_eq!(flash(Duration::ZERO), 0.0);
        let first = flash(FLASH / 4);
        let second = flash(FLASH * 3 / 4);
        assert!(first > 0.6 && second > 0.1 && second < first, "{first} {second}");
        assert_eq!(flash(FLASH), 0.0);
    }

    #[test]
    fn lock_overlay_only_when_asked() {
        let n = positions().1.len();
        let mut colors = vec![[0.0; 3]; n];
        let mut o = Overlays::default();
        assert!(o.lock_rows[0].is_some() && o.lock_rows[1].is_some(), "Caps Lock and Num Lock are on the AK980 V2");
        o.apply(&mut colors, &Settings::default());
        assert!(colors.iter().all(|c| *c == [0.0; 3]), "off by default");
        o.locks = [true, false, false];
        o.locks_at = Some(Instant::now()); // as if just read from the lights
        let settings = Settings { overlay_locks: true, overlay_lock_color: [255, 0, 0], ..Settings::default() };
        o.apply(&mut colors, &settings);
        assert_eq!(colors[o.lock_rows[0].unwrap()], [1.0, 0.0, 0.0]);
        assert_eq!(colors[o.lock_rows[1].unwrap()], [0.0; 3]);
    }

    #[test]
    fn previews_while_the_profile_has_a_pc_effect() {
        let dir = std::env::temp_dir().join(format!("ak980-engine-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        write_config(&path, "host");
        let frames = Arc::new(Mutex::new(Vec::<Vec<Rgbf>>::new()));
        let sink_frames = frames.clone();
        let sink: FrameSink = Arc::new(move |f| sink_frames.lock().unwrap().push(f));
        let mut engine = LightingEngine::new(false, Some(sink), path.clone());
        engine.start();
        std::thread::sleep(Duration::from_millis(300));
        let n = frames.lock().unwrap().len();
        assert!(n >= 5, "{n} frames");
        let brightest = frames.lock().unwrap().iter().flatten().flatten().fold(0.0f32, |a, &b| a.max(b));
        assert!(brightest <= 0.5 + 1e-6, "brightness 50 % caps the preview, got {brightest}");
        assert_eq!(engine.status(), Status::Idle, "no keyboard is driven");

        std::thread::sleep(Duration::from_millis(20));
        write_config(&path, "firmware"); // the profile leaves the PC effect: frames stop
        std::thread::sleep(Duration::from_millis(500));
        let before = frames.lock().unwrap().len();
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(frames.lock().unwrap().len(), before, "frames keep coming after the switch");
        engine.stop();
        let _ = std::fs::remove_dir_all(dir);
    }
}
