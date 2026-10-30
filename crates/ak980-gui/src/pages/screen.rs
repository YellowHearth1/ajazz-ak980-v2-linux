//! Screen: a GIF, picture or folder of frames for the 240×135 display, previewed in the colours
//! the panel shows, and uploaded only after the user confirms it, because the keyboard cannot give
//! the old animation back.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use ak980_core::device::{WiredKeyboard, LCD_HEIGHT, LCD_MAX_FRAMES, LCD_WIDTH};
use ak980_core::i18n::{plural, tr};
use ak980_core::lcd::{self, Fit, Order, RenderOptions, Source, Tone, IMAGE_EXTS, SECONDS_PER_PAGE, TEXT_POSITIONS};
use ak980_core::tr;
use iced::futures::channel::mpsc;
use iced::widget::{button, column, container, image, mouse_area, opaque, progress_bar, row, scrollable, text, text_input,
                   toggler};
use iced::{Alignment, Color, Element, Fill, Length, Task};

use crate::app::{blocking, App, Message};
use crate::fonts;
use crate::pages::Page;
use crate::theme::{self, roles};
use crate::widgets::{self, caption, card, color_picker, section, segmented, setting, Choice};

#[derive(Default)]
pub struct State {
    pub path: Option<PathBuf>,
    source: Option<Arc<Source>>,
    pub options: RenderOptions,
    frames: Vec<(image::Handle, u32)>,
    /// The source frame each shown frame comes from, which frame deletions and delays refer to.
    indices: Vec<usize>,
    pub available: usize,
    index: usize,
    next_at: Option<Instant>,
    pub paused: bool,
    rendering: bool,
    rerender: bool,
    pub loading: bool,
    pub confirm: bool,
    pub upload: Option<f32>,
    upload_started: Option<Instant>,
    cancel: Option<Arc<AtomicBool>>,
    pub history: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Open(bool),
    Picked(Option<PathBuf>),
    Loaded(PathBuf, Result<Arc<Source>, String>),
    Rendered(Vec<Frame>, Vec<usize>, usize),
    /// A frame picked on the strip: it is shown, paused, to be edited.
    Frame(usize),
    DeleteFrame,
    RestoreFrames,
    Fit(Fit),
    Rotate(u8),
    Zoom(i32),
    PanX(i32),
    PanY(i32),
    Sharp(bool),
    Tone(Tone),
    Hue(i32),
    Order(Order),
    Scroll(bool),
    ScrollSpeed(i32),
    Outline(bool),
    FlipH(bool),
    FlipV(bool),
    Brightness(i32),
    Contrast(i32),
    Saturation(i32),
    Speed(i32),
    FixedDelay(i32),
    TrimStart(i32),
    TrimEnd(i32),
    Step(i32),
    Text(String),
    TextPos(Choice<&'static str>),
    TextSize(i32),
    Dither(bool),
    Pause(bool),
    AskUpload,
    CancelConfirm,
    Upload,
    Progress(f32),
    Uploaded(Result<String, String>),
    Stop,
    History(Vec<PathBuf>),
    PickHistory(PathBuf),
}

/// A frame as the panel shows it, RGBA, and its delay in ms.
pub type Frame = (Vec<u8>, u32);

fn wrap(m: Msg) -> Message {
    Message::Screen(m)
}

pub fn load_history() -> Task<Message> {
    Task::perform(blocking(lcd::history_entries), |h| wrap(Msg::History(h.unwrap_or_default())))
}

fn load(app: &mut App, path: PathBuf) -> Task<Message> {
    app.screen.loading = true;
    let p = path.clone();
    Task::perform(blocking(move || Source::load(&p, 1000).map(Arc::new).map_err(|e| e.to_string())), move |r| {
        wrap(Msg::Loaded(path.clone(), r.unwrap_or_else(|| Err("the worker stopped".into()))))
    })
}

/// The frames as the panel shows them (RGBA), their source frames, and how many frames the
/// options select in all.
fn preview(src: &Source, o: &RenderOptions) -> (Vec<Frame>, Vec<usize>, usize) {
    let picked = src.select(o, LCD_MAX_FRAMES);
    let shown = (0..picked.len())
        .map(|k| {
            let (rgb, d) = (lcd::preview_565(&src.render_frame(&picked, k, o), o.dither), &picked[k].1);
            let rgba: Vec<u8> = rgb.pixels().flat_map(|p| [p.0[0], p.0[1], p.0[2], 255]).collect();
            (rgba, *d)
        })
        .collect::<Vec<_>>();
    (shown, picked.into_iter().map(|(i, _)| i).collect(), src.available(o))
}

/// A delay typed for the frame shown, in ms.
pub fn set_frame_delay(app: &mut App, ms: u32) -> Task<Message> {
    let Some(&i) = app.screen.indices.get(app.screen.index) else { return Task::none() };
    app.screen.options.delays.insert(i, ms.clamp(lcd::MIN_DELAY, lcd::MAX_DELAY));
    render(app)
}

/// Shows `src` at once, without a background task (for the screenshots).
#[cfg(test)]
pub fn show_now(app: &mut App, src: Source, path: PathBuf) {
    let (frames, indices, available) = preview(&src, &app.screen.options);
    app.screen.source = Some(Arc::new(src));
    app.screen.path = Some(path);
    let _ = update(app, Msg::Rendered(frames, indices, available));
}

fn render(app: &mut App) -> Task<Message> {
    let Some(src) = app.screen.source.clone() else { return Task::none() };
    if app.screen.rendering {
        app.screen.rerender = true;
        return Task::none();
    }
    app.screen.rendering = true;
    let o = app.screen.options.clone();
    Task::perform(
        blocking(move || preview(&src, &o)),
        |r| {
            let (frames, indices, available) = r.unwrap_or_default();
            wrap(Msg::Rendered(frames, indices, available))
        },
    )
}

pub fn set_text_color(app: &mut App, c: [u8; 3]) -> Task<Message> {
    app.screen.options.text_color = c;
    render(app)
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let s = &mut app.screen;
    let o = &mut s.options;
    match msg {
        Msg::Open(folder) => {
            let dialog = rfd::AsyncFileDialog::new().set_title(tr("Open image"));
            return if folder {
                Task::perform(async move { dialog.pick_folder().await.map(|h| h.path().to_path_buf()) }, |p| wrap(Msg::Picked(p)))
            } else {
                let dialog = dialog.add_filter(tr("Images"), &IMAGE_EXTS);
                Task::perform(async move { dialog.pick_file().await.map(|h| h.path().to_path_buf()) }, |p| wrap(Msg::Picked(p)))
            };
        }
        Msg::Picked(Some(path)) | Msg::PickHistory(path) => return load(app, path),
        Msg::Picked(None) => return Task::none(),
        Msg::Loaded(path, result) => {
            s.loading = false;
            match result {
                Ok(src) => {
                    s.source = Some(src);
                    s.path = Some(path);
                    s.options.deleted.clear();
                    s.options.delays.clear();
                    s.index = 0;
                    return render(app);
                }
                Err(e) => {
                    app.notify(tr!("Could not open it: {error}", error = e), true);
                    return Task::none();
                }
            }
        }
        Msg::Rendered(frames, indices, available) => {
            s.rendering = false;
            s.indices = indices;
            s.frames = frames
                .into_iter()
                .map(|(rgba, d)| (image::Handle::from_rgba(LCD_WIDTH as u32, LCD_HEIGHT as u32, rgba), d))
                .collect();
            s.available = available;
            s.index = s.index.min(s.frames.len().saturating_sub(1));
            if std::mem::take(&mut s.rerender) {
                return render(app);
            }
            return Task::none();
        }
        Msg::Frame(i) => {
            s.index = i.min(s.frames.len().saturating_sub(1));
            s.paused = true;
            app.drafts.remove("frame-delay");
            return Task::none();
        }
        Msg::DeleteFrame => {
            // The last frame stays: an animation needs one
            match s.indices.get(s.index) {
                Some(&i) if s.indices.len() > 1 => {
                    o.deleted.insert(i);
                }
                _ => return Task::none(),
            }
        }
        Msg::RestoreFrames => o.deleted.clear(),
        Msg::Fit(f) => o.fit = f,
        Msg::Rotate(r) => o.rotate = r % 4,
        Msg::Zoom(v) => o.zoom = v as f32 / 100.0,
        Msg::PanX(v) => o.pan.0 = v as f32 / 100.0,
        Msg::PanY(v) => o.pan.1 = v as f32 / 100.0,
        Msg::Sharp(v) => o.sharp = v,
        Msg::Tone(t) => o.tone = t,
        Msg::Hue(v) => o.hue = v,
        Msg::Order(v) => o.order = v,
        Msg::Scroll(on) => o.text_scroll = on.then_some(6),
        Msg::ScrollSpeed(v) => o.text_scroll = Some(v.clamp(1, 20) as u32),
        Msg::Outline(v) => o.text_outline = v,
        Msg::FlipH(v) => o.flip_h = v,
        Msg::FlipV(v) => o.flip_v = v,
        Msg::Brightness(v) => o.brightness = v as f32 / 100.0,
        Msg::Contrast(v) => o.contrast = v as f32 / 100.0,
        Msg::Saturation(v) => o.saturation = v as f32 / 100.0,
        Msg::Speed(v) => o.speed = v as f32 / 100.0,
        Msg::FixedDelay(v) => o.fixed_delay = (v > 0).then_some(v as u32),
        Msg::TrimStart(v) => o.trim_start = v.max(0) as usize,
        Msg::TrimEnd(v) => o.trim_end = v.max(0) as usize,
        Msg::Step(v) => o.frame_step = v.max(1) as usize,
        Msg::Text(t) => o.text = t,
        Msg::TextPos(p) => o.text_pos = p.value.to_string(),
        Msg::TextSize(v) => o.text_size = v.max(6) as u32,
        Msg::Dither(v) => o.dither = v,
        Msg::Pause(v) => {
            s.paused = v;
            return Task::none();
        }
        Msg::AskUpload => {
            s.confirm = true;
            return Task::none();
        }
        Msg::CancelConfirm => {
            s.confirm = false;
            return Task::none();
        }
        Msg::Upload => return start_upload(app),
        Msg::Progress(p) => {
            s.upload = Some(p);
            return Task::none();
        }
        Msg::Uploaded(result) => {
            s.upload = None;
            s.cancel = None;
            match result {
                Ok(t) => app.notify(t, false),
                Err(e) => app.notify(e, true),
            }
            return load_history();
        }
        Msg::Stop => {
            if let Some(c) = &s.cancel {
                c.store(true, Ordering::Relaxed);
            }
            return Task::none();
        }
        Msg::History(h) => {
            s.history = h;
            return Task::none();
        }
    }
    render(app)
}

fn start_upload(app: &mut App) -> Task<Message> {
    let s = &mut app.screen;
    s.confirm = false;
    let (Some(src), Some(path)) = (s.source.clone(), s.path.clone()) else { return Task::none() };
    let o = s.options.clone();
    let slot = app.cfg.settings.lcd_slot;
    let cancel = Arc::new(AtomicBool::new(false));
    s.cancel = Some(cancel.clone());
    s.upload = Some(0.0);
    s.upload_started = Some(Instant::now());
    let (tx, rx) = mpsc::unbounded::<Msg>();
    std::thread::spawn(move || {
        let result = (|| -> ak980_core::Result<String> {
            let frames = src.render(&o, LCD_MAX_FRAMES);
            let blob = lcd::encode_animation(&frames, o.dither, &mut |_| {})?;
            lcd::save_history(&frames, Some(&path))?;
            let mut kb = WiredKeyboard::open()?;
            let progress_tx = tx.clone();
            let done = kb.upload_lcd(&blob, slot, &mut |p| {
                let _ = progress_tx.unbounded_send(Msg::Progress(p));
            }, &|| cancel.load(Ordering::Relaxed))?;
            Ok(if done { tr("Animation uploaded") } else { tr("Upload stopped; the screen keeps the part sent so far.") })
        })();
        let _ = tx.unbounded_send(Msg::Uploaded(result.map_err(|e| e.to_string())));
    });
    Task::run(rx, wrap)
}

pub fn animating(app: &App) -> bool {
    app.page == Page::Screen && !app.screen.paused && app.screen.frames.len() > 1
}

pub fn tick(app: &mut App, now: Instant) {
    if !animating(app) {
        return;
    }
    let s = &mut app.screen;
    let next = *s.next_at.get_or_insert(now);
    if now >= next {
        s.index = (s.index + 1) % s.frames.len();
        s.next_at = Some(now + Duration::from_millis(s.frames[s.index].1 as u64));
    }
}

/// The confirmation shown over the window before an upload.
pub fn modal(app: &App) -> Option<Element<'_, Message>> {
    if !app.screen.confirm {
        return None;
    }
    let r = roles();
    let dialog = container(
        column![
            widgets::heading(tr("Upload to the screen")),
            widgets::body(tr("This replaces the animation on the keyboard. The keyboard cannot give it back, so a copy of what you send goes to Earlier uploads; your current animation is there only if it was sent from this app.")),
            row![
                iced::widget::space().width(Fill),
                widgets::secondary_button(tr("Cancel"), Some(wrap(Msg::CancelConfirm))),
                widgets::primary_button(tr("Upload"), Some(wrap(Msg::Upload))),
            ]
            .spacing(10),
        ]
        .spacing(16)
        .max_width(460),
    )
    .padding(24)
    .style(theme::card);
    let backdrop = container(container(opaque(dialog)).center(Fill))
        .width(Fill)
        .height(Fill)
        .style(move |_| container::Style { background: Some(theme::alpha(Color::BLACK, if r.dark { 0.5 } else { 0.3 }).into()), ..Default::default() });
    Some(opaque(mouse_area(backdrop).on_press(wrap(Msg::CancelConfirm))))
}

