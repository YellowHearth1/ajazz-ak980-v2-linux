//! `ak980-gui`: the window of Ajazz AK980 V2 | Software for Linux.

mod app;
mod fonts;
mod keyboard;
mod pages;
mod theme;
mod wheel;
mod widgets;

use iced::window;

fn main() -> iced::Result {
    ak980_core::i18n::set_language(&ak980_core::config::peek_language());
    let mut app = iced::application(app::App::boot, app::App::update, app::App::view)
        .title(app::App::title)
        .theme(app::App::theme)
        .subscription(app::App::subscription)
        .default_font(fonts::REGULAR)
        // The window writes unsaved settings before it goes (App::update, CloseRequested)
        .exit_on_close_request(false)
        .window(window::Settings {
            size: iced::Size::new(1240.0, 820.0),
            min_size: Some(iced::Size::new(980.0, 640.0)),
            // Under Wayland the compositor finds the icon from the application id instead
            icon: image::load_from_memory(include_bytes!("../assets/icon-128.png"))
                .ok()
                .and_then(|i| window::icon::from_rgba(i.to_rgba8().into_raw(), 128, 128).ok()),
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

    /// The pages the README shows, each in the theme it is shown in.
    const SHOTS: [(Page, &str); 8] = [
        (Page::Lighting, "graphite"),
        (Page::Lighting, "lavender"),
        (Page::Colours, "graphite"),
        (Page::Keys, "lavender"),
        (Page::Macros, "lavender"),
        (Page::Screen, "graphite"),
        (Page::Profiles, "lavender"),
        (Page::Device, "graphite"),
    ];

    /// Twelve frames of a slow rainbow with a caption, as a stand-in for someone's GIF.
    fn demo_animation() -> ak980_core::lcd::Source {
        let frames = (0..12)
            .map(|i| {
                let img = image::RgbaImage::from_fn(240, 135, |x, y| {
                    let h = (x as f32 / 240.0 + i as f32 / 12.0) % 1.0;
                    let c = ak980_core::effects::hsv(h, 0.55, 0.35 + 0.65 * (1.0 - y as f32 / 135.0));
                    image::Rgba([(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255])
                });
                (img, 80)
            })
            .collect();
        ak980_core::lcd::Source { frames }
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
        // AK980_SHOTS_PICK=lighting:mocha,device:latte:graphite:yellow renders just those
        // (page:theme[:colourway[:accent]]), in English, to look at a theme
        let pick = std::env::var("AK980_SHOTS_PICK").ok();
        let shots: Vec<(Page, String, String, String)> = match &pick {
            Some(list) => list
                .split(',')
                .filter_map(|item| {
                    let mut parts = item.split(':');
                    let name = parts.next()?;
                    let page = SHOTS.iter().map(|(p, _)| *p).find(|p| format!("{p:?}").eq_ignore_ascii_case(name))?;
                    let mut next = |default: &str| parts.next().unwrap_or(default).to_string();
                    Some((page, next("auto"), next("auto"), next("mauve")))
                })
                .collect(),
            None => SHOTS.iter().map(|(p, t)| (*p, t.to_string(), "auto".into(), "mauve".into())).collect(),
        };
        // AK980_SHOTS_LANG=uk and AK980_SHOTS_WIDTH=980 (the narrowest window) check other cases
        let langs: Vec<String> = match std::env::var("AK980_SHOTS_LANG") {
            Ok(l) => vec![l],
            Err(_) if pick.is_some() => vec!["en".into()],
            Err(_) => vec!["en".into(), "uk".into()],
        };
        let width: f32 = std::env::var("AK980_SHOTS_WIDTH").ok().and_then(|w| w.parse().ok()).unwrap_or(1240.0);
        // AK980_SHOTS_EFFECT=monitor shows that PC effect with the overlays on; AK980_SHOTS_LAUNCH=profile:next
        // edits the selected key as a Program or link with that command
        let effect = std::env::var("AK980_SHOTS_EFFECT").ok();
        let launch = std::env::var("AK980_SHOTS_LAUNCH").ok();
        for lang in &langs {
            ak980_core::i18n::set_language(lang);
            let dir = std::path::Path::new(&out).join(lang);
            let _ = std::fs::remove_dir_all(&dir);
            for (page, theme, colorway, accent) in &shots {
                let (page, theme) = (*page, theme.as_str());
                let (mut app, _) = App::boot();
                demo(&mut app);
                app.cfg.settings.theme = theme.into();
                app.cfg.settings.colorway.clone_from(colorway);
                app.cfg.settings.accent.clone_from(accent);
                // AK980_SHOTS_MODEL=keys draws the AK980 without a screen
                app.cfg.settings.keyboard_model = std::env::var("AK980_SHOTS_MODEL").unwrap_or_else(|_| "screen".into());
                app.apply_theme();
                app.page = page;
                app.keys.draft = app.cfg.profile().keymap_top[&119].clone();
                if page == Page::Lighting && (theme == "lavender" || effect.is_some()) {
                    // The PC effects tab, with a frame of the rainbow wave on the drawing
                    app.cfg.profile_mut().light_source = LightSource::Host;
                    if let Some(e) = &effect {
                        app.cfg.profile_mut().host_effect.clone_from(e);
                        app.cfg.settings.overlay_locks = true;
                        app.cfg.settings.overlay_notifications = true;
                    }
                    let t0 = std::time::Instant::now();
                    crate::pages::lighting::tick(&mut app, t0);
                    crate::pages::lighting::tick(&mut app, t0 + std::time::Duration::from_millis(900));
                }
                if page == Page::Keys && launch.is_some() {
                    app.keys.launch.clone_from(&launch);
                }
                if page == Page::Screen {
                    app.screen.options.text = "12:34".into();
                    crate::pages::screen::show_now(&mut app, demo_animation(), "rainbow.gif".into());
                }
                let settings = iced::Settings {
                    fonts: fonts::FILES.iter().map(|f| Cow::Borrowed(*f)).collect(),
                    default_font: fonts::REGULAR,
                    ..Default::default()
                };
                let iced_theme = app.theme();
                // A picked Device page is drawn whole, so its settings at the bottom show too
                let tall = page == Page::Device || page == Page::Screen || (page == Page::Lighting && effect.is_some());
                let size = if pick.is_some() && tall { (width, if page == Page::Screen { 2600.0 } else { 1780.0 }) } else { (width, 820.0) };
                let mut ui = iced_test::Simulator::with_size(settings, size, app.view());
                let shot = ui.snapshot(&iced_theme).expect("drawn");
                let mut stem = format!("{}-{theme}", format!("{page:?}").to_lowercase());
                if pick.is_some() {
                    stem = format!("{stem}-{colorway}-{accent}");
                }
                assert!(shot.matches_image(dir.join(&stem)).expect("written"));
                // The simulator names the file after its renderer and draws at twice the size
                let written = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path())
                    .find(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with(&format!("{stem}-"))))
                    .expect("the snapshot file");
                let img = image::open(&written).expect("a PNG").to_rgba8();
                let half = image::imageops::resize(&img, img.width() / 2, img.height() / 2, image::imageops::FilterType::Triangle);
                half.save(dir.join(format!("{stem}.png"))).expect("saved");
                std::fs::remove_file(written).unwrap();
            }
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}
