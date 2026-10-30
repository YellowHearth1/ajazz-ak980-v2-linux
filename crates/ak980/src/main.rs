//! Command line: `ak980 <command> ...`. Without a command the app window opens.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{value_parser, Arg, ArgAction, ArgMatches, Command};

use ak980_core::config::{self, Config, Launcher, LightSource, Profile, Rule};
use ak980_core::device::{self, AnyKeyboard, Dongle, Link, WiredKeyboard, LCD_MAX_FRAMES, LCD_PAGE};
use ak980_core::effects::{self, ParamKind, EFFECTS};
use ak980_core::i18n::{self, plural, tr};
use ak980_core::keymap::{encode_layer, KeyAction, LAUNCHER_KEYS};
use ak980_core::lcd::{self, Fit, RenderOptions, Source, TEXT_POSITIONS};
use ak980_core::lighting::{self, CAP_COLOR, DIRECTIONS, LIGHT_MODES, SIDE_LIGHT_MODES, SLEEP_CHOICES};
use ak980_core::macros::{self, Macro, BUDGET_BYTES};
use ak980_core::profiles::{self, ALL_PARTS};
use ak980_core::{agent, battery, layout, paths, permissions, runtime, service, tr, Error, Result};

// ---------------------------------------------------------------- argument helpers

fn parse_color(s: &str) -> std::result::Result<[u8; 3], String> {
    let h = s.trim().trim_start_matches('#');
    let channel = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("x"), 16);
    match (h.len(), channel(0), channel(2), channel(4)) {
        (6, Ok(r), Ok(g), Ok(b)) => Ok([r, g, b]),
        _ => Err(tr!("a colour is written as RRGGBB: {value}", value = h)),
    }
}

