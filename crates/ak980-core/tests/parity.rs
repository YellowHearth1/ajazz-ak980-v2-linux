//! Replays `golden.json` (written by `tools/golden.py` from the Python version, which was verified on
//! an AK980 V2) through the Rust code. Every report sent to the keyboard must match byte for byte;
//! the config written by Python must load and save back unchanged.

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ak980_core::config::{Config, Profile};
use ak980_core::device::{Dongle, WiredKeyboard};
use ak980_core::effects::{self, Inputs};
use ak980_core::hid::{Hid, HidInfo};
use ak980_core::lcd;
use ak980_core::layout;
use ak980_core::lighting::{KeyboardOptions, LightSettings};
use ak980_core::time::LocalTime;
use image::{Rgb, RgbImage};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

type Reports = Arc<Mutex<Vec<Vec<u8>>>>;

/// Answers like the keyboard: the last feature report comes back with byte 4 set (the same as the
/// fake in tools/golden.py and tests/conftest.py); input reads time out.
struct FakeHid {
    info: HidInfo,
    features: Reports,
    writes: Reports,
}

impl FakeHid {
    fn new(output_len: usize, features: &Reports, writes: &Reports) -> Box<Self> {
        let info = HidInfo { path: "/dev/fake".into(), output_len, input_len: output_len, ..HidInfo::default() };
        Box::new(Self { info, features: features.clone(), writes: writes.clone() })
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
    fn get_feature(&mut self, _report_id: u8, _len: usize) -> io::Result<Vec<u8>> {
        let mut reply = self.features.lock().unwrap().last().cloned().unwrap_or_else(|| vec![0; 65]);
        reply.resize(65, 0);
        reply[4] = 1;
        Ok(reply)
    }
    fn write(&mut self, report: &[u8], _timeout: Duration) -> io::Result<()> {
        self.writes.lock().unwrap().push(report.to_vec());
        Ok(())
    }
    fn read(&mut self, _timeout: Duration) -> io::Result<Option<Vec<u8>>> {
        Ok(None)
    }
    fn drain(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn lock(&mut self) -> io::Result<()> {
        Ok(())
    }
    fn unlock(&mut self) {}
}

fn golden() -> Value {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn sha(b: &[u8]) -> String {
    hex(&Sha256::digest(b))
}

fn time(v: &Value) -> LocalTime {
    let t: Vec<i64> = v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect();
    LocalTime::new(t[0] as i32, t[1] as u8, t[2] as u8, t[3] as u8, t[4] as u8, t[5] as u8)
}

/// The Python config.json, loaded and parsed by the Rust code.
fn rust_config(g: &Value) -> Config {
    // Tests run in parallel: every call gets its own folder
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ak980-parity-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.json");
    std::fs::write(&path, serde_json::to_string(&g["config"]).unwrap()).unwrap();
    let cfg = Config::load_from(&path);
    let _ = std::fs::remove_dir_all(&dir);
    cfg
}

/// Runs one recorded operation through the Rust protocol code; returns (feature reports, writes).
fn replay(op: &Value, profile: &Profile) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let features: Reports = Arc::default();
    let writes: Reports = Arc::default();
    let name = op["op"].as_str().unwrap();
    if name.starts_with("dongle_") {
        let mut d = Dongle::with_transport(FakeHid::new(33, &features, &writes));
        match name {
            "dongle_sync_clock" => {
                d.sync_clock(Some(time(&op["time"])), op["slot"].as_u64().unwrap() as u8).unwrap();
            }
            "dongle_battery" => {
                d.request(&[0x20, 0x01]).unwrap();
            }
            other => panic!("unknown operation {other}"),
        }
    } else {
        let lcd_reports: Reports = Arc::default();
        let mut kb = WiredKeyboard::with_transport(
            FakeHid::new(65, &features, &writes),
            Some(FakeHid::new(4097, &lcd_reports, &lcd_reports)),
        );
        kb.delay = Duration::ZERO;
        match name {
            "set_lighting" => {
                let s: LightSettings = serde_json::from_value(op["settings"].clone()).unwrap();
                kb.set_lighting(&s).unwrap();
            }
            "set_custom_colors" => kb.set_custom_colors(&profile.custom_colors, profile.custom_brightness).unwrap(),
            "set_options" => {
                let o: KeyboardOptions = serde_json::from_value(op["options"].clone()).unwrap();
                kb.set_options(&o).unwrap();
            }
            "set_keymap" => {
                let fn_layer = op["fn_layer"].as_bool().unwrap();
                kb.set_keymap(fn_layer, profile.layer(fn_layer)).unwrap();
            }
            "set_macros" => kb.set_macros(&profile.macros).unwrap(),
            "set_side_light" => kb.set_side_light(op["mode"].as_u64().unwrap() as u8).unwrap(),
            "sync_clock" => kb.sync_clock(Some(time(&op["time"])), op["slot"].as_u64().unwrap() as u8).unwrap(),
            "send_realtime" => {
                let table: [u8; 512] = unhex(op["table"].as_str().unwrap()).try_into().unwrap();
                kb.send_realtime(&table, Duration::ZERO).unwrap();
            }
            other => panic!("unknown operation {other}"),
        }
    }
    let f = features.lock().unwrap().clone();
    let w = writes.lock().unwrap().clone();
    (f, w)
}

fn compare(what: &str, expected: &Value, got: &[Vec<u8>]) -> Option<String> {
    let expected: Vec<Vec<u8>> = expected.as_array().unwrap().iter().map(|v| unhex(v.as_str().unwrap())).collect();
    if expected.len() != got.len() {
        return Some(format!("{what}: {} reports expected, {} sent", expected.len(), got.len()));
    }
    for (i, (e, g)) in expected.iter().zip(got).enumerate() {
        if e != g {
            let at = e.iter().zip(g.iter()).position(|(a, b)| a != b).unwrap_or(e.len().min(g.len()));
            return Some(format!(
                "{what} #{i}: differs at byte {at} (len {} vs {})\n  python {}\n  rust   {}",
                e.len(), g.len(), hex(&e[..e.len().min(48)]), hex(&g[..g.len().min(48)])
            ));
        }
    }
    None
}

#[test]
fn keyboard_reports_match_python() {
    let g = golden();
    let cfg = rust_config(&g);
    let profile = cfg.profile();
    let mut failures = Vec::new();
    for (i, op) in g["ops"].as_array().unwrap().iter().enumerate() {
        let (features, writes) = replay(op, profile);
        let label = format!("op {i} {}", op["op"]);
        failures.extend(compare(&format!("{label} features"), &op["features"], &features));
        failures.extend(compare(&format!("{label} writes"), &op["writes"], &writes));
    }
    assert!(failures.is_empty(), "{} mismatches:\n{}", failures.len(), failures.join("\n"));
}

/// Paths where two JSON values differ (objects compare by key, not order).
fn json_diff(path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys().filter(|k| !x.contains_key(*k))) {
                json_diff(&format!("{path}.{k}"), x.get(k).unwrap_or(&Value::Null), y.get(k).unwrap_or(&Value::Null), out);
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (i, (p, q)) in x.iter().zip(y).enumerate() {
                json_diff(&format!("{path}[{i}]"), p, q, out);
            }
        }
        (Value::Number(x), Value::Number(y)) if x.as_f64() == y.as_f64() => {}
        _ if a == b => {}
        _ => out.push(format!("{path}: python {a} / rust {b}")),
    }
}

