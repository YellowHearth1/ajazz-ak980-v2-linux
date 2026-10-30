//! AJAZZ AK980 V2 protocol (SONiX firmware).
//!
//! Reverse engineered from the vendor's DeviceDriver.exe in donglover69/ak980-studio (MIT); the
//! real-time stream matches the AULA F108 Pro driver in hcode10/OpenRGB-With-AulaF108Pro-support.
//! Re-tested on an AK980 V2 over Linux hidraw; `docs/PROTOCOL.md` has the full write-up.
//!
//! Wired (`0C45:8009`): interface 3, usage page `0xFF13`, 65-byte feature reports (report ID 0 +
//! 64-byte command); interface 2, usage page `0xFF68`, 4097-byte output reports (report ID 0 + one
//! 4 KiB screen page). A transaction is `04 18` (begin), `04 OP ARG .. COUNT`, COUNT data packets,
//! `04 02` (commit) and for lighting and key layers `04 F0` (apply).
//!
//! 2.4 GHz dongle (`0C45:FEFE`), interface 3, usage page `0xFF60`: 33-byte output reports (report
//! ID 0 + command + byte sum in the last byte). The reply repeats the first three command bytes.

use std::collections::BTreeMap;
use std::sync::RwLock;
use std::thread::sleep;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::hid::{self, Hid, HidDevice, HidInfo};
use crate::keymap::{encode_layer, KeyAction, TABLE_PACKETS as KEYMAP_PACKETS};
use crate::lighting::{KeyboardOptions, LightSettings, MAX_LEVEL};
use crate::macros::{encode_macros, Macro};
use crate::time::LocalTime;
use crate::tr;

pub const VENDOR_ID: u16 = 0x0C45;
pub const WIRED_PID: u16 = 0x8009;
pub const DONGLE_PID: u16 = 0xFEFE;

pub const LCD_WIDTH: usize = 240;
pub const LCD_HEIGHT: usize = 135;
pub const LCD_HEADER: usize = 256;
pub const LCD_MAX_FRAMES: usize = 140;
pub const LCD_PAGE: usize = 4096;

/// 8 packets × 16 entries of `[index R G B]`.
pub const REALTIME_LEDS: usize = 128;

const PREFIX: u8 = 0x04;
const OP_BEGIN: u8 = 0x18;
const OP_COMMIT: u8 = 0x02;
const OP_APPLY: u8 = 0xF0;
const OP_LIGHT: u8 = 0x13;
const OP_REALTIME: u8 = 0x20;
const OP_CUSTOM_COLORS: u8 = 0x23;
const OP_READ_CUSTOM: u8 = 0xF5;
const OP_OPTIONS: u8 = 0x17;
const OP_KEYMAP_TOP: u8 = 0x11;
const OP_KEYMAP_FN: u8 = 0x27;
const OP_MACRO_BEGIN: u8 = 0x19;
const OP_MACRO_DATA: u8 = 0x15;
const OP_CLOCK: u8 = 0x28;
const OP_LCD_UPLOAD: u8 = 0x72;
const CUSTOM_PACKETS: usize = 9;

/// `SLOT 5A YY MM DD hh mm ss 00 DOW` (Sunday = 0).
pub fn clock_payload(now: &LocalTime, slot: u8) -> [u8; 10] {
    [slot, 0x5A, (now.year % 100) as u8, now.month, now.day, now.hour, now.minute, now.second, 0, now.weekday]
}

/// How the keyboard is reachable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    Usb,
    Dongle,
}

impl Link {
    pub fn id(self) -> &'static str {
        match self {
            Link::Usb => "usb",
            Link::Dongle => "2.4g",
        }
    }
}

// ------------------------------------------------------------ colour calibration

/// How colours are adjusted for the keyboard's LEDs before they are sent: per channel a gain (the
/// white balance: its red LEDs are stronger, so white looks pink) and a curve (how mid levels come
/// out: 1.00 sends them as they are, more makes them darker, less brighter).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calibration {
    /// Red, green and blue, in percent of full strength.
    pub gains: [u8; 3],
    /// Red, green and blue curves, in hundredths (100 = 1.00).
    pub curves: [u16; 3],
}

impl Calibration {
    pub const NONE: Self = Self { gains: [100; 3], curves: [100; 3] };

