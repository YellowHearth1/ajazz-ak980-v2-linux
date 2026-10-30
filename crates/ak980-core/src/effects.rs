//! Lighting effects computed on the PC and streamed to the keyboard in real time.
//!
//! Every effect works on the keys' positions from layout.json (normalised to 0..1, y grows towards
//! the space bar) and returns one RGB colour per key as floats in 0..1. The same code drives the
//! keyboard and the preview in the app, so the preview is exact.

use std::collections::HashMap;
use std::f32::consts::TAU;

use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamKind {
    Color([u8; 3]),
    Range { default: i32, lo: i32, hi: i32 },
    Choice { default: &'static str, choices: &'static [(&'static str, &'static str)] },
    Toggle(bool),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Param {
    pub id: &'static str,
    /// English; translate with `tr`.
    pub label: &'static str,
    pub kind: ParamKind,
    /// Shown only while another setting has this value (`id`, value; "true" for a toggle).
    pub when: Option<(&'static str, &'static str)>,
}

impl Param {
    const fn when(self, id: &'static str, value: &'static str) -> Self {
        Self { when: Some((id, value)), ..self }
    }

    /// Whether this setting applies with `values`; the window hides the others.
    pub fn shown(&self, values: &Map<String, Value>, info: &EffectInfo) -> bool {
        let Some((id, wanted)) = self.when else { return true };
        let Some(other) = info.params.iter().find(|p| p.id == id) else { return true };
        match value(other, values) {
            ParamValue::Text(t) => t == wanted,
            ParamValue::Bool(b) => b.to_string() == wanted,
            _ => true,
        }
    }
}

const fn color(id: &'static str, label: &'static str, c: [u8; 3]) -> Param {
    Param { id, label, kind: ParamKind::Color(c), when: None }
}
const fn range(id: &'static str, label: &'static str, default: i32, lo: i32, hi: i32) -> Param {
    Param { id, label, kind: ParamKind::Range { default, lo, hi }, when: None }
}
const fn choice(id: &'static str, label: &'static str, default: &'static str,
                choices: &'static [(&'static str, &'static str)]) -> Param {
    Param { id, label, kind: ParamKind::Choice { default, choices }, when: None }
}
const fn toggle(id: &'static str, label: &'static str, default: bool) -> Param {
    Param { id, label, kind: ParamKind::Toggle(default), when: None }
}

pub const DIRECTION_CHOICES: &[(&str, &str)] = &[
    ("right", "To the right"), ("left", "To the left"), ("down", "Downwards"), ("up", "Upwards"),
    ("diagonal", "Diagonal"), ("circle", "From the centre"),
];
pub const PALETTE_CHOICES: &[(&str, &str)] =
    &[("rainbow", "Rainbow"), ("fire", "Fire"), ("ice", "Ice"), ("amber", "Amber"), ("toxic", "Toxic")];
const FIRE_PALETTES: &[(&str, &str)] = &[("fire", "Fire"), ("ice", "Ice"), ("amber", "Amber"), ("toxic", "Toxic")];
const STYLE_CHOICES: &[(&str, &str)] = &[("key", "Pressed key"), ("ripple", "Ripple"), ("cross", "Cross")];
const BAR_CHOICES: &[(&str, &str)] = &[("bars", "Bars"), ("glow", "Glow")];
const SOURCE_CHOICES: &[(&str, &str)] =
    &[("cpu", "Processor load"), ("memory", "Memory in use"), ("temperature", "Processor temperature")];
const ROW_CHOICES: &[(&str, &str)] = &[("numbers", "Number row"), ("function", "F1–F12"), ("all", "Whole keyboard")];

pub struct EffectInfo {
    pub id: &'static str,
    /// English; translate with `tr`.
    pub name: &'static str,
    pub description: &'static str,
    pub params: &'static [Param],
    pub needs_keys: bool,
    pub needs_audio: bool,
    /// Reads the processor's load, the memory in use and the processor's temperature.
    pub needs_system: bool,
}

pub const EFFECTS: [EffectInfo; 10] = [
    EffectInfo {
        id: "wave", name: "Rainbow wave", description: "A rainbow that flows across the keys.",
        params: &[
            choice("direction", "Direction", "right", DIRECTION_CHOICES),
            range("centre_x", "Centre, from the left", 50, 0, 100).when("direction", "circle"),
            range("centre_y", "Centre, from the top", 50, 0, 100).when("direction", "circle"),
            range("speed", "Speed", 40, 0, 100),
            range("size", "Wave length", 100, 20, 300), range("saturation", "Saturation", 100, 0, 100),
        ],
        needs_keys: false, needs_audio: false, needs_system: false,
    },
    EffectInfo {
        id: "drift", name: "Colour drift", description: "Two colours slowly flowing into each other.",
        params: &[
            color("a", "First colour", [255, 143, 31]), color("b", "Second colour", [40, 90, 255]),
            choice("direction", "Direction", "diagonal", DIRECTION_CHOICES),
            range("centre_x", "Centre, from the left", 50, 0, 100).when("direction", "circle"),
            range("centre_y", "Centre, from the top", 50, 0, 100).when("direction", "circle"),
            range("speed", "Speed", 25, 0, 100),
            range("size", "Wave length", 120, 20, 300),
        ],
        needs_keys: false, needs_audio: false, needs_system: false,
    },
    EffectInfo {
        id: "breath", name: "Breathing", description: "The whole keyboard fades in and out.",
        params: &[
            color("a", "Colour", [255, 143, 31]), toggle("alternate", "Alternate with a second colour", false),
            color("b", "Second colour", [40, 90, 255]), range("speed", "Speed", 30, 0, 100),
            range("floor", "Lowest brightness", 5, 0, 60),
        ],
        needs_keys: false, needs_audio: false, needs_system: false,
    },
    EffectInfo {
        id: "starlight", name: "Starlight", description: "Keys light up at random and slowly fade out.",
        params: &[
            color("background", "Background", [6, 8, 24]), toggle("random_hue", "Random colours", true),
            color("star", "Star colour", [255, 220, 150]), range("density", "Density", 35, 0, 100),
            range("fade", "Fade time", 45, 0, 100),
        ],
        needs_keys: false, needs_audio: false, needs_system: false,
    },
    EffectInfo {
        id: "reactive", name: "Reactive",
        description: "Keys respond to typing: the pressed key, a ripple or a cross lights up.",
        params: &[
            choice("style", "Style", "ripple", STYLE_CHOICES), color("idle", "Idle colour", [12, 14, 22]),
            toggle("rainbow", "Rainbow", true), color("press", "Press colour", [255, 143, 31]),
            range("fade", "Fade time", 40, 0, 100), range("speed", "Ripple speed", 50, 0, 100),
        ],
        needs_keys: true, needs_audio: false, needs_system: false,
    },
    EffectInfo {
        id: "fire", name: "Fire", description: "Flames rising from the space bar.",
        params: &[
            choice("palette", "Colours", "fire", FIRE_PALETTES), range("height", "Flame height", 55, 0, 100),
            range("speed", "Speed", 45, 0, 100),
        ],
        needs_keys: false, needs_audio: false, needs_system: false,
    },
    EffectInfo {
        id: "spectrum", name: "Music: spectrum",
        description: "An audio spectrum analyser across the keyboard, from what is playing on the PC.",
        params: &[
            choice("palette", "Colours", "rainbow", PALETTE_CHOICES), choice("style", "Style", "bars", BAR_CHOICES),
            range("sensitivity", "Sensitivity", 55, 0, 100),
        ],
        needs_keys: false, needs_audio: true, needs_system: false,
    },
    EffectInfo {
        id: "pulse", name: "Music: pulse", description: "The keyboard breathes with the loudness of the music.",
        params: &[
            color("a", "Quiet colour", [40, 10, 120]), color("b", "Loud colour", [255, 143, 31]),
            range("sensitivity", "Sensitivity", 55, 0, 100), toggle("from_centre", "Spread from the centre", true),
            range("centre_x", "Centre, from the left", 50, 0, 100).when("from_centre", "true"),
            range("centre_y", "Centre, from the top", 55, 0, 100).when("from_centre", "true"),
        ],
        needs_keys: false, needs_audio: true, needs_system: false,
    },
    EffectInfo {
        id: "monitor", name: "System monitor",
        description: "The processor's load, the memory in use or the processor's temperature as a bar on a row of keys: the longer and the redder, the higher.",
        params: &[
            choice("source", "Shows", "cpu", SOURCE_CHOICES), choice("row", "Where", "numbers", ROW_CHOICES),
            color("low", "Low", [40, 200, 90]), color("high", "High", [255, 50, 30]),
            color("background", "Other keys", [10, 12, 20]),
        ],
        needs_keys: false, needs_audio: false, needs_system: true,
    },
    EffectInfo {
        id: "heatmap", name: "Typing heat map",
        description: "Keys glow by how often you press them: the ones you use most burn brightest.",
        params: &[
            choice("palette", "Colours", "fire", FIRE_PALETTES), range("memory", "Remembers, minutes", 10, 1, 120),
            color("background", "Unused keys", [6, 6, 14]),
        ],
        needs_keys: true, needs_audio: false, needs_system: false,
    },
];

pub fn info(id: &str) -> Option<&'static EffectInfo> {
    EFFECTS.iter().find(|e| e.id == id)
}

