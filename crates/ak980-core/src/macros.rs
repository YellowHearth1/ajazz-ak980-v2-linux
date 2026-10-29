//! Macros (opcodes 0x19 + 0x15).
//!
//! Memory image: a directory of 100 × 4 bytes (offset LE16 + `00 00`; `FF FF FF FF` for a macro
//! without steps), then from byte 400 one record per macro: `[event count LE16][6 × 00][events]`.
//! Every step becomes two 4-byte events: the action `[00 00 CODE TYPE]` (TYPE `B0` key down, `30`
//! key up, `90` mouse down, `10` mouse up) and the pause after it `[ms LE16 00 50]` with ms ≥ 10.
//! The keyboard receives PAGES × 64 bytes with `AA 55` at the end.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::i18n::tr;
use crate::keymap::{key_code, key_name};
use crate::tr;

pub const MAX_MACROS: usize = 100;
pub const DIRECTORY_BYTES: usize = 400;
pub const MIN_DELAY: u16 = 10;
/// Vendor limit: (steps + macros) × 8.
pub const BUDGET_BYTES: usize = 0xA8C;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepAction {
    Down,
    Up,
    MouseDown,
    MouseUp,
}

impl StepAction {
    pub fn id(self) -> &'static str {
        ["down", "up", "mdown", "mup"][self as usize]
    }

    fn type_byte(self) -> u8 {
        [0xB0, 0x30, 0x90, 0x10][self as usize]
    }

    pub fn by_id(id: &str) -> Option<Self> {
        [Self::Down, Self::Up, Self::MouseDown, Self::MouseUp].into_iter().find(|a| a.id() == id)
    }

    pub fn pressed(self) -> bool {
        matches!(self, Self::Down | Self::MouseDown)
    }

    pub fn is_mouse(self) -> bool {
        matches!(self, Self::MouseDown | Self::MouseUp)
    }
}

/// One step: stored in config.json as `["down", 4, 10]`, like the Python version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "(String, u32, u32)", into = "(String, u32, u32)")]
pub struct MacroStep {
    pub action: StepAction,
    /// HID key usage, or mouse button mask (1 left, 2 right, 4 middle).
    pub code: u8,
    /// Pause after the step in ms.
    pub delay: u16,
}

impl TryFrom<(String, u32, u32)> for MacroStep {
    type Error = String;

    fn try_from((action, code, delay): (String, u32, u32)) -> Result<Self, String> {
        let action = StepAction::by_id(&action).ok_or_else(|| format!("unknown macro step {action}"))?;
        Ok(Self { action, code: code.min(255) as u8, delay: delay.min(u16::MAX as u32) as u16 })
    }
}

impl From<MacroStep> for (String, u32, u32) {
    fn from(s: MacroStep) -> Self {
        (s.action.id().to_string(), s.code as u32, s.delay as u32)
    }
}

pub const MOUSE_BUTTONS: [(u8, &str); 3] = [(1, "Left button"), (2, "Right button"), (4, "Middle button")];

impl MacroStep {
    pub fn new(action: StepAction, code: u8) -> Self {
        Self { action, code, delay: MIN_DELAY }
    }

    pub fn target(&self) -> String {
        if self.action.is_mouse() {
            MOUSE_BUTTONS
                .iter()
                .find(|(b, _)| *b == self.code)
                .map(|(_, n)| tr(n))
                .unwrap_or_else(|| tr!("Button {n}", n = self.code))
        } else {
            key_name(self.code)
        }
    }