    /// `c` as the keyboard should get it.
    pub fn apply(&self, c: [u8; 3]) -> [u8; 3] {
        if *self == Self::NONE {
            return c;
        }
        [0, 1, 2].map(|i| {
            let curve = self.curves[i].clamp(25, 400) as f32 / 100.0;
            let v = (c[i] as f32 / 255.0).powf(curve) * self.gains[i].min(100) as f32 / 100.0;
            (v * 255.0).round().clamp(0.0, 255.0) as u8
        })
    }
}

/// The calibration every keyboard opened from now on uses.
static CALIBRATION: RwLock<Calibration> = RwLock::new(Calibration::NONE);

/// Sets the calibration (`settings.white_balance` and `settings.led_curves`) for the keyboards this
/// process opens next.
pub fn set_calibration(c: Calibration) {
    *CALIBRATION.write().unwrap_or_else(|e| e.into_inner()) = c;
}

pub fn calibration() -> Calibration {
    *CALIBRATION.read().unwrap_or_else(|e| e.into_inner())
}

/// The keyboard on a USB cable: every setting is available.
pub struct WiredKeyboard {
    cmd: Box<dyn Hid>,
    lcd: Option<Box<dyn Hid>>,
    /// Pause before each configuration command, as in the vendor driver (`<cmd_delaytime value="35"/>`).
    pub delay: Duration,
    /// Applied to every colour sent (effect colour, per-key colours, real-time frames).
    pub calibration: Calibration,
}

impl WiredKeyboard {
    pub fn find() -> Option<(HidInfo, Option<HidInfo>)> {
        let devs = hid::enumerate(Some(VENDOR_ID), Some(WIRED_PID));
        let cmd = devs.iter().find(|d| d.usage_page == 0xFF13 && d.feature_len >= 65)?.clone();
        let lcd = devs.iter().find(|d| d.usage_page == 0xFF68 && d.output_len > LCD_PAGE).cloned();
        Some((cmd, lcd))
    }

    pub fn open() -> Result<Self> {
        match Self::find() {
            Some((cmd, lcd)) => Self::open_found(&cmd, lcd.as_ref()),
            None if Dongle::find().is_some() => Err(Error::NotConnected(tr!(
                "This needs the USB cable: connect the keyboard with the cable and switch it to wired mode."
            ))),
            None => Err(Error::NotConnected(tr!("AK980 not found on USB"))),
        }
    }

    pub fn open_found(cmd: &HidInfo, lcd: Option<&HidInfo>) -> Result<Self> {
        let cmd = Box::new(HidDevice::open(cmd)?);
        let lcd: Option<Box<dyn Hid>> = match lcd {
            Some(info) => Some(Box::new(HidDevice::open(info)?)),
            None => None,
        };
        let mut kb = Self::with_transport(cmd, lcd);
        kb.calibration = calibration();
        Ok(kb)
    }

    /// For tests: any [`Hid`] implementation. Colours go out as they are (no white balance).
    pub fn with_transport(cmd: Box<dyn Hid>, lcd: Option<Box<dyn Hid>>) -> Self {
        Self { cmd, lcd, delay: Duration::from_millis(35), calibration: Calibration::NONE }
    }

    pub fn command_port(&mut self) -> &mut dyn Hid {
        self.cmd.as_mut()
    }

    pub fn lcd_port(&mut self) -> Option<&mut (dyn Hid + 'static)> {
        self.lcd.as_deref_mut()
    }

    /// Runs `f` with the cross-process lock held; calls nest.
    pub fn locked<R>(&mut self, f: impl FnOnce(&mut Self) -> Result<R>) -> Result<R> {
        self.cmd.lock()?;
        let r = f(self);
        self.cmd.unlock();
        r
    }

    // ------------------------------------------------------------ transport

    fn report(packet: &[u8]) -> [u8; 65] {
        let mut r = [0u8; 65];
        let n = packet.len().min(64);
        r[1..1 + n].copy_from_slice(&packet[..n]);
        r
    }

    /// Sends a 64-byte command and reads the echo (byte 4 of the reply is a status).
    pub fn send_packet(&mut self, packet: &[u8]) -> Result<Vec<u8>> {
        let report = Self::report(packet);
        self.locked(|kb| {
            sleep(kb.delay);
            kb.cmd.set_feature(&report)?;
            sleep(kb.delay);
            Ok(kb.cmd.get_feature(0, 65)?)
        })
    }

    fn send_noack(&mut self, packet: &[u8]) -> Result<()> {
        let report = Self::report(packet);
        self.locked(|kb| {
            sleep(kb.delay);
            kb.cmd.set_feature(&report)?;
            Ok(())
        })
    }

