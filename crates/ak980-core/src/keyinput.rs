//! Key presses on the AK980, read from its /dev/input event devices.
//!
//! Used by the reactive lighting effects and by launcher keys (F13–F24 run a program). Reading
//! needs the `input` group (most desktop users are in it) or an access control list.

use std::collections::HashMap;
use std::ffi::CString;
use std::fs;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::keymap::linux_to_hid;

const EV_KEY: u16 = 1;
const NAME_HINTS: [&str; 1] = ["AK980"];
const EVENT_SIZE: usize = std::mem::size_of::<libc::input_event>();

/// The keyboard's event devices (cable and receiver), found by their names.
pub fn keyboard_event_devices() -> Vec<String> {
    let mut paths: Vec<String> = fs::read_dir("/sys/class/input")
        .map(|rd| rd.filter_map(|e| e.ok()).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|e| {
            let file_name = e.file_name().to_string_lossy().into_owned();
            if !file_name.starts_with("event") {
                return None;
            }
            let name = fs::read_to_string(e.path().join("device").join("name")).ok()?;
            NAME_HINTS.iter().any(|h| name.contains(h)).then(|| format!("/dev/input/{file_name}"))
        })
        .collect();
    paths.sort();
    paths
}

fn can_read(path: &str) -> bool {
    let Ok(c) = CString::new(path) else { return false };
    // SAFETY: access only reads the path we pass
    unsafe { libc::access(c.as_ptr(), libc::R_OK) == 0 }
}

pub fn readable() -> bool {
    let devs = keyboard_event_devices();
    !devs.is_empty() && devs.iter().all(|p| can_read(p))
}

/// Calls `callback(hid_usage, pressed)` for every key press and release in raw `input_event`s;
/// auto-repeat (value 2) and other event types are skipped.
pub fn parse_events(data: &[u8], callback: &mut dyn FnMut(u8, bool)) {
    for chunk in data.chunks_exact(EVENT_SIZE) {
        // SAFETY: the chunk holds exactly one input_event; read_unaligned copes with any alignment
        let ev: libc::input_event = unsafe { std::ptr::read_unaligned(chunk.as_ptr().cast()) };
        if ev.type_ == EV_KEY
            && (ev.value == 0 || ev.value == 1)
            && let Some(usage) = linux_to_hid(ev.code)
        {
            callback(usage, ev.value == 1);
        }
    }
}

/// Reads the keyboard's key presses on a background thread until stopped or dropped.
pub struct KeyListener {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl KeyListener {
    pub fn start(callback: impl FnMut(u8, bool) + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new().name("ak980-keys".into()).spawn(move || run(callback, flag)).ok();
        Self { stop, thread }
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for KeyListener {
    fn drop(&mut self) {
        self.stop();
    }
}

fn rescan(fds: &mut HashMap<String, OwnedFd>) {
    let present = keyboard_event_devices();
    fds.retain(|path, _| present.contains(path));
    for path in present {
        if fds.contains_key(&path) {
            continue;
        }
        let file = fs::OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC).open(&path);
        if let Ok(f) = file {
            fds.insert(path, OwnedFd::from(f));
        }
    }
}

fn run(mut callback: impl FnMut(u8, bool), stop: Arc<AtomicBool>) {
    let mut fds: HashMap<String, OwnedFd> = HashMap::new();
    let mut next_scan = Instant::now();
    let mut buf = vec![0u8; EVENT_SIZE * 64];
    while !stop.load(Ordering::Relaxed) {
        if Instant::now() >= next_scan {
            rescan(&mut fds);
            next_scan = Instant::now() + Duration::from_secs(3);
        }
        if fds.is_empty() {
            std::thread::sleep(Duration::from_millis(250));
            continue;
        }
        let mut polls: Vec<libc::pollfd> =
            fds.values().map(|fd| libc::pollfd { fd: fd.as_raw_fd(), events: libc::POLLIN, revents: 0 }).collect();
        // SAFETY: polls is a valid array of pollfd for the duration of the call
        let ready = unsafe { libc::poll(polls.as_mut_ptr(), polls.len() as libc::nfds_t, 500) };
        if ready <= 0 {
            continue;
        }
        for p in &polls {
            if p.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                next_scan = Instant::now(); // the device went away; rescan on the next turn
                continue;
            }
            if p.revents & libc::POLLIN == 0 {
                continue;
            }
            // SAFETY: reads into our own buffer of the given length
            let n = unsafe { libc::read(p.fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n > 0 {
                parse_events(&buf[..n as usize], &mut callback);
            } else if n < 0 {
                let err = std::io::Error::last_os_error().raw_os_error();
                if err != Some(libc::EAGAIN) && err != Some(libc::EINTR) {
                    next_scan = Instant::now();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(type_: u16, code: u16, value: i32) -> Vec<u8> {
        let ev = libc::input_event { time: libc::timeval { tv_sec: 1, tv_usec: 2 }, type_, code, value };
        // SAFETY: input_event is plain old data
        unsafe { std::slice::from_raw_parts((&ev as *const libc::input_event).cast::<u8>(), EVENT_SIZE) }.to_vec()
    }

    #[test]
    fn presses_and_releases_only() {
        let mut data = event(EV_KEY, 30, 1); // KEY_A down
        data.extend(event(0, 0, 0)); // EV_SYN
        data.extend(event(EV_KEY, 30, 2)); // auto-repeat
        data.extend(event(EV_KEY, 30, 0)); // KEY_A up
        data.extend(event(EV_KEY, 0x2ff, 1)); // no HID usage
        let mut seen = Vec::new();
        parse_events(&data, &mut |usage, pressed| seen.push((usage, pressed)));
        assert_eq!(seen, vec![(0x04, true), (0x04, false)]);
    }
}