/// A setting's value, resolved against its default.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    Color([u8; 3]),
    Int(i32),
    Text(String),
    Bool(bool),
}

impl ParamValue {
    pub fn to_json(&self) -> Value {
        match self {
            Self::Color(c) => Value::from(c.to_vec()),
            Self::Int(i) => Value::from(*i),
            Self::Text(t) => Value::from(t.clone()),
            Self::Bool(b) => Value::from(*b),
        }
    }
}

/// The value of `param` stored in `values`, or its default when missing or malformed.
pub fn value(param: &Param, values: &Map<String, Value>) -> ParamValue {
    let v = values.get(param.id);
    match param.kind {
        ParamKind::Color(d) => {
            let c = v.and_then(Value::as_array).filter(|a| a.len() == 3).map(|a| {
                let ch = |i: usize| a[i].as_u64().unwrap_or(0).min(255) as u8;
                [ch(0), ch(1), ch(2)]
            });
            ParamValue::Color(c.unwrap_or(d))
        }
        ParamKind::Range { default, lo, hi } => {
            ParamValue::Int(v.and_then(Value::as_i64).map(|x| (x as i32).clamp(lo, hi)).unwrap_or(default))
        }
        ParamKind::Choice { default, choices } => {
            let s = v.and_then(Value::as_str).filter(|s| choices.iter().any(|(id, _)| id == s)).unwrap_or(default);
            ParamValue::Text(s.to_string())
        }
        ParamKind::Toggle(d) => ParamValue::Bool(v.and_then(Value::as_bool).unwrap_or(d)),
    }
}