    fn command(&mut self, op: u8, arg: u8, count: u16, extra: &[(usize, u8)]) -> Result<Vec<u8>> {
        let mut p = [0u8; 64];
        p[0] = PREFIX;
        p[1] = op;
        p[2] = arg;
        p[8] = (count & 0xFF) as u8;
        p[9] = (count >> 8) as u8;
        for &(i, v) in extra {
            p[i] = v;
        }
        self.send_packet(&p)
    }

    fn begin(&mut self) -> Result<()> {
        self.command(OP_BEGIN, 0, 0, &[]).map(drop)
    }

    fn commit(&mut self) -> Result<()> {
        self.command(OP_COMMIT, 0, 0, &[]).map(drop)
    }

    fn apply(&mut self, ack: bool) -> Result<()> {
        let p = [PREFIX, OP_APPLY];
        if ack { self.send_packet(&p).map(drop) } else { self.send_noack(&p) }
    }

    // ------------------------------------------------------------ clock and screen

    /// Sets the screen clock. `slot` is the screen page the vendor driver sends with the time.
    pub fn sync_clock(&mut self, now: Option<LocalTime>, slot: u8) -> Result<()> {
        let now = now.unwrap_or_else(LocalTime::now);
        self.locked(|kb| {
            kb.begin()?;
            kb.command(OP_CLOCK, 0, 0, &[(8, 1)])?;
            let mut p = [0u8; 64];
            p[1..11].copy_from_slice(&clock_payload(&now, slot));
            p[62] = 0xAA;
            p[63] = 0x55;
            kb.send_packet(&p)?;
            kb.commit()
        })
    }

    /// Uploads an animation (see [`crate::lcd::encode_animation`]), about 145 ms per 4 KiB page.
    ///
    /// Returns `false` when `cancel` stopped it; the keyboard keeps the pages written so far.
    pub fn upload_lcd(
        &mut self,
        blob: &[u8],
        slot: u8,
        progress: &mut dyn FnMut(f32),
        cancel: &dyn Fn() -> bool,
    ) -> Result<bool> {
        if self.lcd.is_none() {
            return Err(Error::NotConnected(tr!("The screen interface (0xFF68) was not found")));
        }
        if !blob.len().is_multiple_of(LCD_PAGE) {
            return Err(Error::invalid("blob size must be a multiple of 4096"));
        }
        let pages = blob.len() / LCD_PAGE;
        self.locked(|kb| {
            let lcd = kb.lcd.as_mut().expect("checked above");
            lcd.drain()?;
            kb.begin()?;
            kb.command(OP_LCD_UPLOAD, slot, pages as u16, &[])?;
            let mut done = true;
            let mut page = vec![0u8; LCD_PAGE + 1];
            for i in 0..pages {
                if cancel() {
                    done = false;
                    break;
                }
                page[1..].copy_from_slice(&blob[i * LCD_PAGE..(i + 1) * LCD_PAGE]);
                let lcd = kb.lcd.as_mut().expect("checked above");
                lcd.write(&page, Duration::from_secs(3))?;
                lcd.read(Duration::from_millis(300))?; // the keyboard acknowledges every flash write
                progress((i + 1) as f32 / pages as f32);
            }
            kb.commit()?;
            Ok(done)
        })
    }

    // ------------------------------------------------------------ lighting

    pub fn set_lighting(&mut self, s: &LightSettings) -> Result<()> {
        let mut s = s.clone();
        [s.r, s.g, s.b] = self.calibration.apply([s.r, s.g, s.b]);
        self.locked(|kb| {
            kb.begin()?;
            kb.command(OP_LIGHT, 0, 1, &[])?;
            kb.send_noack(&s.to_packet())?;
            kb.commit()?;
            kb.apply(false)
        })
    }

