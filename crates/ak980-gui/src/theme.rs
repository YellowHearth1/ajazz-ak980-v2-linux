//! The two looks of the app, taken from the two colourways of the AK980 V2.
//!
//! *Graphite* is the black case with light-grey and graphite caps and yellow Esc, Enter and space
//! bar; *Lavender* the white case with white and lavender caps and sky-blue Esc, Enter and space
//! bar. Colours have roles as in Catppuccin's style guide: `crust`, `mantle` and `base` are the
//! grounds from the outermost in, `surface0..2` raised things, `overlay0..2` borders and quiet
//! marks, `text`, `subtext1` and `subtext0` the type, and text on the accent is `base`. Every
//! widget style below reads these roles and nothing else, so a colour cannot drift per widget.

use std::sync::RwLock;

use iced::theme::palette::{self, Background, Danger, Extended, Pair, Primary, Secondary, Success, Warning};
use iced::widget::{button, checkbox, container, overlay, pick_list, progress_bar, scrollable, slider, text_input,
                   toggler};
use iced::{Border, Color, Shadow, Theme, Vector};

/// The colours of the keyboard itself, for its drawing.
#[derive(Clone, Copy, Debug)]
pub struct Colorway {
    pub case: Color,
    pub case_edge: Color,
    /// Letter and number caps, their sides and legends.
    pub alpha: Color,
    pub alpha_side: Color,
    pub alpha_legend: Color,
    /// Modifiers, F5–F8, the navigation and the numpad operators.
    pub modifier: Color,
    pub modifier_side: Color,
    pub modifier_legend: Color,
    /// Esc, both Enters and the space bar.
    pub accent: Color,
    pub accent_side: Color,
    pub accent_legend: Color,
    pub knob: Color,
}

#[derive(Clone, Copy, Debug)]
pub struct Roles {
    pub name: &'static str,
    pub dark: bool,
    pub crust: Color,
    pub mantle: Color,
    pub base: Color,
    pub surface0: Color,
    pub surface1: Color,
    pub surface2: Color,
    pub overlay0: Color,
    pub overlay1: Color,
    pub subtext0: Color,
    pub subtext1: Color,
    pub text: Color,
    /// Primary actions, the selection, focus.
    pub accent: Color,
    /// The second colour of the colourway (graphite caps, sky-blue caps), for quiet highlights.
    pub second: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub keyboard: Colorway,
}