#[test]
fn config_written_by_python_round_trips() {
    let g = golden();
    let cfg = rust_config(&g);
    let saved: Value = serde_json::from_str(&cfg.to_json()).unwrap();
    let mut diffs = Vec::new();
    json_diff("config", &g["config"], &saved, &mut diffs);
    // Settings that 0.0.2 adds; Python 0.0.1 keeps only the fields it knows, so it still loads these files
    const NEW_IN_0_0_2: [&str; 8] = ["config.settings.notifications: python null", "config.settings.theme: python null",
                                     "config.settings.accent: python null", "config.settings.colorway: python null",
                                     "config.settings.overlay_locks: python null",
                                     "config.settings.overlay_lock_color: python null",
                                     "config.settings.overlay_notifications: python null",
                                     "config.settings.overlay_notify_color: python null"];
    diffs.retain(|d| !NEW_IN_0_0_2.iter().any(|new| d.starts_with(new)));
    assert!(diffs.is_empty(), "{} differences:\n{}", diffs.len(), diffs.join("\n"));
}

fn gradient(flip: bool) -> RgbImage {
    let img = RgbImage::from_fn(240, 135, |x, y| Rgb([(x * 255 / 239) as u8, (y * 255 / 134) as u8, ((x + y) % 256) as u8]));
    if flip { image::imageops::flip_horizontal(&img) } else { img }
}