    /// Static per-key colours stored in the keyboard: {LED index: RGB}; the rest are black.
    pub fn set_custom_colors(&mut self, colors: &BTreeMap<u8, [u8; 3]>, brightness: u8) -> Result<()> {
        let mut mode = [0u8; 64];
        mode[0] = 0x80;
        mode[9] = brightness.min(MAX_LEVEL);
        mode[14] = 0xAA;
        mode[15] = 0x55;
        let mut table = vec![0u8; CUSTOM_PACKETS * 64];
        for (&idx, rgb) in colors {
            let i = idx as usize * 4;
            if i + 3 < table.len() - 2 {
                table[i] = idx;
                table[i + 1..i + 4].copy_from_slice(&self.calibration.apply(*rgb));
            }
        }
        let n = table.len();
        table[n - 2] = 0xAA;
        table[n - 1] = 0x55;
        self.locked(|kb| {
            kb.begin()?;
            kb.command(OP_LIGHT, 0, 1, &[])?;
            kb.send_noack(&mode)?;
            kb.commit()?;
            kb.apply(false)?;
            kb.begin()?;
            kb.command(OP_CUSTOM_COLORS, 0, CUSTOM_PACKETS as u16, &[])?;
            for chunk in table.chunks(64) {
                kb.send_noack(chunk)?;
            }
            kb.commit()?;
            kb.apply(false)
        })
    }

    /// One frame of the real-time stream (nothing is written to flash).
    ///
    /// `table` holds `[index R G B]` × 128. The keyboard shows the frame until the next one, or
    /// until a configuration command replaces it. Reports closer than ~1 ms make it stall the bus.
    pub fn send_realtime(&mut self, table: &[u8; REALTIME_LEDS * 4], gap: Duration) -> Result<()> {
        let mut table = *table;
        if self.calibration != Calibration::NONE {
            for entry in table.chunks_mut(4) {
                let [r, g, b] = self.calibration.apply([entry[1], entry[2], entry[3]]);
                entry[1..4].copy_from_slice(&[r, g, b]);
            }
        }
        self.locked(|kb| {
            let mut head = [0u8; 65];
            head[1] = PREFIX;
            head[2] = OP_REALTIME;
            head[9] = 8;
            kb.cmd.set_feature(&head)?;
            kb.cmd.get_feature(0, 65)?;
            let mut report = [0u8; 65];
            for chunk in table.chunks(64) {
                sleep(gap);
                report[1..].copy_from_slice(chunk);
                kb.cmd.set_feature(&report)?;
            }
            sleep(gap);
            let mut tail = [0u8; 65];
            tail[1] = PREFIX;
            tail[2] = OP_COMMIT;
            kb.cmd.set_feature(&tail)?;
            kb.cmd.get_feature(0, 65)?;
            Ok(())
        })
    }

    pub fn set_side_light(&mut self, mode: u8) -> Result<()> {
        let mut p = [0u8; 64];
        p[0] = mode;
        p[14] = 0xAA;
        p[15] = 0x55;
        self.locked(|kb| {
            kb.begin()?;
            kb.command(OP_LIGHT, 0, 1, &[])?;
            kb.send_noack(&p)?;
            kb.commit()?;
            kb.apply(false)
        })
    }

    /// Reads the per-key colour table (`[idx R G B]` × 144) without changing anything.
    ///
    /// On the AK980 V2 the table does not follow animated effects, so it is only a diagnostic.
    pub fn read_custom_table(&mut self) -> Result<Vec<u8>> {
        self.locked(|kb| {
            sleep(Duration::from_millis(10));
            let mut cmd = [0u8; 65];
            cmd[1] = PREFIX;
            cmd[2] = OP_READ_CUSTOM;
            kb.cmd.set_feature(&cmd)?;
            sleep(Duration::from_millis(5));
            let mut echo = None;
            for _ in 0..10 {
                match kb.cmd.get_feature(0, 65) {
                    Ok(r) => {
                        echo = Some(r);
                        break;
                    }
                    Err(_) => sleep(Duration::from_millis(5)),
                }
            }
            let mut out = Vec::new();
            if echo.as_ref().is_some_and(|e| e.get(4) == Some(&1)) {
                let (mut got, mut reads) = (0, 0);
                while got < CUSTOM_PACKETS && reads < CUSTOM_PACKETS + 2 {
                    reads += 1;
                    sleep(Duration::from_millis(3));
                    let t0 = Instant::now();
                    let Ok(r) = kb.cmd.get_feature(0, 65) else { break };
                    if t0.elapsed() > Duration::from_millis(360) {
                        break;
                    }
                    if r.get(1) == Some(&PREFIX) && r.get(2) == Some(&OP_READ_CUSTOM) {
                        continue;
                    }
                    out.extend_from_slice(&r[1..r.len().min(65)]);
                    got += 1;
                }
            }
            kb.commit()?;
            Ok(out)
        })
    }

    // ------------------------------------------------------------ options, keys, macros

    pub fn set_options(&mut self, o: &KeyboardOptions) -> Result<()> {
        self.locked(|kb| {
            kb.begin()?;
            kb.command(OP_OPTIONS, 1, 1, &[])?;
            kb.send_noack(&o.to_packet())?;
            kb.commit()
        })
    }

