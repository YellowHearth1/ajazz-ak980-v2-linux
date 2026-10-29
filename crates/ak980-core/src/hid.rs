//! HID access through Linux hidraw, with no third-party dependencies.

use std::ffi::CString;
use std::fs;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::tr;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HidInfo {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
    /// The USB interface number, -1 when it is not known.
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
    /// Report lengths in bytes including the report ID byte (like HIDP_CAPS on Windows).
    pub input_len: usize,
    pub output_len: usize,
    pub feature_len: usize,
}

impl std::fmt::Display for HidInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}  {:04x}:{:04x}  if{}  page {:#06x}/{:#04x}  in {} out {} feat {}  {}",
            self.path, self.vendor_id, self.product_id, self.interface, self.usage_page, self.usage,
            self.input_len, self.output_len, self.feature_len, self.name
        )
    }
}

/// (usage page, usage, input, output, feature lengths) of the first top-level collection.
pub fn parse_descriptor(desc: &[u8]) -> (u16, u16, usize, usize, usize) {
    let (mut usage_page, mut usage) = (0u32, 0u32);
    let mut first: Option<(u32, u32)> = None;
    let (mut report_size, mut report_count) = (0u32, 0u32);
    let (mut input_bits, mut output_bits, mut feature_bits) = (0u32, 0u32, 0u32);
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < desc.len() {
        let prefix = desc[i];
        if prefix == 0xFE {
            // long item
            let len = desc.get(i + 1).copied().unwrap_or(0) as usize;
            i += 3 + len;
            continue;
        }
        let size = [0usize, 1, 2, 4][(prefix & 0x03) as usize];
        let mut value = 0u32;
        for (k, b) in desc.iter().skip(i + 1).take(size).enumerate() {
            value |= (*b as u32) << (8 * k);
        }
        let tag = prefix & 0xFC;
        i += 1 + size;
        match tag {
            0x04 => usage_page = value,
            0x08 => usage = value,
            0x74 => report_size = value,
            0x94 => report_count = value,
            0xA0 => {
                if depth == 0 && first.is_none() {
                    first = Some((usage_page, usage));
                }
                depth += 1;
            }
            0xC0 => depth -= 1,
            0x80 => input_bits += report_size * report_count,
            0x90 => output_bits += report_size * report_count,
            0xB0 => feature_bits += report_size * report_count,
            _ => {}
        }
    }
    let length = |bits: u32| if bits == 0 { 0 } else { (bits as usize).div_ceil(8) + 1 };
    let (page, use_) = first.unwrap_or((0, 0));
    (page as u16, use_ as u16, length(input_bits), length(output_bits), length(feature_bits))
}

fn uevent(sys_dir: &Path) -> Vec<(String, String)> {
    fs::read_to_string(sys_dir.join("device/uevent"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_string(), v.trim().to_string())))
        .collect()
}

/// Every hidraw node, optionally only those of one vendor and product.
pub fn enumerate(vendor_id: Option<u16>, product_id: Option<u16>) -> Vec<HidInfo> {
    let mut dirs: Vec<_> = fs::read_dir("/sys/class/hidraw")
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    dirs.sort_by_key(|p| {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        name.trim_start_matches("hidraw").parse::<u32>().unwrap_or(u32::MAX)
    });
    let mut found = Vec::new();
    for dir in dirs {
        let ev = uevent(&dir);
        let get = |k: &str| ev.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str()).unwrap_or("");
        // HID_ID=0003:00000C45:0000FEFE
        let mut parts = get("HID_ID").split(':').skip(1);
        let (Some(v), Some(p)) = (parts.next(), parts.next()) else { continue };
        let (Ok(vid), Ok(pid)) = (u32::from_str_radix(v, 16), u32::from_str_radix(p, 16)) else { continue };
        let (vid, pid) = (vid as u16, pid as u16);
        if vendor_id.is_some_and(|x| x != vid) || product_id.is_some_and(|x| x != pid) {
            continue;
        }
        let Ok(desc) = fs::read(dir.join("device/report_descriptor")) else { continue };
        let phys = get("HID_PHYS");
        let interface = phys
            .rsplit_once("input")
            .and_then(|(_, n)| n.parse::<i32>().ok())
            .unwrap_or(-1);
        let (usage_page, usage, input_len, output_len, feature_len) = parse_descriptor(&desc);
        found.push(HidInfo {
            path: format!("/dev/{}", dir.file_name().and_then(|n| n.to_str()).unwrap_or("")),
            vendor_id: vid,
            product_id: pid,
            name: get("HID_NAME").to_string(),
            interface,
            usage_page,
            usage,
            input_len,
            output_len,
            feature_len,
        });
    }
    found
}

