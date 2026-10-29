//! The pages of the window, one module each: `view(&App)` and `update(&mut App, Msg)`.

use ak980_core::i18n::tr;

pub mod colours;
pub mod device;
pub mod keys;
pub mod lighting;
pub mod macros;
pub mod profiles;
pub mod screen;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Screen,
    Lighting,
    Colours,
    Keys,
    Macros,
    Profiles,
    Device,
}

impl Page {
    pub const ALL: [Page; 7] =
        [Page::Lighting, Page::Colours, Page::Keys, Page::Macros, Page::Screen, Page::Profiles, Page::Device];

    pub fn title(self) -> String {
        tr(match self {
            Page::Screen => "Screen",
            Page::Lighting => "Lighting",
            Page::Colours => "Colours",
            Page::Keys => "Keys",
            Page::Macros => "Macros",
            Page::Profiles => "Profiles",
            Page::Device => "Device",
        })
    }

    /// Pages whose settings the keyboard only takes over the cable.
    pub fn needs_cable(self) -> bool {
        matches!(self, Page::Screen | Page::Lighting | Page::Colours | Page::Keys | Page::Macros)
    }
}