    /// Replaces a whole layer: keys missing from `actions` go back to their default.
    pub fn set_keymap(&mut self, fn_layer: bool, actions: &BTreeMap<u8, KeyAction>) -> Result<()> {
        let table = encode_layer(actions);
        self.locked(|kb| {
            kb.begin()?;
            kb.command(if fn_layer { OP_KEYMAP_FN } else { OP_KEYMAP_TOP }, 0, KEYMAP_PACKETS as u16, &[])?;
            for chunk in table.chunks(64) {
                kb.send_noack(chunk)?;
            }
            kb.commit()?;
            kb.apply(true)
        })
    }

    pub fn set_macros(&mut self, macros: &[Macro]) -> Result<()> {
        let (data, pages) = encode_macros(macros)?;
        self.locked(|kb| {
            kb.command(OP_MACRO_BEGIN, 0, 0, &[])?;
            kb.command(OP_MACRO_DATA, 0, pages as u16, &[])?;
            sleep(Duration::from_millis(30));
            for chunk in data.chunks(64) {
                kb.send_noack(chunk)?;
            }
            sleep(Duration::from_millis(30));
            kb.commit()
        })
    }
}

/// The keyboard in 2.4 GHz mode through its dongle: battery level and clock.
pub struct Dongle {
    dev: Box<dyn Hid>,
}

impl Dongle {
    pub fn find() -> Option<HidInfo> {
        let mut devs: Vec<HidInfo> = hid::enumerate(Some(VENDOR_ID), Some(DONGLE_PID))
            .into_iter()
            .filter(|d| d.output_len >= 33 && d.input_len >= 33)
            .collect();
        devs.sort_by_key(|d| (d.usage_page != 0xFF60, d.interface != 3));
        devs.into_iter().next()
    }

    pub fn open() -> Result<Self> {
        let info = Self::find().ok_or_else(|| Error::NotConnected(tr!("AK980 2.4 GHz dongle not found")))?;
        Self::open_found(&info)
    }

    pub fn open_found(info: &HidInfo) -> Result<Self> {
        Ok(Self { dev: Box::new(HidDevice::open(info)?) })
    }

    pub fn with_transport(dev: Box<dyn Hid>) -> Self {
        Self { dev }
    }

    pub fn info(&self) -> &HidInfo {
        self.dev.info()
    }

    /// Sends a command and waits for the reply that repeats its first three bytes.
    pub fn request(&mut self, command: &[u8]) -> Result<Option<Vec<u8>>> {
        let n = self.dev.info().output_len.max(33);
        let mut report = vec![0u8; n];
        let len = command.len().min(n - 2);
        report[1..1 + len].copy_from_slice(&command[..len]);
        report[n - 1] = report[..n - 1].iter().fold(0u8, |s, b| s.wrapping_add(*b));
        let dev = &mut self.dev;
        dev.lock()?;
        let result = (|| -> Result<Option<Vec<u8>>> {
            dev.drain()?;
            dev.write(&report, Duration::from_millis(500))?;
            for _ in 0..20 {
                let Some(r) = dev.read(Duration::from_millis(50))? else { continue };
                // hidraw drops the report ID of unnumbered reports; Windows keeps it
                let data = if r.len() == n { r[1..].to_vec() } else { r };
                if data.len() >= 3 && data[..3] == report[1..4] {
                    return Ok(Some(data));
                }
            }
            Ok(None)
        })();
        dev.unlock();
        result
    }

    /// Battery charge in percent, `None` when the keyboard did not answer.
    pub fn read_battery(&mut self) -> Result<Option<u8>> {
        // After a while idle the first request only wakes the radio link
        let reply = match self.request(&[0x20, 0x01])? {
            Some(r) => Some(r),
            None => self.request(&[0x20, 0x01])?,
        };
        Ok(reply.filter(|r| r.len() >= 4 && r[3] != 0).map(|r| r[3].min(100)))
    }

    /// `0C 10 00 00 SLOT 5A YY MM DD hh mm ss 00 DOW .. .. .. AA 55` (vendor driver, wireless variant).
    pub fn sync_clock(&mut self, now: Option<LocalTime>, slot: u8) -> Result<bool> {
        let now = now.unwrap_or_else(LocalTime::now);
        let mut cmd = [0u8; 19];
        cmd[0] = 0x0C;
        cmd[1] = 0x10;
        cmd[4..14].copy_from_slice(&clock_payload(&now, slot));
        cmd[17] = 0xAA;
        cmd[18] = 0x55;
        Ok(self.request(&cmd)?.is_some())
    }
}