/// The sysfs directory of the USB device a hidraw node belongs to (`/sys/devices/.../1-1.2`).
pub fn usb_device_dir(hidraw_path: &str) -> Option<std::path::PathBuf> {
    let name = Path::new(hidraw_path).file_name()?;
    let dev = fs::canonicalize(Path::new("/sys/class/hidraw").join(name).join("device")).ok()?;
    // hidraw -> HID device -> USB interface (1-1.2:1.3) -> USB device (1-1.2)
    let usb = dev.parent()?.parent()?;
    usb.join("idVendor").exists().then(|| usb.to_path_buf())
}

/// What the protocol code needs from a HID device; the tests replace it with a fake.
pub trait Hid: Send {
    fn info(&self) -> &HidInfo;
    /// Sends a feature report; byte 0 is the report ID.
    fn set_feature(&mut self, report: &[u8]) -> io::Result<()>;
    /// Reads a feature report, returned with the report ID in byte 0.
    fn get_feature(&mut self, report_id: u8, len: usize) -> io::Result<Vec<u8>>;
    /// Sends an output report; byte 0 is the report ID (0 for unnumbered reports).
    fn write(&mut self, report: &[u8], timeout: Duration) -> io::Result<()>;
    /// An input report (without a report ID byte for unnumbered reports), `None` on timeout.
    fn read(&mut self, timeout: Duration) -> io::Result<Option<Vec<u8>>>;
    fn drain(&mut self) -> io::Result<()> {
        while self.read(Duration::ZERO)?.is_some() {}
        Ok(())
    }
    /// Takes the cross-process lock; calls nest.
    fn lock(&mut self) -> io::Result<()>;
    fn unlock(&mut self);
}

const fn ioc_rw(nr: u64, size: usize) -> u64 {
    (3 << 30) | ((size as u64) << 16) | ((b'H' as u64) << 8) | nr
}

/// The keyboard briefly stalls (EPIPE) when it is busy; a short pause is enough.
fn retry<T>(mut f: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    let mut attempt = 0;
    loop {
        match f() {
            Err(e) if attempt < 3 && matches!(e.raw_os_error(), Some(libc::EPIPE) | Some(libc::ETIMEDOUT)) => {
                attempt += 1;
                sleep(Duration::from_millis(5 * attempt));
            }
            other => return other,
        }
    }
}

fn poll_fd(fd: i32, events: i16, timeout: Duration) -> io::Result<bool> {
    let mut p = libc::pollfd { fd, events, revents: 0 };
    let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    loop {
        // SAFETY: one valid pollfd
        let n = unsafe { libc::poll(&mut p, 1, ms) };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        if n > 0 && p.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 && p.revents & events == 0 {
            return Err(io::Error::from_raw_os_error(libc::ENODEV));
        }
        return Ok(n > 0);
    }
}

/// An open hidraw node.
pub struct HidDevice {
    info: HidInfo,
    fd: OwnedFd,
    lock_depth: u32,
}

impl HidDevice {
    pub fn open(info: &HidInfo) -> Result<Self> {
        let path = CString::new(info.path.as_str()).map_err(|_| Error::invalid("bad path"))?;
        // SAFETY: a valid C string; the fd is owned from here on
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDWR | libc::O_NONBLOCK | libc::O_CLOEXEC) };
        if fd < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::PermissionDenied {
                return Err(Error::Permission(tr!(
                    "No access to {path}. Install the udev rule (Device page or `ak980 setup`) and reconnect the keyboard.",
                    path = info.path
                )));
            }
            return Err(e.into());
        }
        Ok(Self { info: info.clone(), fd: unsafe { OwnedFd::from_raw_fd(fd) }, lock_depth: 0 })
    }

    fn raw(&self) -> i32 {
        self.fd.as_raw_fd()
    }
}