#[test]
fn screen_encoding_matches_python() {
    let g = golden();
    let s = &g["screen"];
    assert_eq!(sha(&lcd::to_rgb565(&gradient(false), true)), s["rgb565_dither"], "RGB565 with dithering");
    assert_eq!(sha(&lcd::to_rgb565(&gradient(false), false)), s["rgb565_plain"], "RGB565 without dithering");
    let red = RgbImage::from_pixel(240, 135, Rgb([255, 0, 0]));
    let frames = vec![(gradient(false), 100), (red, 50), (gradient(true), 510)];
    let blob = lcd::encode_animation(&frames, true, &mut |_| {}).unwrap();
    assert_eq!(blob.len() as u64, s["animation_len"].as_u64().unwrap(), "animation length");
    assert_eq!(sha(&blob), s["animation"], "animation bytes");
    let pages: Vec<u64> = [1, 2, 24, 140].iter().map(|&n| lcd::page_count(n) as u64).collect();
    let expected: Vec<u64> = s["page_count"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).collect();
    assert_eq!(pages, expected, "page counts");
}

#[test]
fn deterministic_effects_match_python() {
    let g = golden();
    let (leds, pos) = layout::positions();
    let bands: Vec<f32> = (0..16).map(|i| i as f32 / 15.0).collect();
    let mut failures = Vec::new();
    for case in g["effects"].as_array().unwrap() {
        let id = case["effect"].as_str().unwrap();
        let mut e = effects::create(id, pos, &Map::new());
        for (i, (t, expected)) in [0.0f32, 0.5, 1.25].iter().zip(case["frames"].as_array().unwrap()).enumerate() {
            let inp = Inputs {
                presses: if i == 0 { vec![3, 10] } else { vec![] },
                bands: bands.clone(),
                level: 0.6,
                beat: i == 1,
                system: [0.0; 3],
            };
            let got = effects::to_bytes(&e.render(*t, 0.5, &inp), leds, 0.8);
            let want = unhex(expected.as_str().unwrap());
            let worst = got.iter().zip(&want).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap_or(0);
            if worst > 1 {
                let at = got.iter().zip(&want).position(|(a, b)| (*a as i32 - *b as i32).abs() > 1).unwrap();
                failures.push(format!("{id} frame {i}: off by {worst} (first at byte {at}: python {} rust {})",
                                      want[at], got[at]));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn audio_analysis_matches_python() {
    let g = golden();
    let tone = |f: f64, amp: f64| -> Vec<f32> {
        (0..1024).map(|n| (amp * (2.0 * std::f64::consts::PI * f * n as f64 / 48000.0).sin()) as f32).collect()
    };
    let two: Vec<f32> = tone(100.0, 0.3).iter().zip(tone(5000.0, 0.2)).map(|(a, b)| a + b).collect();
    let signals = [tone(1000.0, 0.5), two, vec![0.0; 1024], tone(440.0, 0.9)];
    let mut a = ak980_core::audio::Analyser::new(16);
    let mut failures = Vec::new();
    for (i, (samples, want)) in signals.iter().zip(g["audio"].as_array().unwrap()).enumerate() {
        a.analyse(samples);
        let bands: Vec<f64> = want["bands"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let worst = a.bands.iter().zip(&bands).map(|(x, y)| (*x as f64 - y).abs()).fold(0.0, f64::max);
        if worst > 2e-3 {
            failures.push(format!("window {i}: bands off by {worst:.5}\n  python {bands:?}\n  rust   {:?}", a.bands));
        }
        if (a.level as f64 - want["level"].as_f64().unwrap()).abs() > 2e-3 || a.beat != want["beat"].as_bool().unwrap() {
            failures.push(format!("window {i}: level/beat {} {} vs python {} {}", a.level, a.beat, want["level"], want["beat"]));
        }
        a.beat = false;
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
