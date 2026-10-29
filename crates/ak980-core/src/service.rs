//! Starting the agent automatically, whatever the init system.
//!
//! Per-user (the full agent: clock, PC lighting effects, profile switching, launcher keys):
//! session autostart (XDG, `~/.config/autostart`) with any init, a systemd user service, or a dinit
//! user service (`dinit --user`, for example through turnstile on Artix or Chimera).
//!
//! System-wide (clock only, works without logging in), installed with root rights: systemd,
//! OpenRC, dinit, runit and s6. Packages ship the service files; for the AppImage or a source
//! install they are written from the templates in `ak980/data/init` with the path to the program.

use std::fs;
use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{Error, Result, paths, runtime, tr};

/// Init templates; `@AK980@` stands for the command that starts this program.
const TEMPLATES: &[(&str, &str)] = &[
    ("systemd/ak980-agent.service", include_str!("../../../ak980/data/init/systemd/ak980-agent.service")),
    ("systemd/ak980d.service", include_str!("../../../ak980/data/init/systemd/ak980d.service")),
    ("openrc/ak980d", include_str!("../../../ak980/data/init/openrc/ak980d")),
    ("dinit/ak980d", include_str!("../../../ak980/data/init/dinit/ak980d")),
    ("dinit/user/ak980-agent", include_str!("../../../ak980/data/init/dinit/user/ak980-agent")),
    ("runit/ak980d/run", include_str!("../../../ak980/data/init/runit/ak980d/run")),
    ("s6/ak980d-srv/type", include_str!("../../../ak980/data/init/s6/ak980d-srv/type")),
    ("s6/ak980d-srv/run", include_str!("../../../ak980/data/init/s6/ak980d-srv/run")),
    ("s6/ak980d-srv/dependencies.d/base", include_str!("../../../ak980/data/init/s6/ak980d-srv/dependencies.d/base")),
    ("xdg/ak980-agent.desktop", include_str!("../../../ak980/data/init/xdg/ak980-agent.desktop")),
];

/// `shlex.quote`: the text as it is when the shell takes it literally, otherwise in single quotes.
pub fn shell_quote(s: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "@%+=:,./-_".contains(c);
    if !s.is_empty() && s.chars().all(safe) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\"'\"'"))
    }
}

/// An executable called `name` on `PATH`.
pub fn which(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|p| p.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// The command line that starts the `ak980` program, as service files need it.
pub fn executable() -> String {
    if let Ok(appimage) = std::env::var("APPIMAGE")
        && Path::new(&appimage).is_file()
    {
        return shell_quote(&appimage);
    }
    // Installed or in a build tree, the CLI sits next to whichever of our programs runs
    if let Ok(me) = std::env::current_exe() {
        let sibling = me.with_file_name("ak980");
        if sibling.is_file() {
            return shell_quote(&sibling.to_string_lossy());
        }
    }
    which("ak980").map(|p| shell_quote(&p.to_string_lossy())).unwrap_or_else(|| "ak980".into())
}

pub fn render(template: &str, exe: Option<&str>) -> String {
    let text = TEMPLATES.iter().find(|(name, _)| *name == template).map(|(_, t)| *t).unwrap_or_default();
    match exe {
        Some(exe) => text.replace("@AK980@", exe),
        None => text.replace("@AK980@", &executable()),
    }
}

/// What a finished command left behind; code 127 when it could not start, 124 on timeout.
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
    argv: Vec<String>,
}

pub fn run(argv: &[&str], timeout: Duration) -> Output {
    let fail = |code, e: String| Output {
        code,
        stdout: String::new(),
        stderr: e,
        argv: argv.iter().map(|s| s.to_string()).collect(),
    };
    let mut child = match Command::new(argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return fail(127, e.to_string()),
    };
    // Read both pipes while waiting, so a chatty command cannot block on a full pipe
    let reader = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut s = String::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_string(&mut s);
            }
            s
        })
    };
    let out = reader(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err = reader(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let deadline = Instant::now() + timeout;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code().unwrap_or(-1),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill(); // our own child, by its handle
                let _ = child.wait();
                break 124;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => break -1,
        }
    };
    Output {
        code,
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
        argv: argv.iter().map(|s| s.to_string()).collect(),
    }
}