struct Values(HashMap<&'static str, ParamValue>);

impl Values {
    fn new(info: &EffectInfo, values: &Map<String, Value>) -> Self {
        Self(info.params.iter().map(|p| (p.id, value(p, values))).collect())
    }
    fn rgb(&self, id: &str) -> [f32; 3] {
        match self.0.get(id) {
            Some(ParamValue::Color(c)) => [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0],
            _ => [0.0; 3],
        }
    }
    fn int(&self, id: &str) -> f32 {
        match self.0.get(id) {
            Some(ParamValue::Int(i)) => *i as f32,
            _ => 0.0,
        }
    }
    fn text(&self, id: &str) -> &str {
        match self.0.get(id) {
            Some(ParamValue::Text(t)) => t,
            _ => "",
        }
    }
    fn flag(&self, id: &str) -> bool {
        matches!(self.0.get(id), Some(ParamValue::Bool(true)))
    }
    /// The point waves start from, as a share of the keyboard's width and height.
    fn centre(&self) -> [f32; 2] {
        let share = |id: &str, default: f32| if self.0.contains_key(id) { self.int(id) / 100.0 } else { default };
        [share("centre_x", 0.5), share("centre_y", 0.5)]
    }
}

/// What the effects react to.
#[derive(Clone, Debug, Default)]
pub struct Inputs {
    /// Rows (into the key arrays) pressed since the last frame.
    pub presses: Vec<usize>,
    /// Audio spectrum, 0..1 per band.
    pub bands: Vec<f32>,
    /// Overall loudness 0..1.
    pub level: f32,
    pub beat: bool,
    /// Processor load, memory in use and the processor's temperature on a 35–95 °C scale, 0..1 each.
    pub system: [f32; 3],
}

pub type Rgbf = [f32; 3];

pub fn hsv(h: f32, s: f32, v: f32) -> Rgbf {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor() as i32 % 6;
    let f = h - h.floor();
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

pub fn mix(a: Rgbf, b: Rgbf, w: f32) -> Rgbf {
    [a[0] * (1.0 - w) + b[0] * w, a[1] * (1.0 - w) + b[1] * w, a[2] * (1.0 - w) + b[2] * w]
}

fn scale(c: Rgbf, k: f32) -> Rgbf {
    [c[0] * k, c[1] * k, c[2] * k]
}

fn palette_stops(name: &str) -> Option<&'static [(f32, [u8; 3])]> {
    match name {
        "fire" => Some(&[(0.0, [0, 0, 0]), (0.3, [180, 10, 0]), (0.6, [255, 90, 0]), (0.85, [255, 190, 40]),
                         (1.0, [255, 250, 210])]),
        "ice" => Some(&[(0.0, [0, 0, 0]), (0.35, [0, 40, 160]), (0.7, [40, 170, 255]), (1.0, [230, 250, 255])]),
        "amber" => Some(&[(0.0, [0, 0, 0]), (0.5, [200, 70, 0]), (1.0, [255, 190, 60])]),
        "toxic" => Some(&[(0.0, [0, 0, 0]), (0.4, [0, 120, 30]), (0.75, [120, 255, 0]), (1.0, [240, 255, 160])]),
        _ => None,
    }
}

/// A colour from a named palette; for "rainbow" the hue comes from `hue_x`.
pub fn palette(name: &str, v: f32, hue_x: Option<f32>) -> Rgbf {
    let v = v.clamp(0.0, 1.0);
    let Some(stops) = palette_stops(name) else { return hsv(hue_x.unwrap_or(v), 1.0, v) };
    let k = stops.iter().position(|(x, _)| *x >= v).unwrap_or(stops.len() - 1).max(1);
    let ((x0, c0), (x1, c1)) = (stops[k - 1], stops[k]);
    let w = if x1 > x0 { ((v - x0) / (x1 - x0)).clamp(0.0, 1.0) } else { 1.0 };
    let f = |c: [u8; 3]| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0];
    mix(f(c0), f(c1), w)
}

