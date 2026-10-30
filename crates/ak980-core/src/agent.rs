//! The background agent.
//!
//! User mode (`ak980 agent`), started with the desktop session or as a user service: keeps the
//! screen clock in sync (when the keyboard appears, after sleep, when the system time or time zone
//! changes, at midnight and then every hour), streams PC lighting effects, switches profiles when a
//! configured program (a game) starts or exits, turns launcher keys (F13–F24 sent by remapped keys)
//! into programs and links, and keeps the 2.4 GHz receiver polled while recording its link.
//!
//! System mode (`ak980 agent --system`), started by the init system as root or a service user, only
//! keeps the clock in sync and reads /etc/ak980/agent.conf.

use std::collections::{HashMap, HashSet};
use std::ffi::CStr;
use std::fs;
use std::io::Write;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use log::{debug, info, warn};

use crate::battery;
use crate::config::{self, Config, PROFILE_NEXT, PROFILE_USE, Settings};
use crate::device::{DONGLE_PID, Dongle, VENDOR_ID, WiredKeyboard};
use crate::engine::LightingEngine;
use crate::hid::enumerate;
use crate::i18n::{set_language, tr};
use crate::keyinput::KeyListener;
use crate::keymap::key_code;
use crate::paths;
use crate::profiles::{ALL_PARTS, apply_profile};
use crate::runtime::InstanceLock;
use crate::time::LocalTime;
use crate::{Result, tr};

pub const POLL_SECONDS: u64 = 3;
pub const SWITCH_SECONDS: u64 = 3;
/// The dongle's vendor interfaces: 3 (usage page FF60, battery and clock) and 4 (FF59).
pub const DONGLE_VENDOR_INTERFACES: [i32; 2] = [3, 4];
/// How often the agent asks the keyboard for its battery over 2.4 GHz; the stock software polls too.
pub const LINK_CHECK_SECONDS: u64 = 60;
/// The link log keeps about this much; the older half moves to `link.log.1`.
const LINK_LOG_MAX: u64 = 256 * 1024;

// ------------------------------------------------------------ logging

struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }

    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            // "ak980_core::agent" → "ak980.agent", the names the Python version logged under
            let name = r.target().rsplit("::").next().unwrap_or("ak980");
            eprintln!("{} ak980.{name}: {}", &LocalTime::now().stamp()[11..], r.args());
        }
    }

    fn flush(&self) {}
}

/// "HH:MM:SS ak980.agent: message" lines on stderr, which service managers keep.
pub fn init_logging() {
    if log::set_logger(&StderrLogger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

// ------------------------------------------------------------ the 2.4 GHz link record

/// Appends a line to the link log (user agent only), a record that outlives the service's log buffer.
pub fn link_event(message: &str) {
    info!("{message}");
    append_link_line(&paths::link_log(), message);
}

fn append_link_line(path: &Path, message: &str) {
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        if path.metadata().is_ok_and(|m| m.len() > LINK_LOG_MAX) {
            let mut old = path.as_os_str().to_owned();
            old.push(".1");
            fs::rename(path, old)?;
        }
        let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(f, "{} {message}", LocalTime::now().stamp())
    };
    let _ = write();
}

/// What a battery request over the dongle showed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkAnswer {
    /// The keyboard answered with its battery level.
    Level(u8),
    /// The dongle took the request, the keyboard stayed silent: the radio link.
    Silent,
    /// The dongle did not take the request: the dongle itself (or USB).
    Unsent(String),
}

fn ask_battery() -> LinkAnswer {
    let Some(info) = Dongle::find() else { return LinkAnswer::Silent };
    match Dongle::open_found(&info).and_then(|mut d| d.read_battery()) {
        Ok(Some(level)) => LinkAnswer::Level(level),
        Ok(None) => LinkAnswer::Silent,
        Err(e) => LinkAnswer::Unsent(e.to_string()),
    }
}

