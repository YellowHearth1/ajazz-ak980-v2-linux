//! Starting the agent automatically, whatever the init system.
//!
//! Per-user (the full agent: clock, PC lighting effects, profile switching, launcher keys):
//! session autostart (XDG, `~/.config/autostart`) with any init, a systemd user service, or a
//! service of the user's own dinit or runsvdir. Those two are started at login by turnstile (it
//! has its own PAM module and suits seatd) or by userspawn (it follows logind, so elogind), each
//! with its own directories; they are read from the running instance, never assumed.
//!
//! System-wide (clock only, works without logging in), installed with root rights: systemd,
//! OpenRC, dinit, runit and s6. Packages ship the service files; for the AppImage or a source
//! install they are written from the templates in `ak980/data/init` with the path to the program.
//!
//! None of this may keep a system from booting. dinit will not start `boot` at all while a service
//! it waits for has a description it cannot load (a missing description it skips), and the user's
//! `boot` brings up dbus and the sound server too. So every description is checked by the installed
//! dinit's own checker before it is written, every file is replaced in one rename and is never
//! half-written, services are switched on only through the init system's own tool, which loads them
//! first, and what such a tool refuses is taken away again. A system service only ever runs a
//! program that nobody but root can change.

use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
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
    ("runit/user/ak980-agent/run", include_str!("../../../ak980/data/init/runit/user/ak980-agent/run")),
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
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(name))
        .find(|p| p.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// The path of the `ak980` program that service files start: the AppImage, the CLI next to whichever
/// of our programs runs, or the one on `PATH`.
pub fn executable() -> String {
    if let Ok(appimage) = std::env::var("APPIMAGE")
        && Path::new(&appimage).is_file()
    {
        return appimage;
    }
    if let Ok(me) = std::env::current_exe() {
        let sibling = me.with_file_name("ak980");
        if sibling.is_file() {
            return sibling.to_string_lossy().into_owned();
        }
    }
    which("ak980").map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "ak980".into())
}

/// How the program's path is written in each kind of service file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Syntax {
    Shell,
    Dinit,
    Systemd,
    Desktop,
}

fn syntax(template: &str) -> Syntax {
    match template.split('/').next() {
        Some("dinit") => Syntax::Dinit,
        Some("systemd") => Syntax::Systemd,
        Some("xdg") => Syntax::Desktop,
        _ => Syntax::Shell,
    }
}

/// A path that every kind of service file takes as it is.
fn plain(path: &str) -> bool {
    !path.is_empty() && path.chars().all(|c| c.is_ascii_alphanumeric() || "/._+-".contains(c))
}

/// `path` as one argument in a file of this syntax. dinit, for one, knows no single quotes: a path
/// the shell way would split at its first space.
fn quote(path: &str, syntax: Syntax) -> String {
    if plain(path) {
        return path.to_string();
    }
    let escaped = |special: &str| {
        let mut s = String::new();
        for c in path.chars() {
            if special.contains(c) {
                s.push('\\');
            }
            s.push(c);
        }
        s
    };
    match syntax {
        Syntax::Shell => shell_quote(path),
        // Double quotes and backslash escapes; `$` starts a variable in both, `%` a systemd specifier
        Syntax::Dinit => format!("\"{}\"", escaped("\\\"").replace('$', "$$")),
        Syntax::Systemd => format!("\"{}\"", escaped("\\\"").replace('$', "$$").replace('%', "%%")),
        // The Exec quoting, then the escaping of every string value, which doubles each backslash
        Syntax::Desktop => format!("\"{}\"", escaped("\"`$\\")).replace('\\', "\\\\").replace('%', "%%"),
    }
}

/// The template with `exe` as the program, written the way that kind of file reads it.
pub fn render(template: &str, exe: &str) -> String {
    let text = TEMPLATES.iter().find(|(name, _)| *name == template).map(|(_, t)| *t).unwrap_or_default();
    text.replace("@AK980@", &quote(exe, syntax(template)))
}