fn hz(speed: f32, lo: f32, hi: f32) -> f32 {
    lo + (hi - lo) * (speed.clamp(0.0, 100.0) / 100.0).powf(1.5)
}

/// Where a key is along the direction of a wave; "circle" measures from `centre`. The keyboard is
/// about 2.8 times as wide as it is high, hence the factor on y.
fn projection(direction: &str, p: [f32; 2], centre: [f32; 2]) -> f32 {
    let [x, y] = p;
    match direction {
        "left" => 1.0 - x,
        "down" => y * 0.36,
        "up" => (1.0 - y) * 0.36,
        "diagonal" => (x + y * 0.36) / 1.36,
        "circle" => (x - centre[0]).hypot((y - centre[1]) * 0.36) * 2.0,
        _ => x,
    }
}

/// A small deterministic random generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next_f32(&mut self) -> f32 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        ((self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 40) as f32) / (1u64 << 24) as f32
    }
}

pub trait Effect: Send {
    fn info(&self) -> &'static EffectInfo;
    /// Colours for every key at time `t` (seconds), `dt` after the previous frame.
    fn render(&mut self, t: f32, dt: f32, inp: &Inputs) -> Vec<Rgbf>;
}

struct Base {
    info: &'static EffectInfo,
    pos: Vec<[f32; 2]>,
    v: Values,
}

struct Wave(Base);
struct Drift(Base);
struct Breath(Base);
struct Starlight { b: Base, energy: Vec<f32>, hue: Vec<f32>, rng: Rng }
struct Reactive { b: Base, energy: Vec<f32>, color: Vec<Rgbf>, ripples: Vec<(f32, f32, f32, f32)>, hue: f32 }
struct Fire { b: Base, grid: Vec<f32> }
struct Spectrum(Base);
struct Pulse { b: Base, smooth: f32 }
/// `rows`: the key rows of the number row and of F1–F12, left to right.
struct Monitor { b: Base, rows: [Vec<usize>; 2], shown: f32 }
struct Heatmap { b: Base, heat: Vec<f32> }

impl Effect for Wave {
    fn info(&self) -> &'static EffectInfo {
        self.0.info
    }
    fn render(&mut self, t: f32, _dt: f32, _inp: &Inputs) -> Vec<Rgbf> {
        let v = &self.0.v;
        let cycles = 100.0 / v.int("size").max(20.0);
        let speed = hz(v.int("speed"), 0.03, 1.4);
        let sat = v.int("saturation") / 100.0;
        let centre = v.centre();
        self.0.pos.iter().map(|p| hsv(projection(v.text("direction"), *p, centre) * cycles - t * speed, sat, 1.0)).collect()
    }
}

impl Effect for Drift {
    fn info(&self) -> &'static EffectInfo {
        self.0.info
    }
    fn render(&mut self, t: f32, _dt: f32, _inp: &Inputs) -> Vec<Rgbf> {
        let v = &self.0.v;
        let cycles = 100.0 / v.int("size").max(20.0);
        let speed = hz(v.int("speed"), 0.03, 1.4);
        let (a, b) = (v.rgb("a"), v.rgb("b"));
        self.0.pos.iter().map(|p| {
            let w = 0.5 + 0.5 * (TAU * (projection(v.text("direction"), *p, v.centre()) * cycles - t * speed)).cos();
            mix(a, b, w)
        }).collect()
    }
}

