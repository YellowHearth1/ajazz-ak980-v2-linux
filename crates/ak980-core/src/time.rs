//! Local wall-clock time through libc, which knows the system's time zone.

use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    /// 0 = Sunday, as the keyboard counts.
    pub weekday: u8,
    /// Seconds east of UTC.
    pub utc_offset: i64,
}


unsafe extern "C" {
    /// POSIX; the libc crate does not export it for every target.
    fn tzset();
}

impl LocalTime {
    pub fn now() -> Self {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        Self::from_unix(secs)
    }

    pub fn from_unix(secs: i64) -> Self {
        let t: libc::time_t = secs as libc::time_t;
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        // SAFETY: tzset only rereads TZ; localtime_r only writes into the tm we pass
        unsafe {
            tzset();
            libc::localtime_r(&t, &mut tm);
        }
        Self {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u8,
            day: tm.tm_mday as u8,
            hour: tm.tm_hour as u8,
            minute: tm.tm_min as u8,
            second: tm.tm_sec as u8,
            weekday: tm.tm_wday as u8,
            utc_offset: tm.tm_gmtoff as i64,
        }
    }

    /// A fixed date and time, for tests and for the CLI's `--at`.
    pub fn new(year: i32, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Self {
        Self { year, month, day, hour, minute, second, weekday: weekday(year, month, day), utc_offset: 0 }
    }

    pub fn date(&self) -> (i32, u8, u8) {
        (self.year, self.month, self.day)
    }

    /// `2026-10-29 17:21:22`
    pub fn stamp(&self) -> String {
        format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", self.year, self.month, self.day, self.hour, self.minute,
                self.second)
    }

    /// `2026-10-29_17-21-22`, for file names
    pub fn file_stamp(&self) -> String {
        format!("{:04}-{:02}-{:02}_{:02}-{:02}-{:02}", self.year, self.month, self.day, self.hour, self.minute,
                self.second)
    }

    /// `17:21:22`
    pub fn clock(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }
}

/// Day of the week with Sunday = 0 (Sakamoto's method).
pub fn weekday(year: i32, month: u8, day: u8) -> u8 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    ((y + y / 4 - y / 100 + y / 400 + T[(month - 1) as usize] + day as i32).rem_euclid(7)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekdays() {
        assert_eq!(weekday(2026, 10, 28), 3); // a Wednesday
        assert_eq!(weekday(2026, 11, 1), 0); // a Sunday
        assert_eq!(weekday(2000, 2, 29), 2);
    }
}
