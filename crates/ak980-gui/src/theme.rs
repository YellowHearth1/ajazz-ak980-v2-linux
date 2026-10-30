//! The looks of the app: five taken from colourways of the AK980, and Catppuccin's four.
//!
//! *Graphite* is the black case with light-grey and graphite caps and yellow Esc, Enter and space
//! bar; *Lavender* the white case with white and lavender caps and sky-blue Esc, Enter and space
//! bar; *Mint* the black case with white letters, black modifiers and mint Esc, Enter and space bar;
//! *Dusk* the black case with caps that fade from black at the top through grey to pink at the
//! bottom; *Frost* the white case with white letters, light blue-grey modifiers and dark slate Esc,
//! Enter, space bar and arrows. Colours have roles as in Catppuccin's style guide: `crust`, `mantle` and `base` are the
//! grounds from the outermost in, `surface0..2` raised things, `overlay0..2` borders and quiet
//! marks, `text`, `subtext1` and `subtext0` the type, and text on the accent is `base`. Every
//! widget style below reads these roles and nothing else, so a colour cannot drift per widget.
//!
//! The Catppuccin flavours use the official palette (github.com/catppuccin/palette, MIT). Where a
//! colour would read below WCAG AA in these roles (Latte's light accents, Frappé's subtext), it is
//! moved towards the text colour just far enough, never further. The keyboard's colourway is chosen
//! apart from the theme; the stage under the drawing adapts so the case always stands out.

use std::sync::RwLock;

use iced::theme::palette::{Background, Danger, Extended, Pair, Primary, Secondary, Success, Warning};
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
    /// Caps that fade from the top row to the bottom one (top, middle, bottom) instead of the three
    /// kinds of cap.
    pub gradient: Option<[Color; 3]>,
    /// The arrows wear the accent too.
    pub accent_arrows: bool,
    /// The numpad's Enter wears the accent; otherwise it is a modifier.
    pub accent_numpad_enter: bool,
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
    /// Primary actions and the selection, as a fill.
    pub accent: Color,
    /// Text and icons on an accent fill.
    pub on_accent: Color,
    /// The accent as a line or mark on the page (focus, sliders, switches): 3:1 off the grounds.
    pub accent_mark: Color,
    /// The second colour of the colourway (graphite caps, sky-blue caps), for quiet highlights.
    pub second: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub keyboard: Colorway,
    /// The keyboard drawn has the screen and the knob; otherwise Insert, Print, Scroll and Pause.
    pub screen: bool,
}