impl Effect for Breath {
    fn info(&self) -> &'static EffectInfo {
        self.0.info
    }
    fn render(&mut self, t: f32, _dt: f32, _inp: &Inputs) -> Vec<Rgbf> {
        let v = &self.0.v;
        let cycle = t * hz(v.int("speed"), 0.08, 1.2);
        let level = (1.0 - (TAU * cycle).cos()) / 2.0;
        let floor = v.int("floor") / 100.0;
        let level = floor + (1.0 - floor) * level.powf(1.6);
        let c = if v.flag("alternate") && (cycle as i64) % 2 == 1 { v.rgb("b") } else { v.rgb("a") };
        vec![scale(c, level); self.0.pos.len()]
    }
}

impl Effect for Starlight {
    fn info(&self) -> &'static EffectInfo {
        self.b.info
    }
    fn render(&mut self, _t: f32, dt: f32, _inp: &Inputs) -> Vec<Rgbf> {
        let v = &self.b.v;
        let rate = 0.02 + v.int("density") / 100.0 * 0.9; // sparks per key per second
        let tau = 0.15 + v.int("fade") / 100.0 * 2.5;
        let decay = (-dt / tau).exp();
        let (bg, star, random) = (v.rgb("background"), v.rgb("star"), v.flag("random_hue"));
        (0..self.b.pos.len()).map(|i| {
            if self.rng.next_f32() < rate * dt {
                self.hue[i] = self.rng.next_f32();
                self.energy[i] = 1.0;
            }
            self.energy[i] *= decay;
            let s = if random { hsv(self.hue[i], 0.85, 1.0) } else { star };
            mix(bg, s, self.energy[i])
        }).collect()
    }
}

impl Reactive {
    fn press_color(&mut self) -> Rgbf {
        if self.b.v.flag("rainbow") {
            self.hue = (self.hue + 0.13) % 1.0;
            hsv(self.hue, 1.0, 1.0)
        } else {
            self.b.v.rgb("press")
        }
    }
}

impl Effect for Reactive {
    fn info(&self) -> &'static EffectInfo {
        self.b.info
    }
    fn render(&mut self, t: f32, dt: f32, inp: &Inputs) -> Vec<Rgbf> {
        let tau = 0.08 + self.b.v.int("fade") / 100.0 * 1.5;
        let style = self.b.v.text("style").to_string();
        let n = self.b.pos.len();
        for &i in inp.presses.iter().filter(|i| **i < n) {
            let c = self.press_color();
            let [xi, yi] = self.b.pos[i];
            match style.as_str() {
                "key" => {
                    self.energy[i] = 1.0;
                    self.color[i] = c;
                }
                "cross" => {
                    for (k, [x, y]) in self.b.pos.iter().enumerate() {
                        if (y - yi).abs() < 0.08 || (x - xi).abs() < 0.03 {
                            self.energy[k] = 1.0;
                            self.color[k] = c;
                        }
                    }
                }
                _ => {
                    let hue = if self.b.v.flag("rainbow") { self.hue } else { -1.0 };
                    self.ripples.push((xi, yi * 0.36, t, hue));
                }
            }
        }
        let decay = (-dt / tau).exp();
        for e in &mut self.energy {
            *e *= decay;
        }
        let idle = self.b.v.rgb("idle");
        if style == "ripple" {
            let speed = 0.15 + self.b.v.int("speed") / 100.0 * 1.2;
            let press = self.b.v.rgb("press");
            let start = self.ripples.len().saturating_sub(24);
            self.ripples.drain(..start);
            self.ripples.retain(|(_, _, t0, _)| (-(t - t0) / (tau * 1.5)).exp() >= 0.02);
            return self.b.pos.iter().map(|[x, y]| {
                let mut out = idle;
                for &(rx, ry, t0, hue) in &self.ripples {
                    let age = t - t0;
                    let life = (-age / (tau * 1.5)).exp();
                    let d = (x - rx).hypot(y * 0.36 - ry);
                    let ring = (-((d - speed * age).powi(2)) / 0.0025).exp() * life;
                    let c = if hue >= 0.0 { hsv(hue, 1.0, 1.0) } else { press };
                    out = mix(out, c, ring);
                }
                out
            }).collect();
        }
        (0..n).map(|i| mix(idle, self.color[i], self.energy[i])).collect()
    }
}