/// Writes the answer to `log` when its kind differs from the last one; returns the new kind.
///
/// Three ways a dropout can look: the write itself fails (the dongle stopped answering USB), the
/// dongle takes the request but the keyboard stays silent (the radio link), or the keyboard keeps
/// answering while keys do not arrive (the PC side).
pub fn record_link(last: Option<&str>, answer: &LinkAnswer, log: &Path) -> String {
    let (state, message) = match answer {
        LinkAnswer::Level(level) => ("answers", format!("2.4 GHz: the keyboard answers, battery {level}%")),
        LinkAnswer::Silent => ("silent", "2.4 GHz: the dongle took the request, the keyboard did not answer".into()),
        LinkAnswer::Unsent(e) => ("unsent", format!("2.4 GHz: the dongle did not take the request ({e})")),
    };
    if last != Some(state) {
        info!("{message}");
        append_link_line(log, &message);
    }
    state.to_string()
}

// ------------------------------------------------------------ desktop notifications

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub summary: String,
    pub body: String,
    pub urgent: bool,
}

/// What to tell the user after a link check, if anything: the keyboard stopped answering over
/// 2.4 GHz, or its battery got low. `alerted` remembers the last battery warning (20 or 10 %), so
/// each one comes once per discharge; charging past 30 % clears it.
pub fn link_notice(last: Option<&str>, now: &str, answer: &LinkAnswer, alerted: &mut Option<u8>) -> Option<Notice> {
    if last == Some("answers") && now != "answers" {
        return Some(Notice {
            summary: tr("The keyboard stopped answering over 2.4 GHz"),
            body: tr("If keys stop arriving, press Reconnect on the Device page or run `ak980 reconnect`."),
            urgent: false,
        });
    }
    let LinkAnswer::Level(level) = answer else { return None };
    let level = *level;
    if level >= 30 {
        *alerted = None;
        return None;
    }
    let step = if level <= 10 { 10 } else if level <= 20 { 20 } else { return None };
    if alerted.is_some_and(|a| a <= step) {
        return None;
    }
    *alerted = Some(step);
    Some(Notice {
        summary: tr!("Keyboard battery at {level}%", level = level),
        body: tr("Connect the cable to charge it; it keeps working while it charges."),
        urgent: step == 10,
    })
}

/// Shows a notice through the desktop's notification service (`notify-send`).
pub fn notify(n: &Notice) {
    let Some(exe) = crate::service::which("notify-send") else {
        warn!("cannot show a notification: notify-send is not installed");
        return;
    };
    let mut cmd = Command::new(exe);
    cmd.arg(format!("--app-name={}", crate::APP_NAME))
        .arg(format!("--icon={}", crate::APP_ID))
        .arg(if n.urgent { "--urgency=critical" } else { "--urgency=normal" })
        .arg(&n.summary)
        .arg(&n.body)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match cmd.spawn() {
        // Waited for on a thread of its own, so no zombie is left behind
        Ok(mut child) => drop(std::thread::spawn(move || child.wait())),
        Err(e) => warn!("cannot show a notification: {e}"),
    }
}

// ------------------------------------------------------------ keeping the dongle polled

