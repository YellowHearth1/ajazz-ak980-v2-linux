//! The keyboard's charge over time, from the agent's battery checks over 2.4 GHz, and how long it
//! will last at the rate it has been going down since the last charge. The keyboard itself only
//! tells its level, and only in 2.4 GHz mode.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Lines kept in the record: weeks of readings, as a line is only written when the level changes.
const KEEP: usize = 2000;

/// `UNIX_SECONDS LEVEL` per line.
pub fn read(log: &Path) -> Vec<(u64, u8)> {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .collect()
}

/// Adds a reading when the level differs from the last one written.
pub fn record(log: &Path, now: u64, level: u8) -> io::Result<()> {
    let history = read(log);
    if history.last().is_some_and(|(_, l)| *l == level) {
        return Ok(());
    }
    if let Some(dir) = log.parent() {
        fs::create_dir_all(dir)?;
    }
    if history.len() >= KEEP {
        let kept: String = history[history.len() + 1 - KEEP..].iter().map(|(t, l)| format!("{t} {l}\n")).collect();
        return crate::service::write_atomic(log, &(kept + &format!("{now} {level}\n")));
    }
    fs::OpenOptions::new().create(true).append(true).open(log)?.write_all(format!("{now} {level}\n").as_bytes())
}

/// Hours until the battery is empty, from a straight line through the readings since it was last
/// charged (the level went up). A guess needs at least a 3 % drop over at least an hour, so a single
/// step of the keyboard's coarse level does not make one.
pub fn hours_left(history: &[(u64, u8)], now: u64) -> Option<f32> {
    let start = history.windows(2).rposition(|w| w[1].1 > w[0].1).map_or(0, |i| i + 1);
    let since = &history[start..];
    let (first, last) = (since.first()?, since.last()?);
    if last.0.saturating_sub(first.0) < 3600 || first.1.saturating_sub(last.1) < 3 {
        return None;
    }
    // Least squares in hours and percent
    let n = since.len() as f64;
    let t0 = first.0 as f64;
    let (sx, sy) = since.iter().fold((0.0, 0.0), |(sx, sy), (t, l)| (sx + (*t as f64 - t0) / 3600.0, sy + *l as f64));
    let (mx, my) = (sx / n, sy / n);
    let (sxy, sxx) = since.iter().fold((0.0, 0.0), |(sxy, sxx), (t, l)| {
        let x = (*t as f64 - t0) / 3600.0 - mx;
        (sxy + x * (*l as f64 - my), sxx + x * x)
    });
    let slope = sxy / sxx; // percent per hour
    if !slope.is_finite() || slope >= 0.0 {
        return None;
    }
    let elapsed = now.saturating_sub(last.0) as f64 / 3600.0;
    let hours = last.1 as f64 / -slope - elapsed;
    (hours > 0.0).then_some(hours as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_keeps_changes_only() {
        let dir = std::env::temp_dir().join(format!("ak980-battery-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let log = dir.join("battery.log");
        record(&log, 100, 90).unwrap();
        record(&log, 160, 90).unwrap();
        record(&log, 220, 89).unwrap();
        assert_eq!(read(&log), vec![(100, 90), (220, 89)]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn estimate_from_the_last_discharge() {
        let h = 3600;
        // charged to 100 % at hour 10, then 1 % an hour
        let history = vec![(0, 40), (5 * h, 35), (10 * h, 100), (14 * h, 96), (18 * h, 92), (20 * h, 90)];
        let left = hours_left(&history, 20 * h).unwrap();
        assert!((left - 90.0).abs() < 0.5, "{left}");
        assert!((hours_left(&history, 30 * h).unwrap() - 80.0).abs() < 0.5, "time since the last reading counts");
        assert!((hours_left(&history[..4], 14 * h).unwrap() - 96.0).abs() < 0.5, "two readings after the charge do");
        assert_eq!(hours_left(&history[..3], 10 * h), None, "nothing since the charge");
        assert_eq!(hours_left(&[(0, 100), (600, 97)], 600), None, "less than an hour");
        assert_eq!(hours_left(&[(0, 100), (2 * h, 99)], 2 * h), None, "less than 3 %");
        assert_eq!(hours_left(&[], 0), None);
    }
}