impl Fire {
    fn noise(&self, u: f32, v: f32) -> f32 {
        let (u0, v0) = (u.floor() as i64, v.floor() as i64);
        let (mut fu, mut fv) = (u - u0 as f32, v - v0 as f32);
        fu = fu * fu * (3.0 - 2.0 * fu);
        fv = fv * fv * (3.0 - 2.0 * fv);
        let g = |a: i64, b: i64| self.grid[(a.rem_euclid(64) * 64 + b.rem_euclid(64)) as usize];
        let (a, b, c, d) = (g(u0, v0), g(u0 + 1, v0), g(u0, v0 + 1), g(u0 + 1, v0 + 1));
        (a * (1.0 - fu) + b * fu) * (1.0 - fv) + (c * (1.0 - fu) + d * fu) * fv
    }
}

impl Effect for Fire {
    fn info(&self) -> &'static EffectInfo {
        self.b.info
    }
    fn render(&mut self, t: f32, _dt: f32, _inp: &Inputs) -> Vec<Rgbf> {
        let v = &self.b.v;
        let sp = 0.4 + v.int("speed") / 100.0 * 3.0;
        let height = 0.25 + v.int("height") / 100.0 * 0.9;
        let pal = v.text("palette").to_string();
        self.b.pos.iter().map(|[x, y]| {
            let n = 0.65 * self.noise(x * 9.0, y * 3.0 + t * sp) + 0.35 * self.noise(x * 19.0 + 7.0, y * 6.0 + t * sp * 1.7);
            let heat = (n * 1.25 - (1.0 - y) / height + 0.45).clamp(0.0, 1.0);
            palette(&pal, heat, None)
        }).collect()
    }
}

impl Effect for Spectrum {
    fn info(&self) -> &'static EffectInfo {
        self.0.info
    }
    fn render(&mut self, _t: f32, _dt: f32, inp: &Inputs) -> Vec<Rgbf> {
        let v = &self.0.v;
        if inp.bands.is_empty() {
            return vec![[0.0; 3]; self.0.pos.len()];
        }
        let nb = inp.bands.len();
        let gain = 0.4 + v.int("sensitivity") / 100.0 * 1.6;
        let bars = v.text("style") != "glow";
        let pal = v.text("palette");
        self.0.pos.iter().map(|[x, y]| {
            let band = ((x * nb as f32) as usize).min(nb - 1);
            let level = (inp.bands[band] * gain).clamp(0.0, 1.0);
            let value = if bars { ((level - (1.0 - y)) * 6.0 + 1.0).clamp(0.0, 1.0) } else { level };
            if pal == "rainbow" {
                hsv(x * 0.85, 1.0, value)
            } else {
                palette(pal, value * (0.35 + 0.65 * if bars { 1.0 - y } else { level }), None)
            }
        }).collect()
    }
}

impl Effect for Pulse {
    fn info(&self) -> &'static EffectInfo {
        self.b.info
    }
    fn render(&mut self, _t: f32, dt: f32, inp: &Inputs) -> Vec<Rgbf> {
        let v = &self.b.v;
        let gain = 0.5 + v.int("sensitivity") / 100.0 * 2.0;
        let target = (inp.level * gain).min(1.0);
        let k = 1.0 - (-dt / if target > self.smooth { 0.04 } else { 0.25 }).exp();
        self.smooth += (target - self.smooth) * k;
        let lvl = self.smooth;
        let (a, b) = (v.rgb("a"), v.rgb("b"));
        let base = scale(a, 0.25 + 0.5 * lvl);
        let (spread, [cx, cy]) = (v.flag("from_centre"), v.centre());
        self.b.pos.iter().map(|[x, y]| {
            let m = if spread {
                let d = (x - cx).hypot((y - cy) * 0.36) * 2.1;
                ((lvl * 1.15 - d) * 3.0 + 1.0).clamp(0.0, 1.0)
            } else {
                lvl
            };
            mix(base, b, m)
        }).collect()
    }
}