/// Keeps the dongle's vendor interfaces open, so that Linux keeps polling them.
///
/// Linux reads a HID interface's interrupt endpoint only while a program has it open; Windows and
/// macOS always do. Wireless receivers with a vendor interface next to the keyboard can stop
/// answering on every interface once a report waits unread there (the kernel fixes that per device
/// with HID_QUIRK_ALWAYS_POLL; the Cooler Master dongle got it in 2025). The same dongle as the
/// AK980's (0c45:fefe) loses its keyboard under Linux but not under macOS. Nothing is sent; any
/// report the dongle sends without being asked is logged, because it shows what wakes it up.
pub struct DonglePoller {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl DonglePoller {
    /// `record`: write unsolicited reports to the link log.
    pub fn start(record: bool) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread =
            std::thread::Builder::new().name("ak980-dongle-poller".into()).spawn(move || poll_dongle(record, &flag)).ok();
        Self { stop, thread }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for DonglePoller {
    fn drop(&mut self) {
        self.stop();
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

fn poll_dongle(record: bool, stop: &AtomicBool) {
    let mut fds: HashMap<String, OwnedFd> = HashMap::new();
    let mut rescan_at = Instant::now();
    let mut buf = [0u8; 128];
    while !stop.load(Ordering::Relaxed) {
        if Instant::now() >= rescan_at {
            rescan_at = Instant::now() + Duration::from_secs(POLL_SECONDS);
            for info in enumerate(Some(VENDOR_ID), Some(DONGLE_PID)) {
                if !DONGLE_VENDOR_INTERFACES.contains(&info.interface) || fds.contains_key(&info.path) {
                    continue;
                }
                let file = fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
                    .open(&info.path);
                match file {
                    Ok(f) => {
                        info!("keeping {} (dongle interface {}) polled", info.path, info.interface);
                        fds.insert(info.path, OwnedFd::from(f));
                    }
                    Err(e) => warn!("cannot keep {} (dongle interface {}) open: {e}", info.path, info.interface),
                }
            }
        }
        if fds.is_empty() {
            wait(stop, Duration::from_secs(POLL_SECONDS));
            continue;
        }
        let paths: Vec<String> = fds.keys().cloned().collect();
        let mut polls: Vec<libc::pollfd> =
            paths.iter().map(|p| libc::pollfd { fd: fds[p].as_raw_fd(), events: libc::POLLIN, revents: 0 }).collect();
        // SAFETY: polls is a valid array of pollfd for the duration of the call
        if unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, 1000) } <= 0 {
            continue;
        }
        for (path, p) in paths.iter().zip(&polls) {
            if p.revents == 0 {
                continue;
            }
            // SAFETY: reads into our own buffer of the given length
            let n = unsafe { libc::read(p.fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n > 0 {
                let data = &buf[..n as usize];
                // Replies to the battery (20) and clock (0C) requests reach every open node
                if data[0] == 0x20 || data[0] == 0x0C {
                    debug!("dongle {path} sent {}", hex(data));
                } else if record {
                    link_event(&format!("dongle {path} sent {}", hex(data)));
                } else {
                    info!("dongle {path} sent {}", hex(data));
                }
            } else if n == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::EAGAIN) {
                info!("dongle interface {path} closed"); // unplugged
                fds.remove(path);
            }
        }
    }
    for path in fds.keys() {
        info!("dongle interface {path} closed");
    }
}

// ------------------------------------------------------------ helpers

/// Sleeps up to `d`, waking early when asked to stop.
fn wait(stop: &AtomicBool, d: Duration) {
    let end = Instant::now() + d;
    while !stop.load(Ordering::Relaxed) {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        std::thread::sleep(left.min(Duration::from_millis(100)));
    }
}

unsafe extern "C" {
    /// POSIX; the libc crate does not export it for every target.
    fn tzset();
}

/// The current UTC offset and zone name; a change means the time zone changed.
fn tz_key() -> (i64, String) {
    let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as libc::time_t).unwrap_or(0);
    // SAFETY: tzset only rereads TZ; localtime_r only writes into the tm we pass
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        tzset();
        libc::localtime_r(&t, &mut tm);
    }
    let zone = if tm.tm_zone.is_null() {
        String::new()
    } else {
        // SAFETY: tm_zone points at a NUL-terminated name owned by the C library
        unsafe { CStr::from_ptr(tm.tm_zone) }.to_string_lossy().into_owned()
    };
    (tm.tm_gmtoff, zone)
}

/// Sets the clock over whatever link is there. Returns "USB", "2.4G" or None.
pub fn sync_clock_once(slot: u8) -> Result<Option<&'static str>> {
    if let Some((cmd, lcd)) = WiredKeyboard::find() {
        let mut kb = WiredKeyboard::open_found(&cmd, lcd.as_ref())?;
        kb.sync_clock(None, slot)?;
        return Ok(Some("USB"));
    }
    if let Some(info) = Dongle::find() {
        let mut d = Dongle::open_found(&info)?;
        if d.sync_clock(None, slot)? {
            return Ok(Some("2.4G"));
        }
    }
    Ok(None)
}