    pub fn describe(&self) -> String {
        let verb = if self.action.pressed() { tr("press") } else { tr("release") };
        tr!("{verb} {target}, wait {ms} ms", verb = verb, target = self.target(), ms = self.delay)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Macro {
    pub name: String,
    pub steps: Vec<MacroStep>,
}

impl Default for Macro {
    fn default() -> Self {
        Self { name: "Macro".into(), steps: Vec::new() }
    }
}

impl Macro {
    pub fn new(name: impl Into<String>, steps: Vec<MacroStep>) -> Self {
        Self { name: name.into(), steps }
    }

    pub fn duration_ms(&self) -> u32 {
        self.steps.iter().map(|s| s.delay as u32).sum()
    }

    /// Text in the US layout: Shift is wrapped around characters that need it.
    pub fn text_steps(text: &str, delay: u16) -> Vec<MacroStep> {
        let mut steps = Vec::new();
        let step = |a, c| MacroStep { action: a, code: c, delay };
        for ch in text.replace("\r\n", "\n").chars() {
            let Some((shift, code)) = char_to_hid(ch) else { continue };
            if shift {
                steps.push(step(StepAction::Down, 0xE1));
            }
            steps.push(step(StepAction::Down, code));
            steps.push(step(StepAction::Up, code));
            if shift {
                steps.push(step(StepAction::Up, 0xE1));
            }
        }
        steps
    }

    pub fn combo_steps(combo: &str, delay: u16) -> Result<Vec<MacroStep>> {
        let codes =
            combo.split('+').map(str::trim).filter(|p| !p.is_empty()).map(key_code).collect::<Result<Vec<u8>>>()?;
        let mut steps: Vec<MacroStep> =
            codes.iter().map(|c| MacroStep { action: StepAction::Down, code: *c, delay }).collect();
        steps.extend(codes.iter().rev().map(|c| MacroStep { action: StepAction::Up, code: *c, delay }));
        Ok(steps)
    }

    /// `text:hello | combo:ctrl+shift+esc | steps:down:A,wait:50,up:A,tap:B,click:left`
    pub fn parse(spec: &str, name: Option<&str>) -> Result<Self> {
        let (kind, rest) = spec.split_once(':').unwrap_or((spec, ""));
        let mut m = Macro::new(name.unwrap_or(spec), Vec::new());
        match kind.to_lowercase().as_str() {
            "text" => m.steps = Self::text_steps(rest, MIN_DELAY),
            "combo" => m.steps = Self::combo_steps(rest, MIN_DELAY)?,
            "steps" => {
                for part in rest.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                    let (op, arg) = part.split_once(':').unwrap_or((part, ""));
                    match op.to_lowercase().as_str() {
                        "down" => m.steps.push(MacroStep::new(StepAction::Down, key_code(arg)?)),
                        "up" => m.steps.push(MacroStep::new(StepAction::Up, key_code(arg)?)),
                        "tap" => {
                            let c = key_code(arg)?;
                            m.steps.push(MacroStep::new(StepAction::Down, c));
                            m.steps.push(MacroStep::new(StepAction::Up, c));
                        }
                        "click" => {
                            let b = match arg.to_lowercase().as_str() {
                                "left" => 1,
                                "right" => 2,
                                "middle" => 4,
                                _ => return Err(Error::invalid(tr!("Unknown mouse button: {name}", name = arg))),
                            };
                            m.steps.push(MacroStep::new(StepAction::MouseDown, b));
                            m.steps.push(MacroStep::new(StepAction::MouseUp, b));
                        }
                        "wait" => {
                            let Some(last) = m.steps.last_mut() else {
                                return Err(Error::invalid(tr("“wait” must follow a step")));
                            };
                            last.delay = arg.trim().parse().map_err(|_| {
                                Error::invalid(tr!("Unknown macro step: {step}", step = part))
                            })?;
                        }
                        _ => return Err(Error::invalid(tr!("Unknown macro step: {step}", step = part))),
                    }
                }
            }
            _ => return Err(Error::invalid(tr("A macro is text:..., combo:... or steps:..."))),
        }
        Ok(m)
    }
}

pub fn used_bytes(macros: &[Macro]) -> usize {
    (macros.iter().map(|m| m.steps.len()).sum::<usize>() + macros.len()) * 8
}

/// The memory image and its page count.
pub fn encode_macros(macros: &[Macro]) -> Result<(Vec<u8>, usize)> {
    if macros.len() > MAX_MACROS {
        return Err(Error::invalid(tr!("The keyboard holds at most {n} macros", n = MAX_MACROS)));
    }
    let used = used_bytes(macros);
    if used > BUDGET_BYTES {
        return Err(Error::invalid(tr!(
            "Macros need {used} of {total} bytes of keyboard memory; remove some steps",
            used = used,
            total = BUDGET_BYTES
        )));
    }
    let mut img = vec![0u8; DIRECTORY_BYTES + BUDGET_BYTES + 128];
    let mut offset = DIRECTORY_BYTES;
    for (i, m) in macros.iter().enumerate() {
        if m.steps.is_empty() {
            img[i * 4..i * 4 + 4].copy_from_slice(&[0xFF; 4]);
            continue;
        }
        let events = (m.steps.len() * 2) as u16;
        img[i * 4..i * 4 + 2].copy_from_slice(&(offset as u16).to_le_bytes());
        img[offset..offset + 2].copy_from_slice(&events.to_le_bytes());
        let mut p = offset + 8;
        for s in &m.steps {
            img[p + 2] = s.code;
            img[p + 3] = s.action.type_byte();
            img[p + 4..p + 6].copy_from_slice(&s.delay.max(MIN_DELAY).to_le_bytes());
            img[p + 7] = 0x50;
            p += 8;
        }
        offset = p;
    }
    let pages = offset / 64 + if offset.is_multiple_of(64) { 1 } else { 2 };
    let mut data = vec![0u8; pages * 64];
    data[..offset].copy_from_slice(&img[..offset]);
    let n = data.len();
    data[n - 2] = 0xAA;
    data[n - 1] = 0x55;
    Ok((data, pages))
}

fn char_to_hid(ch: char) -> Option<(bool, u8)> {
    match ch {
        'a'..='z' => Some((false, 0x04 + (ch as u8 - b'a'))),
        'A'..='Z' => Some((true, 0x04 + (ch as u8 - b'A'))),
        '1'..='9' => Some((false, 0x1E + (ch as u8 - b'1'))),
        '0' => Some((false, 0x27)),
        '\n' => Some((false, 0x28)),
        '\t' => Some((false, 0x2B)),
        ' ' => Some((false, 0x2C)),
        '-' => Some((false, 0x2D)),
        '=' => Some((false, 0x2E)),
        '[' => Some((false, 0x2F)),
        ']' => Some((false, 0x30)),
        '\\' => Some((false, 0x31)),
        ';' => Some((false, 0x33)),
        '\'' => Some((false, 0x34)),
        '`' => Some((false, 0x35)),
        ',' => Some((false, 0x36)),
        '.' => Some((false, 0x37)),
        '/' => Some((false, 0x38)),
        _ => {
            const SHIFTED: &str = ")!@#$%^&*(_+{}|:\"~<>?";
            const BASE: &str = "0123456789-=[]\\;'`,./";
            let i = SHIFTED.chars().position(|c| c == ch)?;
            let base = BASE.chars().nth(i)?;
            char_to_hid(base).map(|(_, code)| (true, code))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_image() {
        let (data, pages) = encode_macros(&[Macro::parse("text:Hi", None).unwrap(), Macro::new("empty", vec![])])
            .unwrap();
        assert_eq!(data.len(), pages * 64);
        assert_eq!(&data[data.len() - 2..], &[0xAA, 0x55]);
        assert_eq!(&data[0..2], &400u16.to_le_bytes()); // first macro starts after the directory
        assert_eq!(&data[4..8], &[0xFF; 4]); // a macro without steps
        assert_eq!(data[400], 12); // 6 steps -> 12 events
        assert_eq!((data[408 + 3], data[408 + 7]), (0xB0, 0x50)); // key down, then its pause
    }

    #[test]
    fn budget() {
        let big = Macro::new("big", Macro::text_steps(&"x".repeat(400), MIN_DELAY));
        assert!(used_bytes(std::slice::from_ref(&big)) > BUDGET_BYTES);
        assert!(encode_macros(&[big]).is_err());
    }

    #[test]
    fn steps_and_json() {
        let m = Macro::parse("steps:down:A,wait:50,up:A,click:left", Some("x")).unwrap();
        assert_eq!(m.steps.len(), 4);
        assert_eq!(m.steps[0].delay, 50);
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#"{"name":"x","steps":[["down",4,50],["up",4,10],["mdown",1,10],["mup",1,10]]}"#);
        let back: Macro = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert_eq!(Macro::text_steps("A!", 10).len(), 8);
    }
}