fn check(r: Output) -> Result<()> {
    if r.code == 0 {
        return Ok(());
    }
    let mut message = if r.stderr.trim().is_empty() { r.stdout.trim().to_string() } else { r.stderr.trim().to_string() };
    if message.is_empty() {
        let cmd: Vec<String> = r.argv.iter().map(|a| shell_quote(a)).collect();
        message = tr!("Command failed: {cmd}", cmd = cmd.join(" "));
    }
    if (r.code == 126 || r.code == 127) && r.argv.first().is_some_and(|a| a == "pkexec") {
        message = tr!("Authorisation was cancelled");
    }
    Err(Error::Io(io::Error::other(message)))
}

pub fn detect_init() -> &'static str {
    let pid1 = fs::read_to_string("/proc/1/comm").unwrap_or_default();
    let pid1 = pid1.trim();
    if Path::new("/run/systemd/system").is_dir() {
        "systemd"
    } else if pid1 == "dinit" {
        "dinit"
    } else if Path::new("/run/openrc").is_dir() {
        "openrc"
    } else if pid1 == "runit" || pid1 == "runit-init" || Path::new("/run/runit").is_dir() {
        "runit"
    } else if pid1 == "s6-svscan" || Path::new("/run/s6-rc").is_dir() {
        "s6"
    } else {
        "unknown"
    }
}

// ------------------------------------------------------------ the agent process

/// The PID of the running agent, from its instance lock, if that process really is our agent.
pub fn agent_pid() -> Option<i32> {
    if !runtime::agent_running() {
        return None;
    }
    let pid: i32 = fs::read_to_string(runtime::lock_path("agent")).ok()?.trim().parse().ok()?;
    let cmdline = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let args: Vec<&[u8]> = cmdline.split(|b| *b == 0).collect();
    let ours = args.iter().any(|a| *a == b"agent") && args.iter().any(|a| a.windows(5).any(|w| w == b"ak980"));
    ours.then_some(pid)
}

pub fn start_agent_now() -> Result<()> {
    if runtime::agent_running() {
        return Ok(());
    }
    let log = paths::state_home().join("ak980.log");
    if let Some(dir) = log.parent() {
        fs::create_dir_all(dir)?;
    }
    let out = fs::OpenOptions::new().create(true).append(true).open(&log)?;
    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(format!("exec {} agent", executable()))
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out);
    // SAFETY: setsid only detaches the child from our session, as Python's start_new_session does
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn()?;
    Ok(())
}

/// Stops the agent that holds the instance lock: by its PID, never by a name pattern, so nothing
/// else in the session can be hit.
pub fn stop_agent_now() {
    if let Some(pid) = agent_pid() {
        // SAFETY: a plain signal to a process we just identified
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
}

// ------------------------------------------------------------ session autostart (XDG)

const XDG_SYSTEM: &str = "/etc/xdg/autostart/ak980-agent.desktop";
const XDG_HIDDEN: &str = "[Desktop Entry]\nType=Application\nName=Ajazz AK980 V2 agent\nHidden=true\n";

/// The autostart entries: the user's own and the one a package installs.
pub struct Xdg {
    pub user: PathBuf,
    pub system: PathBuf,
}

impl Xdg {
    pub fn real() -> Self {
        Self { user: paths::config_home().join("autostart").join("ak980-agent.desktop"), system: XDG_SYSTEM.into() }
    }

    fn hidden(path: &Path) -> bool {
        match fs::read_to_string(path) {
            Ok(text) => text.contains("Hidden=true") || text.contains("X-GNOME-Autostart-enabled=false"),
            Err(_) => true,
        }
    }

    pub fn enabled(&self) -> bool {
        if self.user.exists() {
            return !Self::hidden(&self.user);
        }
        self.system.exists() && !Self::hidden(&self.system)
    }

    pub fn enable(&self, start: bool) -> Result<()> {
        if let Some(dir) = self.user.parent() {
            fs::create_dir_all(dir)?;
        }
        if self.system.exists() {
            if self.user.exists() {
                fs::remove_file(&self.user)?; // the packaged entry applies again
            }
        } else {
            fs::write(&self.user, render("xdg/ak980-agent.desktop", None))?;
        }
        if start {
            start_agent_now()?;
        }
        Ok(())
    }

    pub fn disable(&self, stop: bool) -> Result<()> {
        if self.system.exists() {
            if let Some(dir) = self.user.parent() {
                fs::create_dir_all(dir)?;
            }
            fs::write(&self.user, XDG_HIDDEN)?;
        } else if self.user.exists() {
            fs::remove_file(&self.user)?;
        }
        if stop {
            stop_agent_now();
        }
        Ok(())
    }
}

// ------------------------------------------------------------ systemd user service

const SYSTEMD_USER_PACKAGED: &str = "/usr/lib/systemd/user/ak980-agent.service";
const SHORT: Duration = Duration::from_secs(10);

fn systemd_user_unit() -> PathBuf {
    paths::config_home().join("systemd").join("user").join("ak980-agent.service")
}

pub fn systemd_user_available() -> bool {
    which("systemctl").is_some() && Path::new("/run/systemd/system").is_dir()
}

pub fn systemd_user_enabled() -> bool {
    run(&["systemctl", "--user", "is-enabled", "ak980-agent.service"], SHORT).stdout.trim() == "enabled"
}

pub fn systemd_user_enable() -> Result<()> {
    if !Path::new(SYSTEMD_USER_PACKAGED).exists() {
        let unit = systemd_user_unit();
        if let Some(dir) = unit.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&unit, render("systemd/ak980-agent.service", None))?;
        run(&["systemctl", "--user", "daemon-reload"], SHORT);
    }
    check(run(&["systemctl", "--user", "enable", "--now", "ak980-agent.service"], SHORT))
}

