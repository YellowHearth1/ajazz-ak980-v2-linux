//! The AK980 V2 drawn from its layout file, in the colourway of the current theme.
//!
//! It is the stage of every page that touches keys: LED colours glow between the caps and shine
//! through the legends, a selected key gets an outline, remapped keys a dot. Clicks select a key;
//! in paint mode a drag paints every key it passes.

use std::collections::{BTreeMap, BTreeSet};

use ak980_core::layout::{self, LayoutKey, CASE_RECT, KNOB_CENTER, KNOB_RADIUS, SCREEN_RECT};
use iced::mouse::{self, Cursor};
use iced::widget::canvas::{self, Action, Frame, Geometry, Path, Stroke, Text};
use iced::{alignment, Color, Font, Pixels, Point, Rectangle, Renderer, Size, Theme};

use crate::fonts;
use crate::theme::{self, mix, Colorway};

/// What a click on the drawing does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Look only.
    View,
    /// A click selects one key.
    Select,
    /// Press and drag to paint; the right button erases.
    Paint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyEvent {
    Pressed(u8),
    Painted(u8),
    Erased(u8),
}

pub struct Keyboard<'a, Message> {
    /// LED colours by LED index; keys without an entry are dark.
    pub leds: BTreeMap<u8, Color>,
    pub selected: BTreeSet<u8>,
    /// Keys that carry something (a remap): drawn with a dot.
    pub marked: BTreeSet<u8>,
    pub mode: Mode,
    pub on_event: Option<Box<dyn Fn(KeyEvent) -> Message + 'a>>,
    /// The time shown on the drawn screen.
    pub clock: String,
}

#[derive(Default)]
pub struct State {
    hovered: Option<u8>,
    dragging: Option<mouse::Button>,
    last: Option<u8>,
}

/// Layout units → screen: scale and offset that fit the case into `size`.
fn fit(size: Size) -> (f32, Point) {
    let (cx, cy, cw, ch) = CASE_RECT;
    let s = (size.width / cw).min(size.height / ch);
    (s, Point::new((size.width - cw * s) / 2.0 - cx * s, (size.height - ch * s) / 2.0 - cy * s))
}

fn key_at(size: Size, p: Point) -> Option<u8> {
    let (s, o) = fit(size);
    let (u, v) = ((p.x - o.x) / s, (p.y - o.y) / s);
    layout::keys()
        .iter()
        .find(|k| u >= k.x as f32 && u < (k.x + k.w) as f32 && v >= k.y as f32 && v < (k.y + k.h) as f32)
        .map(|k| k.index)
}

/// Which caps carry the accent colour and which the modifier colour, as on both colourways.
fn cap_kind(k: &LayoutKey) -> u8 {
    match k.code {
        0x29 | 0x28 | 0x2C | 0x58 => 2,                       // Esc, Enter, Space, numpad Enter
        0x3E..=0x41 | 0x4C | 0x2A | 0x2B | 0x39 | 0xE0..=0xE7 | 0xAF | 0x4F..=0x52 | 0x4A | 0x4B | 0x4E
        | 0x53..=0x57 => 1,                                   // F5–F8, editing, modifiers, arrows, numpad ops
        _ => 0,
    }
}

const SHIFTED: &str = "~!@#$%^&*()_+{}|:\"<>?";

/// (top, bottom) legends, or (label, "") for one-line labels.
fn legends(label: &str) -> (String, String) {
    let chars: Vec<char> = label.chars().collect();
    if chars.len() == 2 && chars.iter().any(|c| SHIFTED.contains(*c)) {
        let (a, b) = (chars[0], chars[1]);
        return if SHIFTED.contains(a) { (a.to_string(), b.to_string()) } else { (b.to_string(), a.to_string()) };
    }
    match label {
        "CapsLock" => ("Caps Lock".into(), String::new()),
        "Space" => (String::new(), String::new()),
        _ => (label.to_string(), String::new()),
    }
}

fn led_on(c: Color) -> bool {
    c.r + c.g + c.b > 0.02
}

