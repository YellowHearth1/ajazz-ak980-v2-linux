//! What the information effects and overlays read from the PC: the processor's load, memory in
//! use, the processor's temperature, the lock keys and desktop notifications. Nothing here talks to
//! the keyboard; the lighting engine turns it into colours.

use std::fs;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// (idle, total) jiffies of all processors from the first line of `/proc/stat`.
pub fn parse_proc_stat(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    if v.len() < 4 {
        return None;
    }
    // idle + iowait count as idle; guest time is already part of user time
    let idle = v[3] + v.get(4).copied().unwrap_or(0);
    let total = v.iter().take(8).sum();
    Some((idle, total))
}

/// The share of memory in use, 0..1, from `/proc/meminfo` (what is not available to programs).
pub fn parse_meminfo(text: &str) -> Option<f32> {
    let field = |name: &str| {
        text.lines().find(|l| l.starts_with(name))?.split_whitespace().nth(1)?.parse::<f64>().ok()
    };
    let (total, available) = (field("MemTotal:")?, field("MemAvailable:")?);
    (total > 0.0).then(|| (1.0 - available / total).clamp(0.0, 1.0) as f32)
}

/// Processor load between two calls.
#[derive(Default)]
pub struct CpuLoad {
    last: Option<(u64, u64)>,
}

impl CpuLoad {
    /// The share of time the processors were busy since the previous call, 0..1.
    pub fn sample(&mut self) -> Option<f32> {
        let now = parse_proc_stat(&fs::read_to_string("/proc/stat").ok()?)?;
        let busy = self.last.and_then(|(idle0, total0)| {
            let total = now.1.checked_sub(total0)?;
            let idle = now.0.checked_sub(idle0)?;
            (total > 0).then(|| (1.0 - idle as f32 / total as f32).clamp(0.0, 1.0))
        });
        self.last = Some(now);
        busy
    }
}

pub fn memory_used() -> Option<f32> {
    parse_meminfo(&fs::read_to_string("/proc/meminfo").ok()?)
}

/// The processor's temperature in °C: the package sensor of `coretemp`, `k10temp` or `zenpower`
/// when there is one, otherwise a thermal zone of the processor.
pub fn cpu_temperature() -> Option<f32> {
    for entry in fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let dir = entry.path();
        let name = fs::read_to_string(dir.join("name")).unwrap_or_default();
        if ["coretemp", "k10temp", "zenpower", "cpu_thermal"].contains(&name.trim())
            && let Some(t) = read_millidegrees(&dir.join("temp1_input"))
        {
            return Some(t);
        }
    }
    for entry in fs::read_dir("/sys/class/thermal").ok()?.flatten() {
        let kind = fs::read_to_string(entry.path().join("type")).unwrap_or_default();
        if ["x86_pkg_temp", "cpu-thermal", "cpu_thermal"].contains(&kind.trim())
            && let Some(t) = read_millidegrees(&entry.path().join("temp"))
        {
            return Some(t);
        }
    }
    None
}

fn read_millidegrees(path: &std::path::Path) -> Option<f32> {
    fs::read_to_string(path).ok()?.trim().parse::<f32>().ok().map(|m| m / 1000.0)
}

/// A temperature on the scale the effect draws: 35 °C empty, 95 °C full.
pub fn temperature_share(celsius: f32) -> f32 {
    ((celsius - 35.0) / 60.0).clamp(0.0, 1.0)
}

/// The system monitor's three readings (processor load, memory in use, temperature share), taken
/// at most twice a second however often they are asked for.
#[derive(Default)]
pub struct SystemReadings {
    cpu: CpuLoad,
    at: Option<Instant>,
    values: [f32; 3],
}

impl SystemReadings {
    pub fn get(&mut self) -> [f32; 3] {
        if self.at.is_none_or(|t| t.elapsed() >= Duration::from_millis(500)) {
            self.at = Some(Instant::now());
            self.values = [
                self.cpu.sample().unwrap_or(self.values[0]),
                memory_used().unwrap_or(0.0),
                cpu_temperature().map(temperature_share).unwrap_or(0.0),
            ];
        }
        self.values
    }
}