pub fn systemd_user_disable() {
    run(&["systemctl", "--user", "disable", "--now", "ak980-agent.service"], SHORT);
}

// ------------------------------------------------------------ dinit user service

const DINIT_USER_PACKAGED: &str = "/etc/dinit.d/user/ak980-agent";

fn dinit_user_dir() -> PathBuf {
    paths::config_home().join("dinit.d")
}

pub fn dinit_user_available() -> bool {
    which("dinitctl").is_some() && run(&["dinitctl", "--user", "list"], Duration::from_secs(3)).code == 0
}

pub fn dinit_user_enabled() -> bool {
    dinit_user_dir().join("boot.d").join("ak980-agent").symlink_metadata().is_ok()
}

pub fn dinit_user_enable() -> Result<()> {
    if !Path::new(DINIT_USER_PACKAGED).exists() {
        fs::create_dir_all(dinit_user_dir())?;
        fs::write(dinit_user_dir().join("ak980-agent"), render("dinit/user/ak980-agent", None))?;
    }
    check(run(&["dinitctl", "--user", "enable", "ak980-agent"], SHORT))
}

pub fn dinit_user_disable() {
    run(&["dinitctl", "--user", "disable", "ak980-agent"], SHORT);
    run(&["dinitctl", "--user", "stop", "ak980-agent"], SHORT);
}

// ------------------------------------------------------------ system services (root)

/// (template, destination, packaged copy that makes writing unnecessary, mode) per init system.
pub const SYSTEM_FILES: &[(&str, &[(&str, &str, Option<&str>, u32)])] = &[
    ("systemd", &[("systemd/ak980d.service", "/etc/systemd/system/ak980d.service",
                   Some("/usr/lib/systemd/system/ak980d.service"), 0o644)]),
    ("openrc", &[("openrc/ak980d", "/etc/init.d/ak980d", None, 0o755)]),
    ("dinit", &[("dinit/ak980d", "/etc/dinit.d/ak980d", None, 0o644)]),
    ("runit", &[("runit/ak980d/run", "{runit_sv}/ak980d/run", None, 0o755)]),
    ("s6", &[("s6/ak980d-srv/type", "/etc/s6/sv/ak980d-srv/type", None, 0o644),
             ("s6/ak980d-srv/run", "/etc/s6/sv/ak980d-srv/run", None, 0o755),
             ("s6/ak980d-srv/dependencies.d/base", "/etc/s6/sv/ak980d-srv/dependencies.d/base", None, 0o644)]),
];

pub const INIT_TITLES: [(&str, &str); 5] =
    [("systemd", "systemd"), ("openrc", "OpenRC"), ("dinit", "dinit"), ("runit", "runit"), ("s6", "s6")];

fn system_files(init: &str) -> &'static [(&'static str, &'static str, Option<&'static str>, u32)] {
    SYSTEM_FILES.iter().find(|(name, _)| *name == init).map(|(_, files)| *files).unwrap_or(&[])
}

/// (service definitions, enabled services) for Artix-style or Void-style runit.
fn runit_dirs() -> (&'static str, &'static str) {
    if Path::new("/etc/runit/sv").is_dir() { ("/etc/runit/sv", "/run/runit/service") } else { ("/etc/sv", "/var/service") }
}

fn lexists(path: impl AsRef<Path>) -> bool {
    path.as_ref().symlink_metadata().is_ok()
}

