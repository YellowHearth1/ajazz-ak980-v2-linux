//! The keyboard's charge over time, from the agent's battery checks over 2.4 GHz, and what it shows
//! once whole cycles have been seen: how long a full charge lasts, how long charging takes, and how
//! that changes as the battery ages. The keyboard itself only tells its level, and only in
//! 2.4 GHz mode; on the cable in wired mode it charges without saying how far, so the level there
//! is read off the measured charging curve.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::i18n::plural;
use crate::tr;

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

// ------------------------------------------------------------ measured cycles

/// A whole discharge or charge seen in the record.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Measured {
    /// When it ended (Unix seconds); also tells one cycle from another.
    pub end: u64,
    pub from: u8,
    pub to: u8,
    /// Hours for the whole 100 %, at the rate of this cycle.
    pub hours_per_100: f32,
}

/// What the record has taught about the battery, kept apart from the record itself, which only
/// holds its last 2000 changes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Measurements {
    pub discharges: Vec<Measured>,
    pub charges: Vec<Measured>,
    /// (level, seconds since the start) of the last measured charge.
    pub charge_curve: Vec<(u8, u64)>,
    /// When the keyboard went onto the cable in wired mode, and its last level before.
    pub cable_since: Option<(u64, u8)>,
    /// The measurements were started over at this time (Unix seconds): readings from before it
    /// are not learnt from again.
    pub since: u64,
}

/// A rise or fall this small does not end a discharge or a charge: the level wobbles a little.
const WOBBLE: u8 = 2;
/// Cycles kept: years of charging.
const KEEP_CYCLES: usize = 200;

/// The stretches of `history` that only go down or only go up, as (first index, index of the
/// lowest or highest point, falling). A move back of up to `WOBBLE` points does not end one; a
/// slow but steady change the other way does, however small its single steps.
fn stretches(history: &[(u64, u8)]) -> Vec<(usize, usize, bool)> {
    let mut out = vec![];
    if history.is_empty() {
        return out;
    }
    let (mut start, mut extreme, mut falling): (usize, usize, Option<bool>) = (0, 0, None);
    // Until the first clear move, the lowest and highest points so far
    let (mut low, mut high) = (0, 0);
    for (i, &(_, level)) in history.iter().enumerate().skip(1) {
        let at = history[extreme].1;
        match falling {
            None => {
                if level < history[low].1 {
                    low = i;
                }
                if level > history[high].1 {
                    high = i;
                }
                if level > history[low].1 + WOBBLE {
                    if low > 0 {
                        out.push((0, low, true));
                    }
                    (start, extreme, falling) = (low, i, Some(false));
                } else if level + WOBBLE < history[high].1 {
                    if high > 0 {
                        out.push((0, high, false));
                    }
                    (start, extreme, falling) = (high, i, Some(true));
                }
            }
            Some(true) if level <= at => extreme = i,
            Some(true) if level > at + WOBBLE => {
                out.push((start, extreme, true));
                (start, extreme, falling) = (extreme, i, Some(false));
            }
            Some(false) if level >= at => extreme = i,
            Some(false) if level + WOBBLE < at => {
                out.push((start, extreme, false));
                (start, extreme, falling) = (extreme, i, Some(true));
            }
            Some(_) => {}
        }
    }
    if let Some(f) = falling {
        out.push((start, extreme, f));
    }
    out
}

/// The last full discharge in `history`: from 90 % or more to 20 % or less, without a charge in
/// between. It starts when the level first leaves its top, as the time spent at the top may have
/// been on the charger.
pub fn last_discharge(history: &[(u64, u8)]) -> Option<Measured> {
    let &(start, end, _) = stretches(history).iter().rev().find(|(_, end, falling)| *falling && history[*end].1 <= 20)?;
    let first = start + history[start..=end].iter().position(|(_, l)| *l < history[start].1)?;
    let ((t0, from), (t1, to)) = (history[first], history[end]);
    (from >= 90 && t1 > t0).then(|| Measured {
        end: t1,
        from,
        to,
        hours_per_100: (t1 - t0) as f32 / 3600.0 / (from - to) as f32 * 100.0,
    })
}