fn percent(v: f32) -> i32 {
    (v * 100.0).round() as i32
}

pub fn view(app: &App) -> Element<'_, Message> {
    let s = &app.screen;
    let o = &s.options;
    let panel: Element<'_, Message> = match s.frames.get(s.index) {
        Some((h, _)) => image(h.clone()).width(480).height(270).filter_method(image::FilterMethod::Nearest).into(),
        None => container(caption(if s.loading { tr("Opening…") } else { tr("Open a picture first") }))
            .center_x(480)
            .center_y(270)
            .into(),
    };
    let bezel = container(panel).padding(14).style(|_| container::Style {
        background: Some(Color::from_rgb8(8, 8, 9).into()),
        border: iced::Border { radius: 10.0.into(), width: 1.0, color: theme::alpha(roles().overlay0, 0.6) },
        ..Default::default()
    });
    let name = s.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let pages = lcd::page_count(s.frames.len());
    let mut parts = vec![];
    if !s.frames.is_empty() {
        parts.push(plural(s.frames.len() as i64, "{n} frame", "{n} frames"));
        parts.push(tr!("upload ≈ {s} s", s = (pages as f32 * SECONDS_PER_PAGE).round()));
        if s.available > s.frames.len() {
            parts.push(tr!("{used} of {total} frames fit", used = s.frames.len(), total = s.available));
        }
    }
    let info = parts.join(" · ");
    let top = row![
        column![
            bezel,
            row![
                widgets::secondary_button(tr("Open image…"), Some(wrap(Msg::Open(false)))),
                widgets::secondary_button(tr("Open folder of frames…"), Some(wrap(Msg::Open(true)))),
                toggler(!s.paused).label(tr("Play")).on_toggle(|v| wrap(Msg::Pause(!v))).style(theme::switch),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            caption(if name.is_empty() { String::new() } else { format!("{name} — {info}") }),
        ]
        .spacing(10),
        history(app),
    ]
    .spacing(20);

    let fits = vec![(Fit::Fit, tr("Fit")), (Fit::Fill, tr("Fill")), (Fit::Stretch, tr("Stretch"))];
    let positions: Vec<Choice<&'static str>> = TEXT_POSITIONS.iter().map(|p| Choice::new(*p, tr(position_label(p)))).collect();
    let current_pos = positions.iter().find(|c| c.value == o.text_pos).cloned();
    let turns = vec![(0u8, "0°".to_string()), (1, "90°".into()), (2, "180°".into()), (3, "270°".into())];
    let tones = vec![(Tone::Natural, tr("Natural")), (Tone::Grey, tr("Black and white")), (Tone::Sepia, tr("Sepia")),
                     (Tone::Negative, tr("Negative"))];
    let mut picture = vec![
        setting(tr("Scale"), None, segmented(fits, o.fit, |f| wrap(Msg::Fit(f)))),
        setting(tr("Turn"), None, segmented(turns, o.rotate, |r| wrap(Msg::Rotate(r)))),
        setting(tr("Zoom"), Some(tr("Shows a part of the picture; move it below")),
                widgets::ranged(percent(o.zoom), 100, 400, "%", |v| wrap(Msg::Zoom(v)))),
    ];
    if o.zoom > 1.0 {
        picture.push(setting(tr("Left – right"), None, widgets::ranged((o.pan.0 * 100.0).round() as i32, -100, 100, "",
                                                                     |v| wrap(Msg::PanX(v)))));
        picture.push(setting(tr("Up – down"), None, widgets::ranged((o.pan.1 * 100.0).round() as i32, -100, 100, "",
                                                                  |v| wrap(Msg::PanY(v)))));
    }
    picture.extend([
        setting(tr("Sharp pixels"), Some(tr("No smoothing when scaling, for pixel art")),
                toggler(o.sharp).on_toggle(|v| wrap(Msg::Sharp(v))).style(theme::switch)),
        setting(tr("Tone"), None, segmented(tones, o.tone, |t| wrap(Msg::Tone(t)))),
        setting(tr("Shift the hues"), None, widgets::ranged(o.hue, -180, 180, "°", |v| wrap(Msg::Hue(v)))),
        setting(tr("Mirror"), None, toggler(o.flip_h).on_toggle(|v| wrap(Msg::FlipH(v))).style(theme::switch)),
        setting(tr("Upside down"), None, toggler(o.flip_v).on_toggle(|v| wrap(Msg::FlipV(v))).style(theme::switch)),
        setting(tr("Brightness"), None, widgets::ranged(percent(o.brightness), 20, 200, "%", |v| wrap(Msg::Brightness(v)))),
        setting(tr("Contrast"), None, widgets::ranged(percent(o.contrast), 20, 200, "%", |v| wrap(Msg::Contrast(v)))),
        setting(tr("Saturation"), None, widgets::ranged(percent(o.saturation), 0, 200, "%", |v| wrap(Msg::Saturation(v)))),
        setting(tr("Smooth gradients"), Some(tr("Dithering hides the steps of the panel's 16-bit colour")),
                toggler(o.dither).on_toggle(|v| wrap(Msg::Dither(v))).style(theme::switch)),
    ]);
    let orders = vec![(Order::Forward, tr("Forward")), (Order::Backward, tr("Backward")), (Order::Bounce, tr("There and back"))];
    let timing = vec![
        setting(tr("Plays"), None, segmented(orders, o.order, |v| wrap(Msg::Order(v)))),
        setting(tr("Speed"), None, widgets::ranged(percent(o.speed), 25, 400, "%", |v| wrap(Msg::Speed(v)))),
        setting(tr("Same delay for every frame"), Some(tr("0 keeps the delays of the file")),
                widgets::ranged(o.fixed_delay.unwrap_or(0) as i32, 0, lcd::MAX_DELAY as i32, &tr(" ms"),
                                |v| wrap(Msg::FixedDelay(v)))),
        setting(tr("Keep every Nth"), Some(tr("Keeps a long GIF within the screen's 140 frames")),
                widgets::ranged(o.frame_step as i32, 1, 10, "", |v| wrap(Msg::Step(v)))),
        setting(tr("Cut frames from the start"), None, widgets::ranged(o.trim_start as i32, 0, 200, "", |v| wrap(Msg::TrimStart(v)))),
        setting(tr("Cut frames from the end"), None, widgets::ranged(o.trim_end as i32, 0, 200, "", |v| wrap(Msg::TrimEnd(v)))),
    ];
    let mut caption_rows = vec![
        setting(tr("Caption"), None, text_input(&tr("Optional caption"), &o.text).on_input(|t| wrap(Msg::Text(t)))
            .padding(6).style(theme::input).width(260)),
        setting(tr("Running text"), Some(tr("The caption crosses the screen from right to left; a still picture gets the frames it needs")),
                toggler(o.text_scroll.is_some()).on_toggle(|v| wrap(Msg::Scroll(v))).style(theme::switch)),
    ];
    if let Some(px) = o.text_scroll {
        caption_rows.push(setting(tr("How fast it runs"), Some(tr("Faster needs fewer frames, so it uploads sooner")),
                                  widgets::ranged(px as i32, 1, 20, &tr(" px a frame"), |v| wrap(Msg::ScrollSpeed(v)))));
    }
    caption_rows.extend([
        setting(tr("Outline"), Some(tr("A dark edge around the letters instead of a shadow, for busy pictures")),
                toggler(o.text_outline).on_toggle(|v| wrap(Msg::Outline(v))).style(theme::switch)),
        setting(tr("Position"), None, iced::widget::pick_list(positions, current_pos, |p| wrap(Msg::TextPos(p)))
            .style(theme::pick).menu_style(theme::menu).width(200)),
        setting(tr("Size"), None, widgets::ranged(o.text_size as i32, 8, 48, " px", |v| wrap(Msg::TextSize(v)))),
        setting(tr("Colour"), None, color_picker("color:screen:text".into(), o.text_color, app.drafts.get("color:screen:text"),
                                                   |c| Message::Draft("color:screen:text".into(), widgets::html(c)))),
    ]);
    let upload: Element<'_, Message> = match s.upload {
        Some(p) => {
            let left = s.upload_started.map(|t| {
                let el = t.elapsed().as_secs_f32();
                if p > 0.02 { (el / p - el).max(0.0).round() as i64 } else { (pages as f32 * SECONDS_PER_PAGE) as i64 }
            });
            column![
                text(tr("Uploading to the screen…")).font(fonts::MEDIUM),
                progress_bar(0.0..=1.0, p).style(theme::progress).girth(8),
                row![
                    caption(tr!("{p}% · {s} s left", p = (p * 100.0).round() as i32, s = left.unwrap_or(0))),
                    iced::widget::space().width(Fill),
                    widgets::secondary_button(tr("Stop uploading"), Some(wrap(Msg::Stop))),
                ]
                .align_y(Alignment::Center),
            ]
            .spacing(8)
            .into()
        }
        None => row![
            widgets::primary_button(tr("Upload to the screen"),
                                    (app.wired() && !s.frames.is_empty() && !s.rendering).then(|| wrap(Msg::AskUpload))),
            caption(tr("Every upload is kept in Earlier uploads.")),
        ]
        .spacing(14)
        .align_y(Alignment::Center)
        .into(),
    };
    column![
        top,
        strip(app),
        upload,
        section(tr("Picture"), card(picture)),
        section(tr("Timing"), card(timing)),
        section(tr("Caption"), card(caption_rows)),
        crate::pages::reminders::view(app),
    ]
    .spacing(20)
    .into()
}