/// Whether Caps Lock, Num Lock and Scroll Lock are on, as the keyboard's own lights show them.
/// The AK980's input devices come first; any keyboard's lights will do otherwise, since the
/// desktop keeps them all in step.
pub fn lock_keys() -> [bool; 3] {
    let Ok(entries) = fs::read_dir("/sys/class/leds") else { return [false; 3] };
    let mut inputs: Vec<(bool, String)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let input = name.strip_suffix("::capslock")?.to_string();
            let ours = fs::read_to_string(e.path().join("device/name")).is_ok_and(|n| n.contains("AK980"));
            Some((!ours, input))
        })
        .collect();
    inputs.sort();
    let Some((_, input)) = inputs.first() else { return [false; 3] };
    let on = |what: &str| {
        fs::read_to_string(format!("/sys/class/leds/{input}::{what}/brightness"))
            .is_ok_and(|b| b.trim().parse::<u32>().is_ok_and(|v| v > 0))
    };
    [on("capslock"), on("numlock"), on("scrolllock")]
}

/// Whether a line of `dbus-monitor --profile` is a call that shows a notification. That mode
/// prints one line per message without its arguments, so the text of a notification is never read.
pub fn is_notify_call(line: &str) -> bool {
    let fields: Vec<&str> = line.split('\t').collect();
    fields.first() == Some(&"mc") && fields.last().is_some_and(|m| m.trim() == "Notify")
}

/// Counts the desktop notifications shown while it runs.
pub struct NotificationWatch {
    child: Child,
    count: Arc<AtomicU32>,
    reader: Option<JoinHandle<()>>,
}

impl NotificationWatch {
    /// Starts `dbus-monitor` on the session bus; `None` without one.
    pub fn start() -> Option<Self> {
        let mut child = Command::new("dbus-monitor")
            .args(["--session", "--profile",
                   "type='method_call',interface='org.freedesktop.Notifications',member='Notify'"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let out = child.stdout.take()?;
        let count = Arc::new(AtomicU32::new(0));
        let seen = count.clone();
        let reader = std::thread::Builder::new()
            .name("ak980-notifications".into())
            .spawn(move || {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    if is_notify_call(&line) {
                        seen.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
            .ok();
        Some(Self { child, count, reader })
    }

    /// How many notifications came since it started.
    pub fn count(&self) -> u32 {
        self.count.load(Ordering::Relaxed)
    }
}

impl Drop for NotificationWatch {
    fn drop(&mut self) {
        let _ = self.child.kill(); // our own child, by its handle
        let _ = self.child.wait();
        if let Some(r) = self.reader.take() {
            let _ = r.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processor_and_memory() {
        let stat = "cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 50 0 25 400 25 0 0 0 0 0\n";
        assert_eq!(parse_proc_stat(stat), Some((850, 1000)));
        assert_eq!(parse_proc_stat("intr 1 2 3\n"), None);
        let mem = "MemTotal:       16000000 kB\nMemFree:  1000000 kB\nMemAvailable:    4000000 kB\n";
        assert!((parse_meminfo(mem).unwrap() - 0.75).abs() < 1e-6);
        assert_eq!(parse_meminfo("MemTotal: 0 kB\nMemAvailable: 0 kB\n"), None);
        assert_eq!(temperature_share(20.0), 0.0);
        assert_eq!(temperature_share(65.0), 0.5);
        assert_eq!(temperature_share(120.0), 1.0);
    }

    #[test]
    fn this_machine_reads() {
        let mut cpu = CpuLoad::default();
        assert_eq!(cpu.sample(), None, "the first call has nothing to compare with");
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert!(cpu.sample().is_some_and(|v| (0.0..=1.0).contains(&v)));
        assert!(memory_used().is_some_and(|v| (0.0..=1.0).contains(&v)));
        let _ = lock_keys(); // whatever the lights are, reading them must not fail
    }

    #[test]
    fn only_notify_calls_count() {
        assert!(is_notify_call("mc\t1793366385.36\t2\t:1.89\torg.freedesktop.Notifications\t/org/freedesktop/Notifications\torg.freedesktop.Notifications\tNotify"));
        assert!(!is_notify_call("mc\t1.0\t2\t:1.89\torg.freedesktop.Notifications\t/x\torg.freedesktop.Notifications\tGetServerInformation"));
        assert!(!is_notify_call("#type\ttimestamp\tserial\tsender\tdestination\tpath\tinterface\tmember"));
        assert!(!is_notify_call("sig\t1.0\t2\torg.freedesktop.DBus\t:1.2\t/org/freedesktop/DBus\torg.freedesktop.DBus\tNameAcquired"));
    }
}