/// The path of the program for a service file; one with a line break or another control character
/// cannot be written into any of them.
fn service_exe() -> Result<String> {
    let exe = executable();
    if exe.chars().any(char::is_control) {
        return Err(Error::invalid(tr!("The path to the program cannot be used in a service: {path}", path = exe)));
    }
    Ok(exe)
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
    // pkexec answers 126 (dismissed) or 127 (not authorised) with "Error executing command as
    // another user"; a 127 of the script itself (a command it did not find) keeps its own message
    let refused = r.stderr.contains("another user") || r.stderr.trim().is_empty();
    if (r.code == 126 || r.code == 127) && refused && r.argv.first().is_some_and(|a| a == "pkexec") {
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

// ------------------------------------------------------------ writing service files safely

/// A new private directory for files that only exist to be checked.
fn scratch_dir(tag: &str) -> io::Result<PathBuf> {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ak980-{tag}-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir)?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
    Ok(dir)
}

/// Replaces `path` in one rename, so that at any moment it is the old file or the whole new one,
/// even if the computer goes off half-way.
pub fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let new = dir.join(format!(".{name}.ak980-new"));
    let mut f = fs::File::create(&new)?;
    f.write_all(text.as_bytes())?;
    f.sync_all()?;
    drop(f);
    fs::rename(&new, path)?;
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// Whether `sh` reads `script` without a syntax error.
fn shell_syntax(script: &str) -> Result<()> {
    let dir = scratch_dir("sh-check")?;
    let file = dir.join("script");
    fs::write(&file, script)?;
    let r = run(&["sh", "-n", &file.to_string_lossy()], SHORT);
    let _ = fs::remove_dir_all(&dir);
    if r.code == 0 {
        Ok(())
    } else {
        Err(Error::invalid(tr!("The service script has an error:\n{msg}", msg = r.stderr.trim())))
    }
}

/// Whether a system service may start `exe`: a file that only root can change, in folders that only
/// root can change. One that ran a program from a home folder would give root (or the `input`
/// group, and with it every keystroke) to whatever can write there.
pub fn root_only(exe: &Path) -> bool {
    let Ok(real) = fs::canonicalize(exe) else { return false };
    real.is_file() && real.ancestors().all(|p| p.metadata().is_ok_and(|m| m.uid() == 0 && m.mode() & 0o022 == 0))
}

// ------------------------------------------------------------ dinit

/// dinit's own checker: `dinit-check` since dinit 0.20, `dinitcheck` before.
fn dinit_checker() -> Option<PathBuf> {
    which("dinit-check").or_else(|| which("dinitcheck"))
}

/// The installed dinit's (major, minor) version, from `Dinit version 0.22.1.`
fn dinit_version() -> Option<(u32, u32)> {
    which("dinit")?;
    parse_dinit_version(&run(&["dinit", "--version"], Duration::from_secs(3)).stdout)
}

fn parse_dinit_version(text: &str) -> Option<(u32, u32)> {
    let version = text.lines().next()?.split_whitespace().nth(2)?;
    let mut parts = version.trim_end_matches('.').split('.');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// Our description with only the settings this dinit knows: `log-type` came with 0.17, and an
/// unknown setting makes the whole description unloadable.
fn fit_dinit(text: &str, version: Option<(u32, u32)>) -> String {
    if version.is_some_and(|v| v >= (0, 17)) {
        return text.to_string();
    }
    text.lines().filter(|l| !l.trim_start().starts_with("log-type")).map(|l| format!("{l}\n")).collect()
}

/// Whether dinit's checker reads `text` as the description `name` without a single error or
/// warning (a program that is not there is a warning too). `Ok(false)`: there is no checker.
fn dinit_accepts(name: &str, text: &str) -> Result<bool> {
    let Some(checker) = dinit_checker() else { return Ok(false) };
    let dir = scratch_dir("dinit-check")?;
    fs::write(dir.join(name), text)?;
    let r = run(&[&checker.to_string_lossy(), "-d", &dir.to_string_lossy(), name], SHORT);
    let _ = fs::remove_dir_all(&dir);
    if r.code == 0 {
        return Ok(true);
    }
    let found: Vec<&str> = r.stdout.lines().chain(r.stderr.lines())
        .filter(|l| l.contains("error") || l.contains("warning") || l.contains("unknown"))
        .collect();
    Err(Error::invalid(tr!("dinit does not accept the service description, so it was not written:\n{msg}",
                           msg = found.join("\n"))))
}

/// The command lines of this user's processes.
fn own_processes() -> Vec<Vec<String>> {
    // SAFETY: getuid has no preconditions
    let uid = unsafe { libc::getuid() };
    let Ok(entries) = fs::read_dir("/proc") else { return vec![] };
    entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
        .filter(|e| e.metadata().is_ok_and(|m| m.uid() == uid))
        .filter_map(|e| fs::read(e.path().join("cmdline")).ok())
        .map(|raw| raw.split(|b| *b == 0).filter(|a| !a.is_empty()).map(|a| String::from_utf8_lossy(a).into_owned()).collect())
        .collect()
}

fn program_is(args: &[String], name: &str) -> bool {
    args.first().is_some_and(|a| Path::new(a).file_name().is_some_and(|n| n == name))
}

/// The service directories of a dinit started with `args`, in its order of search: the ones it was
/// given (turnstile gives its generated `boot` first, then `~/.config/dinit.d` and the system's),
/// or dinit's defaults for a user (a plain `dinit --user`, as userspawn starts it).
fn dinit_dirs(args: &[String]) -> Vec<PathBuf> {
    let mut dirs = vec![];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--services-dir" || a == "-d" {
            dirs.extend(it.next().map(PathBuf::from));
        } else if let Some(d) = a.strip_prefix("--services-dir=") {
            dirs.push(d.into());
        }
    }
    if dirs.is_empty() {
        dirs = vec![paths::config_home().join("dinit.d"), "/etc/dinit.d/user".into(),
                    "/usr/lib/dinit.d/user".into(), "/usr/local/lib/dinit.d/user".into()];
    }
    dirs
}

