//! The few building blocks every page is made of: titles, cards of rows, labelled controls,
//! a colour picker and level sliders. Sizes and colours come from `theme`.

use ak980_core::i18n::tr;
use iced::widget::{button, canvas, column, container, row, slider, space, text, text_input, tooltip, Column};
use iced::{Alignment, Color, Element, Fill, Length};

use crate::app::Message;
use crate::fonts;
use crate::theme::{self, roles};
use crate::wheel::Wheel;

/// An item of a drop-down list: a value and the text shown for it.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice<T> {
    pub value: T,
    pub label: String,
}

impl<T> Choice<T> {
    pub fn new(value: T, label: impl Into<String>) -> Self {
        Self { value, label: label.into() }
    }
}

impl<T> std::fmt::Display for Choice<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

pub fn title<'a>(s: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(s).size(theme::TITLE).font(fonts::MEDIUM)
}

pub fn heading<'a>(s: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(s).size(theme::HEADING).font(fonts::MEDIUM)
}

pub fn body<'a>(s: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(s).size(theme::TEXT)
}

/// Secondary text: hints under a label, counts, units.
pub fn caption<'a>(s: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(s).size(theme::SMALL).color(roles().subtext0)
}

/// A heading above a card, outside it, as in a list of settings.
pub fn section<'a>(heading_text: String, card_content: Element<'a, Message>) -> Element<'a, Message> {
    column![heading(heading_text), card_content].spacing(theme::SPACE).into()
}

/// A card whose rows are separated by hairlines.
pub fn card<'a>(rows: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut col = Column::new();
    let n = rows.len();
    for (i, r) in rows.into_iter().enumerate() {
        col = col.push(container(r).padding([10, 16]).width(Fill));
        if i + 1 < n {
            col = col.push(container(space().height(1)).width(Fill).height(1).style(theme::rule_line));
        }
    }
    container(col).style(theme::card).width(Fill).into()
}

/// One row: a label (with an optional hint under it) on the left, the control on the right.
pub fn setting<'a>(label: String, hint: Option<String>, control: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let mut left = column![body(label)].spacing(2);
    if let Some(h) = hint {
        left = left.push(caption(h));
    }
    row![left.width(Fill), control.into()].spacing(16).align_y(Alignment::Center).into()
}

/// A 0..=max level with its number, for brightness and speed.
pub fn level<'a>(value: u8, max: u8, on_change: impl Fn(u8) -> Message + 'a) -> Element<'a, Message> {
    row![
        slider(0..=max, value, on_change).width(180).style(theme::slider_style),
        text(value.to_string()).size(theme::TEXT).width(24).font(fonts::MEDIUM),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

/// A wider slider over any integer range, with its value and unit.
pub fn ranged<'a>(value: i32, lo: i32, hi: i32, unit: &str, on_change: impl Fn(i32) -> Message + 'a) -> Element<'a, Message> {
    row![
        slider(lo..=hi, value, on_change).width(200).style(theme::slider_style),
        text(format!("{value}{unit}")).size(theme::TEXT).width(52).font(fonts::MEDIUM),
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .into()
}

/// Options shown side by side, one of them chosen.
pub fn segmented<'a, T: Copy + PartialEq + 'a>(
    options: Vec<(T, String)>,
    selected: T,
    on: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message> {
    let mut r = row![].spacing(2);
    for (value, label) in options {
        let b = button(text(label).size(theme::TEXT)).padding([6, 14]).style(theme::segment(value == selected));
        r = r.push(if value == selected { b } else { b.on_press(on(value)) });
    }
    // Wraps onto a second line in a narrow window rather than running off the edge
    container(r.wrap().vertical_spacing(2)).padding(2).style(|t| {
        let mut s = theme::card(t);
        s.background = Some(theme::roles().surface1.into());
        s
    })
    .into()
}

/// A tooltip naming what a control without a text label does, as icon buttons need.
pub fn hint<'a>(control: impl Into<Element<'a, Message>>, label: String) -> Element<'a, Message> {
    tooltip(control, container(text(label).size(theme::SMALL)).padding([4, 8]).style(theme::card), tooltip::Position::Bottom)
        .into()
}