pub fn system_enabled(init: &str) -> bool {
    match init {
        "systemd" => run(&["systemctl", "is-enabled", "ak980d.service"], SHORT).stdout.trim() == "enabled",
        "openrc" => lexists("/etc/runlevels/default/ak980d"),
        "dinit" => lexists("/etc/dinit.d/boot.d/ak980d"),
        "runit" => lexists(Path::new(runit_dirs().1).join("ak980d")),
        "s6" => lexists("/etc/s6/adminsv/default/contents.d/ak980d-srv"),
        _ => false,
    }
}

/// Shell script (run as root) that installs and enables, or disables, the system service.
pub fn system_script(init: &str, enable: bool) -> String {
    let (sv, active) = runit_dirs();
    let mut lines = vec!["set -e".to_string()];
    if enable {
        for (template, dest, packaged, mode) in system_files(init) {
            if packaged.is_some_and(|p| Path::new(p).exists()) {
                continue;
            }
            let dest = dest.replace("{runit_sv}", sv);
            if Path::new(&dest).exists() {
                continue;
            }
            let parent = Path::new(&dest).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
            lines.push(format!("mkdir -p {}", shell_quote(&parent)));
            lines.push(format!("cat > {} <<'AK980_EOF'\n{}AK980_EOF", shell_quote(&dest), render(template, None)));
            lines.push(format!("chmod {mode:o} {}", shell_quote(&dest)));
        }
    }
    let command = match (init, enable) {
        ("systemd", true) => "systemctl daemon-reload && systemctl enable --now ak980d.service".to_string(),
        ("systemd", false) => "systemctl disable --now ak980d.service".into(),
        ("openrc", true) => "rc-update add ak980d default && rc-service ak980d start".into(),
        ("openrc", false) => "rc-service ak980d stop; rc-update del ak980d default".into(),
        ("dinit", true) => "dinitctl enable ak980d".into(),
        ("dinit", false) => "dinitctl disable ak980d; dinitctl stop ak980d".into(),
        ("runit", true) => format!("ln -sf {sv}/ak980d {active}/"),
        ("runit", false) => format!("rm -f {active}/ak980d"),
        ("s6", true) => "s6-service add default ak980d-srv && s6-db-reload".into(),
        ("s6", false) => "s6-service delete default ak980d-srv && s6-db-reload".into(),
        _ => "true".into(),
    };
    lines.push(command);
    lines.join("\n") + "\n"
}

/// Runs a script as root: directly when we are root, otherwise through pkexec, which asks the user.
pub fn run_as_root(script: &str) -> Result<()> {
    // SAFETY: geteuid has no preconditions
    if unsafe { libc::geteuid() } == 0 {
        return check(run(&["sh", "-c", script], Duration::from_secs(60)));
    }
    if which("pkexec").is_none() {
        return Err(Error::Permission(tr!(
            "Root rights are needed. Run it with sudo:\n{cmd}",
            cmd = format!("sudo sh -c {}", shell_quote(script))
        )));
    }
    check(run(&["pkexec", "sh", "-c", script], Duration::from_secs(300)))
}

// ------------------------------------------------------------ listing

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Method {
    pub id: String,
    /// English; translate with `i18n::tr`.
    pub title: &'static str,
    /// English; translate with `i18n::tr`.
    pub detail: &'static str,
    /// session, user or system
    pub scope: &'static str,
    pub available: bool,
    pub enabled: bool,
}

pub const USER_METHODS: [&str; 3] = ["session", "systemd-user", "dinit-user"];

pub fn methods() -> Vec<Method> {
    let init = detect_init();
    let mut out = vec![Method {
        id: "session".into(),
        title: "Start with the desktop session",
        detail: "XDG autostart. The full agent, with any init system.",
        scope: "session",
        available: true,
        enabled: Xdg::real().enabled(),
    }];
    if systemd_user_available() {
        out.push(Method {
            id: "systemd-user".into(),
            title: "systemd user service",
            detail: "The full agent, managed with systemctl --user.",
            scope: "user",
            available: true,
            enabled: systemd_user_enabled(),
        });
    }
    if which("dinitctl").is_some() {
        let available = dinit_user_available();
        out.push(Method {
            id: "dinit-user".into(),
            title: "dinit user service",
            detail: "The full agent, managed with dinitctl --user.",
            scope: "user",
            available,
            enabled: available && dinit_user_enabled(),
        });
    }
    if !system_files(init).is_empty() {
        out.push(Method {
            id: format!("system-{init}"),
            title: "System service ({init})",
            detail: "Clock only, also before you log in. Needs root rights once.",
            scope: "system",
            available: true,
            enabled: system_enabled(init),
        });
    }
    out
}

