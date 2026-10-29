//! Ubuntu, bundled so the app looks the same everywhere (Ubuntu Font Licence, ak980/data/fonts).

use iced::font::Weight;
use iced::Font;

pub const REGULAR: Font = Font::with_name("Ubuntu");
pub const MEDIUM: Font = Font { weight: Weight::Medium, ..REGULAR };
pub const BOLD: Font = Font { weight: Weight::Bold, ..REGULAR };
pub const MONO: Font = Font::with_name("Ubuntu Mono");

pub const FILES: [&[u8]; 4] = [
    include_bytes!("../../../ak980/data/fonts/Ubuntu-R.ttf"),
    include_bytes!("../../../ak980/data/fonts/Ubuntu-M.ttf"),
    include_bytes!("../../../ak980/data/fonts/Ubuntu-B.ttf"),
    include_bytes!("../../../ak980/data/fonts/UbuntuMono-R.ttf"),
];