const fn hex(v: u32) -> Color {
    Color::from_rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// The black AK980 V2: light-grey letters, graphite modifiers, yellow Esc, Enter and space bar.
pub const GRAPHITE_KEYS: Colorway = Colorway {
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
    gradient: None,
    accent_arrows: false,
    accent_numpad_enter: true,
};

/// The white AK980 V2: white letters with lavender legends, lavender modifiers, sky-blue accents.
pub const LAVENDER_KEYS: Colorway = Colorway {
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
    gradient: None,
    accent_arrows: false,
    accent_numpad_enter: true,
};

/// The black AK980 with white letters, black modifiers and mint Esc, Enter and space bar.
pub const MINT_KEYS: Colorway = Colorway {
    case: hex(0x1D1E20),
    case_edge: hex(0x34363A),
    alpha: hex(0xF1F1EE),
    alpha_side: hex(0xCFCFCB),
    alpha_legend: hex(0x1B1B1D),
    modifier: hex(0x2B2C2F),
    modifier_side: hex(0x1F2022),
    modifier_legend: hex(0xEDEDEA),
    accent: hex(0x93D3B8),
    accent_side: hex(0x73B89B),
    accent_legend: hex(0xFFFFFF),
    knob: hex(0x202123),
    gradient: None,
    accent_arrows: false,
    accent_numpad_enter: false,
};

/// The black AK980 whose caps fade from black at the top through grey to pink at the bottom.
pub const DUSK_KEYS: Colorway = Colorway {
    case: hex(0x1A191C),
    case_edge: hex(0x33303A),
    alpha: hex(0x6F6878),
    alpha_side: hex(0x564F5E),
    alpha_legend: hex(0x2E2A33),
    modifier: hex(0x6F6878),
    modifier_side: hex(0x564F5E),
    modifier_legend: hex(0x2E2A33),
    accent: hex(0xF3BAC4),
    accent_side: hex(0xD99AA6),
    accent_legend: hex(0xC98E9A),
    knob: hex(0x1E1C20),
    gradient: Some([hex(0x2B292E), hex(0x766E7E), hex(0xF5BCC6)]),
    accent_arrows: false,
    accent_numpad_enter: true,
};

/// The white AK980 with white letters, light blue-grey modifiers and dark slate Esc, Enter, space
/// bar, arrows and numpad Enter.
pub const FROST_KEYS: Colorway = Colorway {
    case: hex(0xF3F4F6),
    case_edge: hex(0xD8DCE2),
    alpha: hex(0xFCFCFC),
    alpha_side: hex(0xE1E4E9),
    alpha_legend: hex(0x2B2E36),
    modifier: hex(0xCADCE8),
    modifier_side: hex(0xA9C2D3),
    modifier_legend: hex(0x2B2E36),
    accent: hex(0x4B5160),
    accent_side: hex(0x383D49),
    accent_legend: hex(0xF4F6F8),
    knob: hex(0xF0F1F3),
    gradient: None,
    accent_arrows: true,
    accent_numpad_enter: true,
};

/// WCAG relative luminance of an opaque colour.
pub fn luminance(c: Color) -> f32 {
    let lin = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG contrast ratio, 1 to 21.
pub fn contrast(a: Color, b: Color) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// `fg`, moved towards `toward` in small steps until it stands `min`:1 off every one of
/// `grounds`; unchanged when it already does.
pub fn readable(fg: Color, toward: Color, grounds: &[Color], min: f32) -> Color {
    (0..=20)
        .map(|step| mix(fg, toward, step as f32 / 20.0))
        .find(|c| grounds.iter().all(|g| contrast(*c, *g) >= min))
        .unwrap_or(toward)
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
    on_accent: hex(0x1B1C20),
    accent_mark: hex(0xF2CF2E),
    second: hex(0x7B7C7F),
    success: hex(0x8FCF7A),
    warning: hex(0xF0A35A),
    danger: hex(0xF07167),
    keyboard: GRAPHITE_KEYS,
    screen: true,
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
    on_accent: hex(0xF7F6FB),
    accent_mark: hex(0x6F5CC9),
    second: hex(0xA9CBE3),
    success: hex(0x2E8B57),
    warning: hex(0xA86200),
    danger: hex(0xC4364C),
    keyboard: LAVENDER_KEYS,
    screen: true,
};

pub const MINT: Roles = Roles {
    name: "mint",
    dark: true,
    crust: hex(0x0E0F10),
    mantle: hex(0x141517),
    base: hex(0x191B1D),
    surface0: hex(0x222427),
    surface1: hex(0x2C2F33),
    surface2: hex(0x383C41),
    overlay0: hex(0x50555B),
    overlay1: hex(0x7C8288),
    subtext0: hex(0xA9AEB2),
    subtext1: hex(0xC4C8CB),
    text: hex(0xE8EAE8),
    accent: hex(0x8FD3B6),
    on_accent: hex(0x191B1D),
    accent_mark: hex(0x8FD3B6),
    second: hex(0xF1F1EE),
    success: hex(0x8FCF7A),
    warning: hex(0xF0A35A),
    danger: hex(0xF07167),
    keyboard: MINT_KEYS,
    screen: true,
};

pub const DUSK: Roles = Roles {
    name: "dusk",
    dark: true,
    crust: hex(0x121014),
    mantle: hex(0x18151A),
    base: hex(0x1E1A20),
    surface0: hex(0x28232B),
    surface1: hex(0x322C36),
    surface2: hex(0x3E3743),
    overlay0: hex(0x59505F),
    overlay1: hex(0x86798C),
    subtext0: hex(0xB3A6B5),
    subtext1: hex(0xCBBFCB),
    text: hex(0xF0E6EB),
    accent: hex(0xF2A9B8),
    on_accent: hex(0x1E1A20),
    accent_mark: hex(0xF2A9B8),
    second: hex(0x766E7E),
    success: hex(0x9AD08A),
    warning: hex(0xF2B266),
    danger: hex(0xF47A7A),
    keyboard: DUSK_KEYS,
    screen: true,
};

pub const FROST: Roles = Roles {
    name: "frost",
    dark: false,
    crust: hex(0xE2E8EE),
    mantle: hex(0xECF0F4),
    base: hex(0xF5F7FA),
    surface0: hex(0xFFFFFF),
    surface1: hex(0xEDF2F6),
    surface2: hex(0xDFE7EE),
    overlay0: hex(0xB7C4CF),
    overlay1: hex(0x6E7C89),
    subtext0: hex(0x4D5966),
    subtext1: hex(0x3D4854),
    text: hex(0x222932),
    accent: hex(0x4A5363),
    on_accent: hex(0xF5F7FA),
    accent_mark: hex(0x4A5363),
    second: hex(0xCADCE8),
    success: hex(0x2E7D4F),
    warning: hex(0x9A5B00),
    danger: hex(0xC0392B),
    keyboard: FROST_KEYS,
    screen: true,
};

/// A Catppuccin flavour: the 14 accents in `ACCENTS` order, then text, subtext1, subtext0,
/// overlay2, overlay1, overlay0, surface2, surface1, surface0, base, mantle and crust.
pub struct Flavour {
    pub id: &'static str,
    pub dark: bool,
    pub colors: [u32; 26],
}

/// Catppuccin's accents, in its order; the names are English and translated where shown.
pub const ACCENTS: [(&str, &str); 14] = [
    ("rosewater", "Rosewater"), ("flamingo", "Flamingo"), ("pink", "Pink"), ("mauve", "Mauve"), ("red", "Red"),
    ("maroon", "Maroon"), ("peach", "Peach"), ("yellow", "Yellow"), ("green", "Green"), ("teal", "Teal"),
    ("sky", "Sky"), ("sapphire", "Sapphire"), ("blue", "Blue"), ("lavender", "Lavender"),
];

#[rustfmt::skip]
pub const FLAVOURS: [Flavour; 4] = [
    Flavour { id: "latte", dark: false, colors: [
        0xdc8a78, 0xdd7878, 0xea76cb, 0x8839ef, 0xd20f39, 0xe64553, 0xfe640b, 0xdf8e1d, 0x40a02b, 0x179299,
        0x04a5e5, 0x209fb5, 0x1e66f5, 0x7287fd, 0x4c4f69, 0x5c5f77, 0x6c6f85, 0x7c7f93, 0x8c8fa1, 0x9ca0b0,
        0xacb0be, 0xbcc0cc, 0xccd0da, 0xeff1f5, 0xe6e9ef, 0xdce0e8] },
    Flavour { id: "frappe", dark: true, colors: [
        0xf2d5cf, 0xeebebe, 0xf4b8e4, 0xca9ee6, 0xe78284, 0xea999c, 0xef9f76, 0xe5c890, 0xa6d189, 0x81c8be,
        0x99d1db, 0x85c1dc, 0x8caaee, 0xbabbf1, 0xc6d0f5, 0xb5bfe2, 0xa5adce, 0x949cbb, 0x838ba7, 0x737994,
        0x626880, 0x51576d, 0x414559, 0x303446, 0x292c3c, 0x232634] },
    Flavour { id: "macchiato", dark: true, colors: [
        0xf4dbd6, 0xf0c6c6, 0xf5bde6, 0xc6a0f6, 0xed8796, 0xee99a0, 0xf5a97f, 0xeed49f, 0xa6da95, 0x8bd5ca,
        0x91d7e3, 0x7dc4e4, 0x8aadf4, 0xb7bdf8, 0xcad3f5, 0xb8c0e0, 0xa5adcb, 0x939ab7, 0x8087a2, 0x6e738d,
        0x5b6078, 0x494d64, 0x363a4f, 0x24273a, 0x1e2030, 0x181926] },
    Flavour { id: "mocha", dark: true, colors: [
        0xf5e0dc, 0xf2cdcd, 0xf5c2e7, 0xcba6f7, 0xf38ba8, 0xeba0ac, 0xfab387, 0xf9e2af, 0xa6e3a1, 0x94e2d5,
        0x89dceb, 0x74c7ec, 0x89b4fa, 0xb4befe, 0xcdd6f4, 0xbac2de, 0xa6adc8, 0x9399b2, 0x7f849c, 0x6c7086,
        0x585b70, 0x45475a, 0x313244, 0x1e1e2e, 0x181825, 0x11111b] },
];

/// The roles of a Catppuccin flavour with one of its accents.
fn catppuccin(f: &Flavour, accent: &str) -> Roles {
    let c = |i: usize| hex(f.colors[i]);
    let (text, overlay0, surface0, base, mantle, crust) = (c(14), c(19), c(22), c(23), c(24), c(25));
    let mut r = Roles {
        name: f.id,
        dark: f.dark,
        crust,
        mantle,
        base,
        // Raised things as in Catppuccin, with the controls half a step between surface0 and surface1
        surface0,
        surface1: mix(surface0, c(21), 0.5),
        surface2: c(21),
        overlay0: c(20),
        overlay1: c(17),
        subtext0: c(16),
        subtext1: c(15),
        text,
        accent: c(ACCENTS.iter().position(|(id, _)| *id == accent).unwrap_or(3)),
        on_accent: base,
        accent_mark: base,
        second: c(13),
        success: c(8),
        warning: c(6),
        danger: c(4),
        keyboard: if f.dark { GRAPHITE_KEYS } else { LAVENDER_KEYS },
        screen: true,
    };
    if !f.dark {
        // Latte: cards brighter than the page, as in Lavender; Latte's own surfaces become the controls
        (r.mantle, r.base, r.surface0, r.surface1, r.surface2, r.overlay0) = (crust, mantle, base, crust, surface0, overlay0);
    }
    // Moved towards the text colour only as far as WCAG AA asks: 4.5:1 for text, 3:1 for marks
    let grounds = [r.base, r.mantle, r.surface0, r.surface1];
    r.subtext1 = readable(r.subtext1, text, &grounds, 4.5);
    r.subtext0 = readable(r.subtext0, text, &grounds, 4.5);
    r.overlay1 = readable(r.overlay1, text, &grounds, 3.0);
    for status in [&mut r.success, &mut r.warning, &mut r.danger] {
        *status = readable(*status, text, &[r.surface0], 3.0);
    }
    // A fill keeps the palette's accent and takes `base` (Catppuccin's text on accents) or, when
    // that is too faint, the darkest or lightest text that reads at 4.5:1; only if neither does is
    // the fill itself moved, away from `base`. Shading towards black or white keeps the hue.
    let (dark_end, light_end) = (Color::BLACK, Color::WHITE);
    let away = if f.dark { light_end } else { dark_end };
    let other = readable(text, if f.dark { light_end } else { dark_end }, &[r.accent], 4.5);
    if contrast(r.base, r.accent) >= 4.5 {
        r.on_accent = r.base;
    } else if contrast(other, r.accent) >= 4.5 {
        r.on_accent = other;
    } else {
        r.accent = readable(r.accent, away, &[r.base], 4.5);
        r.on_accent = r.base;
    }
    // Thin marks on the page need 3:1 against every ground they sit on
    r.accent_mark = readable(c(ACCENTS.iter().position(|(id, _)| *id == accent).unwrap_or(3)),
                             if f.dark { light_end } else { dark_end }, &[r.base, r.mantle, r.surface0], 3.0);
    r
}

/// The themes in the order the window offers them; "auto" follows the system's light or dark
/// preference.
pub const THEMES: [(&str, &str); 10] = [
    ("auto", "As the system"),
    ("graphite", "Graphite"),
    ("lavender", "Lavender"),
    ("mint", "Mint"),
    ("dusk", "Dusk"),
    ("frost", "Frost"),
    ("latte", "Catppuccin Latte"),
    ("frappe", "Catppuccin Frappé"),
    ("macchiato", "Catppuccin Macchiato"),
    ("mocha", "Catppuccin Mocha"),
];

/// How the keyboard is drawn: the colourway that suits the theme, or one of the AK980's.
pub const COLORWAYS: [(&str, &str); 6] = [("auto", "As the theme"), ("graphite", "Graphite"), ("lavender", "Lavender"),
                                          ("mint", "Mint"), ("dusk", "Dusk"), ("frost", "Frost")];

/// Which AK980 is drawn: the V2 with its screen and knob, or the one with Insert, Print Screen,
/// Scroll Lock and Pause in their place (only the drawing changes).
pub const MODELS: [(&str, &str); 2] = [("screen", "Screen and knob"), ("keys", "Insert … Pause keys")];

/// The themes made from a colourway, which keep it under "As the theme".
const OWN: [(&str, Roles); 5] = [("graphite", GRAPHITE), ("lavender", LAVENDER), ("mint", MINT), ("dusk", DUSK),
                                 ("frost", FROST)];

pub fn is_catppuccin(theme: &str) -> bool {
    FLAVOURS.iter().any(|f| f.id == theme)
}

pub fn roles_for(theme: &str, accent: &str, colorway: &str, model: &str, system_dark: bool) -> Roles {
    let mut r = match OWN.iter().find(|(id, _)| *id == theme) {
        Some((_, own)) => *own,
        None => match FLAVOURS.iter().find(|f| f.id == theme) {
            Some(f) => catppuccin(f, accent),
            None if system_dark => GRAPHITE,
            None => LAVENDER,
        },
    };
    if let Some((_, own)) = OWN.iter().find(|(id, _)| *id == colorway) {
        r.keyboard = own.keyboard;
    } else if !OWN.iter().any(|(_, own)| own.name == r.name) {
        r.keyboard = if r.dark { GRAPHITE_KEYS } else { LAVENDER_KEYS };
    }
    r.screen = model != "keys";
    r
}

/// The theme made from a keyboard colourway, for "Match the theme to the keyboard".
pub fn theme_for_colorway(colorway: &str) -> Option<&'static str> {
    OWN.iter().find(|(id, _)| *id == colorway).map(|(id, _)| *id)
}

/// How far the case's colour must stand off the stage for the case to read as a shape.
const STAGE_SEPARATION: f32 = 1.4;

/// The ground under the keyboard drawing: the theme's quiet ground, unless the case would melt
/// into it; then the theme's ground nearest to it that sets the case apart.
pub fn stage_ground(r: &Roles) -> Color {
    let case = r.keyboard.case;
    let quiet = mix(r.base, r.crust, 0.6);
    if contrast(case, quiet) >= STAGE_SEPARATION {
        return quiet;
    }
    let grounds = [r.base, r.mantle, r.crust, r.surface0, r.surface1, r.surface2, r.overlay0];
    grounds
        .into_iter()
        .filter(|g| contrast(case, *g) >= STAGE_SEPARATION)
        .min_by(|a, b| contrast(*a, quiet).total_cmp(&contrast(*b, quiet)))
        .unwrap_or_else(|| grounds.into_iter().max_by(|a, b| contrast(case, *a).total_cmp(&contrast(case, *b))).unwrap_or(quiet))
}

/// The case's outline: its edge colour moved towards the theme's text until it stands 3:1 off the
/// stage, WCAG's contrast for the outline of a graphic.
pub fn case_outline(r: &Roles) -> Color {
    readable(r.keyboard.case_edge, r.text, &[stage_ground(r)], 3.0)
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
    let (on_accent, on_status) = (r.on_accent, r.base);
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
        success: Success { base: pair(r.success, on_status), weak: pair(weak(r.success), r.text),
                           strong: pair(mix(r.success, r.text, 0.15), on_status) },
        warning: Warning { base: pair(r.warning, on_status), weak: pair(weak(r.warning), r.text),
                           strong: pair(mix(r.warning, r.text, 0.15), on_status) },
        danger: Danger { base: pair(r.danger, on_status), weak: pair(weak(r.danger), r.text),
                         strong: pair(mix(r.danger, r.text, 0.15), on_status) },
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

/// The stage the keyboard drawing sits on, set off from the case whatever the colourway.
pub fn stage(_: &Theme) -> container::Style {
    let r = roles();
    container::Style {
        background: Some(stage_ground(&r).into()),
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
    button_base(Some(bg), if status == button::Status::Disabled { alpha(r.on_accent, 0.7) } else { r.on_accent }, None)
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
        let tint = mix(r.mantle, r.accent, if r.dark { 0.16 } else { 0.14 });
        let bg = match (selected, status) {
            (true, _) => Some(tint),
            (false, button::Status::Hovered) => Some(r.surface1),
            (false, button::Status::Pressed) => Some(r.surface2),
            _ => None,
        };
        let text = if selected {
            let start = if r.dark { r.accent_mark } else { mix(r.accent_mark, r.text, 0.35) };
            readable(start, r.text, &[tint], 4.5)
        } else {
            r.text
        };
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
        text_input::Status::Focused { .. } => r.accent_mark,
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
        pick_list::Status::Opened { .. } => r.accent_mark,
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
        selected_text_color: r.on_accent,
        selected_background: r.accent.into(),
        shadow: Shadow { color: alpha(Color::BLACK, 0.25), offset: Vector::new(0.0, 4.0), blur_radius: 12.0 },
    }
}

pub fn slider_style(_: &Theme, status: slider::Status) -> slider::Style {
    let r = roles();
    let handle = match status {
        slider::Status::Hovered | slider::Status::Dragged => mix(r.accent_mark, r.text, 0.1),
        _ => r.accent_mark,
    };
    slider::Style {
        rail: slider::Rail {
            backgrounds: (r.accent_mark.into(), r.surface2.into()),
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
        background: (if on { r.accent_mark } else if hovered { r.overlay0 } else { r.surface2 }).into(),
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
        background: (if checked { r.accent_mark } else { r.base }).into(),
        icon_color: r.base,
        border: border(if checked { r.accent_mark } else { r.overlay1 }, 1.0, 4.0),
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
        bar: r.accent_mark.into(),
        border: border(Color::TRANSPARENT, 0.0, 3.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_readable_in_both_themes() {
        for r in OWN.map(|(_, r)| r) {
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

    /// Every theme the window offers, with every accent a Catppuccin flavour can take.
    fn every_theme() -> Vec<Roles> {
        let mut all: Vec<Roles> = OWN.iter().map(|(_, r)| *r).collect();
        for f in &FLAVOURS {
            all.extend(ACCENTS.iter().map(|(a, _)| roles_for(f.id, a, "auto", "screen", true)));
        }
        all
    }

    #[test]
    fn every_theme_and_accent_meets_wcag_aa() {
        for r in every_theme() {
            let n = format!("{} with accent {:?}", r.name, r.accent);
            for (ground, g) in [(r.base, "base"), (r.mantle, "mantle"), (r.surface0, "surface0"), (r.surface1, "surface1")] {
                for (fg, f, min) in [(r.text, "text", 4.5), (r.subtext1, "subtext1", 4.5), (r.subtext0, "subtext0", 4.5),
                                     (r.overlay1, "overlay1", 3.0)] {
                    assert!(contrast(fg, ground) >= min, "{n}: {f} on {g} is {:.2}", contrast(fg, ground));
                }
            }
            // Hovered buttons and toasts sit on surface2
            assert!(contrast(r.text, r.surface2) >= 4.5, "{n}: text on surface2 {:.2}", contrast(r.text, r.surface2));
            assert!(contrast(r.on_accent, r.accent) >= 4.5, "{n}: text on the accent {:.2}", contrast(r.on_accent, r.accent));
            for (ground, g) in [(r.base, "base"), (r.mantle, "mantle"), (r.surface0, "surface0")] {
                assert!(contrast(r.accent_mark, ground) >= 3.0, "{n}: accent mark on {g} {:.2}", contrast(r.accent_mark, ground));
            }
            for (c, s) in [(r.danger, "danger"), (r.success, "success"), (r.warning, "warning")] {
                assert!(contrast(c, r.surface0) >= 3.0, "{n}: {s} {:.2}", contrast(c, r.surface0));
            }
        }
    }

    #[test]
    fn catppuccin_keeps_its_colours_where_it_can() {
        // Mocha needs no change, so its colours are the palette's own
        let mocha = roles_for("mocha", "mauve", "auto", "screen", true);
        assert_eq!((mocha.accent, mocha.text, mocha.base), (hex(0xcba6f7), hex(0xcdd6f4), hex(0x1e1e2e)));
        // Latte's yellow keeps its colour as a fill and gets dark text; its marks are a darker yellow
        let latte = roles_for("latte", "yellow", "auto", "screen", false);
        assert_eq!(latte.accent, hex(0xdf8e1d));
        assert!(luminance(latte.on_accent) < luminance(latte.accent) && luminance(latte.accent_mark) < luminance(latte.accent));
        assert!(latte.accent_mark.r > latte.accent_mark.b, "the mark keeps the hue: {:?}", latte.accent_mark);
        // Latte's blue reads under neither white nor black at 4.5:1, so only that fill is darkened
        let blue = roles_for("latte", "blue", "auto", "screen", false);
        assert!(luminance(blue.accent) < luminance(hex(0x1e66f5)) && blue.accent.b > blue.accent.r);
        assert!(!latte.dark && roles_for("latte", "nonsense", "auto", "screen", false).accent != Color::BLACK);
    }

    #[test]
    fn the_keyboard_stands_out_on_every_theme() {
        for theme in THEMES.iter().map(|(t, _)| *t).filter(|t| *t != "auto") {
            for (colorway, _) in COLORWAYS {
                let r = roles_for(theme, "mauve", colorway, "screen", true);
                let ground = stage_ground(&r);
                assert!(contrast(r.keyboard.case, ground) >= STAGE_SEPARATION, "{theme}/{colorway}: case on stage {:.2}",
                        contrast(r.keyboard.case, ground));
                assert!(contrast(case_outline(&r), ground) >= 3.0, "{theme}/{colorway}: outline {:.2}",
                        contrast(case_outline(&r), ground));
            }
        }
        // "As the theme" keeps the pairing the two own themes were made with
        assert_eq!(roles_for("graphite", "mauve", "auto", "screen", true).keyboard.case, GRAPHITE_KEYS.case);
        assert_eq!(roles_for("latte", "mauve", "auto", "screen", false).keyboard.case, LAVENDER_KEYS.case);
        assert_eq!(roles_for("dusk", "mauve", "auto", "screen", true).keyboard.case, DUSK_KEYS.case);
        assert_eq!(roles_for("frost", "mauve", "mint", "keys", true).keyboard.case, MINT_KEYS.case);
        assert!(!roles_for("frost", "mauve", "mint", "keys", true).screen);
    }

    #[test]
    fn every_label_is_translated() {
        // These tables are translated where they are shown, so the source scan in ak980-core misses them
        let uk = ak980_core::i18n::catalog("uk").expect("the Ukrainian catalog");
        for (_, label) in THEMES.iter().chain(&COLORWAYS).chain(&ACCENTS).chain(&MODELS) {
            assert!(uk.contains_key(*label), "{label:?} has no Ukrainian translation");
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