fn parse_onoff(s: &str) -> std::result::Result<bool, String> {
    match s.to_lowercase().as_str() {
        "on" | "1" | "yes" | "true" => Ok(true),
        "off" | "0" | "no" | "false" => Ok(false),
        _ => Err(tr("expected on or off")),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

fn html(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

fn flag(id: &'static str, help: String) -> Arg {
    Arg::new(id).long(id).action(ArgAction::SetTrue).help(help)
}

fn color_arg(id: &'static str) -> Arg {
    Arg::new(id).long(id).value_name("RRGGBB").value_parser(parse_color)
}

fn onoff_arg(id: &'static str) -> Arg {
    Arg::new(id).long(id).value_name("on|off").value_parser(parse_onoff)
}

fn numbered(items: &[&str], first: usize) -> String {
    items.iter().enumerate().map(|(i, s)| format!("{} {}", i + first, tr(s))).collect::<Vec<_>>().join(", ")
}

fn build() -> Command {
    let sub = |name: &'static str, help: String| Command::new(name).about(help.clone()).long_about(help);
    Command::new("ak980")
        .about(ak980_core::APP_TITLE)
        .version(ak980_core::VERSION)
        .after_help(tr!("Without a command the app window opens. Docs: {url}", url = ak980_core::HOMEPAGE))
        .subcommand_value_name(tr("command"))
        .arg(Arg::new("lang").long("lang").global(true).value_parser(["auto", "en", "uk"]).help(tr("interface language")))
        .subcommand(sub("gui", tr("open the app window")).arg(flag("tray", tr("start hidden in the tray"))))
        .subcommand(
            sub("status", tr("connection, battery, profile, agent"))
                .arg(flag("json", tr("print it as JSON, for scripts")))
                .arg(flag("waybar", tr("print it as a Waybar custom module (return-type json) reads it"))),
        )
        .subcommand(sub("list", tr("the keyboard's HID interfaces")))
        .subcommand(
            sub("battery", tr("battery measurements: how long a charge lasts, charging time, condition"))
                .arg(Arg::new("show").long("show").value_name("on|off").value_parser(parse_onoff)
                    .help(tr("show the estimates in the window and in `ak980 status`")))
                .arg(flag("reset", tr("forget the measured cycles and measure again (the readings stay)"))),
        )
        .subcommand(
            sub("time", tr("sync the screen clock with the system time"))
                .arg(Arg::new("slot").long("slot").value_parser(value_parser!(u8))
                    .help(tr("screen page sent with the time (default 1)"))),
        )
        .subcommand(
            sub("agent", tr("background agent: clock, PC effects, profiles, launcher keys"))
                .visible_alias("daemon")
                .arg(flag("system", tr("system service mode: clock only")))
                .arg(Arg::new("interval").long("interval").value_parser(value_parser!(u32).range(1..))
                    .help(tr("minutes between clock syncs (default 60)")))
                .arg(flag("once", tr("sync once and exit"))),
        )
        .subcommand(
            sub("lcd", tr("upload a GIF, an image or a folder of frames to the screen"))
                .arg(Arg::new("file").value_parser(value_parser!(PathBuf)))
                .arg(Arg::new("fit").long("fit").value_parser(["fit", "fill", "stretch"]).default_value("fit"))
                .arg(color_arg("background").default_value("000000"))
                .arg(Arg::new("flip-h").long("flip-h").action(ArgAction::SetTrue))
                .arg(Arg::new("flip-v").long("flip-v").action(ArgAction::SetTrue))
                .arg(Arg::new("brightness").long("brightness").value_parser(value_parser!(f32)).default_value("1.0"))
                .arg(Arg::new("contrast").long("contrast").value_parser(value_parser!(f32)).default_value("1.0"))
                .arg(Arg::new("saturation").long("saturation").value_parser(value_parser!(f32)).default_value("1.0"))
                .arg(Arg::new("speed").long("speed").value_parser(value_parser!(f32)).default_value("1.0"))
                .arg(Arg::new("delay").long("delay").value_parser(value_parser!(u32).range(2..=510))
                    .help(tr("fixed frame delay in ms (2..510)")))
                .arg(Arg::new("trim-start").long("trim-start").value_parser(value_parser!(usize)).default_value("0"))
                .arg(Arg::new("trim-end").long("trim-end").value_parser(value_parser!(usize)).default_value("0"))
                .arg(Arg::new("step").long("step").value_parser(value_parser!(usize)).default_value("1")
                    .help(tr("keep every Nth frame")))
                .arg(Arg::new("text").long("text"))
                .arg(color_arg("text-color").default_value("ffffff"))
                .arg(Arg::new("text-size").long("text-size").value_parser(value_parser!(u32)).default_value("18"))
                .arg(Arg::new("text-pos").long("text-pos").value_parser(TEXT_POSITIONS).default_value("bottom-right"))
                .arg(Arg::new("no-dither").long("no-dither").action(ArgAction::SetTrue))
                .arg(Arg::new("slot").long("slot").value_parser(value_parser!(u8)))
                .arg(Arg::new("preview").long("preview").value_name("OUT.gif").value_parser(value_parser!(PathBuf))
                    .help(tr("only save how it will look")))
                .arg(flag("history", tr("list earlier uploads"))),
        )
        .subcommand(sub("effects", tr("list keyboard and PC lighting effects")).visible_alias("modes"))
        .subcommand(
            sub("light", tr("keyboard lighting effect (stored in the keyboard)"))
                .arg(Arg::new("mode"))
                .arg(color_arg("color"))
                .arg(onoff_arg("rainbow"))
                .arg(Arg::new("brightness").long("brightness").value_parser(value_parser!(u8).range(0..=5)))
                .arg(Arg::new("speed").long("speed").value_parser(value_parser!(u8).range(0..=5)))
                .arg(Arg::new("direction").long("direction").value_parser(value_parser!(u8).range(0..=3))
                    .help(numbered(&DIRECTIONS, 0))),
        )
        .subcommand(
            sub("fx", tr("PC lighting effect, e.g. ak980 fx spectrum palette=fire"))
                .arg(Arg::new("effect"))
                .arg(Arg::new("values").value_name("setting=value").num_args(0..))
                .arg(Arg::new("brightness").long("brightness").value_name("0..100")
                    .value_parser(value_parser!(u8).range(0..=100)))
                .arg(Arg::new("list").long("list").action(ArgAction::SetTrue)),
        )
        .subcommand(
            sub("side", tr("side light (hidden in the vendor driver, may do nothing)"))
                .arg(Arg::new("mode").required(true).value_parser(SIDE_LIGHT_MODES.map(|(n, _)| n))),
        )
        .subcommand(
            sub("custom", tr("per-key colours stored in the keyboard"))
                .arg(color_arg("all"))
                .arg(Arg::new("key").long("key").value_name("KEY=RRGGBB").action(ArgAction::Append))
                .arg(color_arg("wasd"))
                .arg(Arg::new("clear").long("clear").action(ArgAction::SetTrue))
                .arg(Arg::new("brightness").long("brightness").value_parser(value_parser!(u8).range(0..=5)))
                .arg(flag("read", tr("print the keyboard's colour table (diagnostics)"))),
        )
        .subcommand(
            sub("options", tr("Win/Alt+Tab/Alt+F4 locks, Fn, sleep, key response"))
                .arg(onoff_arg("win-lock"))
                .arg(onoff_arg("alt-tab-lock"))
                .arg(onoff_arg("alt-f4-lock"))
                .arg(onoff_arg("fn-toggle"))
                .arg(Arg::new("sleep").long("sleep").value_parser(value_parser!(u8).range(0..=3))
                    .help(numbered(&SLEEP_CHOICES, 0)))
                .arg(Arg::new("response").long("response").value_parser(value_parser!(u8).range(1..=5))),
        )
        .subcommand(sub("keys", tr("key names for remap and custom")))
        .subcommand(
            sub("remap", tr("remap a key"))
                .after_help("default | off | key:ctrl+shift+A | mouse:left|right|middle|double|btn4|btn5|wheelup|\
                             wheeldown | media:play|next|prev|volup|voldown|mute|calc|... | \
                             macro:N[:once|repeat:K|toggle|hold|click] | multi:A,B,C")
                .arg(Arg::new("key"))
                .arg(Arg::new("action"))
                .arg(flag("fn", tr("the Fn layer (while Fn is held)")))
                .arg(flag("reset", tr("reset the whole layer")))
                .arg(Arg::new("list").long("list").action(ArgAction::SetTrue))
                .arg(flag("dry", tr("only print the bytes"))),
        )
        .subcommand(
            sub("macro", tr("macros"))
                .subcommand_required(true)
                .subcommand(Command::new("list").arg(Arg::new("verbose").short('v').long("verbose").action(ArgAction::SetTrue)))
                .subcommand(
                    Command::new("add")
                        .about("text:Hello | combo:ctrl+shift+esc | steps:down:A,wait:50,up:A,click:left")
                        .arg(Arg::new("name").required(true))
                        .arg(Arg::new("spec").required(true)),
                )
                .subcommand(Command::new("rm").arg(Arg::new("index").required(true).value_parser(value_parser!(usize))))
                .subcommand(Command::new("clear"))
                .subcommand(Command::new("upload")),
        )
        .subcommand(
            sub("launch", tr("launcher keys F13–F24: run a program or open a link"))
                .after_help(tr!("A command can also switch profiles: {next} for the next one, {named} for one by name.",
                                next = config::PROFILE_NEXT, named = format!("{}NAME", config::PROFILE_USE)))
                .arg(Arg::new("key"))
                .arg(Arg::new("command").num_args(0..).trailing_var_arg(true).allow_hyphen_values(true))
                .arg(Arg::new("rm").long("rm").value_name("KEY"))
                .arg(Arg::new("list").long("list").action(ArgAction::SetTrue)),
        )
        .subcommand(
            sub("profile", tr("profiles"))
                .subcommand_required(true)
                .subcommand(Command::new("list"))
                .subcommand(Command::new("use").arg(Arg::new("name").required(true)))
                .subcommand(Command::new("apply"))
                .subcommand(
                    Command::new("new")
                        .arg(Arg::new("name").required(true))
                        .arg(flag("copy", tr("start from the active profile"))),
                )
                .subcommand(Command::new("rm").arg(Arg::new("name").required(true)))
                .subcommand(Command::new("rename").arg(Arg::new("name").required(true)).arg(Arg::new("new").required(true)))
                .subcommand(
                    Command::new("export")
                        .arg(Arg::new("name").required(true))
                        .arg(Arg::new("file").required(true).value_parser(value_parser!(PathBuf))),
                )
                .subcommand(Command::new("import").arg(Arg::new("file").required(true).value_parser(value_parser!(PathBuf)))),
        )
        .subcommand(
            sub("auto", tr("switch profiles when a program starts"))
                .subcommand_required(true)
                .subcommand(Command::new("on"))
                .subcommand(Command::new("off"))
                .subcommand(Command::new("list"))
                .subcommand(Command::new("add").arg(Arg::new("process").required(true)).arg(Arg::new("profile").required(true)))
                .subcommand(Command::new("rm").arg(Arg::new("process").required(true)))
                .subcommand(Command::new("default").arg(Arg::new("profile"))),
        )
        .subcommand(
            sub("service", tr("start the agent automatically (XDG, systemd, dinit, OpenRC, runit, s6)"))
                .subcommand_required(true)
                .subcommand(Command::new("list"))
                .subcommand(Command::new("enable").arg(Arg::new("id").required(true)
                    .help("session | systemd-user | dinit-user | runit-user | system-<init>")))
                .subcommand(Command::new("disable").arg(Arg::new("id").required(true)
                    .help("session | systemd-user | dinit-user | runit-user | system-<init>"))),
        )
        .subcommand(
            sub("setup", tr("let your user access the keyboard (installs the udev rule)"))
                .arg(Arg::new("force").long("force").action(ArgAction::SetTrue)),
        )
        .subcommand(sub("apply", tr("send the active profile and the time to the keyboard")))
        .subcommand(
            sub("link", tr("the 2.4 GHz link record: when the keyboard answered and when it switched"))
                .arg(Arg::new("lines").short('n').long("lines").value_parser(value_parser!(usize)).default_value("20")),
        )
        .subcommand(sub("reconnect", tr("reconnect the 2.4 GHz receiver as if it had been plugged in again (asks for your password)")))
}

// ---------------------------------------------------------------- commands

fn say(text: impl AsRef<str>) {
    println!("{}", text.as_ref());
}

/// What `ak980 status` reports, gathered once for every output format.
struct StatusReport {
    link: Option<Link>,
    access: Option<bool>,
    /// The charge: read now over 2.4 GHz, or the agent's last reading.
    battery: Option<u8>,
    battery_live: bool,
    estimate: Option<battery::Estimate>,
    /// Battery measurements are on: the estimate is shown.
    insights: bool,
    profile: String,
    agent: bool,
}

fn status_report(cfg: &Config) -> StatusReport {
    let link = device::connection_state();
    let live = (link == Some(Link::Dongle))
        .then(Dongle::find)
        .flatten()
        .and_then(|info| Dongle::open_found(&info).and_then(|mut d| d.read_battery()).ok().flatten());
    let history = battery::read(&paths::battery_log());
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    StatusReport {
        link,
        access: permissions::accessible(),
        battery: live.or_else(|| history.last().map(|(_, l)| *l)),
        battery_live: live.is_some(),
        estimate: link.and_then(|l| {
            battery::Measurements::load(&paths::battery_measurements()).estimate(&history, l == Link::Usb, now)
        }),
        insights: cfg.settings.battery_insights,
        profile: cfg.active.clone(),
        agent: runtime::agent_running(),
    }
}

fn status_lines(r: &StatusReport) -> Vec<String> {
    let link = match r.link {
        Some(Link::Usb) => tr("USB cable"),
        Some(Link::Dongle) => tr("2.4 GHz dongle"),
        None => tr("not found"),
    };
    let mut lines = vec![tr!("Connection: {state}", state = link)];
    if r.access == Some(false) {
        lines.push(tr("Access: no permission — run `ak980 setup`"));
    }
    let estimate = r.estimate.filter(|_| r.insights);
    match (r.battery, r.link, estimate) {
        // On the cable in wired mode only the measured charging can tell
        (_, Some(Link::Usb), Some(e)) => lines.push(tr!("Battery: {level}", level = battery::describe(&e))),
        (Some(level), _, e) => {
            let mut text = format!("{level}%");
            if !r.battery_live {
                text = tr!("{level} at the last check", level = text);
            }
            if let Some(e) = e {
                text = format!("{text}, {}", battery::describe(&e));
            }
            lines.push(tr!("Battery: {level}", level = text));
        }
        (None, Some(Link::Dongle), _) => {
            lines.push(tr!("Battery: {level}", level = tr("no answer (is the keyboard in 2.4 GHz mode and awake?)")))
        }
        _ => {}
    }
    lines.push(tr!("Active profile: {name}", name = r.profile));
    let agent = if r.agent { tr("running") } else { tr("not running") };
    lines.push(tr!("Background agent: {state}", state = agent));
    lines
}

fn cmd_status(m: &ArgMatches, cfg: &Config) -> Result<()> {
    let r = status_report(cfg);
    let link = match r.link {
        Some(Link::Usb) => Some("usb"),
        Some(Link::Dongle) => Some("2.4g"),
        None => None,
    };
    if m.get_flag("json") {
        let round = |h: f32| (h * 10.0).round() / 10.0;
        let estimate = match r.estimate {
            Some(battery::Estimate::Lasts(h)) => serde_json::json!({"state": "lasts", "hours": round(h)}),
            Some(battery::Estimate::Charging { hours, level }) => {
                serde_json::json!({"state": "charging", "hours": round(hours), "level": level})
            }
            Some(battery::Estimate::Full) => serde_json::json!({"state": "full"}),
            None => serde_json::Value::Null,
        };
        let json = serde_json::json!({
            "link": link, "access": r.access, "battery": r.battery, "battery_live": r.battery_live,
            "estimate": estimate, "profile": r.profile, "agent": r.agent,
        });
        say(json.to_string());
    } else if m.get_flag("waybar") {
        let text = match (r.link, r.battery) {
            (Some(Link::Dongle), Some(level)) => format!("{level}%"),
            (Some(Link::Usb), _) => "USB".to_string(),
            (Some(Link::Dongle), None) => "2.4G".to_string(),
            (None, _) => String::new(),
        };
        let mut class = vec![link.unwrap_or("absent").replace('.', "")];
        if r.link == Some(Link::Dongle) && r.battery.is_some_and(|l| l <= 20) {
            class.push("low".into());
        }
        let json = serde_json::json!({
            "text": text, "tooltip": status_lines(&r).join("\n"), "class": class,
            "percentage": r.battery.filter(|_| r.link == Some(Link::Dongle)),
        });
        say(json.to_string());
    } else {
        for line in status_lines(&r) {
            say(line);
        }
    }
    Ok(())
}

fn local_date(secs: u64) -> String {
    let (y, m, d) = ak980_core::time::LocalTime::from_unix(secs as i64).date();
    format!("{y}-{m:02}-{d:02}")
}

fn cmd_battery(m: &ArgMatches, cfg: &mut Config) -> Result<()> {
    let path = paths::battery_measurements();
    if m.get_flag("reset") {
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        say(tr("The measured cycles are forgotten; the agent measures again from the readings to come."));
        return Ok(());
    }
    if let Some(on) = m.get_one::<bool>("show") {
        cfg.settings.battery_insights = *on;
        cfg.save()?;
    }
    let history = battery::read(&paths::battery_log());
    let meas = battery::Measurements::load(&path);
    match history.first() {
        Some((t, _)) => say(tr!("Readings: {count} changes since {date}", count = history.len(), date = local_date(*t))),
        None => say(tr("No readings yet: the background agent reads the battery every minute in 2.4 GHz mode.")),
    }
    match (meas.full_charge_hours(), meas.discharges.last()) {
        (Some(h), Some(last)) => say(tr!("A full charge lasts about {time} (measurements: {count}, the last on {date})",
                                         time = battery::duration(h), count = meas.discharges.len(), date = local_date(last.end))),
        _ => say(tr("How long a charge lasts: not measured yet. Use the keyboard over 2.4 GHz from 90 % or more down to 20 % or less.")),
    }
    match meas.charging_hours() {
        Some(h) => say(tr!("Charging from empty to full takes about {time}", time = battery::duration(h))),
        None => say(tr("Charging time: not measured yet. Charge it from 20 % or less to full while it stays in 2.4 GHz mode, so it can tell its level.")),
    }
    if let Some((share, cycles)) = meas.condition() {
        say(tr!("Condition: a charge lasts about {share} of the first measured one (measured cycles: {cycles})",
                share = format!("{}%", (share * 100.0).round()), cycles = cycles));
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    if let Some(e) = device::connection_state().and_then(|l| meas.estimate(&history, l == Link::Usb, now)) {
        say(tr!("Now: {estimate}", estimate = battery::describe(&e)));
    }
    if !cfg.settings.battery_insights {
        say(tr("The window and `ak980 status` show the estimates with `ak980 battery --show on`."));
    }
    Ok(())
}

fn cmd_list() {
    for d in ak980_core::hid::enumerate(Some(device::VENDOR_ID), None) {
        say(d.to_string());
    }
}

fn cmd_time(m: &ArgMatches, cfg: &Config) -> Result<()> {
    let slot = m.get_one::<u8>("slot").copied().unwrap_or(cfg.settings.clock_slot);
    let mut kb = AnyKeyboard::open()?;
    if !kb.sync_clock(None, slot)? {
        return Err(Error::NotConnected(tr("The keyboard did not answer through the dongle (is it in 2.4 GHz mode?)")));
    }
    say(tr("Clock synced."));
    Ok(())
}

fn render_options(m: &ArgMatches) -> RenderOptions {
    let num = |id: &str| m.get_one::<f32>(id).copied().unwrap_or(1.0);
    let count = |id: &str| m.get_one::<usize>(id).copied().unwrap_or(0);
    RenderOptions {
        fit: m.get_one::<String>("fit").and_then(|f| Fit::by_id(f)).unwrap_or_default(),
        background: m.get_one::<[u8; 3]>("background").copied().unwrap_or([0, 0, 0]),
        flip_h: m.get_flag("flip-h"),
        flip_v: m.get_flag("flip-v"),
        brightness: num("brightness"),
        contrast: num("contrast"),
        saturation: num("saturation"),
        speed: num("speed"),
        fixed_delay: m.get_one::<u32>("delay").copied(),
        trim_start: count("trim-start"),
        trim_end: count("trim-end"),
        frame_step: count("step").max(1),
        text: m.get_one::<String>("text").cloned().unwrap_or_default(),
        text_color: m.get_one::<[u8; 3]>("text-color").copied().unwrap_or([255, 255, 255]),
        text_size: m.get_one::<u32>("text-size").copied().unwrap_or(18),
        text_pos: m.get_one::<String>("text-pos").cloned().unwrap_or_else(|| "bottom-right".into()),
        dither: !m.get_flag("no-dither"),
        ..RenderOptions::default()
    }
}

fn cmd_lcd(m: &ArgMatches, cfg: &mut Config) -> Result<()> {
    if m.get_flag("history") {
        for path in lcd::history_entries() {
            say(path.display().to_string());
        }
        return Ok(());
    }
    let Some(file) = m.get_one::<PathBuf>("file") else {
        return Err(Error::invalid(tr("Give a GIF, an image or a folder of frames")));
    };
    let o = render_options(m);
    let src = Source::load(file, 1000)?;
    let frames = src.render(&o, LCD_MAX_FRAMES);
    let total = src.available(&o);
    if total > frames.len() {
        say(tr!("Using {used} of {total} frames (the screen holds {max}).", used = frames.len(), total = total,
                max = LCD_MAX_FRAMES));
    }
    if let Some(out) = m.get_one::<PathBuf>("preview") {
        let shown: Vec<_> = frames.iter().map(|(f, d)| (lcd::preview_565(f, o.dither), *d)).collect();
        lcd::save_gif(&shown, out)?;
        say(tr!("Preview saved: {path}", path = out.display()));
        return Ok(());
    }
    let blob = lcd::encode_animation(&frames, o.dither, &mut |_| {})?;
    let pages = blob.len() / LCD_PAGE;
    say(tr!("{frames} frames, {pages} pages of 4 KiB, about {seconds} s", frames = frames.len(), pages = pages,
            seconds = (pages as f32 * lcd::SECONDS_PER_PAGE).round()));
    let slot = m.get_one::<u8>("slot").copied().unwrap_or(cfg.settings.lcd_slot);
    let kept = lcd::save_history(&frames, Some(file.as_path()))?;
    say(tr!("Copy kept in {path}", path = kept.display()));
    let mut kb = WiredKeyboard::open()?;
    let mut progress = |p: f32| {
        let n = (p * 40.0) as usize;
        print!("\r[{}{}] {:3.0}%", "#".repeat(n), ".".repeat(40 - n), p * 100.0);
        let _ = std::io::stdout().flush();
    };
    kb.upload_lcd(&blob, slot, &mut progress, &|| false)?;
    println!();
    say(tr("Uploaded to the keyboard."));
    cfg.settings.last_image = std::fs::canonicalize(file).unwrap_or_else(|_| file.clone()).display().to_string();
    cfg.save()
}

fn cmd_effects() {
    say(tr("Keyboard effects (stored in the keyboard, `ak980 light`):"));
    for m in &LIGHT_MODES {
        say(format!("  {:2}  {}", m.id, tr(m.name)));
    }
    say(tr("PC effects (need the agent or the app running, and the cable; `ak980 fx`):"));
    for e in &EFFECTS {
        say(format!("  {:10} {} — {}", e.id, tr(e.name), tr(e.description)));
    }
}

fn cmd_light(m: &ArgMatches, cfg: &mut Config) -> Result<()> {
    let p = cfg.profile_mut();
    let s = &mut p.lighting;
    if let Some(mode) = m.get_one::<String>("mode") {
        s.mode = lighting::mode_by_name(mode)?.id;
    }
    if let Some(c) = m.get_one::<[u8; 3]>("color") {
        s.set_rgb(*c);
        s.rainbow = false;
    }
    if let Some(r) = m.get_one::<bool>("rainbow") {
        s.rainbow = *r;
    }
    if let Some(v) = m.get_one::<u8>("brightness") {
        s.brightness = *v;
    }
    if let Some(v) = m.get_one::<u8>("speed") {
        s.speed = *v;
    }
    if let Some(v) = m.get_one::<u8>("direction") {
        s.direction = *v;
    }
    p.light_source = LightSource::Firmware;
    let s = p.lighting.clone();
    WiredKeyboard::open()?.set_lighting(&s)?;
    cfg.save()?;
    let mode = lighting::mode_by_id(s.mode);
    let colour = if mode.has(CAP_COLOR) && !s.rainbow { format!(", {}", html(s.rgb())) } else { String::new() };
    say(tr!("Lighting: {name}, brightness {b}, speed {s}", name = tr(mode.name), b = s.brightness, s = s.speed) + &colour);
    Ok(())
}

fn cmd_fx(m: &ArgMatches, cfg: &mut Config) -> Result<()> {
    let effect = m.get_one::<String>("effect");
    if m.get_flag("list") || effect.is_none() {
        for e in &EFFECTS {
            say(format!("{:10} {}", e.id, tr(e.name)));
            for prm in e.params {
                let extra = match prm.kind {
                    ParamKind::Choice { choices, .. } => choices.iter().map(|(id, _)| *id).collect::<Vec<_>>().join("|"),
                    ParamKind::Range { lo, hi, .. } => format!("{lo}..{hi}"),
                    ParamKind::Color(_) => "RRGGBB".into(),
                    ParamKind::Toggle(_) => "on|off".into(),
                };
                say(format!("    {}={extra}  ({})", prm.id, tr(prm.label)));
            }
        }
        return Ok(());
    }
    let id = effect.expect("checked above");
    let info = effects::info(id).ok_or_else(|| Error::invalid(tr!("Unknown PC effect: {name}", name = id)))?;
    let p = cfg.profile_mut();
    let mut values = p.host_values.get(info.id).cloned().unwrap_or_default();
    for item in m.get_many::<String>("values").into_iter().flatten() {
        let (key, value) = item.split_once('=').unwrap_or((item.as_str(), ""));
        let prm = info.params.iter().find(|x| x.id == key).ok_or_else(|| {
            Error::invalid(tr!("{effect} has no setting “{name}”", effect = info.id, name = key))
        })?;
        let v = match prm.kind {
            ParamKind::Color(_) => serde_json::Value::from(parse_color(value).map_err(Error::invalid)?.to_vec()),
            ParamKind::Range { lo, hi, .. } => {
                let n: i32 = value.trim().parse().map_err(|_| Error::invalid(format!("{key}={value}")))?;
                serde_json::Value::from(n.clamp(lo, hi))
            }
            ParamKind::Toggle(_) => serde_json::Value::from(parse_onoff(value).map_err(Error::invalid)?),
            ParamKind::Choice { choices, .. } => {
                if !choices.iter().any(|(c, _)| *c == value) {
                    let all = choices.iter().map(|(c, _)| *c).collect::<Vec<_>>().join(", ");
                    return Err(Error::invalid(tr!("{name} is one of: {choices}", name = key, choices = all)));
                }
                serde_json::Value::from(value)
            }
        };
        values.insert(key.to_string(), v);
    }
    p.host_effect = info.id.to_string();
    p.host_values.insert(info.id.to_string(), values);
    if let Some(b) = m.get_one::<u8>("brightness") {
        p.host_brightness = *b;
    }
    p.light_source = LightSource::Host;
    cfg.save()?;
    say(tr!("PC effect: {name}", name = tr(info.name)));
    if !runtime::agent_running() {
        say(tr("The agent is not running, so nothing is streamed yet. Start it with `ak980 agent` or enable it: \
                `ak980 service enable session`."));
    }
    Ok(())
}

fn cmd_side(m: &ArgMatches) -> Result<()> {
    let name = m.get_one::<String>("mode").expect("required");
    let mode = SIDE_LIGHT_MODES.iter().find(|(n, _)| n == name).map(|(_, v)| *v).unwrap_or(0x29);
    WiredKeyboard::open()?.set_side_light(mode)
}

fn cmd_custom(m: &ArgMatches, cfg: &mut Config) -> Result<()> {
    if m.get_flag("read") {
        let table = WiredKeyboard::open()?.read_custom_table()?;
        for chunk in table.chunks(64) {
            say(hex(chunk));
        }
        return Ok(());
    }
    let p = cfg.profile_mut();
    let mut colors = if m.get_flag("clear") { Default::default() } else { p.custom_colors.clone() };
    if let Some(c) = m.get_one::<[u8; 3]>("all") {
        colors = layout::keys().iter().map(|k| (k.index, *c)).collect();
    }
    if let Some(c) = m.get_one::<[u8; 3]>("wasd") {
        for name in ["W", "A", "S", "D"] {
            colors.insert(layout::find_key(name)?.index, *c);
        }
    }
    for spec in m.get_many::<String>("key").into_iter().flatten() {
        let (name, col) = spec.rsplit_once('=').unwrap_or((spec.as_str(), ""));
        colors.insert(layout::find_key(name)?.index, parse_color(col).map_err(Error::invalid)?);
    }
    if let Some(b) = m.get_one::<u8>("brightness") {
        p.custom_brightness = *b;
    }
    p.custom_colors = colors.clone();
    p.light_source = LightSource::Custom;
    let brightness = p.custom_brightness;
    WiredKeyboard::open()?.set_custom_colors(&colors, brightness)?;
    cfg.save()?;
    say(tr!("Colours of {n} keys written.", n = colors.len()));
    Ok(())
}

fn cmd_options(m: &ArgMatches, cfg: &mut Config) -> Result<()> {
    let o = &mut cfg.profile_mut().options;
    let mut changed = false;
    for (id, field) in [
        ("win-lock", &mut o.disable_win),
        ("alt-tab-lock", &mut o.disable_alt_tab),
        ("alt-f4-lock", &mut o.disable_alt_f4),
        ("fn-toggle", &mut o.fn_switch),
    ] {
        if let Some(v) = m.get_one::<bool>(id) {
            *field = *v;
            changed = true;
        }
    }
    for (id, field) in [("sleep", &mut o.sleep_time), ("response", &mut o.key_response)] {
        if let Some(v) = m.get_one::<u8>(id) {
            *field = *v;
            changed = true;
        }
    }
    let o = o.clone();
    if changed {
        WiredKeyboard::open()?.set_options(&o)?;
        cfg.save()?;
    }
    let yn = |b: bool| if b { tr("yes") } else { tr("no") };
    say(tr!("Win key locked: {v}", v = yn(o.disable_win)));
    say(tr!("Alt+Tab locked: {v}", v = yn(o.disable_alt_tab)));
    say(tr!("Alt+F4 locked: {v}", v = yn(o.disable_alt_f4)));
    say(tr!("Fn toggles instead of being held: {v}", v = yn(o.fn_switch)));
    let sleep = SLEEP_CHOICES.get(o.sleep_time as usize).map(|s| tr(s)).unwrap_or_else(|| o.sleep_time.to_string());
    say(tr!("Sleep after: {v}", v = sleep));
    say(tr!("Key response level: {v}", v = o.key_response));
    if !changed {
        say(tr("(saved values shown; nothing was sent)"));
    }
    Ok(())
}

fn cmd_keys() {
    let mut keys: Vec<_> = layout::keys().iter().collect();
    keys.sort_by_key(|k| (k.y, k.x));
    for k in keys {
        say(format!("{:4}  {:12} {}", k.index, k.id(), k.label));
    }
}

fn cmd_remap(m: &ArgMatches, cfg: &mut Config) -> Result<()> {
    let fn_layer = m.get_flag("fn");
    if m.get_flag("list") {
        let p = cfg.profile();
        let names = p.macro_names();
        for (layer, table) in [(tr("base"), &p.keymap_top), ("Fn".to_string(), &p.keymap_fn)] {
            for (idx, act) in table {
                let key = layout::by_index(*idx).map(|k| k.id()).unwrap_or_else(|| idx.to_string());
                say(format!("[{layer}] {key} → {}  ({})", act.describe(&names), act.to_spec()));
            }
        }
        if p.keymap_top.is_empty() && p.keymap_fn.is_empty() {
            say(tr("No keys are remapped."));
        }
        return Ok(());
    }
    let p = cfg.profile_mut();
    let names = p.macro_names();
    if m.get_flag("reset") {
        p.layer_mut(fn_layer).clear();
    } else if let (Some(key), Some(action)) = (m.get_one::<String>("key"), m.get_one::<String>("action")) {
        let k = layout::find_key(key)?;
        let act = KeyAction::parse(action)?;
        say(format!("{} → {}  ({})", k.id(), act.describe(&names), hex(&act.to_entry())));
        if act.is_default() {
            p.layer_mut(fn_layer).remove(&k.index);
        } else {
            p.layer_mut(fn_layer).insert(k.index, act);
        }
    } else {
        return Err(Error::invalid(tr("Give KEY ACTION, --reset or --list")));
    }
    let table = p.layer(fn_layer).clone();
    if m.get_flag("dry") {
        for chunk in encode_layer(&table).chunks(64) {
            say(hex(chunk));
        }
        return Ok(());
    }
    WiredKeyboard::open()?.set_keymap(fn_layer, &table)?;
    cfg.save()?;
    say(tr("Layer written to the keyboard."));
    Ok(())
}

fn cmd_macro(m: &ArgMatches, cfg: &mut Config) -> Result<()> {
    let (action, sub) = m.subcommand().expect("required");
    let p = cfg.profile_mut();
    match action {
        "list" => {
            for (i, mac) in p.macros.iter().enumerate() {
                say(format!("{i:3}  {}  ({})", mac.name, mac.steps.len()));
                if sub.get_flag("verbose") {
                    for s in &mac.steps {
                        say(format!("       {}", s.describe()));
                    }
                }
            }
            say(tr!("Memory: {used} of {total} bytes", used = macros::used_bytes(&p.macros), total = BUDGET_BYTES));
            return Ok(());
        }
        "add" => {
            let name = sub.get_one::<String>("name").expect("required");
            let mac = Macro::parse(sub.get_one::<String>("spec").expect("required"), Some(name))?;
            p.macros.push(mac);
            say(tr!("Macro {n}: {name}. Assign it to a key: ak980 remap KEY macro:{n}", n = p.macros.len() - 1,
                    name = name));
        }
        "rm" => {
            let i = *sub.get_one::<usize>("index").expect("required");
            let Some((removed, reset)) = p.remove_macro(i) else { return Err(Error::invalid(format!("{i}"))) };
            say(tr!("Removed: {name}. The macros after it moved up by one.", name = removed.name));
            if reset > 0 {
                say(plural(reset as i64, "{n} key that played it does what is printed on it again.",
                           "{n} keys that played it do what is printed on them again."));
            }
        }
        "clear" => {
            for i in (0..p.macros.len()).rev() {
                p.remove_macro(i);
            }
        }
        _ => {} // upload: send what the profile has
    }
    let list = p.macros.clone();
    let (top, fn_layer) = (p.keymap_top.clone(), p.keymap_fn.clone());
    let mut kb = WiredKeyboard::open()?;
    kb.set_macros(&list)?;
    // Keys refer to macros by slot: the layers follow when a removal moved them
    if action == "rm" || action == "clear" {
        kb.set_keymap(false, &top)?;
        kb.set_keymap(true, &fn_layer)?;
    }
    cfg.save()?;
    say(tr!("Macros written to the keyboard ({n}).", n = list.len()));
    Ok(())
}

fn cmd_launch(m: &ArgMatches, cfg: &mut Config) -> Result<()> {
    let s = &mut cfg.settings;
    let key = m.get_one::<String>("key");
    let rm = m.get_one::<String>("rm");
    if m.get_flag("list") || (key.is_none() && rm.is_none()) {
        for x in &s.launchers {
            say(format!("{:4} → {}", x.key, x.command));
        }
        if s.launchers.is_empty() {
            say(tr("No launcher keys. Example: ak980 launch F13 firefox, then ak980 remap F1 key:F13 --fn"));
        }
        return Ok(());
    }
    if let Some(rm) = rm {
        s.launchers.retain(|x| !x.key.eq_ignore_ascii_case(rm));
    } else {
        let key = key.expect("checked above").to_uppercase();
        if !LAUNCHER_KEYS.contains(&key.as_str()) {
            return Err(Error::invalid(tr("Launcher keys are F13–F24")));
        }
        let command = m.get_many::<String>("command").into_iter().flatten().cloned().collect::<Vec<_>>().join(" ");
        s.launchers.retain(|x| x.key != key);
        s.launchers.push(Launcher { key: key.clone(), command: command.clone(), label: String::new() });
        say(tr!("{key} runs: {cmd}. Now send {key} from a key: ak980 remap KEY key:{key}", key = key, cmd = command));
    }
    cfg.save()
}

fn apply_with_steps(cfg: &Config) -> Result<()> {
    let mut kb = WiredKeyboard::open()?;
    profiles::apply_profile(&mut kb, cfg.profile(), &ALL_PARTS, &mut |s| say(format!("… {s}")))
}

fn cmd_profile(m: &ArgMatches, cfg: &mut Config) -> Result<()> {
    let (action, sub) = m.subcommand().expect("required");
    let name = sub.try_get_one::<String>("name").ok().flatten().cloned().unwrap_or_default();
    let missing = |n: &str| Error::invalid(tr!("No profile named “{name}”", name = n));
    match action {
        "list" => {
            for p in &cfg.profiles {
                say(format!("{}{}", if p.name == cfg.active { "* " } else { "  " }, p.name));
            }
            return Ok(());
        }
        "new" => {
            let base = if sub.get_flag("copy") {
                Profile { name: name.clone(), ..cfg.profile().clone() }
            } else {
                Profile::named(name.clone())
            };
            let created = cfg.add(base);
            say(tr!("Profile created: {name}", name = created));
        }
        "rm" => cfg.remove(&name)?,
        "rename" => {
            cfg.rename(&name, sub.get_one::<String>("new").expect("required"))?;
        }
        "export" => {
            let p = cfg.get(&name).ok_or_else(|| missing(&name))?;
            config::export_profile(p, sub.get_one::<PathBuf>("file").expect("required"))?;
        }
        "import" => {
            let p = config::import_profile(sub.get_one::<PathBuf>("file").expect("required"))?;
            let added = cfg.add(p);
            say(tr!("Profile imported: {name}", name = added));
        }
        _ => {
            // use, apply
            if action == "use" {
                if cfg.get(&name).is_none() {
                    return Err(missing(&name));
                }
                cfg.active = name.clone();
            }
            apply_with_steps(cfg)?;
            say(tr!("Profile “{name}” is on the keyboard.", name = cfg.active));
        }
    }
    cfg.save()
}

fn cmd_auto(m: &ArgMatches, cfg: &mut Config) -> Result<()> {
    let (action, sub) = m.subcommand().expect("required");
    match action {
        "on" => cfg.settings.auto_switch = true,
        "off" => cfg.settings.auto_switch = false,
        "add" => {
            let process = sub.get_one::<String>("process").expect("required");
            let profile = sub.get_one::<String>("profile").expect("required");
            if cfg.get(profile).is_none() {
                return Err(Error::invalid(tr!("No profile named “{name}”", name = profile)));
            }
            cfg.settings.rules.retain(|r| !r.process.eq_ignore_ascii_case(process));
            cfg.settings.rules.push(Rule { process: process.clone(), profile: profile.clone() });
        }
        "rm" => {
            let process = sub.get_one::<String>("process").expect("required");
            cfg.settings.rules.retain(|r| !r.process.eq_ignore_ascii_case(process));
        }
        "default" => cfg.settings.default_profile = sub.get_one::<String>("profile").cloned().unwrap_or_default(),
        _ => {}
    }
    cfg.save()?;
    let state = if cfg.settings.auto_switch { tr("on") } else { tr("off") };
    say(tr!("Automatic switching: {state}", state = state));
    for r in &cfg.settings.rules {
        say(format!("  {} → {}", r.process, r.profile));
    }
    if !cfg.settings.default_profile.is_empty() {
        say(tr!("After the program exits: {name}", name = cfg.settings.default_profile));
    }
    Ok(())
}

fn cmd_service(m: &ArgMatches) -> Result<()> {
    let (action, sub) = m.subcommand().expect("required");
    if action == "list" {
        for method in service::methods() {
            let mark = if method.enabled { "●" } else { "○" };
            let note = if method.available { String::new() } else { format!("  {}", tr("(not available here)")) };
            let init = method.id.split_once('-').map(|(_, i)| i).unwrap_or(&method.id).to_string();
            let title = i18n::format(&tr(method.title), &[("init", &init)]);
            say(format!("{mark} {:14} {title}{note}", method.id));
            say(format!("  {:14} {}", "", tr(method.detail)));
        }
        return Ok(());
    }
    let id = sub.get_one::<String>("id").expect("required");
    if action == "enable" { service::enable(id)? } else { service::disable(id)? }
    say(tr("Done."));
    Ok(())
}

fn cmd_setup(m: &ArgMatches) -> Result<()> {
    if permissions::accessible() == Some(true) && !permissions::rule_outdated() && !m.get_flag("force") {
        say(tr("The keyboard is already accessible."));
        return Ok(());
    }
    permissions::install_rule()?;
    say(tr("Access rule installed. If the keyboard still is not accessible, reconnect it."));
    Ok(())
}

fn cmd_apply(cfg: &Config) -> Result<()> {
    let mut kb = WiredKeyboard::open()?;
    profiles::apply_profile(&mut kb, cfg.profile(), &ALL_PARTS, &mut |s| say(format!("… {s}")))?;
    kb.sync_clock(None, cfg.settings.clock_slot)?;
    say(tr("Done."));
    Ok(())
}

fn cmd_link(m: &ArgMatches) -> Result<()> {
    let n = m.get_one::<usize>("lines").copied().unwrap_or(20);
    let path = paths::link_log();
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let lines: Vec<&str> = text.lines().collect();
            for line in &lines[lines.len().saturating_sub(n)..] {
                say(line);
            }
        }
        Err(_) => say(tr!("Nothing recorded yet: {path} appears once the agent has seen the keyboard.",
                          path = path.display())),
    }
    Ok(())
}

fn cmd_reconnect() -> Result<()> {
    let dir = device::receiver_usb_dir().ok_or_else(|| Error::NotConnected(tr("AK980 2.4 GHz dongle not found")))?;
    service::run_as_root(&device::reconnect_script(&dir))?;
    say(tr("The receiver was reconnected. Give the keyboard a few seconds to find it again."));
    Ok(())
}

/// Starts the window: `ak980-gui` next to this program, or the one on PATH.
fn open_gui(tray: bool) -> Result<()> {
    let next_to_us = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("ak980-gui")));
    let exe = next_to_us.filter(|p| p.is_file()).or_else(|| service::which("ak980-gui"))
        .ok_or_else(|| Error::NotConnected(tr("The app window is not installed (ak980-gui).")))?;
    let mut cmd = std::process::Command::new(exe);
    if tray {
        cmd.arg("--tray");
    }
    use std::os::unix::process::CommandExt;
    Err(cmd.exec().into()) // exec only returns on failure
}