impl Effect for Monitor {
    fn info(&self) -> &'static EffectInfo {
        self.b.info
    }
    fn render(&mut self, _t: f32, dt: f32, inp: &Inputs) -> Vec<Rgbf> {
        let reading = match self.b.v.text("source") {
            "memory" => inp.system[1],
            "temperature" => inp.system[2],
            _ => inp.system[0],
        };
        // Glide towards each reading, so the bar moves smoothly between samples
        self.shown += (reading.clamp(0.0, 1.0) - self.shown) * (1.0 - (-dt / 0.35).exp());
        let (low, high, background) = (self.b.v.rgb("low"), self.b.v.rgb("high"), self.b.v.rgb("background"));
        let n = self.b.pos.len();
        let row = match self.b.v.text("row") {
            "all" => return vec![mix(low, high, self.shown); n],
            "function" => &self.rows[1],
            _ => &self.rows[0],
        };
        let mut out = vec![background; n];
        let m = row.len().max(1) as f32;
        // The whole bar takes the colour of the reading: green while idle, red under load. The rest of
        // the row glows faintly, so the bar is seen to be short rather than missing.
        let bar = mix(low, high, self.shown);
        let track = mix(background, bar, 0.12);
        for (j, &i) in row.iter().enumerate() {
            // The last lit key shows how far into it the reading goes; the first is always lit
            let fill = ((self.shown - j as f32 / m) * m).clamp(if j == 0 { 1.0 } else { 0.0 }, 1.0);
            out[i] = mix(track, bar, fill);
        }
        out
    }
}

impl Effect for Heatmap {
    fn info(&self) -> &'static EffectInfo {
        self.b.info
    }
    fn render(&mut self, _t: f32, dt: f32, inp: &Inputs) -> Vec<Rgbf> {
        let half_life = self.b.v.int("memory") * 60.0;
        let decay = 0.5f32.powf(dt / half_life.max(1.0));
        for h in &mut self.heat {
            *h *= decay;
        }
        for &i in &inp.presses {
            if let Some(h) = self.heat.get_mut(i) {
                *h += 1.0;
            }
        }
        // Relative to the most used key, and a few presses before anything glows at full
        let top = self.heat.iter().copied().fold(4.0f32, f32::max);
        let (name, background) = (self.b.v.text("palette"), self.b.v.rgb("background"));
        self.heat.iter().map(|h| {
            let v = (h / top).sqrt();
            mix(background, palette(name, 0.25 + 0.75 * v, None), v)
        }).collect()
    }
}

/// The effect with `id` (the rainbow wave when unknown), set up with stored `values`.
pub fn create(id: &str, pos: &[[f32; 2]], values: &Map<String, Value>) -> Box<dyn Effect> {
    let info = info(id).unwrap_or(&EFFECTS[0]);
    let b = Base { info, pos: pos.to_vec(), v: Values::new(info, values) };
    let n = pos.len();
    let keys = crate::layout::keys(); // in the same order as the positions
    match info.id {
        "drift" => Box::new(Drift(b)),
        "breath" => Box::new(Breath(b)),
        "starlight" => {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ 7);
            let hue = (0..n).map(|_| rng.next_f32()).collect();
            Box::new(Starlight { b, energy: vec![0.0; n], hue, rng })
        }
        "reactive" => Box::new(Reactive { b, energy: vec![0.0; n], color: vec![[0.0; 3]; n], ripples: Vec::new(), hue: 0.0 }),
        "fire" => {
            let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ 11);
            Box::new(Fire { b, grid: (0..64 * 64).map(|_| rng.next_f32()).collect() })
        }
        "spectrum" => Box::new(Spectrum(b)),
        "pulse" => Box::new(Pulse { b, smooth: 0.0 }),
        "monitor" => {
            let row = |codes: &[u8]| {
                let mut row: Vec<usize> = (0..n).filter(|&i| keys.get(i).is_some_and(|k| codes.contains(&k.code))).collect();
                row.sort_by(|a, b| pos[*a][0].total_cmp(&pos[*b][0]));
                row
            };
            let numbers: Vec<u8> = (0x1E..=0x27).chain([0x2D, 0x2E]).collect();
            let function: Vec<u8> = (0x3A..=0x45).collect();
            Box::new(Monitor { b, rows: [row(&numbers), row(&function)], shown: 0.0 })
        }
        "heatmap" => Box::new(Heatmap { b, heat: vec![0.0; n] }),
        _ => Box::new(Wave(b)),
    }
}

