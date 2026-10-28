"""Starting the agent automatically, whatever the init system.

Per-user (the full agent: clock, PC lighting effects, profile switching, launcher keys):
  * session autostart (XDG, ~/.config/autostart) — works with most desktops, any init;
  * systemd user service;
  * dinit user service (``dinit --user``, e.g. through turnstile on Artix or Chimera).

System-wide (clock only, works without logging in), installed with root rights:
  * systemd, OpenRC, dinit, runit, s6.

Packages ship the service files; for the AppImage or a source install they are written from the
templates in ak980/data/init with the right path to the program.
"""

from __future__ import annotations

import os
import shlex
import shutil
import subprocess
import sys
from dataclasses import dataclass

from .i18n import tr

TEMPLATES = os.path.join(os.path.dirname(__file__), "data", "init")
HOME_CONFIG = os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config"))
AGENT_PATTERN = r"(-m ak980|/ak980|\.AppImage) agent( |$)"


def executable() -> str:
    """Command line that starts this program, as service files need it."""
    appimage = os.environ.get("APPIMAGE")
    if appimage and os.path.isfile(appimage):
        return shlex.quote(appimage)
    exe = shutil.which("ak980")
    if exe:
        return exe
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    return f"/usr/bin/env PYTHONPATH={shlex.quote(root)} {shlex.quote(sys.executable)} -m ak980"


def render(template: str, exe: str | None = None) -> str:
    with open(os.path.join(TEMPLATES, template), encoding="utf-8") as f:
        return f.read().replace("@AK980@", exe or executable())


def _run(*argv: str, timeout: float = 10) -> subprocess.CompletedProcess:
    try:
        return subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.SubprocessError) as e:
        return subprocess.CompletedProcess(argv, 127, "", str(e))


def detect_init() -> str:
    try:
        with open("/proc/1/comm") as f:
            pid1 = f.read().strip()
    except OSError:
        pid1 = ""
    if os.path.isdir("/run/systemd/system"):
        return "systemd"
    if pid1 == "dinit":
        return "dinit"
    if os.path.isdir("/run/openrc"):
        return "openrc"
    if pid1 in ("runit", "runit-init") or os.path.isdir("/run/runit"):
        return "runit"
    if pid1 == "s6-svscan" or os.path.isdir("/run/s6-rc"):
        return "s6"
    return "unknown"


def agent_processes_running() -> bool:
    return _run("pgrep", "-u", str(os.getuid()), "-f", AGENT_PATTERN).returncode == 0


def start_agent_now() -> None:
    if agent_processes_running():
        return
    log = os.path.join(os.environ.get("XDG_STATE_HOME", os.path.expanduser("~/.local/state")), "ak980.log")
    os.makedirs(os.path.dirname(log), exist_ok=True)
    with open(log, "a") as out:
        subprocess.Popen(shlex.split(executable()) + ["agent"], stdout=out, stderr=subprocess.STDOUT,
                         stdin=subprocess.DEVNULL, start_new_session=True)


def stop_agent_now() -> None:
    _run("pkill", "-u", str(os.getuid()), "-f", AGENT_PATTERN)


@dataclass
class Method:
    id: str
    title: str          # English; translate with tr()
    detail: str         # English; translate with tr()
    scope: str          # session | user | system
    available: bool
    enabled: bool


# ------------------------------------------------------------ session autostart (XDG)

XDG_USER = os.path.join(HOME_CONFIG, "autostart", "ak980-agent.desktop")
XDG_SYSTEM = "/etc/xdg/autostart/ak980-agent.desktop"