impl Hid for HidDevice {
    fn info(&self) -> &HidInfo {
        &self.info
    }

    fn set_feature(&mut self, report: &[u8]) -> io::Result<()> {
        let mut buf = report.to_vec();
        let fd = self.raw();
        retry(|| {
            // SAFETY: HIDIOCSFEATURE reads buf.len() bytes from buf
            let r = unsafe { libc::ioctl(fd, ioc_rw(0x06, buf.len()) as _, buf.as_mut_ptr()) };
            if r < 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
        })
    }

    fn get_feature(&mut self, report_id: u8, len: usize) -> io::Result<Vec<u8>> {
        let fd = self.raw();
        retry(|| {
            let mut buf = vec![0u8; len];
            buf[0] = report_id;
            // SAFETY: HIDIOCGFEATURE writes at most buf.len() bytes into buf
            let r = unsafe { libc::ioctl(fd, ioc_rw(0x07, buf.len()) as _, buf.as_mut_ptr()) };
            if r < 0 {
                return Err(io::Error::last_os_error());
            }
            if r > 0 && (r as usize) < buf.len() {
                buf.truncate(r as usize);
            }
            Ok(buf)
        })
    }

    fn write(&mut self, report: &[u8], timeout: Duration) -> io::Result<()> {
        let fd = self.raw();
        let deadline = Instant::now() + timeout;
        let mut rest = report;
        while !rest.is_empty() {
            let left = deadline.saturating_duration_since(Instant::now());
            if !poll_fd(fd, libc::POLLOUT, left)? {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    tr!("Timed out writing to {path}", path = self.info.path),
                ));
            }
            // SAFETY: writes from a valid slice
            let n = unsafe { libc::write(fd, rest.as_ptr() as *const libc::c_void, rest.len()) };
            if n < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            }
            rest = &rest[n as usize..];
        }
        Ok(())
    }

    fn read(&mut self, timeout: Duration) -> io::Result<Option<Vec<u8>>> {
        let fd = self.raw();
        if !poll_fd(fd, libc::POLLIN, timeout)? {
            return Ok(None);
        }
        let mut buf = vec![0u8; 4097];
        // SAFETY: reads into a valid buffer
        let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::WouldBlock {
                return Ok(None);
            }
            return Err(e);
        }
        buf.truncate(n as usize);
        Ok(Some(buf))
    }

    /// An advisory flock on the node keeps the app, the agent and the system service from
    /// interleaving their begin/data/commit sequences.
    fn lock(&mut self) -> io::Result<()> {
        if self.lock_depth == 0 {
            // SAFETY: flock on our own fd
            if unsafe { libc::flock(self.raw(), libc::LOCK_EX) } < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        self.lock_depth += 1;
        Ok(())
    }

    fn unlock(&mut self) {
        if self.lock_depth == 0 {
            return;
        }
        self.lock_depth -= 1;
        if self.lock_depth == 0 {
            // SAFETY: flock on our own fd
            unsafe { libc::flock(self.raw(), libc::LOCK_UN) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_of_the_command_interface() {
        // Vendor page FF13, usage 01, 64-byte input, output and feature reports
        let desc = [
            0x06, 0x13, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x15, 0x00, 0x26, 0xFF, 0x00, 0x75, 0x08, 0x95, 0x40,
            0x09, 0x02, 0x81, 0x02, 0x09, 0x03, 0x91, 0x02, 0x09, 0x04, 0xB1, 0x02, 0xC0,
        ];
        assert_eq!(parse_descriptor(&desc), (0xFF13, 0x01, 65, 65, 65));
    }

    #[test]
    fn ioctl_numbers_match_the_kernel() {
        // HIDIOCSFEATURE(65) and HIDIOCGFEATURE(65) from linux/hidraw.h
        assert_eq!(ioc_rw(0x06, 65), 0xC041_4806);
        assert_eq!(ioc_rw(0x07, 65), 0xC041_4807);
    }
}