/// The last full charge in `history`, from 20 % or less to 95 % or more, and its curve. It starts
/// at the first rise, as before that the keyboard may have lain unplugged.
pub fn last_charge(history: &[(u64, u8)]) -> Option<(Measured, Vec<(u8, u64)>)> {
    let &(start, top, _) = stretches(history).iter().rev().find(|(_, top, falling)| !*falling && history[*top].1 >= 95)?;
    if history[start].1 > 20 {
        return None;
    }
    let rise = start + history[start..=top].iter().position(|(_, l)| *l > history[start].1)?;
    let end = rise + history[rise..=top].iter().position(|(_, l)| *l == history[top].1)?;
    let ((t0, from), (t1, to)) = (history[rise], history[end]);
    if t1 <= t0 || to <= from {
        return None;
    }
    let curve = history[rise..=end].iter().map(|(t, l)| (*l, t - t0)).collect();
    let m = Measured { end: t1, from, to, hours_per_100: (t1 - t0) as f32 / 3600.0 / (to - from) as f32 * 100.0 };
    Some((m, curve))
}

/// Unix seconds now, as the record counts time.
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}

impl Measurements {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        crate::service::write_atomic(path, &serde_json::to_string_pretty(self).map_err(io::Error::other)?)
    }

    /// Measurements started over `now`: the record stays, but nothing from before is learnt again.
    pub fn started_over(now: u64) -> Self {
        Self { since: now, ..Self::default() }
    }

    /// Takes in the cycles of `history` not seen before; whether anything was new.
    pub fn learn(&mut self, history: &[(u64, u8)]) -> bool {
        let start = history.partition_point(|(t, _)| *t < self.since);
        let history = &history[start..];
        let mut changed = false;
        if let Some(d) = last_discharge(history)
            && !self.discharges.iter().any(|m| m.end == d.end)
        {
            self.discharges.push(d);
            changed = true;
        }
        if let Some((c, curve)) = last_charge(history)
            && !self.charges.iter().any(|m| m.end == c.end)
        {
            self.charges.push(c);
            self.charge_curve = curve;
            changed = true;
        }
        for list in [&mut self.discharges, &mut self.charges] {
            if list.len() > KEEP_CYCLES {
                list.drain(..list.len() - KEEP_CYCLES);
            }
        }
        changed
    }

    /// How long a whole charge lasted in the last measured discharge.
    pub fn full_charge_hours(&self) -> Option<f32> {
        self.discharges.last().map(|m| m.hours_per_100)
    }

    /// How long charging from empty to full took in the last measured charge.
    pub fn charging_hours(&self) -> Option<f32> {
        self.charges.last().map(|m| m.hours_per_100)
    }

    /// The last measured discharge against the first one (at most 1), and the number of cycles.
    /// Usage changes how long a charge lasts too, so it is a rough guide.
    pub fn condition(&self) -> Option<(f32, usize)> {
        let (first, last) = (self.discharges.first()?, self.discharges.last()?);
        (self.discharges.len() >= 2 && first.hours_per_100 > 0.0)
            .then(|| ((last.hours_per_100 / first.hours_per_100).min(1.0), self.discharges.len()))
    }

    /// Seconds of the measured charge to go from `level` to its top, and the level reached after
    /// `elapsed` seconds more on the cable from `level`.
    fn along_curve(&self, level: u8, elapsed: u64) -> Option<(u64, u8)> {
        let curve = &self.charge_curve;
        let (first_level, _) = *curve.first()?;
        let (_, total) = *curve.last()?;
        // Below the curve's start, the first stretch's rate covers the difference
        let rate = curve.get(1).map_or(1.0, |(l, t)| (*l - first_level) as f32 / (*t).max(1) as f32);
        let at = match curve.iter().find(|(l, _)| *l >= level) {
            Some((_, t)) if level >= first_level => *t as f32,
            Some(_) => -((first_level - level) as f32) / rate.max(1e-6),
            None => total as f32,
        };
        let now = at + elapsed as f32;
        let reached = if now <= 0.0 {
            first_level as f32 + now * rate
        } else {
            let after = curve.iter().position(|(_, t)| *t as f32 >= now);
            match after {
                None => curve.last()?.0 as f32,
                Some(0) => first_level as f32,
                Some(i) => {
                    let ((l0, t0), (l1, t1)) = (curve[i - 1], curve[i]);
                    l0 as f32 + (l1 - l0) as f32 * (now - t0 as f32) / (t1 - t0).max(1) as f32
                }
            }
        };
        Some(((total as f32 - now).max(0.0) as u64, reached.clamp(0.0, 100.0).round() as u8))
    }

    /// What to say about the charge now. `on_cable`: the keyboard is in wired mode.
    pub fn estimate(&self, history: &[(u64, u8)], on_cable: bool, now: u64) -> Option<Estimate> {
        if on_cable {
            let (since, level) = self.cable_since?;
            let (left, reached) = self.along_curve(level, now.saturating_sub(since))?;
            return Some(if left == 0 { Estimate::Full } else {
                Estimate::Charging { hours: left as f32 / 3600.0, level: Some(reached) }
            });
        }
        let &(t, level) = history.last()?;
        // Rising lately: it charges while staying in 2.4 GHz mode
        let rising = history.len() >= 2 && history[history.len() - 2].1 < level && now.saturating_sub(t) < 30 * 60;
        if rising {
            if level >= 100 {
                return Some(Estimate::Full);
            }
            let hours = match self.along_curve(level, 0) {
                Some((left, _)) => left as f32 / 3600.0,
                None => self.charging_hours()? * (100 - level) as f32 / 100.0,
            };
            return Some(Estimate::Charging { hours, level: None });
        }
        hours_left(history, now)
            .or_else(|| self.full_charge_hours().map(|h| h * level as f32 / 100.0 - now.saturating_sub(t) as f32 / 3600.0))
            .filter(|h| *h > 0.0)
            .map(Estimate::Lasts)
    }
}