pub fn primary_button<'a>(label: String, msg: Option<Message>) -> button::Button<'a, Message> {
    button(text(label).size(theme::TEXT).font(fonts::MEDIUM)).padding([8, 18]).style(theme::primary).on_press_maybe(msg)
}

pub fn secondary_button<'a>(label: String, msg: Option<Message>) -> button::Button<'a, Message> {
    button(text(label).size(theme::TEXT)).padding([7, 14]).style(theme::secondary).on_press_maybe(msg)
}

pub fn danger_button<'a>(label: String, msg: Option<Message>) -> button::Button<'a, Message> {
    button(text(label).size(theme::TEXT)).padding([7, 14]).style(theme::danger).on_press_maybe(msg)
}

pub fn to_color(c: [u8; 3]) -> Color {
    Color::from_rgb8(c[0], c[1], c[2])
}

pub fn html(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

pub fn parse_html(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let ch = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([ch(0)?, ch(2)?, ch(4)?])
}

fn to_hsv(c: [u8; 3]) -> (f32, f32, f32) {
    let [r, g, b] = c.map(|v| v as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}


/// Colours a keyboard is often set to: the two colourways, the user's yellow, the basics.
pub const SWATCHES: [[u8; 3]; 12] = [
    [0xFF, 0xD2, 0x00], [0xF3, 0xD2, 0x30], [0xFF, 0x8F, 0x1F], [0xFF, 0x3B, 0x30], [0xFF, 0x4F, 0xA3],
    [0xA9, 0x9C, 0xDF], [0x6F, 0x5C, 0xC9], [0x3A, 0x7B, 0xFF], [0xB7, 0xD4, 0xE8], [0x2E, 0xD1, 0xA2],
    [0x5B, 0xD0, 0x4F], [0xFF, 0xFF, 0xFF],
];

/// Swatches, a hex field and hue and saturation sliders (how bright is set where it applies).
/// `id` keys the hex field's draft.
pub fn color_picker<'a>(
    id: String,
    current: [u8; 3],
    draft: Option<&'a String>,
    on_pick: impl Fn([u8; 3]) -> Message + Clone + 'a,
) -> Element<'a, Message> {
    let mut swatches = row![].spacing(6);
    for c in SWATCHES {
        let tile = container(space().width(22).height(22)).style(theme::swatch(to_color(c), c == current));
        swatches = swatches.push(button(tile).padding(0).style(theme::flat).on_press(on_pick(c)));
    }
    let hex_value = draft.cloned().unwrap_or_else(|| html(current));
    let hex = text_input("#RRGGBB", &hex_value)
        .on_input({
            let id = id.clone();
            move |t| Message::Draft(id.clone(), t)
        })
        .on_submit(Message::DraftSubmit(id))
        .width(100)
        .padding(6)
        .style(theme::input)
        .font(fonts::MONO);
    let preview = container(space().width(100).height(36)).style(theme::swatch(to_color(current), false));
    let wheel = canvas(Wheel { hsv: to_hsv(current), on_pick: Box::new(on_pick) }).width(WHEEL).height(WHEEL);
    column![
        swatches,
        row![
            wheel,
            column![preview, hex, caption(tr("Click or drag on the circle"))].spacing(8).width(Length::Fixed(140.0)),
        ]
        .spacing(16)
        .align_y(Alignment::Center),
    ]
    .spacing(10)
    .into()
}

/// The colour wheel's size.
const WHEEL: f32 = 128.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn from_hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
        let c = ak980_core::effects::hsv(h / 360.0, s, v);
        c.map(|x| (x * 255.0).round().clamp(0.0, 255.0) as u8)
    }

    #[test]
    fn hsv_round_trip() {
        for c in SWATCHES {
            let (h, s, v) = to_hsv(c);
            let back = from_hsv(h, s, v);
            for i in 0..3 {
                assert!((back[i] as i32 - c[i] as i32).abs() <= 1, "{c:?} -> {back:?}");
            }
        }
        assert_eq!(parse_html("#ffd200"), Some([255, 210, 0]));
        assert_eq!(parse_html("ffd20"), None);
    }
}