/// Packs per-key colours into the 512-byte real-time table (`[index R G B]` × 128).
pub fn to_bytes(colors: &[Rgbf], leds: &[u8], brightness: f32) -> [u8; 512] {
    let mut table = [0u8; 512];
    for (c, &idx) in colors.iter().zip(leds) {
        if (idx as usize) < 128 {
            let i = idx as usize * 4;
            table[i] = idx;
            for k in 0..3 {
                table[i + 1 + k] = (c[k] * brightness * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
            }
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::positions;

    #[test]
    fn every_effect_renders_in_range() {
        let (leds, pos) = positions();
        for e in &EFFECTS {
            let mut fx = create(e.id, pos, &Map::new());
            let inp = Inputs { presses: vec![0, 10], bands: vec![0.5; 16], level: 0.7, beat: true, system: [0.5, 0.3, 0.9] };
            for frame in 0..30 {
                let colors = fx.render(frame as f32 / 30.0, 1.0 / 30.0, &inp);
                assert_eq!(colors.len(), pos.len(), "{}", e.id);
                assert!(colors.iter().flatten().all(|c| c.is_finite() && (-1e-4..=1.0001).contains(c)), "{}", e.id);
            }
            let table = to_bytes(&fx.render(1.0, 0.03, &inp), leds, 1.0);
            assert_eq!(table[4], 1, "{}: Esc has LED index 1", e.id);
        }
    }

    #[test]
    fn system_monitor_fills_its_row() {
        let (_, pos) = positions();
        let mut fx = create("monitor", pos, &Map::new());
        let half = Inputs { system: [0.5, 0.0, 0.0], ..Inputs::default() };
        let mut colors = vec![];
        for _ in 0..200 {
            colors = fx.render(0.0, 0.05, &half); // long enough to settle on the reading
        }
        let row: Vec<usize> = (0..pos.len()).filter(|&i| (0x1E..=0x27).contains(&crate::layout::keys()[i].code)).collect();
        assert_eq!(row.len(), 10, "1 to 0");
        let glow = |i: usize| colors[i].iter().sum::<f32>();
        let key = |code: u8| (0..pos.len()).find(|&i| crate::layout::keys()[i].code == code).unwrap();
        assert!(glow(key(0x1E)) > 0.6 && glow(key(0x22)) > 0.6, "1 and 5 are lit at 50 %");
        let track = glow(key(0x25));
        assert!(track < 0.6 && glow(key(0x2E)) < 0.6, "8 and = only glow faintly");
        assert!(glow(key(0x29)) < track, "Esc keeps the background, darker than the track");
    }

    #[test]
    fn heat_map_glows_where_keys_are_pressed() {
        let (_, pos) = positions();
        let mut fx = create("heatmap", pos, &Map::new());
        let mut colors = vec![];
        for _ in 0..8 {
            colors = fx.render(0.0, 0.1, &Inputs { presses: vec![5], ..Inputs::default() });
        }
        let glow = |i: usize| colors[i].iter().sum::<f32>();
        assert!(glow(5) > 1.0 && glow(6) < 0.2, "{} {}", glow(5), glow(6));
    }

    #[test]
    fn values_fall_back_to_defaults() {
        let mut m = Map::new();
        m.insert("speed".into(), Value::from(500));
        m.insert("direction".into(), Value::from("sideways"));
        let wave = &EFFECTS[0];
        let param = |id: &str| wave.params.iter().find(|p| p.id == id).unwrap();
        assert_eq!(value(param("speed"), &m), ParamValue::Int(100));
        assert_eq!(value(param("direction"), &m), ParamValue::Text("right".into()));
    }

    #[test]
    fn waves_spread_from_the_centre_they_are_given() {
        let wave = &EFFECTS[0];
        let mut m = Map::new();
        m.insert("direction".into(), Value::from("circle"));
        assert!(param_shown(wave, "centre_x", &m), "the centre goes with From the centre");
        m.insert("centre_x".into(), Value::from(10));
        m.insert("centre_y".into(), Value::from(90));
        let v = Values::new(wave, &m);
        assert_eq!(v.centre(), [0.1, 0.9]);
        assert_eq!(projection("circle", [0.1, 0.9], v.centre()), 0.0, "the centre is where the wave starts");
        m.insert("direction".into(), Value::from("right"));
        assert!(!param_shown(wave, "centre_x", &m), "other directions have no centre");
    }

    fn param_shown(info: &EffectInfo, id: &str, values: &Map<String, Value>) -> bool {
        info.params.iter().find(|p| p.id == id).unwrap().shown(values, info)
    }

    #[test]
    fn palettes() {
        assert_eq!(palette("fire", 0.0, None), [0.0, 0.0, 0.0]);
        let top = palette("fire", 1.0, None);
        assert!((top[0] - 1.0).abs() < 1e-6 && top[2] > 0.8);
        assert_eq!(hsv(0.0, 1.0, 1.0), [1.0, 0.0, 0.0]);
    }
}