const fn hex(v: u32) -> Color {
    Color::from_rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const GRAPHITE: Roles = Roles {
    name: "graphite",
    dark: true,
    crust: hex(0x0F1012),
    mantle: hex(0x15161A),
    base: hex(0x1B1C20),
    surface0: hex(0x24262B),
    surface1: hex(0x2E3036),
    surface2: hex(0x3A3D44),
    overlay0: hex(0x555860),
    overlay1: hex(0x7A7D84),
    subtext0: hex(0xA6A8AD),
    subtext1: hex(0xC2C3C5),
    text: hex(0xE2E2DE),
    accent: hex(0xF2CF2E),
    second: hex(0x7B7C7F),
    success: hex(0x8FCF7A),
    warning: hex(0xF0A35A),
    danger: hex(0xF07167),
    keyboard: Colorway {
        case: hex(0x141416),
        case_edge: hex(0x2A2B2F),
        alpha: hex(0xD8D8D4),
        alpha_side: hex(0xB6B6B1),
        alpha_legend: hex(0x1B1B1D),
        modifier: hex(0x7B7C7F),
        modifier_side: hex(0x616265),
        modifier_legend: hex(0x141416),
        accent: hex(0xF3D230),
        accent_side: hex(0xCFAF18),
        accent_legend: hex(0x1B1B1D),
        knob: hex(0x1C1C1F),
    },
};

pub const LAVENDER: Roles = Roles {
    name: "lavender",
    dark: false,
    crust: hex(0xE6E3F1),
    mantle: hex(0xEEECF6),
    base: hex(0xF7F6FB),
    surface0: hex(0xFFFFFF),
    surface1: hex(0xEFECF9),
    surface2: hex(0xE2DDF4),
    overlay0: hex(0xC8C1E4),
    overlay1: hex(0x837AB3),
    subtext0: hex(0x635B8C),
    subtext1: hex(0x524A7A),
    text: hex(0x2C2847),
    accent: hex(0x6F5CC9),
    second: hex(0xA9CBE3),
    success: hex(0x2E8B57),
    warning: hex(0xA86200),
    danger: hex(0xC4364C),
    keyboard: Colorway {
        case: hex(0xF4F4F7),
        case_edge: hex(0xDCDBE4),
        alpha: hex(0xFCFCFC),
        alpha_side: hex(0xE2E1EA),
        alpha_legend: hex(0x7466C6),
        modifier: hex(0xA99CDF),
        modifier_side: hex(0x8C7ECF),
        modifier_legend: hex(0xFFFFFF),
        accent: hex(0xB7D4E8),
        accent_side: hex(0x93B8D3),
        accent_legend: hex(0xFFFFFF),
        knob: hex(0xF2F2F5),
    },
};

/// "auto" follows the system's light or dark preference.
pub const CHOICES: [&str; 3] = ["auto", "graphite", "lavender"];

pub fn roles_for(choice: &str, system_dark: bool) -> Roles {
    match choice {
        "graphite" => GRAPHITE,
        "lavender" => LAVENDER,
        _ if system_dark => GRAPHITE,
        _ => LAVENDER,
    }
}

static CURRENT: RwLock<Roles> = RwLock::new(GRAPHITE);

/// The roles of the theme in use; every style function reads them.
pub fn roles() -> Roles {
    *CURRENT.read().unwrap_or_else(|e| e.into_inner())
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::from_rgba(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}

fn pair(color: Color, text: Color) -> Pair {
    Pair { color, text }
}

/// Makes `r` current and returns the iced theme built from it, so that even widgets without a
/// style of their own use these colours.
pub fn activate(r: Roles) -> Theme {
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = r;
    let palette = iced::theme::Palette {
        background: r.base,
        text: r.text,
        primary: r.accent,
        success: r.success,
        warning: r.warning,
        danger: r.danger,
    };
    Theme::custom_with_fn(r.name.to_string(), palette, move |_| extended(&r))
}

fn extended(r: &Roles) -> Extended {
    let on_accent = r.base;
    let weak = |c: Color| mix(r.base, c, 0.22);
    Extended {
        background: Background {
            base: pair(r.base, r.text),
            weakest: pair(r.crust, r.text),
            weaker: pair(r.mantle, r.text),
            weak: pair(r.surface0, r.text),
            neutral: pair(r.surface1, r.text),
            strong: pair(r.surface2, r.text),
            stronger: pair(r.overlay0, r.text),
            strongest: pair(r.overlay1, r.text),
        },
        primary: Primary {
            base: pair(r.accent, on_accent),
            weak: pair(weak(r.accent), r.text),
            strong: pair(mix(r.accent, r.text, 0.15), on_accent),
        },
        secondary: Secondary {
            base: pair(r.second, r.text),
            weak: pair(weak(r.second), r.text),
            strong: pair(mix(r.second, r.text, 0.2), r.text),
        },
        success: Success { base: pair(r.success, on_accent), weak: pair(weak(r.success), r.text),
                           strong: pair(mix(r.success, r.text, 0.15), on_accent) },
        warning: Warning { base: pair(r.warning, on_accent), weak: pair(weak(r.warning), r.text),
                           strong: pair(mix(r.warning, r.text, 0.15), on_accent) },
        danger: Danger { base: pair(r.danger, on_accent), weak: pair(weak(r.danger), r.text),
                         strong: pair(mix(r.danger, r.text, 0.15), on_accent) },
        is_dark: r.dark,
    }
}

pub fn system_is_dark(mode: iced::theme::Mode) -> bool {
    mode != iced::theme::Mode::Light
}

// ---------------------------------------------------------------- sizes

pub const RADIUS: f32 = 8.0;
pub const RADIUS_SMALL: f32 = 6.0;
pub const SPACE: f32 = 8.0;
pub const PAGE_PAD: f32 = 24.0;
pub const TEXT: f32 = 14.0;
pub const SMALL: f32 = 12.5;
pub const HEADING: f32 = 15.0;
pub const TITLE: f32 = 21.0;

fn border(color: Color, width: f32, radius: f32) -> Border {
    Border { color, width, radius: radius.into() }
}

// ---------------------------------------------------------------- containers

/// The sidebar: the outermost ground, like the keyboard's case around the keys.
pub fn sidebar(_: &Theme) -> container::Style {
    let r = roles();
    container::Style { background: Some(r.mantle.into()), text_color: Some(r.text), ..Default::default() }
}

/// A group of settings: a raised card with a hairline.
pub fn card(_: &Theme) -> container::Style {
    let r = roles();
    container::Style {
        background: Some(r.surface0.into()),
        border: border(alpha(r.overlay0, if r.dark { 0.45 } else { 0.7 }), 1.0, RADIUS),
        text_color: Some(r.text),
        ..Default::default()
    }
}

/// The stage the keyboard drawing sits on.
pub fn stage(_: &Theme) -> container::Style {
    let r = roles();
    container::Style {
        background: Some(mix(r.base, r.crust, 0.6).into()),
        border: border(alpha(r.overlay0, 0.35), 1.0, RADIUS),
        ..Default::default()
    }
}

/// A persistent notice above a page: no access, cable needed.
pub fn banner(_: &Theme) -> container::Style {
    let r = roles();
    container::Style {
        background: Some(mix(r.surface0, r.warning, if r.dark { 0.16 } else { 0.12 }).into()),
        border: border(alpha(r.warning, 0.45), 1.0, RADIUS),
        text_color: Some(r.text),
        ..Default::default()
    }
}

pub fn toast(error: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let r = roles();
        let edge = if error { r.danger } else { r.overlay1 };
        container::Style {
            background: Some(r.surface2.into()),
            border: border(edge, 1.0, RADIUS),
            text_color: Some(r.text),
            shadow: Shadow { color: alpha(Color::BLACK, 0.25), offset: Vector::new(0.0, 4.0), blur_radius: 16.0 },
            ..Default::default()
        }
    }
}

/// A hairline between rows of a card.
pub fn rule_line(_: &Theme) -> container::Style {
    let r = roles();
    container::Style { background: Some(alpha(r.overlay0, if r.dark { 0.4 } else { 0.55 }).into()), ..Default::default() }
}

/// A colour swatch with a thin edge so that it shows on its own colour too.
pub fn swatch(color: Color, selected: bool) -> impl Fn(&Theme) -> container::Style {
    move |_| {
        let r = roles();
        container::Style {
            background: Some(color.into()),
            border: if selected { border(r.text, 2.0, RADIUS_SMALL) } else { border(alpha(r.overlay1, 0.6), 1.0, RADIUS_SMALL) },
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------- buttons

fn button_base(bg: Option<Color>, text: Color, edge: Option<Color>) -> button::Style {
    button::Style {
        background: bg.map(Into::into),
        text_color: text,
        border: border(edge.unwrap_or(Color::TRANSPARENT), if edge.is_some() { 1.0 } else { 0.0 }, RADIUS_SMALL),
        ..Default::default()
    }
}

/// The one action of a view: filled with the accent.
pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    let r = roles();
    let bg = match status {
        button::Status::Hovered => mix(r.accent, r.text, 0.12),
        button::Status::Pressed => mix(r.accent, r.base, 0.15),
        button::Status::Disabled => alpha(r.accent, 0.35),
        button::Status::Active => r.accent,
    };
    button_base(Some(bg), if status == button::Status::Disabled { alpha(r.base, 0.7) } else { r.base }, None)
}

/// Everything else that is a button.
pub fn secondary(_: &Theme, status: button::Status) -> button::Style {
    let r = roles();
    let bg = match status {
        button::Status::Hovered => r.surface2,
        button::Status::Pressed => mix(r.surface2, r.overlay0, 0.5),
        _ => r.surface1,
    };
    let text = if status == button::Status::Disabled { r.overlay1 } else { r.text };
    button_base(Some(bg), text, Some(alpha(r.overlay0, 0.5)))
}

/// Destructive actions.
pub fn danger(_: &Theme, status: button::Status) -> button::Style {
    let r = roles();
    let bg = match status {
        button::Status::Hovered => mix(r.surface1, r.danger, 0.25),
        button::Status::Pressed => mix(r.surface1, r.danger, 0.35),
        _ => r.surface1,
    };
    button_base(Some(bg), if status == button::Status::Disabled { r.overlay1 } else { r.danger },
                Some(alpha(r.danger, 0.5)))
}

/// Text-like buttons in lists and toolbars.
pub fn flat(_: &Theme, status: button::Status) -> button::Style {
    let r = roles();
    let bg = match status {
        button::Status::Hovered => Some(r.surface1),
        button::Status::Pressed => Some(r.surface2),
        _ => None,
    };
    button_base(bg, if status == button::Status::Disabled { r.overlay1 } else { r.text }, None)
}

/// A row in a list that can be selected (pages in the sidebar, macros, history).
pub fn list_item(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let r = roles();
        let bg = match (selected, status) {
            (true, _) => Some(mix(r.mantle, r.accent, if r.dark { 0.16 } else { 0.14 })),
            (false, button::Status::Hovered) => Some(r.surface1),
            (false, button::Status::Pressed) => Some(r.surface2),
            _ => None,
        };
        let text = if selected { if r.dark { r.accent } else { mix(r.accent, r.text, 0.35) } } else { r.text };
        button_base(bg, text, None)
    }
}

/// One option of a segmented choice (Firmware | PC, base layer | Fn layer).
pub fn segment(selected: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let r = roles();
        let bg = if selected {
            r.surface0
        } else {
            match status {
                button::Status::Hovered => mix(r.surface1, r.surface2, 0.5),
                _ => r.surface1,
            }
        };
        let edge = if selected { alpha(r.overlay0, 0.8) } else { Color::TRANSPARENT };
        button_base(Some(bg), if selected { r.text } else { r.subtext1 }, Some(edge))
    }
}

// ---------------------------------------------------------------- inputs

pub fn input(_: &Theme, status: text_input::Status) -> text_input::Style {
    let r = roles();
    let edge = match status {
        text_input::Status::Focused { .. } => r.accent,
        text_input::Status::Hovered => r.overlay1,
        _ => alpha(r.overlay0, 0.9),
    };
    text_input::Style {
        background: (if status == text_input::Status::Disabled { r.surface0 } else { r.base }).into(),
        border: border(edge, 1.0, RADIUS_SMALL),
        icon: r.subtext0,
        placeholder: r.overlay1,
        value: r.text,
        selection: alpha(r.accent, 0.35),
    }
}

pub fn pick(_: &Theme, status: pick_list::Status) -> pick_list::Style {
    let r = roles();
    let edge = match status {
        pick_list::Status::Opened { .. } => r.accent,
        pick_list::Status::Hovered => r.overlay1,
        _ => alpha(r.overlay0, 0.9),
    };
    pick_list::Style {
        text_color: r.text,
        placeholder_color: r.overlay1,
        handle_color: r.subtext0,
        background: r.surface1.into(),
        border: border(edge, 1.0, RADIUS_SMALL),
    }
}

pub fn menu(_: &Theme) -> overlay::menu::Style {
    let r = roles();
    overlay::menu::Style {
        background: r.surface1.into(),
        border: border(alpha(r.overlay0, 0.9), 1.0, RADIUS_SMALL),
        text_color: r.text,
        selected_text_color: r.base,
        selected_background: r.accent.into(),
        shadow: Shadow { color: alpha(Color::BLACK, 0.25), offset: Vector::new(0.0, 4.0), blur_radius: 12.0 },
    }
}

pub fn slider_style(_: &Theme, status: slider::Status) -> slider::Style {
    let r = roles();
    let handle = match status {
        slider::Status::Hovered | slider::Status::Dragged => mix(r.accent, r.text, 0.1),
        _ => r.accent,
    };
    slider::Style {
        rail: slider::Rail {
            backgrounds: (r.accent.into(), r.surface2.into()),
            width: 4.0,
            border: border(Color::TRANSPARENT, 0.0, 2.0),
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 8.0 },
            background: handle.into(),
            border_width: 2.0,
            border_color: r.surface0,
        },
    }
}