/// The charge, as `Measurements::estimate` sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Estimate {
    /// Hours until empty.
    Lasts(f32),
    /// Charging: hours until full, and on the cable the level read off the charging curve.
    Charging { hours: f32, level: Option<u8> },
    Full,
}

/// "35 min", "2 h 30 min", "31 h", "4 days": a rough length of time, for the phrases below.
pub fn duration(hours: f32) -> String {
    let minutes = (hours * 60.0).round().max(1.0);
    if minutes < 60.0 {
        tr!("{minutes} min", minutes = ((minutes / 5.0).round() * 5.0).max(5.0))
    } else if hours < 10.0 {
        let whole = hours.floor();
        let rest = ((hours - whole) * 6.0).round() * 10.0;
        if rest < 10.0 || rest >= 60.0 {
            tr!("{hours} h", hours = hours.round())
        } else {
            tr!("{hours} h {minutes} min", hours = whole, minutes = rest)
        }
    } else if hours < 48.0 {
        tr!("{hours} h", hours = hours.round())
    } else {
        plural((hours / 24.0).round() as i64, "{n} day", "{n} days")
    }
}

/// The estimate in words: "lasts about 31 h more", "charging, full in about 40 min"…
pub fn describe(e: &Estimate) -> String {
    match *e {
        Estimate::Lasts(h) => tr!("lasts about {time} more", time = duration(h)),
        Estimate::Charging { hours, level: None } => tr!("charging, full in about {time}", time = duration(hours)),
        Estimate::Charging { hours, level: Some(l) } => tr!(
            "charging, about {level} by the measured charging, full in about {time}",
            level = format!("{l}%"),
            time = duration(hours)
        ),
        Estimate::Full => crate::i18n::tr("charged"),
    }
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

    /// A discharge from 100 % at 1 % an hour, then a charge at 50 % an hour, readings every 1 %.
    fn cycle(h: u64) -> Vec<(u64, u8)> {
        let mut v: Vec<(u64, u8)> = (0..=85).map(|i| (i * h, 100 - i as u8)).collect(); // 100 → 15
        let t = v.last().unwrap().0 + 5 * h; // lies at 15 % for five hours, then is plugged in
        v.extend((1..=85).map(|i| (t + i * h / 50, 15 + i as u8))); // 16 → 100
        v
    }

    #[test]
    fn stretches_ignore_wobbles_but_not_slow_turns() {
        let at = |levels: &[u8]| levels.iter().enumerate().map(|(i, l)| (i as u64, *l)).collect::<Vec<_>>();
        assert_eq!(stretches(&at(&[90, 89, 90, 88, 87])), vec![(0, 4, true)], "a 1-point bounce");
        assert_eq!(stretches(&at(&[20, 19, 18, 19, 20, 21, 22])), vec![(0, 2, true), (2, 6, false)],
                   "1-point steps up add up to a charge");
        assert_eq!(stretches(&at(&[50])), vec![]);
    }

    #[test]
    fn whole_cycles_are_measured() {
        let h = 3600;
        let history = cycle(h);
        let d = last_discharge(&history).unwrap();
        assert_eq!((d.from, d.to), (99, 15), "from the first step off the top");
        assert!((d.hours_per_100 - 100.0).abs() < 0.5, "{}", d.hours_per_100);
        let (c, curve) = last_charge(&history).unwrap();
        assert_eq!((c.from, c.to), (16, 100), "from the first rise, not from when it lay unplugged");
        assert!((c.hours_per_100 - 2.0).abs() < 0.05, "{}", c.hours_per_100);
        assert_eq!(curve.first(), Some(&(16, 0)));
        assert_eq!(last_discharge(&history[..60]), None, "not down to 20 % yet");
        assert_eq!(last_charge(&history[..120]), None, "not up to 95 % yet");

        let mut m = Measurements::default();
        assert!(m.learn(&history));
        assert!(!m.learn(&history), "the same cycles are not counted twice");
        assert_eq!(m.condition(), None, "one cycle compares with nothing");
        let mut aged = history.clone();
        let t = aged.last().unwrap().0;
        aged.extend((1..=85).map(|i| (t + i * h * 9 / 10, 100 - i as u8))); // 10 % shorter
        assert!(m.learn(&aged));
        let (share, cycles) = m.condition().unwrap();
        assert!((share - 0.9).abs() < 0.01 && cycles == 2, "{share}");

        // Started over: the cycles already in the record are not learnt again
        let mut fresh = Measurements::started_over(aged.last().unwrap().0 + 1);
        assert!(!fresh.learn(&aged));
        assert!(fresh.discharges.is_empty() && fresh.charges.is_empty());
    }

    #[test]
    fn estimates_on_battery_while_charging_and_on_the_cable() {
        let h = 3600;
        let history = cycle(h);
        let mut m = Measurements::default();
        m.learn(&history);
        let t_end = history.last().unwrap().0;
        assert_eq!(m.estimate(&history, false, t_end), Some(Estimate::Full));
        // Charging in 2.4 GHz mode at 50 %: about an hour of the measured charge is left
        let at50 = history.iter().position(|(t, l)| *l == 50 && *t > history[86].0).unwrap();
        let Some(Estimate::Charging { hours, level: None }) = m.estimate(&history[..=at50], false, history[at50].0) else {
            panic!("charging")
        };
        assert!((hours - 1.0).abs() < 0.05, "{hours}");
        // On the cable in wired mode from 20 %: after 36 minutes about 50 %, then about 1 h to go
        m.cable_since = Some((1000, 20));
        let Some(Estimate::Charging { hours, level: Some(l) }) = m.estimate(&history, true, 1000 + 36 * 60) else {
            panic!("on the cable")
        };
        assert!((48..=52).contains(&l) && (hours - 1.0).abs() < 0.1, "{l} {hours}");
        assert_eq!(m.estimate(&history, true, 1000 + 10 * h), Some(Estimate::Full));
        // Unplugged at 80 % with no readings since: the measured discharge rate
        let unplugged = vec![(0, 70), (h, 80)];
        let mut m2 = m.clone();
        m2.cable_since = None;
        let Some(Estimate::Lasts(left)) = m2.estimate(&unplugged, false, 2 * h) else { panic!("on battery") };
        assert!((left - 79.0).abs() < 1.0, "{left}");
    }

    #[test]
    fn durations_read_naturally() {
        crate::i18n::set_language("en");
        assert_eq!(duration(0.6), "35 min");
        assert_eq!(duration(2.5), "2 h 30 min");
        assert_eq!(duration(31.2), "31 h");
        assert_eq!(duration(100.0), "4 days");
        assert_eq!(describe(&Estimate::Lasts(31.0)), "lasts about 31 h more");
        assert_eq!(describe(&Estimate::Charging { hours: 0.7, level: None }), "charging, full in about 40 min");
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
