//! `ak980-gui`: the window of Ajazz AK980 V2 | Software for Linux.

mod app;
mod fonts;
mod keyboard;
mod pages;
mod theme;
mod widgets;

use iced::window;

fn main() -> iced::Result {
    ak980_core::i18n::set_language(&ak980_core::config::peek_language());
    let mut app = iced::application(app::App::boot, app::App::update, app::App::view)
        .title(app::App::title)
        .theme(app::App::theme)
        .subscription(app::App::subscription)
        .default_font(fonts::REGULAR)
        .window(window::Settings {
            size: iced::Size::new(1240.0, 820.0),
            min_size: Some(iced::Size::new(980.0, 640.0)),
            platform_specific: window::settings::PlatformSpecific {
                application_id: ak980_core::APP_ID.to_string(),
                ..Default::default()
            },
            ..Default::default()
        });
    for font in fonts::FILES {
        app = app.font(font);
    }
    app.run()
}