/// The native per-user service manager when there is one, session autostart otherwise.
pub fn preferred_user_method() -> String {
    methods().into_iter().find(|m| m.scope == "user" && m.available).map(|m| m.id).unwrap_or_else(|| "session".into())
}

/// Enables one way to start the agent; the other per-user ways are turned off, so one agent runs.
pub fn enable(method_id: &str) -> Result<()> {
    if USER_METHODS.contains(&method_id) {
        for other in USER_METHODS.iter().filter(|m| **m != method_id) {
            let _ = disable_user(other);
        }
    }
    match method_id {
        "session" => Xdg::real().enable(true),
        "systemd-user" => systemd_user_enable(),
        "dinit-user" => dinit_user_enable(),
        id if id.starts_with("system-") => run_as_root(&system_script(&id["system-".len()..], true)),
        id => Err(Error::invalid(id.to_string())),
    }
}

fn disable_user(method_id: &str) -> Result<()> {
    match method_id {
        "session" if Xdg::real().enabled() => Xdg::real().disable(false),
        "systemd-user" if systemd_user_available() && systemd_user_enabled() => {
            systemd_user_disable();
            Ok(())
        }
        "dinit-user" if which("dinitctl").is_some() && dinit_user_enabled() => {
            dinit_user_disable();
            Ok(())
        }
        _ => Ok(()),
    }
}

pub fn disable(method_id: &str) -> Result<()> {
    match method_id {
        "session" => Xdg::real().disable(true),
        "systemd-user" => {
            systemd_user_disable();
            Ok(())
        }
        "dinit-user" => {
            dinit_user_disable();
            Ok(())
        }
        id if id.starts_with("system-") => run_as_root(&system_script(&id["system-".len()..], false)),
        id => Err(Error::invalid(id.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ak980-service-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn templates_render() {
        for (name, _) in TEMPLATES {
            let text = render(name, Some("/opt/ak980"));
            assert!(!text.contains("@AK980@"), "{name} keeps the placeholder");
        }
        assert!(render("systemd/ak980-agent.service", Some("/opt/ak980")).contains("/opt/ak980 agent"));
    }

    #[test]
    fn system_scripts() {
        for (init, _) in SYSTEM_FILES {
            let on = system_script(init, true);
            let off = system_script(init, false);
            assert!(on.starts_with("set -e\n") && off.starts_with("set -e\n"));
            assert_ne!(on.lines().last(), off.lines().last(), "{init}");
        }
        assert!(system_script("openrc", true).ends_with("rc-update add ak980d default && rc-service ak980d start\n"));
    }

    #[test]
    fn session_method_always_offered() {
        assert_eq!(methods()[0].id, "session");
    }

    #[test]
    fn xdg_without_packaged_entry() {
        let d = temp("plain");
        let x = Xdg { user: d.join("user/ak980-agent.desktop"), system: d.join("missing.desktop") };
        assert!(!x.enabled());
        x.enable(false).unwrap();
        assert!(x.enabled() && fs::read_to_string(&x.user).unwrap().contains(" agent"));
        x.disable(false).unwrap();
        assert!(!x.enabled() && !x.user.exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn xdg_with_packaged_entry() {
        let d = temp("packaged");
        let x = Xdg { user: d.join("user/ak980-agent.desktop"), system: d.join("system.desktop") };
        fs::write(&x.system, render("xdg/ak980-agent.desktop", Some("/usr/bin/ak980"))).unwrap();
        assert!(x.enabled());
        x.disable(false).unwrap(); // a Hidden=true override, the packaged file stays
        assert!(!x.enabled() && x.system.exists());
        x.enable(false).unwrap(); // the override goes, the packaged entry applies again
        assert!(x.enabled() && !x.user.exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn quoting_and_commands() {
        assert_eq!(shell_quote("/usr/bin/ak980"), "/usr/bin/ak980");
        assert_eq!(shell_quote("/home/x/My Apps/a'b.AppImage"), "'/home/x/My Apps/a'\"'\"'b.AppImage'");
        let r = run(&["sh", "-c", "echo out; echo err >&2; exit 3"], SHORT);
        assert_eq!((r.code, r.stdout.trim(), r.stderr.trim()), (3, "out", "err"));
        assert_eq!(run(&["/nonexistent/ak980-command"], SHORT).code, 127);
        assert_eq!(run(&["sleep", "5"], Duration::from_millis(100)).code, 124);
    }
}