impl<Message> canvas::Program<Message> for Keyboard<'_, Message> {
    type State = State;

    fn update(&self, state: &mut State, event: &canvas::Event, bounds: Rectangle, cursor: Cursor) -> Option<Action<Message>> {
        if self.mode == Mode::View {
            return None;
        }
        let at = cursor.position_in(bounds).and_then(|p| key_at(bounds.size(), p));
        let send = |e: KeyEvent| self.on_event.as_ref().map(|f| Action::publish(f(e)).and_capture());
        match event {
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let changed = state.hovered != at;
                state.hovered = at;
                if let (Some(button), Some(key)) = (state.dragging, at)
                    && self.mode == Mode::Paint && state.last != Some(key) {
                        state.last = Some(key);
                        return send(if button == mouse::Button::Right { KeyEvent::Erased(key) } else { KeyEvent::Painted(key) });
                    }
                changed.then(Action::request_redraw)
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(button @ (mouse::Button::Left | mouse::Button::Right))) => {
                let key = at?;
                match (self.mode, button) {
                    (Mode::Paint, _) => {
                        state.dragging = Some(*button);
                        state.last = Some(key);
                        send(if *button == mouse::Button::Right { KeyEvent::Erased(key) } else { KeyEvent::Painted(key) })
                    }
                    (_, mouse::Button::Left) => send(KeyEvent::Pressed(key)),
                    _ => None,
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(_)) => {
                state.dragging = None;
                state.last = None;
                None
            }
            canvas::Event::Mouse(mouse::Event::CursorLeft) => {
                state.hovered = None;
                Some(Action::request_redraw())
            }
            _ => None,
        }
    }

    fn mouse_interaction(&self, state: &State, _bounds: Rectangle, _cursor: Cursor) -> mouse::Interaction {
        if self.mode != Mode::View && state.hovered.is_some() {
            if self.mode == Mode::Paint { mouse::Interaction::Crosshair } else { mouse::Interaction::Pointer }
        } else {
            mouse::Interaction::default()
        }
    }

    fn draw(&self, state: &State, renderer: &Renderer, _theme: &Theme, bounds: Rectangle, _cursor: Cursor) -> Vec<Geometry> {
        let r = theme::roles();
        let k = r.keyboard;
        let mut frame = Frame::new(renderer, bounds.size());
        let (s, o) = fit(bounds.size());
        let at = |x: f32, y: f32| Point::new(o.x + x * s, o.y + y * s);

        // The case, with a lighter rim where the light catches its edge
        let (cx, cy, cw, ch) = CASE_RECT;
        let case = Path::rounded_rectangle(at(cx, cy), Size::new(cw * s, ch * s), (12.0 * s).into());
        frame.fill(&case, k.case);
        frame.stroke(&case, Stroke::default().with_color(k.case_edge).with_width(1.0));

        // The screen and the knob above the numpad
        let (sx, sy, sw, sh) = SCREEN_RECT;
        let screen = Path::rounded_rectangle(at(sx, sy), Size::new(sw * s, sh * s), (3.0 * s).into());
        frame.fill(&screen, Color::from_rgb8(6, 6, 7));
        frame.stroke(&screen, Stroke::default().with_color(k.case_edge).with_width(1.0));
        frame.fill_text(Text {
            content: self.clock.clone(),
            position: at(sx + sw / 2.0, sy + sh / 2.0),
            color: Color::from_rgb8(236, 236, 230),
            size: Pixels(13.0 * s),
            font: fonts::MEDIUM,
            align_x: iced::widget::text::Alignment::Center,
            align_y: alignment::Vertical::Center,
            ..Text::default()
        });
        let knob = Path::circle(at(KNOB_CENTER.0, KNOB_CENTER.1), KNOB_RADIUS * s);
        frame.fill(&knob, k.knob);
        frame.stroke(&knob, Stroke::default().with_color(mix(k.knob, k.case_edge, 0.6)).with_width(1.5 * s));
        let grip = Path::circle(at(KNOB_CENTER.0, KNOB_CENTER.1), KNOB_RADIUS * s * 0.72);
        frame.stroke(&grip, Stroke::default().with_color(theme::alpha(k.case_edge, 0.8)).with_width(1.0));

        for key in layout::keys() {
            self.draw_key(&mut frame, key, &k, s, o, state.hovered == Some(key.index));
        }
        vec![frame.into_geometry()]
    }
}