/// Lower-case names of running programs: /proc/<pid>/comm and the executable's file name.
pub fn running_processes() -> HashSet<String> {
    let mut names = HashSet::new();
    let Ok(entries) = fs::read_dir("/proc") else { return names };
    for e in entries.flatten() {
        if !e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let dir = e.path();
        let Ok(comm) = fs::read_to_string(dir.join("comm")) else { continue };
        names.insert(comm.trim().to_lowercase());
        if let Ok(exe) = fs::read_link(dir.join("exe"))
            && let Some(name) = exe.file_name()
        {
            names.insert(name.to_string_lossy().to_lowercase());
        }
        if let Ok(cmdline) = fs::read(dir.join("cmdline")) {
            let first = cmdline.split(|b| *b == 0).next().unwrap_or_default();
            let first = String::from_utf8_lossy(&first[..first.len().min(512)]).replace('\\', "/");
            if let Some(name) = first.rsplit('/').next().filter(|n| !n.is_empty()) {
                names.insert(name.to_lowercase());
            }
        }
    }
    names
}

pub fn matching_profile(settings: &Settings, names: &HashSet<String>) -> Option<String> {
    settings.rules.iter().find(|r| names.contains(&r.process.trim().to_lowercase())).map(|r| r.profile.clone())
}

/// Starts a launcher key's program or link, detached from the agent.
pub fn run_launcher(command: &str) {
    let cmd = command.trim();
    if cmd.is_empty() {
        return;
    }
    let mut c = if cmd.contains("://") && !cmd.contains(' ') {
        let mut c = Command::new("xdg-open");
        c.arg(cmd);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c").arg(cmd);
        c
    };
    c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // SAFETY: setsid only detaches the child from the agent's session
    unsafe {
        c.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    match c.spawn() {
        Ok(mut child) => {
            info!("launched: {cmd}");
            // Collect the exit status, or every launch would leave a zombie behind
            std::thread::spawn(move || child.wait());
        }
        Err(e) => warn!("cannot launch {cmd}: {e}"),
    }
}

// ------------------------------------------------------------ the agent

static STOP: AtomicBool = AtomicBool::new(false);
/// Set when a profile key is pressed, so the agent acts on it at once instead of after its pause.
static WAKE: AtomicBool = AtomicBool::new(false);

/// The profile a profile key asks for: the next one in the list, or one by name.
pub fn requested_profile(cfg: &Config, request: &str) -> Option<String> {
    if request == PROFILE_NEXT {
        let names: Vec<&str> = cfg.profiles.iter().map(|p| p.name.as_str()).collect();
        let at = names.iter().position(|n| *n == cfg.active).unwrap_or(0);
        return names.get((at + 1) % names.len().max(1)).map(|n| n.to_string());
    }
    let name = request.strip_prefix(PROFILE_USE)?;
    cfg.get(name).map(|p| p.name.clone())
}

/// Waits like `wait`, but a profile key ends the pause.
fn wait_or_wake(d: Duration) {
    let end = Instant::now() + d;
    while !STOP.load(Ordering::Relaxed) && !WAKE.swap(false, Ordering::Relaxed) {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        std::thread::sleep(left.min(Duration::from_millis(50)));
    }
}

extern "C" fn request_stop(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

extern "C" fn exit_now(_: libc::c_int) {
    // SAFETY: _exit is async-signal-safe
    unsafe { libc::_exit(0) }
}

fn on_termination(handler: extern "C" fn(libc::c_int)) {
    // SAFETY: both handlers only touch an atomic or call _exit
    unsafe {
        libc::signal(libc::SIGTERM, handler as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, handler as *const () as libc::sighandler_t);
    }
}

pub struct Agent {
    pub system: bool,
    pub interval_override: Option<u32>,
    /// Off in tests: profile switches then never reach a keyboard.
    pub drive_device: bool,
    engine: Option<LightingEngine>,
    keys: Option<KeyListener>,
    poller: Option<DonglePoller>,
    launchers: Arc<Mutex<HashMap<u8, String>>>,
    /// Profile keys pressed since the agent last looked.
    profile_requests: Arc<Mutex<Vec<String>>>,
    /// The profile the running programs asked for last time.
    wanted: Option<String>,
    /// The profile switched to because a program runs.
    for_program: Option<String>,
    /// The profile that was active before that.
    before_program: Option<String>,
    /// The last low-battery warning given (20 or 10 %).
    battery_alert: Option<u8>,
}

impl Agent {
    pub fn new(system: bool, interval_override: Option<u32>) -> Self {
        Self {
            system,
            interval_override,
            drive_device: true,
            engine: None,
            keys: None,
            poller: None,
            launchers: Arc::default(),
            profile_requests: Arc::default(),
            wanted: None,
            for_program: None,
            before_program: None,
            battery_alert: None,
        }
    }

    fn switch_to(&mut self, cfg: &mut Config, name: &str) {
        let Some(profile) = cfg.get(name).cloned() else { return };
        if cfg.active == name {
            return;
        }
        info!("switching to profile {name}");
        cfg.active = name.to_string();
        if let Err(e) = cfg.save() {
            warn!("cannot save the profile switch: {e}");
        }
        if !self.drive_device {
            return;
        }
        if let Some((cmd, lcd)) = WiredKeyboard::find() {
            let applied = WiredKeyboard::open_found(&cmd, lcd.as_ref())
                .and_then(|mut kb| apply_profile(&mut kb, &profile, &ALL_PARTS, &mut |_| {}));
            if let Err(e) = applied {
                warn!("cannot apply profile {name}: {e}");
            }
        }
    }

    /// Switches to a program's profile when it starts and back when it exits.
    ///
    /// Only a change in what runs triggers a switch, so a profile picked by hand in between stays.
    /// Going back is skipped when the user has moved away from the program's profile.
    pub fn auto_switch(&mut self, cfg: &mut Config, running: Option<&HashSet<String>>) {
        if !cfg.settings.auto_switch || cfg.settings.rules.is_empty() {
            (self.wanted, self.for_program, self.before_program) = (None, None, None);
            return;
        }
        let wanted = match running {
            Some(names) => matching_profile(&cfg.settings, names),
            None => matching_profile(&cfg.settings, &running_processes()),
        };
        if wanted == self.wanted {
            return;
        }
        self.wanted.clone_from(&wanted);
        if let Some(wanted) = wanted {
            if self.for_program.is_none() {
                self.before_program = Some(cfg.active.clone());
            }
            self.for_program = Some(wanted.clone());
            self.switch_to(cfg, &wanted);
        } else if let Some(for_program) = self.for_program.take() {
            let back = Some(cfg.settings.default_profile.clone()).filter(|p| !p.is_empty()).or(self.before_program.take());
            if let Some(back) = back
                && cfg.active == for_program
            {
                self.switch_to(cfg, &back);
            }
            self.before_program = None;
        }
    }

    fn update_launchers(&mut self, cfg: &Config) {
        let table: HashMap<u8, String> = cfg
            .settings
            .launchers
            .iter()
            .filter_map(|l| key_code(&l.key).ok().map(|code| (code, l.command.clone())))
            .collect();
        let any = !table.is_empty();
        *self.launchers.lock().unwrap() = table;
        if any && self.keys.is_none() {
            let (launchers, requests) = (self.launchers.clone(), self.profile_requests.clone());
            self.keys = Some(KeyListener::start(move |usage, pressed| {
                let command = if pressed { launchers.lock().unwrap().get(&usage).cloned() } else { None };
                match command {
                    // Profiles are switched by the agent's own loop, which owns the configuration
                    Some(c) if c.starts_with("profile:") => {
                        requests.lock().unwrap().push(c);
                        WAKE.store(true, Ordering::Relaxed);
                    }
                    Some(c) => run_launcher(&c),
                    None => {}
                }
            }));
        } else if !any {
            self.keys = None; // dropping the listener stops it
        }
    }

    /// Runs until SIGTERM or SIGINT; with `once`, syncs the clock once and returns 0 on success.
    pub fn run(&mut self, once: bool) -> i32 {
        let mut lock = InstanceLock::new(if self.system { "system-agent" } else { "agent" });
        if !once {
            match lock.acquire(false) {
                Ok(true) => {}
                Ok(false) => {
                    // Another agent runs (say, from session autostart while a user service starts
                    // too). Exiting would make some supervisors restart us in a loop, so wait as a
                    // standby and take over if that agent stops.
                    info!("{}", tr!("The agent is already running"));
                    on_termination(exit_now);
                    let _ = lock.acquire(true);
                    info!("the other agent stopped; taking over");
                }
                Err(e) => warn!("cannot take the instance lock: {e}"),
            }
        }
        on_termination(request_stop);

        if !self.system {
            set_language(&config::peek_language());
        }
        let config_path = paths::config_path();
        let mut cfg = (!self.system).then(Config::load);
        let mut seen = cfg.as_ref().and_then(|_| config::mtime(&config_path));
        if !once {
            self.poller = Some(DonglePoller::start(!self.system));
            if let Some(c) = &cfg {
                let mut engine = LightingEngine::new(true, None, config_path.clone());
                engine.start();
                self.engine = Some(engine);
                self.update_launchers(c);
            }
        }

        let mut present: Option<&str> = None;
        let mut link_state: Option<String> = None; // how the keyboard answered over 2.4 GHz last time
        let mut next_link_check = Instant::now();
        let mut last_sync: Option<Instant> = None;
        let mut retry_at: Option<Instant> = None;
        let base = Instant::now();
        let wall = || SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64() - base.elapsed().as_secs_f64();
        let mut wall_ref = wall();
        let mut tz = tz_key();
        let mut day = LocalTime::now().date();
        let mut next_switch = Instant::now();
        info!("ak980 agent started ({} mode)", if self.system { "system" } else { "user" });

        let code = loop {
            if STOP.load(Ordering::SeqCst) {
                break 0;
            }
            if cfg.is_some() {
                let m = config::mtime(&config_path);
                if m != seen {
                    seen = m;
                    let fresh = Config::load();
                    self.update_launchers(&fresh);
                    cfg = Some(fresh);
                }
            }
            let settings = cfg.as_ref().map_or_else(config::load_system, |c| c.settings.clone());
            let minutes = self.interval_override.filter(|m| *m > 0).unwrap_or(settings.sync_interval_min);
            let interval = Duration::from_secs(60 * u64::from(if minutes == 0 { 60 } else { minutes }));
            let now_present = if WiredKeyboard::find().is_some() {
                Some("usb")
            } else if Dongle::find().is_some() {
                Some("2.4g")
            } else {
                None
            };

            let mut reason: Option<String> = None;
            if now_present.is_some() && now_present != present {
                reason = Some(format!("connected ({})", now_present.unwrap_or_default()));
            } else if now_present.is_some() {
                if (wall() - wall_ref).abs() > 2.0 {
                    reason = Some("system time changed or resumed from sleep".into());
                } else if tz_key() != tz {
                    reason = Some("time zone changed".into());
                } else if LocalTime::now().date() != day {
                    reason = Some("new day".into());
                } else if last_sync.is_some_and(|t| t.elapsed() > interval) {
                    reason = Some("periodic".into());
                } else if retry_at.is_some_and(|t| Instant::now() >= t) {
                    reason = Some("retry".into());
                }
            }
            if now_present != present && !once && !self.system {
                link_event(&format!(
                    "keyboard: {} -> {}",
                    present.unwrap_or("not connected"),
                    now_present.unwrap_or("not connected")
                ));
                link_state = None;
                next_link_check = Instant::now() + Duration::from_secs(5); // the radio link takes a moment
            }
            present = now_present;

            if now_present == Some("2.4g") && !once && !self.system && Instant::now() >= next_link_check {
                next_link_check = Instant::now() + Duration::from_secs(LINK_CHECK_SECONDS);
                let answer = ask_battery();
                if let LinkAnswer::Level(level) = answer {
                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                    if let Err(e) = battery::record(&paths::battery_log(), now, level) {
                        warn!("cannot keep the battery record: {e}");
                    }
                }
                let state = record_link(link_state.as_deref(), &answer, &paths::link_log());
                let notice = link_notice(link_state.as_deref(), &state, &answer, &mut self.battery_alert);
                if let Some(n) = notice.filter(|_| settings.notifications) {
                    notify(&n);
                }
                link_state = Some(state);
            }

            // `--once` is asked for by hand or by a timer: it syncs whatever the setting says
            if let Some(reason) = reason.as_deref().filter(|_| settings.clock_auto || once) {
                if reason.starts_with("connected") && !once {
                    wait(&STOP, Duration::from_secs(2)); // let the firmware bring up every interface
                }
                let via = sync_clock_once(settings.clock_slot).unwrap_or_else(|e| {
                    warn!("clock sync failed: {e}");
                    None
                });
                if let Some(via) = via {
                    info!("clock synced over {via} ({reason})");
                    last_sync = Some(Instant::now());
                    retry_at = None;
                } else {
                    // The dongle is there but the keyboard is not in 2.4 GHz mode, or asleep
                    retry_at = Some(Instant::now() + Duration::from_secs(30));
                }
                wall_ref = wall();
                tz = tz_key();
                day = LocalTime::now().date();
                if once {
                    break if via.is_some() { 0 } else { 1 };
                }
            } else if once {
                break if now_present.is_none() { 1 } else { 0 };
            }

            let requests = std::mem::take(&mut *self.profile_requests.lock().unwrap());
            if !requests.is_empty()
                && let Some(mut c) = cfg.take()
            {
                for request in requests {
                    if let Some(name) = requested_profile(&c, &request) {
                        info!("profile key: {name}");
                        self.switch_to(&mut c, &name);
                    }
                }
                cfg = Some(c);
            }

            if Instant::now() >= next_switch
                && let Some(mut c) = cfg.take()
            {
                next_switch = Instant::now() + Duration::from_secs(SWITCH_SECONDS);
                self.auto_switch(&mut c, None);
                cfg = Some(c);
            }
            wait_or_wake(Duration::from_secs(POLL_SECONDS));
        };

        if let Some(mut engine) = self.engine.take() {
            engine.stop();
        }
        self.keys = None;
        if let Some(mut poller) = self.poller.take() {
            poller.stop();
        }
        lock.release();
        code
    }
}

/// A system agent that its service manager started as root keeps only what it needs: the `input`
/// group, which the access rule opens the keyboard to, as the user `nobody`. dinit and s6 start it
/// as root; systemd, OpenRC and runit already start it without root.
fn drop_root() -> Result<(), String> {
    // SAFETY: plain libc calls; the group and user entries are read before anything else runs
    unsafe {
        if libc::geteuid() != 0 {
            return Ok(());
        }
        let group = libc::getgrnam(c"input".as_ptr());
        if group.is_null() {
            return Err("there is no group called input".into());
        }
        let gid = (*group).gr_gid;
        let user = libc::getpwnam(c"nobody".as_ptr());
        let uid = if user.is_null() { 65534 } else { (*user).pw_uid };
        if libc::setgroups(1, &gid) != 0 || libc::setgid(gid) != 0 || libc::setuid(uid) != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if libc::setuid(0) == 0 {
            return Err("root could be taken back".into());
        }
    }
    Ok(())
}

pub fn run(system: bool, interval: Option<u32>, once: bool) -> i32 {
    init_logging();
    if system && let Err(e) = drop_root() {
        warn!("the system agent does not run as root, and cannot leave root: {e}");
        return 1;
    }
    Agent::new(system, interval).run(once)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notices_come_once_per_event() {
        let mut alerted = None;
        let n = |last: Option<&str>, now: &str, a: LinkAnswer, al: &mut Option<u8>| link_notice(last, now, &a, al);
        // losing the keyboard after it answered
        assert!(n(Some("answers"), "silent", LinkAnswer::Silent, &mut alerted).is_some());
        assert!(n(Some("silent"), "silent", LinkAnswer::Silent, &mut alerted).is_none(), "said once");
        assert!(n(None, "silent", LinkAnswer::Silent, &mut alerted).is_none(), "not right after a switch");
        // the battery: 20 %, then 10 % (urgent), each once, and again after a charge
        assert!(n(Some("answers"), "answers", LinkAnswer::Level(50), &mut alerted).is_none());
        let low = n(Some("answers"), "answers", LinkAnswer::Level(19), &mut alerted).unwrap();
        assert!(!low.urgent && low.summary.contains("19"));
        assert!(n(Some("answers"), "answers", LinkAnswer::Level(18), &mut alerted).is_none());
        assert!(n(Some("answers"), "answers", LinkAnswer::Level(9), &mut alerted).unwrap().urgent);
        assert!(n(Some("answers"), "answers", LinkAnswer::Level(8), &mut alerted).is_none());
        assert!(n(Some("answers"), "answers", LinkAnswer::Level(80), &mut alerted).is_none());
        assert!(n(Some("answers"), "answers", LinkAnswer::Level(20), &mut alerted).is_some(), "after charging");
    }
    use crate::config::{Profile, Rule};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ak980-agent-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn profile_keys_pick_the_next_or_a_named_profile() {
        let dir = temp_dir("profile-keys");
        let mut c = Config::load_from(&dir.join("c.json"));
        let main = c.active.clone();
        c.add(Profile::named("Game"));
        c.add(Profile::named("Work"));
        assert_eq!(requested_profile(&c, PROFILE_NEXT).as_deref(), Some("Game"));
        c.active = "Work".into();
        assert_eq!(requested_profile(&c, PROFILE_NEXT), Some(main), "after the last comes the first");
        assert_eq!(requested_profile(&c, &format!("{PROFILE_USE}Game")).as_deref(), Some("Game"));
        assert_eq!(requested_profile(&c, &format!("{PROFILE_USE}Gone")), None);
        assert_eq!(requested_profile(&c, "steam"), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn auto_switch_follows_the_program() {
        let dir = temp_dir("switch");
        let mut c = Config::load_from(&dir.join("c.json")); // saves go to this temporary file
        let main = c.active.clone();
        c.add(Profile::named("Game"));
        c.add(Profile::named("Work"));
        c.settings.auto_switch = true;
        c.settings.rules = vec![Rule { process: "cs2".into(), profile: "Game".into() }];
        let mut a = Agent::new(false, None);
        a.drive_device = false; // never reaches a keyboard
        let none = HashSet::new();
        let cs2: HashSet<String> = ["cs2".to_string()].into();

        a.auto_switch(&mut c, Some(&none));
        assert_eq!(c.active, main, "nothing runs, nothing changes");
        a.auto_switch(&mut c, Some(&cs2));
        assert_eq!(c.active, "Game");
        a.auto_switch(&mut c, Some(&cs2));
        a.auto_switch(&mut c, Some(&none));
        assert_eq!(c.active, main, "the profile from before comes back");
        c.active = "Work".into(); // picked by hand
        a.auto_switch(&mut c, Some(&none));
        assert_eq!(c.active, "Work");
        a.auto_switch(&mut c, Some(&cs2));
        c.active = main.clone(); // the user leaves the game's profile while it runs
        a.auto_switch(&mut c, Some(&cs2));
        assert_eq!(c.active, main, "it is not forced back");
        a.auto_switch(&mut c, Some(&none));
        assert_eq!(c.active, main, "nor replaced when the game exits");
        c.settings.default_profile = "Work".into();
        a.auto_switch(&mut c, Some(&cs2));
        a.auto_switch(&mut c, Some(&none));
        assert_eq!(c.active, "Work", "an explicit profile for after the program exits");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn link_log_records_changes_only() {
        let dir = temp_dir("link");
        let log = dir.join("link.log");
        let answers = [
            LinkAnswer::Level(80),
            LinkAnswer::Level(79),
            LinkAnswer::Silent,
            LinkAnswer::Silent,
            LinkAnswer::Unsent("Connection timed out (os error 110)".into()),
            LinkAnswer::Level(81),
        ];
        let mut state: Option<String> = None;
        for a in &answers {
            state = Some(record_link(state.as_deref(), a, &log));
        }
        let text = fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = text.lines().map(|l| l.splitn(3, ' ').nth(2).unwrap()).collect();
        assert_eq!(lines, [
            "2.4 GHz: the keyboard answers, battery 80%",
            "2.4 GHz: the dongle took the request, the keyboard did not answer",
            "2.4 GHz: the dongle did not take the request (Connection timed out (os error 110))",
            "2.4 GHz: the keyboard answers, battery 81%",
        ]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn processes_and_rules() {
        let me = fs::read_to_string("/proc/self/comm").unwrap().trim().to_lowercase();
        let names = running_processes();
        assert!(names.contains(&me), "{me} missing");
        let s = Settings { rules: vec![Rule { process: " CS2 ".into(), profile: "Game".into() }], ..Settings::default() };
        assert_eq!(matching_profile(&s, &["cs2".to_string()].into()), Some("Game".into()));
        assert_eq!(matching_profile(&s, &names), None);
        assert!(!tz_key().1.is_empty());
    }
}