fn language_from_args(args: &[String]) -> Option<String> {
    args.iter().enumerate().find_map(|(i, a)| {
        a.strip_prefix("--lang=").map(str::to_string).or_else(|| (a == "--lang").then(|| args.get(i + 1).cloned()).flatten())
    })
}

fn run(m: &ArgMatches) -> Result<ExitCode> {
    let Some((name, sub)) = m.subcommand() else {
        open_gui(false)?;
        return Ok(ExitCode::SUCCESS);
    };
    if name == "agent" || name == "daemon" {
        // The agent keeps SIGPIPE ignored: a closed log pipe must not end it
        let code = agent::run(sub.get_flag("system"), sub.get_one::<u32>("interval").copied(), sub.get_flag("once"));
        return Ok(ExitCode::from(code.clamp(0, 255) as u8));
    }
    // Like other command-line tools, end quietly when the output is cut short (`ak980 keys | head`)
    // SAFETY: restoring the default action of one signal before any thread starts
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    let mut cfg = Config::load();
    match name {
        "gui" => open_gui(sub.get_flag("tray"))?,
        "status" => cmd_status(sub, &cfg)?,
        "list" => cmd_list(),
        "battery" => cmd_battery(sub, &mut cfg)?,
        "time" => cmd_time(sub, &cfg)?,
        "lcd" => cmd_lcd(sub, &mut cfg)?,
        "effects" | "modes" => cmd_effects(),
        "light" => cmd_light(sub, &mut cfg)?,
        "fx" => cmd_fx(sub, &mut cfg)?,
        "side" => cmd_side(sub)?,
        "custom" => cmd_custom(sub, &mut cfg)?,
        "options" => cmd_options(sub, &mut cfg)?,
        "keys" => cmd_keys(),
        "remap" => cmd_remap(sub, &mut cfg)?,
        "macro" => cmd_macro(sub, &mut cfg)?,
        "launch" => cmd_launch(sub, &mut cfg)?,
        "profile" => cmd_profile(sub, &mut cfg)?,
        "auto" => cmd_auto(sub, &mut cfg)?,
        "service" => cmd_service(sub)?,
        "setup" => cmd_setup(sub)?,
        "apply" => cmd_apply(&cfg)?,
        "link" => cmd_link(sub)?,
        "reconnect" => cmd_reconnect()?,
        _ => unreachable!("clap only accepts the commands above"),
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    // The language comes first: the help texts are translated
    i18n::set_language(&language_from_args(&args).unwrap_or_else(config::peek_language));
    let m = build().get_matches_from(&args);
    match run(&m) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{}", tr!("Error: {message}", message = e));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_is_well_formed() {
        build().debug_assert();
    }

    #[test]
    fn colours_and_switches() {
        assert_eq!(parse_color("#FFD200"), Ok([0xFF, 0xD2, 0x00]));
        assert!(parse_color("FFD2").is_err());
        assert!(parse_color("zzzzzz").is_err());
        assert_eq!(parse_onoff("ON"), Ok(true));
        assert!(parse_onoff("maybe").is_err());
    }

    #[test]
    fn language_option() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(language_from_args(&a(&["ak980", "--lang", "uk", "status"])), Some("uk".into()));
        assert_eq!(language_from_args(&a(&["ak980", "status", "--lang=en"])), Some("en".into()));
        assert_eq!(language_from_args(&a(&["ak980", "status"])), None);
    }

    #[test]
    fn subcommands_parse() {
        let m = build().try_get_matches_from(["ak980", "remap", "Delete", "key:PageDown", "--fn", "--dry"]).unwrap();
        let (name, sub) = m.subcommand().unwrap();
        assert_eq!(name, "remap");
        assert!(sub.get_flag("fn") && sub.get_flag("dry"));
        let m = build().try_get_matches_from(["ak980", "light", "single on", "--color", "ffd200", "--brightness", "5"]).unwrap();
        assert_eq!(m.subcommand().unwrap().1.get_one::<[u8; 3]>("color"), Some(&[0xFF, 0xD2, 0]));
        assert!(build().try_get_matches_from(["ak980", "light", "--brightness", "9"]).is_err());
        let m = build().try_get_matches_from(["ak980", "launch", "F13", "xdg-open", "https://example.org"]).unwrap();
        let cmd: Vec<&String> = m.subcommand().unwrap().1.get_many::<String>("command").unwrap().collect();
        assert_eq!(cmd, ["xdg-open", "https://example.org"]);
    }
}