/// What `boot` waits for, as `dinitctl enable` finds it: the `waits-for.d` of the first `boot`
/// description in `dirs`, relative to that description's directory.
fn boot_dir(dirs: &[PathBuf]) -> Option<PathBuf> {
    let boot = dirs.iter().map(|d| d.join("boot")).find(|p| p.is_file())?;
    let text = fs::read_to_string(&boot).ok()?;
    text.lines().find_map(|line| {
        let line = line.split(" #").next().unwrap_or_default().trim();
        let value = line.strip_prefix("waits-for.d")?.trim_start().strip_prefix(['=', ':'])?.trim().trim_matches('"');
        let path = Path::new(value);
        Some(if path.is_absolute() { path.to_path_buf() } else { boot.parent()?.join(path) })
    })
}

/// A dinit instance whose services we manage: the user's own, or a private one in the tests.
struct Dinit {
    /// The directory our descriptions go to.
    dir: PathBuf,
    /// Where the links of the services that `boot` waits for are.
    boot_dir: PathBuf,
    /// What dinitctl needs to reach this instance.
    ctl: Vec<String>,
}

impl Dinit {
    /// The user's running dinit, whoever started it; without one, where dinit looks by default.
    fn user() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let runtime = runtime::runtime_dir();
        let dirs = own_processes()
            .into_iter()
            .find(|args| program_is(args, "dinit") && args.iter().any(|a| a == "--user" || a == "-u"))
            .map(|args| dinit_dirs(&args[1..]))
            .unwrap_or_else(|| dinit_dirs(&[]));
        // The first one of the user's own, not turnstile's generated one in the runtime directory
        let dir = dirs
            .iter()
            .find(|d| d.starts_with(&home) && !d.starts_with(&runtime))
            .cloned()
            .unwrap_or_else(|| paths::config_home().join("dinit.d"));
        let boot_dir = boot_dir(&dirs).unwrap_or_else(|| dir.join("boot.d"));
        Self { dir, boot_dir, ctl: vec!["--user".into()] }
    }

    fn ctl(&self, args: &[&str]) -> Output {
        let mut argv = vec!["dinitctl"];
        argv.extend(self.ctl.iter().map(String::as_str));
        argv.extend(args);
        run(&argv, SHORT)
    }

    fn enabled(&self, name: &str) -> bool {
        lexists(self.boot_dir.join(name))
    }

    /// Puts `text` in place as the description `name` once dinit's checker has accepted it.
    /// Returns whether a description went in that nothing has checked yet (there is no checker):
    /// the running dinit judges that one when the service is enabled.
    fn write(&self, name: &str, text: &str) -> Result<bool> {
        let path = self.dir.join(name);
        if fs::read_to_string(&path).is_ok_and(|old| old == text) {
            return Ok(false);
        }
        let checked = dinit_accepts(name, text)?;
        if !checked && lexists(&path) {
            // This one may be linked from boot.d already, and dinit loads it only at the next boot
            return Err(Error::invalid(tr!(
                "dinit-check is not installed, so the new service description cannot be checked. Remove {path} and try again.",
                path = path.display())));
        }
        write_atomic(&path, text)?;
        // A loaded service takes the new description now, or when it next starts
        let _ = self.ctl(&["reload", name]);
        Ok(!checked)
    }

    /// Enables `name` (dinit loads it first and refuses one it cannot load); a description we just
    /// wrote unchecked goes again if dinit refused it.
    fn enable(&self, name: &str, unchecked: bool) -> Result<()> {
        let r = self.ctl(&["enable", name]);
        if r.code != 0 && unchecked && !self.enabled(name) {
            let _ = fs::remove_file(self.dir.join(name));
        }
        check(r)
    }

    fn disable(&self, name: &str) {
        self.ctl(&["disable", name]);
        self.ctl(&["stop", name]);
        // Without a running instance dinitctl cannot reach boot.d; the link goes all the same
        let link = self.boot_dir.join(name);
        if lexists(&link) {
            let _ = fs::remove_file(link);
        }
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
    let mut cmd = Command::new(executable());
    cmd.arg("agent").stdin(Stdio::null()).stdout(out.try_clone()?).stderr(out);
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
        if self.system.exists() {
            if self.user.exists() {
                fs::remove_file(&self.user)?; // the packaged entry applies again
            }
        } else {
            write_atomic(&self.user, &render("xdg/ak980-agent.desktop", &service_exe()?))?;
        }
        if start {
            start_agent_now()?;
        }
        Ok(())
    }

    pub fn disable(&self, stop: bool) -> Result<()> {
        if self.system.exists() {
            write_atomic(&self.user, XDG_HIDDEN)?;
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

/// systemd loads a unit before it enables it and never holds up the boot for a unit it cannot load.
pub fn systemd_user_enable() -> Result<()> {
    if !Path::new(SYSTEMD_USER_PACKAGED).exists() {
        let unit = systemd_user_unit();
        let text = render("systemd/ak980-agent.service", &service_exe()?);
        if fs::read_to_string(&unit).ok().as_deref() != Some(text.as_str()) {
            write_atomic(&unit, &text)?;
            run(&["systemctl", "--user", "daemon-reload"], SHORT);
        }
    }
    check(run(&["systemctl", "--user", "enable", "--now", "ak980-agent.service"], SHORT))
}

pub fn systemd_user_disable() {
    run(&["systemctl", "--user", "disable", "--now", "ak980-agent.service"], SHORT);
}

// ------------------------------------------------------------ dinit user service

const DINIT_USER_PACKAGED: &str = "/etc/dinit.d/user/ak980-agent";

pub fn dinit_user_available() -> bool {
    which("dinitctl").is_some() && run(&["dinitctl", "--user", "list"], Duration::from_secs(3)).code == 0
}

pub fn dinit_user_enabled() -> bool {
    Dinit::user().enabled("ak980-agent")
}

pub fn dinit_user_enable() -> Result<()> {
    let d = Dinit::user();
    let mut unchecked = false;
    if !Path::new(DINIT_USER_PACKAGED).exists() {
        let text = fit_dinit(&render("dinit/user/ak980-agent", &service_exe()?), dinit_version());
        unchecked = d.write("ak980-agent", &text)?;
    }
    d.enable("ak980-agent", unchecked)
}

pub fn dinit_user_disable() {
    Dinit::user().disable("ak980-agent");
}

// ------------------------------------------------------------ runit user service

/// The directory that this user's own runsvdir supervises, if one runs: turnstile's runit backend
/// uses `~/.config/service`, userspawn's runit setup a directory of its choice.
fn user_runsvdir() -> Option<PathBuf> {
    own_processes()
        .into_iter()
        .find(|args| program_is(args, "runsvdir"))
        .and_then(|args| args[1..].iter().find(|a| !a.starts_with('-')).map(PathBuf::from))
        .filter(|dir| dir.is_dir())
}

pub fn runit_user_enabled() -> bool {
    user_runsvdir().is_some_and(|dir| dir.join("ak980-agent").join("run").is_file())
}

/// A service directory that runsvdir takes up within five seconds. A new one appears whole: it is
/// built under a name runsvdir skips (a leading dot) and then renamed. A broken script would only
/// be restarted once a second; it cannot hold anything else up.
fn runit_install(dir: &Path, script: &str) -> Result<()> {
    shell_syntax(script)?;
    let service = dir.join("ak980-agent");
    if service.is_dir() {
        write_atomic(&service.join("run"), script)?;
        return Ok(fs::set_permissions(service.join("run"), fs::Permissions::from_mode(0o755))?);
    }
    let staging = dir.join(".ak980-agent.new");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir(&staging)?;
    write_atomic(&staging.join("run"), script)?;
    fs::set_permissions(staging.join("run"), fs::Permissions::from_mode(0o755))?;
    Ok(fs::rename(&staging, &service)?)
}

/// Takes the service directory away; runsvdir then stops what ran from it.
fn runit_remove(dir: &Path) {
    let service = dir.join("ak980-agent");
    if !service.is_dir() {
        return;
    }
    run(&["sv", "down", &service.to_string_lossy()], SHORT);
    let gone = dir.join(".ak980-agent.old");
    let _ = fs::remove_dir_all(&gone);
    if fs::rename(&service, &gone).is_ok() {
        let _ = fs::remove_dir_all(&gone);
    }
}

pub fn runit_user_enable() -> Result<()> {
    let Some(dir) = user_runsvdir() else {
        return Err(Error::invalid(tr!("No runsvdir runs for this user: start one with turnstile or userspawn.")));
    };
    runit_install(&dir, &render("runit/user/ak980-agent/run", &service_exe()?))
}

pub fn runit_user_disable() {
    if let Some(dir) = user_runsvdir() {
        runit_remove(&dir);
    }
}

// ------------------------------------------------------------ system services (root)

/// (template, destination, packaged copy that makes writing unnecessary, mode)
pub type SystemFile = (&'static str, &'static str, Option<&'static str>, u32);

/// The files of each init system's service.
pub const SYSTEM_FILES: &[(&str, &[SystemFile])] = &[
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

fn system_files(init: &str) -> &'static [SystemFile] {
    SYSTEM_FILES.iter().find(|(name, _)| *name == init).map(|(_, files)| *files).unwrap_or(&[])
}

/// (service definitions, enabled services) for Artix-style or Void-style runit. On Artix the link
/// goes into the default runlevel, where its own packages put theirs, so it outlasts a reboot.
fn runit_dirs() -> (&'static str, &'static str) {
    if Path::new("/etc/runit/runsvdir/default").is_dir() {
        ("/etc/runit/sv", "/etc/runit/runsvdir/default")
    } else if Path::new("/etc/runit/sv").is_dir() {
        ("/etc/runit/sv", "/run/runit/service")
    } else {
        ("/etc/sv", "/var/service")
    }
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

/// A file a system service needs written: (destination, contents, mode).
pub type FileToWrite = (String, String, u32);

/// The files that enabling the system service of `init` has to write, each checked first: by dinit's
/// checker, or by `sh -n` for scripts. Files that a package ships or that are there already stay as
/// they are; they belong to the package or to whoever wrote them.
fn system_writes(init: &str, exe: &str) -> Result<Vec<FileToWrite>> {
    let (sv, _) = runit_dirs();
    let mut writes = vec![];
    for (template, dest, packaged, mode) in system_files(init) {
        let dest = dest.replace("{runit_sv}", sv);
        if packaged.is_some_and(|p| Path::new(p).exists()) || lexists(&dest) {
            continue;
        }
        if !plain(exe) || !root_only(Path::new(exe)) {
            return Err(Error::invalid(tr!(
                "A system service may only start a program that nobody but root can change. Install the package, or copy the program to /usr/local/bin and start it from there (now: {path}).",
                path = exe)));
        }
        let mut text = render(template, exe);
        if init == "dinit" {
            text = fit_dinit(&text, dinit_version());
            dinit_accepts("ak980d", &text)?;
        } else if mode & 0o111 != 0 {
            shell_syntax(&text)?;
        }
        writes.push((dest, text, *mode));
    }
    Ok(writes)
}

/// Shell script (run as root) that writes `writes` and enables, or disables, the system service.
/// Each file goes in with one rename; a service the init system refuses is taken out again, so
/// that nothing it cannot load stays where boot would look for it.
pub fn system_script(init: &str, enable: bool, writes: &[FileToWrite]) -> String {
    let (sv, active) = runit_dirs();
    let mut lines = vec!["set -e".to_string()];
    for (dest, text, mode) in writes {
        let parent = Path::new(dest).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let new = shell_quote(&format!("{dest}.ak980-new"));
        lines.push(format!("mkdir -p {}", shell_quote(&parent)));
        lines.push(format!("cat > {new} <<'AK980_EOF'\n{text}AK980_EOF"));
        lines.push(format!("chmod {mode:o} {new}"));
        lines.push(format!("mv -f {new} {}", shell_quote(dest)));
    }
    let written: Vec<String> = writes.iter().map(|(dest, ..)| shell_quote(dest)).collect();
    let undo = if written.is_empty() { String::new() } else { format!("rm -f {}; ", written.join(" ")) };
    let command = match (init, enable) {
        ("systemd", true) => "systemctl daemon-reload && systemctl enable --now ak980d.service".to_string(),
        ("systemd", false) => "systemctl disable --now ak980d.service".into(),
        ("openrc", true) => "rc-update add ak980d default && rc-service ak980d start".into(),
        ("openrc", false) => "rc-service ak980d stop || true\nrc-update del ak980d default".into(),
        // dinit loads the description before it links it into boot.d
        ("dinit", true) => format!("dinitctl enable ak980d || {{ {undo}exit 1; }}"),
        ("dinit", false) => "dinitctl disable ak980d || true\ndinitctl stop ak980d || true".into(),
        ("runit", true) => format!("ln -sfn {sv}/ak980d {active}/ak980d"),
        ("runit", false) => format!("rm -f {active}/ak980d"),
        // s6-rc compiles every service at once: one that does not compile leaves the bundle again,
        // or no later change to any service would go through
        ("s6", true) => "s6-service add default ak980d-srv\n\
                         s6-db-reload || { s6-service delete default ak980d-srv; s6-db-reload || true; exit 1; }".into(),
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

pub const USER_METHODS: [&str; 4] = ["session", "systemd-user", "dinit-user", "runit-user"];

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
        let mut m = Method {
            id: "dinit-user".into(),
            title: "dinit user service",
            detail: "The full agent, managed with dinitctl --user.",
            scope: "user",
            available,
            enabled: available && dinit_user_enabled(),
        };
        if !available {
            m.detail = "Needs a dinit of your own, started at login by turnstile (seatd or elogind) or userspawn (elogind).";
        }
        out.push(m);
    }
    if which("runsvdir").is_some() {
        let available = user_runsvdir().is_some();
        let mut m = Method {
            id: "runit-user".into(),
            title: "runit user service",
            detail: "The full agent, supervised by your own runsvdir.",
            scope: "user",
            available,
            enabled: available && runit_user_enabled(),
        };
        if !available {
            m.detail = "Needs a runsvdir of your own, started at login by turnstile or userspawn.";
        }
        out.push(m);
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

/// Enables one way to start the agent; once it is on, the other per-user ways are turned off, so one
/// agent runs. They stay as they were when it cannot be turned on, so a failure never leaves the user
/// without the agent they had.
pub fn enable(method_id: &str) -> Result<()> {
    match method_id {
        "session" => Xdg::real().enable(true),
        "systemd-user" => systemd_user_enable(),
        "dinit-user" => dinit_user_enable(),
        "runit-user" => runit_user_enable(),
        id if id.starts_with("system-") => {
            let init = &id["system-".len()..];
            let writes = system_writes(init, &service_exe()?)?;
            run_as_root(&system_script(init, true, &writes))
        }
        id => Err(Error::invalid(id.to_string())),
    }?;
    if USER_METHODS.contains(&method_id) {
        let mut stopped = false;
        for other in USER_METHODS.iter().filter(|m| **m != method_id) {
            stopped |= disable_user(other).unwrap_or(false);
        }
        // A service manager's agent stops with its service. A new one started by a service manager
        // waited for it as a standby; the session's starts only while none runs, so it starts now
        if method_id == "session" && stopped {
            let deadline = Instant::now() + Duration::from_secs(3);
            while runtime::agent_running() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(100));
            }
            start_agent_now()?;
        }
    }
    Ok(())
}

/// Turns a per-user way off if it is on; `true` when it was on (its agent stops with it).
fn disable_user(method_id: &str) -> Result<bool> {
    match method_id {
        "session" if Xdg::real().enabled() => Xdg::real().disable(false).map(|_| false),
        "systemd-user" if systemd_user_available() && systemd_user_enabled() => {
            systemd_user_disable();
            Ok(true)
        }
        "dinit-user" if which("dinitctl").is_some() && dinit_user_enabled() => {
            dinit_user_disable();
            Ok(true)
        }
        "runit-user" if runit_user_enabled() => {
            runit_user_disable();
            Ok(true)
        }
        _ => Ok(false),
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
        "runit-user" => {
            runit_user_disable();
            Ok(())
        }
        id if id.starts_with("system-") => run_as_root(&system_script(&id["system-".len()..], false, &[])),
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

    /// An executable file at `dir/rel`, for checkers that look for the program.
    fn program(dir: &Path, rel: &str) -> String {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn templates_render() {
        for (name, _) in TEMPLATES {
            let text = render(name, "/opt/ak980");
            assert!(!text.contains("@AK980@"), "{name} keeps the placeholder");
        }
        assert!(render("systemd/ak980-agent.service", "/opt/ak980").contains("ExecStart=/opt/ak980 agent\n"));
    }

    #[test]
    fn every_file_quotes_the_path_its_own_way() {
        let p = "/home/x/My Apps/ak980.AppImage";
        assert_eq!(quote(p, Syntax::Shell), "'/home/x/My Apps/ak980.AppImage'");
        assert_eq!(quote(p, Syntax::Dinit), "\"/home/x/My Apps/ak980.AppImage\"");
        assert_eq!(quote(p, Syntax::Systemd), "\"/home/x/My Apps/ak980.AppImage\"");
        assert_eq!(quote(p, Syntax::Desktop), "\"/home/x/My Apps/ak980.AppImage\"");
        let odd = r#"/x/a"b$c%d\e"#;
        assert_eq!(quote(odd, Syntax::Dinit), r#""/x/a\"b$$c%d\\e""#);
        assert_eq!(quote(odd, Syntax::Systemd), r#""/x/a\"b$$c%%d\\e""#);
        assert_eq!(quote(odd, Syntax::Desktop), r#""/x/a\\"b\\$c%%d\\\\e""#);
        assert_eq!(syntax("dinit/user/ak980-agent"), Syntax::Dinit);
        assert_eq!(syntax("runit/ak980d/run"), Syntax::Shell);
    }

    #[test]
    fn dinit_versions() {
        assert_eq!(parse_dinit_version("Dinit version 0.22.1.\nSupported features: cgroups\n"), Some((0, 22)));
        assert_eq!(parse_dinit_version("Dinit version 0.16.1.\n"), Some((0, 16)));
        assert_eq!(parse_dinit_version(""), None);
        let text = render("dinit/user/ak980-agent", "/usr/bin/ak980");
        assert!(fit_dinit(&text, Some((0, 22))).contains("log-type"));
        let old = fit_dinit(&text, Some((0, 16)));
        assert!(!old.contains("log-type") && old.contains("command  = /usr/bin/ak980 agent\n"));
        assert!(!fit_dinit(&text, None).contains("log-type"), "unknown: only what every dinit knows");
    }

    /// The descriptions as the installed dinit reads them, also with a path that needs quoting, and
    /// a broken one refused. Runs where dinit's checker is installed (CI installs dinit too).
    #[test]
    fn dinit_checker_accepts_ours_and_refuses_broken_ones() {
        if dinit_checker().is_none() {
            eprintln!("no dinit-check here; skipped");
            return;
        }
        let d = temp("dinit-check");
        let version = dinit_version();
        for exe in [program(&d, "bin/ak980"), program(&d, "My Apps/ak980 \"new\".AppImage")] {
            for (template, name) in [("dinit/user/ak980-agent", "ak980-agent"), ("dinit/ak980d", "ak980d")] {
                let text = fit_dinit(&render(template, &exe), version);
                assert!(dinit_accepts(name, &text).unwrap(), "{text}");
            }
        }
        assert!(dinit_accepts("x", "type = process\ncommand = /bin/sh\nbogus-setting = 1\n").is_err());
        // What the checker of dinit 0.22 warns about too; older ones may not look at the program
        if version >= Some((0, 22)) {
            assert!(dinit_accepts("x", "type = process\ncommand = /nonexistent/ak980 agent\n").is_err(), "a missing program");
            let shell_quoted = format!("type = process\ncommand = {} agent\n", shell_quote(&program(&d, "a b/ak980")));
            assert!(dinit_accepts("x", &shell_quoted).is_err(), "single quotes split the path in dinit");
        }
        let _ = fs::remove_dir_all(d);
    }

    /// A private dinit instance, as turnstile runs one for each user: a description that does not
    /// load never gets into boot.d, a good one does, and disabling takes it out again.
    #[test]
    fn private_dinit_never_links_a_broken_service() {
        if which("dinit").is_none() || dinit_checker().is_none() {
            eprintln!("no dinit here; skipped");
            return;
        }
        let d = temp("dinit-instance");
        let sv = d.join("sv");
        fs::create_dir_all(sv.join("boot.d")).unwrap();
        fs::write(sv.join("boot"), "type = internal\nwaits-for.d: ./boot.d/\n").unwrap();
        let socket = d.join("s");
        let mut daemon = Command::new("dinit")
            .args(["-u", "-q", "-d", &sv.to_string_lossy(), "-p", &socket.to_string_lossy(), "boot"])
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .spawn().unwrap();
        let ready = Instant::now();
        while !socket.exists() && ready.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let dinit = Dinit {
            dir: sv.clone(),
            boot_dir: boot_dir(std::slice::from_ref(&sv)).unwrap(),
            ctl: vec!["-p".into(), socket.to_string_lossy().into_owned()],
        };

        let broken = "type = process\ncommand = /bin/sleep 60\nbogus-setting = 1\n";
        assert!(dinit.write("ak980-agent", broken).is_err());
        assert!(!sv.join("ak980-agent").exists(), "nothing written");
        assert!(dinit.enable("ak980-agent", false).is_err() && !dinit.enabled("ak980-agent"));

        let good = fit_dinit(&render("dinit/user/ak980-agent", "/bin/sleep"), dinit_version()).replace(" agent", " 60");
        assert!(!dinit.write("ak980-agent", &good).unwrap(), "checked, so not left for dinitctl to judge");
        dinit.enable("ak980-agent", false).unwrap();
        assert!(dinit.enabled("ak980-agent"));
        dinit.disable("ak980-agent");
        assert!(!dinit.enabled("ak980-agent"));

        let _ = dinit.ctl(&["shutdown"]);
        let _ = daemon.kill(); // our own child, by its handle
        let _ = daemon.wait();
        let _ = fs::remove_dir_all(d);
    }

    /// turnstile names its directories and generates `boot` with an absolute `waits-for.d`;
    /// userspawn runs a plain `dinit --user` with `boot` in `~/.config/dinit.d`.
    #[test]
    fn user_dinit_directories_from_whoever_started_it() {
        let d = temp("user-dinit");
        let generated = d.join("run/dinit");
        let own = d.join("home/.config/dinit.d");
        fs::create_dir_all(&generated).unwrap();
        fs::create_dir_all(&own).unwrap();
        fs::write(generated.join("boot"), format!(
            "type = internal\ndepends-on = system\nwaits-for.d = {}\ndepends-on = login.target\n",
            own.join("boot.d").display())).unwrap();
        let args: Vec<String> = ["--user", "--ready-fd", "3", "--services-dir", &generated.to_string_lossy(),
                                 "--services-dir", &own.to_string_lossy(), "--services-dir", "/etc/dinit.d/user"]
            .map(String::from).to_vec();
        let dirs = dinit_dirs(&args);
        assert_eq!(dirs, vec![generated.clone(), own.clone(), PathBuf::from("/etc/dinit.d/user")]);
        assert_eq!(boot_dir(&dirs), Some(own.join("boot.d")), "turnstile");

        fs::remove_file(generated.join("boot")).unwrap();
        fs::write(own.join("boot"), "type = internal\nwaits-for.d: ./boot.d/  # what starts at login\n").unwrap();
        assert_eq!(boot_dir(std::slice::from_ref(&own)), Some(own.join("./boot.d/")), "userspawn");
        assert_eq!(dinit_dirs(&["--user".to_string()])[1], PathBuf::from("/etc/dinit.d/user"), "defaults");
        assert!(program_is(&["/usr/bin/dinit".into(), "--user".into()], "dinit"));
        assert!(!program_is(&["dinitctl".into()], "dinit"));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn runit_user_service_comes_and_goes_whole() {
        let d = temp("runsvdir");
        let script = render("runit/user/ak980-agent/run", "/usr/bin/ak980");
        assert!(script.contains("exec chpst -e \"$TURNSTILE_ENV_DIR\" /usr/bin/ak980 agent"));
        runit_install(&d, &script).unwrap();
        let run_file = d.join("ak980-agent/run");
        assert_eq!(fs::read_to_string(&run_file).unwrap(), script);
        assert_ne!(fs::metadata(&run_file).unwrap().permissions().mode() & 0o111, 0, "executable");
        runit_install(&d, &script.replace("/usr/bin/ak980", "/opt/ak980")).unwrap(); // in place
        assert!(fs::read_to_string(&run_file).unwrap().contains("/opt/ak980 agent"));
        assert!(runit_install(&d, "if true; then\n").is_err(), "a broken script is not installed");
        runit_remove(&d);
        assert_eq!(fs::read_dir(&d).unwrap().count(), 0, "nothing stays, not even a hidden directory");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn service_scripts_are_valid_shell() {
        for template in ["openrc/ak980d", "runit/ak980d/run", "runit/user/ak980-agent/run", "s6/ak980d-srv/run"] {
            shell_syntax(&render(template, "/usr/bin/ak980")).unwrap();
            shell_syntax(&render(template, "/opt/My Apps/ak980")).unwrap();
        }
        assert!(shell_syntax("if true; then\n").is_err());
    }

    #[test]
    fn system_scripts() {
        let writes = vec![("/etc/dinit.d/ak980d".to_string(), "type = process\n".to_string(), 0o644)];
        for (init, _) in SYSTEM_FILES {
            let on = system_script(init, true, &[]);
            let off = system_script(init, false, &[]);
            assert!(on.starts_with("set -e\n") && off.starts_with("set -e\n"));
            assert_ne!(on, off, "{init}");
            shell_syntax(&on).unwrap();
            shell_syntax(&off).unwrap();
        }
        let on = system_script("dinit", true, &writes);
        shell_syntax(&on).unwrap();
        // written next to its place, moved in with one rename, and taken out again if dinit refuses it
        assert!(on.contains("cat > /etc/dinit.d/ak980d.ak980-new <<'AK980_EOF'\ntype = process\nAK980_EOF\n"));
        assert!(on.contains("mv -f /etc/dinit.d/ak980d.ak980-new /etc/dinit.d/ak980d\n"));
        assert!(on.ends_with("dinitctl enable ak980d || { rm -f /etc/dinit.d/ak980d; exit 1; }\n"));
        assert!(system_script("s6", true, &[]).contains("s6-db-reload || { s6-service delete default ak980d-srv;"));
        assert!(system_script("openrc", true, &[]).ends_with("rc-update add ak980d default && rc-service ak980d start\n"));
    }

    #[test]
    fn system_services_only_start_what_root_owns() {
        assert!(root_only(Path::new("/bin/sh")));
        let d = temp("owner");
        let mine = program(&d, "ak980");
        assert!(!root_only(Path::new(&mine)), "a program the user can change");
        assert!(!root_only(Path::new("/nonexistent/ak980")));
        let refused = system_writes("dinit", &mine);
        assert!(refused.is_err() || lexists("/etc/dinit.d/ak980d"), "refused unless there is nothing to write");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn writes_replace_whole_files() {
        let d = temp("atomic");
        let f = d.join("sub/ak980-agent");
        write_atomic(&f, "one\n").unwrap();
        write_atomic(&f, "two\n").unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "two\n");
        let left: Vec<_> = fs::read_dir(d.join("sub")).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left.len(), 1, "no temporary file stays: {left:?}");
        let _ = fs::remove_dir_all(d);
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
        fs::write(&x.system, render("xdg/ak980-agent.desktop", "/usr/bin/ak980")).unwrap();
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