impl<Message> Keyboard<'_, Message> {
    fn draw_key(&self, frame: &mut Frame, key: &LayoutKey, k: &Colorway, s: f32, o: Point, hovered: bool) {
        let r = theme::roles();
        let (x, y) = (o.x + key.x as f32 * s, o.y + key.y as f32 * s);
        let (w, h) = (key.w as f32 * s, key.h as f32 * s);
        let (cap, side, legend) = match cap_kind(key) {
            2 => (k.accent, k.accent_side, k.accent_legend),
            1 => (k.modifier, k.modifier_side, k.modifier_legend),
            _ => (k.alpha, k.alpha_side, k.alpha_legend),
        };
        let led = self.leds.get(&key.index).copied().filter(|c| led_on(*c));

        // Light leaking out from under the cap: the LEDs face south and the caps are opaque
        if let Some(c) = led {
            for (g, a) in [(2.0 * s, 0.28), (1.0 * s, 0.5)] {
                let glow = Path::rounded_rectangle(Point::new(x - g, y - g), Size::new(w + 2.0 * g, h + 2.0 * g), (5.5 * s).into());
                frame.fill(&glow, theme::alpha(c, a));
            }
        }
        // The cap: its sides, then the top surface set back from the edges as on a sculpted cap
        let inset = 1.2 * s;
        let body = Path::rounded_rectangle(Point::new(x + inset, y + inset), Size::new(w - 2.0 * inset, h - 2.0 * inset), (4.5 * s).into());
        frame.fill(&body, side);
        let (tl, tr, tt, tb) = (3.4 * s, 3.4 * s, 2.4 * s, 5.6 * s);
        let top_pos = Point::new(x + tl, y + tt);
        let top_size = Size::new(w - tl - tr, h - tt - tb);
        let top = Path::rounded_rectangle(top_pos, top_size, (3.5 * s).into());
        frame.fill(&top, cap);

        // Legends are printed on the caps (no shine-through on this keyboard): characters large,
        // words like Shift and the F-keys small, as on the real caps
        let ink = legend;
        let (upper, lower) = legends(&key.label);
        let large = !lower.is_empty() || upper.chars().count() == 1;
        let size = if large { 10.5 * s } else { 8.2 * s };
        let pad = 3.6 * s;
        let text = |content: String, p: Point, size: f32| Text {
            content,
            position: p,
            color: ink,
            size: Pixels(size),
            font: fonts::MEDIUM,
            align_x: iced::widget::text::Alignment::Left,
            align_y: alignment::Vertical::Top,
            max_width: top_size.width,
            ..Text::default()
        };
        if !upper.is_empty() {
            frame.fill_text(text(upper, Point::new(top_pos.x + pad * 0.6, top_pos.y + pad * 0.25), size));
        }
        if !lower.is_empty() {
            frame.fill_text(text(lower, Point::new(top_pos.x + pad * 0.6, top_pos.y + top_size.height * 0.48), size));
        }

        // Marks: remapped keys get a dot, the selection an outline in the accent
        if self.marked.contains(&key.index) {
            let dot = Path::circle(Point::new(top_pos.x + top_size.width - 3.6 * s, top_pos.y + 3.6 * s), 2.1 * s);
            frame.fill(&dot, r.accent);
            frame.stroke(&dot, Stroke::default().with_color(mix(r.accent, Color::BLACK, 0.45)).with_width(0.8));
        }
        let outline = Path::rounded_rectangle(Point::new(x + 0.5, y + 0.5), Size::new(w - 1.0, h - 1.0), (5.0 * s).into());
        if self.selected.contains(&key.index) {
            frame.stroke(&outline, Stroke::default().with_color(r.accent).with_width((2.4 * s).max(2.0)));
        } else if hovered && self.mode != Mode::View {
            frame.stroke(&outline, Stroke::default().with_color(theme::alpha(r.text, 0.55)).with_width(1.5));
        }
    }
}

/// The font the legends use, re-exported for the screen preview.
#[allow(dead_code)]
pub const LEGEND_FONT: Font = fonts::MEDIUM;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_line_legends_put_the_shifted_sign_on_top() {
        assert_eq!(legends("!1"), ("!".into(), "1".into()));
        assert_eq!(legends(",<"), ("<".into(), ",".into()));
        assert_eq!(legends("_-"), ("_".into(), "-".into()));
        assert_eq!(legends("Q"), ("Q".into(), String::new()));
        assert_eq!(legends("F1"), ("F1".into(), String::new()));
        assert_eq!(legends("Space"), (String::new(), String::new()));
    }

    #[test]
    fn hit_testing_finds_esc_and_misses_the_case() {
        let size = Size::new(1000.0, 360.0);
        let (s, o) = fit(size);
        let esc = layout::find_key("esc").unwrap();
        let p = Point::new(o.x + (esc.x as f32 + 5.0) * s, o.y + (esc.y as f32 + 5.0) * s);
        assert_eq!(key_at(size, p), Some(esc.index));
        assert_eq!(key_at(size, Point::new(o.x + 10.0 * s, o.y + 25.0 * s)), None);
    }

    #[test]
    fn caps_follow_the_colourway() {
        let kind = |n: &str| cap_kind(layout::find_key(n).unwrap());
        assert_eq!((kind("esc"), kind("space"), kind("enter")), (2, 2, 2));
        assert_eq!((kind("F5"), kind("LeftShift"), kind("Up")), (1, 1, 1));
        assert_eq!((kind("A"), kind("F1"), kind("Num7")), (0, 0, 0));
    }
}