/// `Usb`, `Dongle` or `None`. Reads sysfs only, so it is cheap enough to poll.
pub fn connection_state() -> Option<Link> {
    if WiredKeyboard::find().is_some() {
        Some(Link::Usb)
    } else if Dongle::find().is_some() {
        Some(Link::Dongle)
    } else {
        None
    }
}

/// The receiver's USB device in sysfs (`/sys/devices/.../1-1.2`), when it is plugged in.
pub fn receiver_usb_dir() -> Option<std::path::PathBuf> {
    hid::enumerate(Some(VENDOR_ID), Some(DONGLE_PID)).iter().find_map(|d| hid::usb_device_dir(&d.path))
}

/// Root script that makes the kernel drop the receiver and set it up again, as if it had been
/// unplugged and plugged back in: the USB device's `authorized` switch, off and on.
pub fn reconnect_script(usb_dir: &std::path::Path) -> String {
    let switch = crate::service::shell_quote(&usb_dir.join("authorized").to_string_lossy());
    format!("set -e\necho 0 > {switch}\nsleep 1\necho 1 > {switch}\n")
}

pub enum AnyKeyboard {
    Wired(WiredKeyboard),
    Dongle(Dongle),
}

impl AnyKeyboard {
    /// The cable wins: every feature is available through it.
    pub fn open() -> Result<Self> {
        if WiredKeyboard::find().is_some() {
            return Ok(Self::Wired(WiredKeyboard::open()?));
        }
        if Dongle::find().is_some() {
            return Ok(Self::Dongle(Dongle::open()?));
        }
        Err(Error::NotConnected(tr!(
            "AJAZZ AK980 not found (neither on USB nor through the 2.4 GHz dongle)"
        )))
    }

    pub fn link(&self) -> Link {
        match self {
            Self::Wired(_) => Link::Usb,
            Self::Dongle(_) => Link::Dongle,
        }
    }

