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

/// Pictures of every page in both themes, drawn without a window (`iced_test`, tiny-skia), from a
/// demo profile in a temporary settings folder. `cargo test -p ak980-gui -- --ignored screenshots`;
/// `AK980_SHOTS=dir` picks where the PNG files go (default `target/screenshots`).
#[cfg(test)]
mod screenshots {
    use std::borrow::Cow;

    use ak980_core::config::{Launcher, LightSource, Rule};
    use ak980_core::device::Link;
    use ak980_core::keymap::KeyAction;
    use ak980_core::macros::Macro;

    use crate::app::App;
    use crate::fonts;
    use crate::pages::Page;

    fn demo(app: &mut App) {
        let p = app.cfg.profile_mut();
        p.lighting.mode = 2;
        p.lighting.set_rgb([0xFF, 0xD2, 0x00]);
        for (i, k) in ak980_core::layout::keys().iter().enumerate() {
            let c = ak980_core::effects::hsv(i as f32 / 97.0, 0.8, 1.0).map(|v| (v * 255.0) as u8);
            p.custom_colors.insert(k.index, c);
        }
        p.keymap_top.insert(119, KeyAction::parse("key:PageDown").unwrap());
        p.keymap_fn.insert(2, KeyAction::parse("media:play").unwrap());
        p.macros.push(Macro::parse("text:Hello", Some("Greeting")).unwrap());
        p.macros.push(Macro::parse("combo:ctrl+shift+esc", Some("Task manager")).unwrap());
        app.cfg.settings.rules.push(Rule { process: "steam".into(), profile: app.cfg.active.clone() });
        app.cfg.settings.launchers.push(Launcher { key: "F13".into(), command: "firefox".into(), label: String::new() });
        app.status.link = Some(Link::Usb);
        app.status.access = Some(true);
        app.status.agent = true;
        app.macros.selected = Some(0);
        app.keys.selected = Some(119);
    }

    #[test]
    #[ignore = "writes PNG files; run with --ignored"]
    fn screenshots() {
        let out = std::env::var("AK980_SHOTS").unwrap_or_else(|_| "target/screenshots".into());
        let home = std::env::temp_dir().join(format!("ak980-shots-{}", std::process::id()));
        // SAFETY: set before any other thread of this test reads the environment
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", home.join("config"));
            std::env::set_var("XDG_DATA_HOME", home.join("data"));
            std::env::set_var("XDG_STATE_HOME", home.join("state"));
        }
        let _ = std::fs::remove_dir_all(&out);
        for lang in ["en", "uk"] {
            ak980_core::i18n::set_language(lang);
            for theme in ["graphite", "lavender"] {
                for page in Page::ALL {
                    let (mut app, _) = App::boot();
                    demo(&mut app);
                    if page == Page::Lighting && theme == "lavender" {
                        app.cfg.profile_mut().light_source = LightSource::Custom;
                    }
                    app.cfg.settings.theme = theme.into();
                    app.apply_theme();
                    app.page = page;
                    app.keys.draft = app.cfg.profile().keymap_top[&119].clone();
                    let settings = iced::Settings {
                        fonts: fonts::FILES.iter().map(|f| Cow::Borrowed(*f)).collect(),
                        default_font: fonts::REGULAR,
                        ..Default::default()
                    };
                    let iced_theme = app.theme();
                    let mut ui = iced_test::Simulator::with_size(settings, (1240.0, 820.0), app.view());
                    let shot = ui.snapshot(&iced_theme).expect("drawn");
                    let name = format!("{out}/{lang}/{theme}-{}", format!("{page:?}").to_lowercase());
                    assert!(shot.matches_image(&name).expect("written"));
                }
            }
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}