/// The frames side by side with their delays, like a film strip, and what can be done to the one
/// picked: its own delay, deleting it, or bringing deleted frames back.
fn strip(app: &App) -> Element<'_, Message> {
    let s = &app.screen;
    if s.frames.len() < 2 {
        return column![].into();
    }
    let mut frames = row![].spacing(6);
    for (n, (handle, delay)) in s.frames.iter().enumerate() {
        let thumb = column![
            image(handle.clone()).width(96).height(54),
            caption(format!("{} · {delay}{}", n + 1, tr(" ms"))),
        ]
        .spacing(2);
        frames = frames.push(button(thumb).padding(4).style(theme::list_item(n == s.index)).on_press(wrap(Msg::Frame(n))));
    }
    let delay = s.frames.get(s.index).map(|(_, d)| *d).unwrap_or_default();
    let value = app.drafts.get("frame-delay").cloned().unwrap_or_else(|| delay.to_string());
    let edit = row![
        widgets::body(tr!("Frame {n}", n = s.index + 1)),
        widgets::hint(text_input("", &value).on_input(|t| Message::Draft("frame-delay".into(), t))
            .on_submit(Message::DraftSubmit("frame-delay".into())).width(70).padding(4).style(theme::input)
            .font(fonts::MONO), tr("How long this frame stays on the screen")),
        caption(tr(" ms").trim().to_string()),
        widgets::secondary_button(tr("Delete frame"), (s.frames.len() > 1).then(|| wrap(Msg::DeleteFrame))),
        widgets::secondary_button(tr("Restore deleted frames"), (!s.options.deleted.is_empty()).then(|| wrap(Msg::RestoreFrames))),
    ]
    .spacing(10)
    .align_y(Alignment::Center);
    column![
        // Room under the captions for the scroll bar
        scrollable(container(frames).padding(iced::Padding { bottom: 14.0, ..Default::default() })).direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::default())).style(theme::scroll),
        edit,
    ]
    .spacing(10)
    .into()
}

fn position_label(p: &str) -> &'static str {
    match p {
        "top-left" => "Top left",
        "top" => "Top",
        "top-right" => "Top right",
        "center" => "Centre",
        "bottom-left" => "Bottom left",
        "bottom" => "Bottom",
        _ => "Bottom right",
    }
}

fn history(app: &App) -> Element<'_, Message> {
    let mut list = column![].spacing(2);
    for path in app.screen.history.iter().take(30) {
        let label = path.parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().replacen('_', " ", 1)).unwrap_or_default();
        let selected = app.screen.path.as_ref() == Some(path);
        list = list.push(
            button(text(label).size(theme::SMALL))
                .width(Fill)
                .padding([6, 10])
                .style(theme::list_item(selected))
                .on_press(wrap(Msg::PickHistory(path.clone()))),
        );
    }
    if app.screen.history.is_empty() {
        list = list.push(caption(tr("Nothing uploaded yet")));
    }
    column![
        widgets::heading(tr("Earlier uploads")),
        container(scrollable(list).style(theme::scroll).height(Length::Fixed(300.0))).padding(6).width(Fill).style(theme::card),
    ]
    .spacing(8)
    .width(Fill)
    .into()
}