def _desktop_hidden(path: str) -> bool:
    try:
        with open(path, encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return True
    return "Hidden=true" in text or "X-GNOME-Autostart-enabled=false" in text


def xdg_enabled() -> bool:
    if os.path.exists(XDG_USER):
        return not _desktop_hidden(XDG_USER)
    return os.path.exists(XDG_SYSTEM) and not _desktop_hidden(XDG_SYSTEM)


def xdg_enable(start: bool = True) -> None:
    os.makedirs(os.path.dirname(XDG_USER), exist_ok=True)
    if os.path.exists(XDG_SYSTEM):
        if os.path.exists(XDG_USER):
            os.remove(XDG_USER)   # the packaged entry applies again
    else:
        with open(XDG_USER, "w", encoding="utf-8") as f:
            f.write(render("xdg/ak980-agent.desktop"))
    if start:
        start_agent_now()


def xdg_disable(stop: bool = True) -> None:
    if os.path.exists(XDG_SYSTEM):
        os.makedirs(os.path.dirname(XDG_USER), exist_ok=True)
        with open(XDG_USER, "w", encoding="utf-8") as f:
            f.write("[Desktop Entry]\nType=Application\nName=Ajazz AK980 V2 agent\nHidden=true\n")
    elif os.path.exists(XDG_USER):
        os.remove(XDG_USER)
    if stop:
        stop_agent_now()


# ------------------------------------------------------------ systemd user service

SYSTEMD_USER_UNIT = os.path.join(HOME_CONFIG, "systemd", "user", "ak980-agent.service")
SYSTEMD_USER_PACKAGED = "/usr/lib/systemd/user/ak980-agent.service"


def systemd_user_available() -> bool:
    return bool(shutil.which("systemctl")) and os.path.isdir("/run/systemd/system")


def systemd_user_enabled() -> bool:
    return _run("systemctl", "--user", "is-enabled", "ak980-agent.service").stdout.strip() == "enabled"


def systemd_user_enable() -> None:
    if not os.path.exists(SYSTEMD_USER_PACKAGED):
        os.makedirs(os.path.dirname(SYSTEMD_USER_UNIT), exist_ok=True)
        with open(SYSTEMD_USER_UNIT, "w", encoding="utf-8") as f:
            f.write(render("systemd/ak980-agent.service"))
        _run("systemctl", "--user", "daemon-reload")
    _check(_run("systemctl", "--user", "enable", "--now", "ak980-agent.service"))


def systemd_user_disable() -> None:
    _run("systemctl", "--user", "disable", "--now", "ak980-agent.service")


# ------------------------------------------------------------ dinit user service

DINIT_USER_DIR = os.path.join(HOME_CONFIG, "dinit.d")
DINIT_USER_SERVICE = os.path.join(DINIT_USER_DIR, "ak980-agent")
DINIT_USER_PACKAGED = "/etc/dinit.d/user/ak980-agent"


def dinit_user_available() -> bool:
    return bool(shutil.which("dinitctl")) and _run("dinitctl", "--user", "list", timeout=3).returncode == 0


def dinit_user_enabled() -> bool:
    return os.path.lexists(os.path.join(DINIT_USER_DIR, "boot.d", "ak980-agent"))


def dinit_user_enable() -> None:
    if not os.path.exists(DINIT_USER_PACKAGED):
        os.makedirs(DINIT_USER_DIR, exist_ok=True)
        with open(DINIT_USER_SERVICE, "w", encoding="utf-8") as f:
            f.write(render("dinit/user/ak980-agent"))
    _check(_run("dinitctl", "--user", "enable", "ak980-agent"))


def dinit_user_disable() -> None:
    _run("dinitctl", "--user", "disable", "ak980-agent")
    _run("dinitctl", "--user", "stop", "ak980-agent")


# ------------------------------------------------------------ system services (root)

SYSTEM_FILES = {
    "systemd": [("systemd/ak980d.service", "/etc/systemd/system/ak980d.service",
                 "/usr/lib/systemd/system/ak980d.service", 0o644)],
    "openrc": [("openrc/ak980d", "/etc/init.d/ak980d", None, 0o755)],
    "dinit": [("dinit/ak980d", "/etc/dinit.d/ak980d", None, 0o644)],
    "runit": [("runit/ak980d/run", "{runit_sv}/ak980d/run", None, 0o755)],
    "s6": [("s6/ak980d-srv/type", "/etc/s6/sv/ak980d-srv/type", None, 0o644),
           ("s6/ak980d-srv/run", "/etc/s6/sv/ak980d-srv/run", None, 0o755),
           ("s6/ak980d-srv/dependencies.d/base", "/etc/s6/sv/ak980d-srv/dependencies.d/base", None, 0o644)],
}


def _runit_dirs() -> tuple[str, str]:
    """(service definitions, enabled services) for Artix-style or Void-style runit."""
    if os.path.isdir("/etc/runit/sv"):
        return "/etc/runit/sv", "/run/runit/service"
    return "/etc/sv", "/var/service"


def system_enabled(init: str) -> bool:
    if init == "systemd":
        return _run("systemctl", "is-enabled", "ak980d.service").stdout.strip() == "enabled"
    if init == "openrc":
        return os.path.lexists("/etc/runlevels/default/ak980d")
    if init == "dinit":
        return os.path.lexists("/etc/dinit.d/boot.d/ak980d")
    if init == "runit":
        return os.path.lexists(os.path.join(_runit_dirs()[1], "ak980d"))
    if init == "s6":
        return os.path.lexists("/etc/s6/adminsv/default/contents.d/ak980d-srv")
    return False


def system_script(init: str, enable: bool) -> str:
    """Shell script (run as root) that installs and enables, or disables, the system service."""
    sv, active = _runit_dirs()
    lines = ["set -e"]
    if enable:
        for template, dest, packaged, mode in SYSTEM_FILES.get(init, []):
            if packaged and os.path.exists(packaged):
                continue
            dest = dest.format(runit_sv=sv)
            if os.path.exists(dest):
                continue
            content = render(template)
            lines.append(f"mkdir -p {shlex.quote(os.path.dirname(dest))}")
            lines.append(f"cat > {shlex.quote(dest)} <<'AK980_EOF'\n{content}AK980_EOF")
            lines.append(f"chmod {mode:o} {shlex.quote(dest)}")
    commands = {
        ("systemd", True): "systemctl daemon-reload && systemctl enable --now ak980d.service",
        ("systemd", False): "systemctl disable --now ak980d.service",
        ("openrc", True): "rc-update add ak980d default && rc-service ak980d start",
        ("openrc", False): "rc-service ak980d stop; rc-update del ak980d default",
        ("dinit", True): "dinitctl enable ak980d",
        ("dinit", False): "dinitctl disable ak980d; dinitctl stop ak980d",
        ("runit", True): f"ln -sf {sv}/ak980d {active}/",
        ("runit", False): f"rm -f {active}/ak980d",
        ("s6", True): "s6-service add default ak980d-srv && s6-db-reload",
        ("s6", False): "s6-service delete default ak980d-srv && s6-db-reload",
    }
    lines.append(commands[(init, enable)])
    return "\n".join(lines) + "\n"


def run_as_root(script: str) -> None:
    if os.geteuid() == 0:
        _check(_run("sh", "-c", script, timeout=60))
        return
    if not shutil.which("pkexec"):
        raise PermissionError(tr("Root rights are needed. Run it with sudo:\n{cmd}",
                                 cmd="sudo sh -c " + shlex.quote(script)))
    _check(_run("pkexec", "sh", "-c", script, timeout=300))


def _check(r: subprocess.CompletedProcess) -> None:
    if r.returncode != 0:
        message = (r.stderr or r.stdout or "").strip() or tr("Command failed: {cmd}", cmd=shlex.join(r.args))
        if r.returncode in (126, 127) and "pkexec" in str(r.args[:1]):
            message = tr("Authorisation was cancelled")
        raise OSError(message)


# ------------------------------------------------------------ listing

INIT_TITLES = {"systemd": "systemd", "openrc": "OpenRC", "dinit": "dinit", "runit": "runit", "s6": "s6"}


def methods() -> list[Method]:
    init = detect_init()
    out = [
        Method("session", "Start with the desktop session",
               "XDG autostart. The full agent, with any init system.", "session", True, xdg_enabled()),
    ]
    if systemd_user_available():
        out.append(Method("systemd-user", "systemd user service",
                          "The full agent, managed with systemctl --user.", "user", True, systemd_user_enabled()))
    if shutil.which("dinitctl"):
        avail = dinit_user_available()
        out.append(Method("dinit-user", "dinit user service",
                          "The full agent, managed with dinitctl --user.", "user", avail,
                          avail and dinit_user_enabled()))
    if init in SYSTEM_FILES:
        out.append(Method(f"system-{init}", "System service ({init})",
                          "Clock only, also before you log in. Needs root rights once.", "system", True,
                          system_enabled(init)))
    return out


USER_METHODS = ("session", "systemd-user", "dinit-user")


def preferred_user_method() -> str:
    """The native per-user service manager when there is one, session autostart otherwise."""
    for m in methods():
        if m.scope == "user" and m.available:
            return m.id
    return "session"


def enable(method_id: str) -> None:
    """Enables one way to start the agent; the other per-user ways are turned off, so one agent runs."""
    if method_id in USER_METHODS:
        for other in USER_METHODS:
            if other != method_id:
                try:
                    _disable_user(other)
                except OSError:
                    pass
    if method_id == "session":
        xdg_enable()
    elif method_id == "systemd-user":
        systemd_user_enable()
    elif method_id == "dinit-user":
        dinit_user_enable()
    elif method_id.startswith("system-"):
        run_as_root(system_script(method_id.split("-", 1)[1], True))
    else:
        raise ValueError(method_id)


def _disable_user(method_id: str) -> None:
    if method_id == "session" and xdg_enabled():
        xdg_disable(stop=False)
    elif method_id == "systemd-user" and systemd_user_available() and systemd_user_enabled():
        systemd_user_disable()
    elif method_id == "dinit-user" and shutil.which("dinitctl") and dinit_user_enabled():
        dinit_user_disable()


def disable(method_id: str) -> None:
    if method_id == "session":
        xdg_disable()
    elif method_id == "systemd-user":
        systemd_user_disable()
    elif method_id == "dinit-user":
        dinit_user_disable()
    elif method_id.startswith("system-"):
        run_as_root(system_script(method_id.split("-", 1)[1], False))
    else:
        raise ValueError(method_id)