pub fn switch(_: &Theme, status: toggler::Status) -> toggler::Style {
    let r = roles();
    let on = matches!(status, toggler::Status::Active { is_toggled: true } | toggler::Status::Hovered { is_toggled: true });
    let hovered = matches!(status, toggler::Status::Hovered { .. });
    toggler::Style {
        background: (if on { r.accent } else if hovered { r.overlay0 } else { r.surface2 }).into(),
        background_border_width: 0.0,
        background_border_color: Color::TRANSPARENT,
        foreground: (if on { r.base } else { r.subtext1 }).into(),
        foreground_border_width: 0.0,
        foreground_border_color: Color::TRANSPARENT,
        text_color: None,
        border_radius: None,
        padding_ratio: 0.1,
    }
}

pub fn check(_: &Theme, status: checkbox::Status) -> checkbox::Style {
    let r = roles();
    let checked = match status {
        checkbox::Status::Active { is_checked } | checkbox::Status::Hovered { is_checked }
        | checkbox::Status::Disabled { is_checked } => is_checked,
    };
    checkbox::Style {
        background: (if checked { r.accent } else { r.base }).into(),
        icon_color: r.base,
        border: border(if checked { r.accent } else { r.overlay1 }, 1.0, 4.0),
        text_color: Some(r.text),
    }
}