    /// Sets the screen clock over whatever link is there; `false` when the dongle got no answer.
    pub fn sync_clock(&mut self, now: Option<LocalTime>, slot: u8) -> Result<bool> {
        match self {
            Self::Wired(kb) => kb.sync_clock(now, slot).map(|_| true),
            Self::Dongle(d) => d.sync_clock(now, slot),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io;
    use std::sync::{Arc, Mutex};

    /// Records what the protocol code sends and answers like the keyboard does.
    #[derive(Clone, Default)]
    pub struct FakeHid {
        pub info: HidInfo,
        pub features: Arc<Mutex<Vec<Vec<u8>>>>,
        pub writes: Arc<Mutex<Vec<Vec<u8>>>>,
        pub replies: Arc<Mutex<Vec<Vec<u8>>>>,
        /// Answer reads with the last write minus its report ID, like the dongle's echo.
        pub echo: bool,
    }

    impl FakeHid {
        pub fn new(output_len: usize) -> Self {
            let mut f = Self::default();
            f.info.output_len = output_len;
            f.info.path = "/dev/fake".into();
            f
        }

        /// The 64-byte command payloads sent so far.
        pub fn commands(&self) -> Vec<Vec<u8>> {
            self.features.lock().unwrap().iter().map(|f| f[1..].to_vec()).collect()
        }
    }

    impl Hid for FakeHid {
        fn info(&self) -> &HidInfo {
            &self.info
        }
        fn set_feature(&mut self, report: &[u8]) -> io::Result<()> {
            self.features.lock().unwrap().push(report.to_vec());
            Ok(())
        }
        fn get_feature(&mut self, _id: u8, len: usize) -> io::Result<Vec<u8>> {
            let last = self.features.lock().unwrap().last().cloned().unwrap_or_else(|| vec![0; 65]);
            let mut reply = last;
            reply.resize(len, 0);
            reply[4] = 1;
            Ok(reply)
        }
        fn write(&mut self, report: &[u8], _t: Duration) -> io::Result<()> {
            self.writes.lock().unwrap().push(report.to_vec());
            Ok(())
        }
        fn read(&mut self, _t: Duration) -> io::Result<Option<Vec<u8>>> {
            if self.echo {
                return Ok(self.writes.lock().unwrap().last().map(|w| w[1..].to_vec()));
            }
            let mut r = self.replies.lock().unwrap();
            Ok(if r.is_empty() { None } else { Some(r.remove(0)) })
        }
        fn drain(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn lock(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn unlock(&mut self) {}
    }

    fn keyboard() -> (WiredKeyboard, FakeHid, FakeHid) {
        let (cmd, lcd) = (FakeHid::new(65), FakeHid::new(4097));
        let mut kb = WiredKeyboard::with_transport(Box::new(cmd.clone()), Some(Box::new(lcd.clone())));
        kb.delay = Duration::ZERO;
        (kb, cmd, lcd)
    }

    fn op(p: &[u8]) -> (u8, u8) {
        (p[0], p[1])
    }

    #[test]
    fn clock_payload_layout() {
        // 28 October 2026 is a Wednesday (3, Sunday = 0)
        let p = clock_payload(&LocalTime::new(2026, 10, 28, 13, 35, 33), 1);
        assert_eq!(p, [1, 0x5A, 26, 10, 28, 13, 35, 33, 0, 3]);
    }

    #[test]
    fn sync_clock_sequence() {
        let (mut kb, cmd, _) = keyboard();
        let t = LocalTime::new(2026, 10, 28, 13, 35, 33);
        kb.sync_clock(Some(t), 1).unwrap();
        let sent = cmd.commands();
        assert_eq!(sent.iter().map(|p| op(p)).collect::<Vec<_>>(), [(4, 0x18), (4, 0x28), (0, 1), (4, 0x02)]);
        assert_eq!(sent[1][8], 1);
        assert_eq!(&sent[2][1..11], &clock_payload(&t, 1));
        assert_eq!(&sent[2][62..64], &[0xAA, 0x55]);
        assert!(cmd.features.lock().unwrap().iter().all(|f| f.len() == 65 && f[0] == 0));
    }

    #[test]
    fn lighting_sequence() {
        let (mut kb, cmd, _) = keyboard();
        let s = LightSettings { mode: 7, r: 1, g: 2, b: 3, brightness: 4, speed: 2, direction: 3, rainbow: false };
        kb.set_lighting(&s).unwrap();
        let sent = cmd.commands();
        assert_eq!([op(&sent[0]), op(&sent[1]), op(&sent[3]), op(&sent[4])], [(4, 0x18), (4, 0x13), (4, 0x02), (4, 0xF0)]);
        assert_eq!(&sent[2][..4], &[7, 1, 2, 3]);
        assert_eq!(&sent[2][9..12], &[4, 2, 3]);
    }

    #[test]
    fn realtime_frame() {
        let (mut kb, cmd, _) = keyboard();
        let mut table = [0u8; 512];
        table[4..8].copy_from_slice(&[1, 10, 20, 30]);
        table[508..512].copy_from_slice(&[127, 1, 2, 3]);
        kb.send_realtime(&table, Duration::ZERO).unwrap();
        let f = cmd.features.lock().unwrap().clone();
        assert_eq!(f.len(), 10);
        assert_eq!((&f[0][1..3], f[0][9]), (&[4u8, 0x20][..], 8));
        let sent: Vec<u8> = f[1..9].iter().flat_map(|x| x[1..].to_vec()).collect();
        assert_eq!(&sent[..], &table[..]);
        assert_eq!(&f[9][1..3], &[4, 0x02]);
    }

    #[test]
    fn keymap_and_options() {
        let (mut kb, cmd, _) = keyboard();
        let mut layer = BTreeMap::new();
        layer.insert(5, KeyAction::parse("media:mute").unwrap());
        kb.set_keymap(true, &layer).unwrap();
        let sent = cmd.commands();
        assert_eq!((sent[0][1], sent[1][1], sent[1][8]), (0x18, 0x27, 9));
        let table: Vec<u8> = sent[2..11].concat();
        assert_eq!(&table[20..24], &[3, 0xE2, 0, 0]);
        assert_eq!(&table[574..], &[0xAA, 0x55]);
        cmd.features.lock().unwrap().clear();
        kb.set_options(&KeyboardOptions { disable_win: true, sleep_time: 3, key_response: 4, ..Default::default() })
            .unwrap();
        let sent = cmd.commands();
        assert_eq!(sent.iter().map(|p| p[1]).collect::<Vec<_>>(), [0x18, 0x17, 0x01, 0x02]);
        assert_eq!((sent[2][4], sent[2][6], sent[2][8]), (1, 3, 4));
    }

    #[test]
    fn calibration_adjusts_every_colour_sent() {
        let balance = Calibration { gains: [80, 100, 90], curves: [100; 3] };
        assert_eq!(balance.apply([255, 255, 255]), [204, 255, 230]);
        assert_eq!(Calibration::NONE.apply([10, 0, 255]), [10, 0, 255]);
        // A curve above 1 darkens the middle and leaves the ends where they are
        let curve = Calibration { gains: [100; 3], curves: [200, 100, 50] };
        assert_eq!(curve.apply([128, 128, 128]), [64, 128, 181]);
        assert_eq!(curve.apply([255, 0, 255]), [255, 0, 255]);
        let (mut kb, cmd, _) = keyboard();
        kb.calibration = balance;
        let s = LightSettings { mode: 1, r: 255, g: 255, b: 255, brightness: 5, speed: 0, direction: 0, rainbow: false };
        kb.set_lighting(&s).unwrap();
        assert_eq!(&cmd.commands()[2][1..4], &[204, 255, 230]);
        cmd.features.lock().unwrap().clear();
        let mut table = [0u8; 512];
        table[4..8].copy_from_slice(&[1, 255, 255, 255]);
        kb.send_realtime(&table, Duration::ZERO).unwrap();
        assert_eq!(&cmd.features.lock().unwrap()[1][5..9], &[1, 204, 255, 230]);
    }

    #[test]
    fn custom_colors() {
        let (mut kb, cmd, _) = keyboard();
        let mut colors = BTreeMap::new();
        colors.insert(1, [255, 0, 0]);
        kb.set_custom_colors(&colors, 3).unwrap();
        let sent = cmd.commands();
        assert_eq!((sent[2][0], sent[2][9]), (0x80, 3));
        let i = sent.iter().position(|p| op(p) == (4, 0x23)).unwrap();
        assert_eq!(sent[i][8], 9);
        let table: Vec<u8> = sent[i + 1..i + 10].concat();
        assert_eq!(&table[4..8], &[1, 255, 0, 0]);
    }

    #[test]
    fn macros_transaction() {
        let (mut kb, cmd, _) = keyboard();
        kb.set_macros(&[Macro::parse("text:a", None).unwrap()]).unwrap();
        let sent = cmd.commands();
        assert_eq!((op(&sent[0]), op(&sent[1])), ((4, 0x19), (4, 0x15)));
        let pages = sent[1][8] as usize;
        assert_eq!(sent.len(), 2 + pages + 1);
        assert_eq!(op(sent.last().unwrap()), (4, 0x02));
    }

    #[test]
    fn reconnect_switches_the_receiver_off_and_on() {
        let s = reconnect_script(std::path::Path::new("/sys/devices/pci0000:00/usb1/1-1/1-1.2"));
        assert_eq!(s, "set -e\necho 0 > /sys/devices/pci0000:00/usb1/1-1/1-1.2/authorized\nsleep 1\n\
                       echo 1 > /sys/devices/pci0000:00/usb1/1-1/1-1.2/authorized\n");
    }

    #[test]
    fn lcd_upload() {
        let (mut kb, cmd, lcd) = keyboard();
        let blob = vec![0u8; 4096 * 3];
        let mut seen = Vec::new();
        assert!(kb.upload_lcd(&blob, 1, &mut |p| seen.push(p), &|| false).unwrap());
        assert_eq!(lcd.writes.lock().unwrap().iter().map(|w| w.len()).collect::<Vec<_>>(), [4097; 3]);
        assert_eq!(seen.last(), Some(&1.0));
        let sent = cmd.commands();
        assert_eq!((op(&sent[1]), sent[1][2], sent[1][8]), ((4, 0x72), 1, 3));
    }

    #[test]
    fn dongle_checksum_and_clock() {
        let mut fake = FakeHid::new(33);
        fake.echo = true;
        let writes = fake.writes.clone();
        let mut d = Dongle::with_transport(Box::new(fake));
        assert!(d.sync_clock(Some(LocalTime::new(2026, 10, 28, 13, 35, 33)), 1).unwrap());
        let report = writes.lock().unwrap().last().unwrap().clone();
        assert_eq!(report.len(), 33);
        assert_eq!(report[32], report[..32].iter().fold(0u8, |s, b| s.wrapping_add(*b)));
        assert_eq!((&report[1..3], &report[5..7], &report[18..20]), (&[0x0C, 0x10][..], &[1, 0x5A][..], &[0xAA, 0x55][..]));
    }
}