pub fn scroll(_: &Theme, status: scrollable::Status) -> scrollable::Style {
    let r = roles();
    let active = !matches!(status, scrollable::Status::Active { .. });
    let rail = scrollable::Rail {
        background: None,
        border: border(Color::TRANSPARENT, 0.0, 4.0),
        scroller: scrollable::Scroller {
            background: (if active { r.overlay1 } else { alpha(r.overlay0, 0.8) }).into(),
            border: border(Color::TRANSPARENT, 0.0, 4.0),
        },
    };
    scrollable::Style {
        container: container::Style::default(),
        vertical_rail: rail,
        horizontal_rail: rail,
        gap: None,
        auto_scroll: scrollable::default(&Theme::Dark, status).auto_scroll,
    }
}

pub fn progress(_: &Theme) -> progress_bar::Style {
    let r = roles();
    progress_bar::Style {
        background: r.surface2.into(),
        bar: r.accent.into(),
        border: border(Color::TRANSPARENT, 0.0, 3.0),
    }
}

/// Unused import guard: `palette` names the module the Extended types come from.
#[allow(dead_code)]
type _Palette = palette::Palette;

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance(c: Color) -> f32 {
        let lin = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
        0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
    }

    fn contrast(a: Color, b: Color) -> f32 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn text_is_readable_in_both_themes() {
        for r in [GRAPHITE, LAVENDER] {
            for (ground, gname) in [(r.base, "base"), (r.surface0, "surface0"), (r.mantle, "mantle"), (r.surface1, "surface1")] {
                // WCAG AA: 4.5:1 for text, 3:1 for quiet marks and large text
                assert!(contrast(r.text, ground) >= 7.0, "{} text on {gname}: {}", r.name, contrast(r.text, ground));
                assert!(contrast(r.subtext1, ground) >= 4.5, "{} subtext1 on {gname}", r.name);
                assert!(contrast(r.subtext0, ground) >= 4.5, "{} subtext0 on {gname}: {}", r.name, contrast(r.subtext0, ground));
                assert!(contrast(r.overlay1, ground) >= 3.0, "{} overlay1 on {gname}: {}", r.name, contrast(r.overlay1, ground));
            }
            assert!(contrast(r.base, r.accent) >= 4.5, "{}: text on the accent {}", r.name, contrast(r.base, r.accent));
            for (c, n) in [(r.danger, "danger"), (r.success, "success"), (r.warning, "warning")] {
                assert!(contrast(c, r.surface0) >= 3.0, "{} {n}: {}", r.name, contrast(c, r.surface0));
            }
            let k = r.keyboard;
            for (cap, legend, n) in [(k.alpha, k.alpha_legend, "alpha"), (k.modifier, k.modifier_legend, "modifier")] {
                assert!(contrast(cap, legend) >= 2.0, "{} {n} legends: {}", r.name, contrast(cap, legend));
            }
        }
    }

    #[test]
    fn grounds_step_from_the_outside_in() {
        // Dark: crust < mantle < base < surfaces; light: the surfaces are the brightest
        let l = |c: Color| luminance(c);
        assert!(l(GRAPHITE.crust) < l(GRAPHITE.mantle) && l(GRAPHITE.mantle) < l(GRAPHITE.base));
        assert!(l(GRAPHITE.base) < l(GRAPHITE.surface0) && l(GRAPHITE.surface0) < l(GRAPHITE.surface1));
        assert!(l(LAVENDER.crust) < l(LAVENDER.mantle) && l(LAVENDER.mantle) < l(LAVENDER.base));
        assert!(l(LAVENDER.surface0) > l(LAVENDER.base));
    }
}
