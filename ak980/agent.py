"""The background agent.

User mode (``ak980 agent``), started with the desktop session or as a user service:
  * keeps the screen clock in sync: when the keyboard appears, after sleep, when the system time
    or time zone changes, at midnight and then every hour;
  * streams PC lighting effects;
  * switches profiles when a configured program (a game) starts or exits;
  * turns launcher keys (F13–F24 sent by remapped keys) into programs and links.

System mode (``ak980 agent --system``), started by the init system as root or a service user,
only keeps the clock in sync and reads /etc/ak980/agent.conf.
"""

from __future__ import annotations

import datetime as dt
import logging
import os
import shlex
import signal
import subprocess
import threading
import time

from . import config as cfgmod
from .device import Dongle, WiredKeyboard
from .i18n import set_language, tr
from .keymap import key_code
from .runtime import InstanceLock

log = logging.getLogger("ak980.agent")

POLL_SECONDS = 3
SWITCH_SECONDS = 3


def _tz_key() -> tuple:
    now = dt.datetime.now().astimezone()
    return now.utcoffset(), now.tzname()


def sync_clock_once(slot: int) -> str | None:
    """Sets the clock over whatever link is there. Returns "USB", "2.4G" or None."""
    found = WiredKeyboard.find()
    if found:
        with WiredKeyboard(*found) as kb:
            kb.sync_clock(slot=slot)
        return "USB"
    info = Dongle.find()
    if info:
        with Dongle(info) as d:
            if d.sync_clock(slot=slot):
                return "2.4G"
    return None


def running_processes() -> set[str]:
    """Lower-case names of running programs: /proc/<pid>/comm and the executable's file name."""
    names: set[str] = set()
    for pid in os.listdir("/proc"):
        if not pid.isdigit():
            continue
        try:
            with open(f"/proc/{pid}/comm") as f:
                names.add(f.read().strip().lower())
        except OSError:
            continue
        try:
            names.add(os.path.basename(os.readlink(f"/proc/{pid}/exe")).lower())
        except OSError:
            pass
        try:
            with open(f"/proc/{pid}/cmdline", "rb") as f:
                first = f.read(512).split(b"\0", 1)[0].decode(errors="replace")
            if first:
                names.add(os.path.basename(first.replace("\\", "/")).lower())
        except OSError:
            pass
    return names


def matching_profile(settings: cfgmod.Settings, names: set[str]) -> str | None:
    for rule in settings.rules:
        if rule.process.strip().lower() in names:
            return rule.profile
    return None


def run_launcher(command: str) -> None:
    cmd = command.strip()
    if not cmd:
        return
    if "://" in cmd and " " not in cmd:
        argv = ["xdg-open", cmd]
    else:
        argv = ["sh", "-c", cmd]
    try:
        subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         start_new_session=True)
        log.info("launched: %s", shlex.join(argv))
    except OSError as e:
        log.warning("cannot launch %s: %s", cmd, e)


class Agent:
    def __init__(self, system: bool = False, interval_min: int | None = None):
        self.system = system
        self.interval_override = interval_min
        self._stop = threading.Event()
        self.engine = None
        self.keys = None
        self._launchers: dict[int, str] = {}
        self._wanted: str | None = None          # profile the running programs asked for last time
        self._for_program: str | None = None     # profile switched to because a program runs
        self._before_program: str | None = None  # the profile that was active before that

    # ------------------------------------------------------------ settings

    def _load(self) -> cfgmod.Config | None:
        return None if self.system else cfgmod.Config.load()

    def _settings(self, cfg: cfgmod.Config | None) -> cfgmod.Settings:
        return cfgmod.load_system() if cfg is None else cfg.settings

    # ------------------------------------------------------------ profile switching

    def _switch_to(self, cfg: cfgmod.Config, name: str) -> None:
        profile = cfg.get(name)
        if profile is None or cfg.active == name:
            return
        log.info("switching to profile %s", name)
        cfg.active = name
        cfg.save()
        found = WiredKeyboard.find()
        if found:
            from .profiles import apply_profile
            try:
                with WiredKeyboard(*found) as kb:
                    apply_profile(kb, profile)
            except OSError as e:
                log.warning("cannot apply profile %s: %s", name, e)

    def _auto_switch(self, cfg: cfgmod.Config, running: set[str] | None = None) -> None:
        """Switches to a program's profile when it starts and back when it exits.

        Only a change in what runs triggers a switch, so a profile picked by hand in between stays.
        Going back is skipped when the user has moved away from the program's profile.
        """
        s = cfg.settings
        if not s.auto_switch or not s.rules:
            self._wanted = self._for_program = self._before_program = None
            return
        wanted = matching_profile(s, running_processes() if running is None else running)
        if wanted == self._wanted:
            return
        self._wanted = wanted
        if wanted:
            if self._for_program is None:
                self._before_program = cfg.active
            self._for_program = wanted
            self._switch_to(cfg, wanted)
        elif self._for_program is not None:
            back = s.default_profile or self._before_program
            if back and cfg.active == self._for_program:
                self._switch_to(cfg, back)
            self._for_program = self._before_program = None

    # ------------------------------------------------------------ launcher keys

    def _update_launchers(self, cfg: cfgmod.Config) -> None:
        table = {}
        for launcher in cfg.settings.launchers:
            try:
                table[key_code(launcher.key)] = launcher.command
            except ValueError:
                continue
        self._launchers = table
        if table and self.keys is None:
            from .keyinput import KeyListener
            self.keys = KeyListener(self._on_key)
            self.keys.start()
        elif not table and self.keys is not None:
            self.keys.stop()
            self.keys = None

    def _on_key(self, usage: int, pressed: bool) -> None:
        if pressed and usage in self._launchers:
            run_launcher(self._launchers[usage])

    # ------------------------------------------------------------ main loop

    def stop(self, *_) -> None:
        self._stop.set()

    def run(self, once: bool = False) -> int:
        lock = InstanceLock("system-agent" if self.system else "agent")
        if not once and not lock.acquire():
            # Another agent runs (say, from session autostart while a user service starts too).
            # Exiting would make some supervisors restart us in a loop, so wait as a standby
            # and take over if that agent stops.
            log.info("%s", tr("The agent is already running"))
            signal.signal(signal.SIGTERM, lambda *_: os._exit(0))
            signal.signal(signal.SIGINT, lambda *_: os._exit(0))
            lock.acquire(wait=True)
            log.info("the other agent stopped; taking over")
        signal.signal(signal.SIGTERM, self.stop)
        signal.signal(signal.SIGINT, self.stop)

        if not self.system:
            set_language(cfgmod.peek_language())
        cfg = self._load()
        seen = cfgmod.mtime() if cfg is not None else 0.0
        if cfg is not None and not once:
            from .engine import LightingEngine
            self.engine = LightingEngine(drive_device=True)
            self.engine.start()
            self._update_launchers(cfg)

        present = None
        last_sync = 0.0
        retry_at = 0.0
        wall_ref = time.time() - time.monotonic()
        tz = _tz_key()
        day = dt.date.today()
        next_switch = 0.0
        log.info("ak980 agent started (%s mode)", "system" if self.system else "user")

        try:
            while not self._stop.is_set():
                if cfg is not None:
                    m = cfgmod.mtime()
                    if m != seen:
                        seen = m
                        cfg = cfgmod.Config.load()
                        self._update_launchers(cfg)
                settings = self._settings(cfg)
                interval = 60 * (self.interval_override or settings.sync_interval_min or 60)
                now_present = "usb" if WiredKeyboard.find() else "2.4g" if Dongle.find() else None

                reason = None
                if now_present and now_present != present:
                    reason = f"connected ({now_present})"
                elif now_present:
                    if abs((time.time() - time.monotonic()) - wall_ref) > 2:
                        reason = "system time changed or resumed from sleep"
                    elif _tz_key() != tz:
                        reason = "time zone changed"
                    elif dt.date.today() != day:
                        reason = "new day"
                    elif last_sync and time.monotonic() - last_sync > interval:
                        reason = "periodic"
                    elif retry_at and time.monotonic() >= retry_at:
                        reason = "retry"
                present = now_present

                if reason and settings.clock_auto:
                    if reason.startswith("connected") and not once:
                        self._stop.wait(2)   # let the firmware bring up every interface
                    try:
                        via = sync_clock_once(settings.clock_slot)
                    except OSError as e:
                        via = None
                        log.warning("clock sync failed: %s", e)
                    if via:
                        log.info("clock synced over %s (%s)", via, reason)
                        last_sync = time.monotonic()
                        retry_at = 0.0
                    else:
                        # The dongle is there but the keyboard is not in 2.4 GHz mode, or asleep
                        retry_at = time.monotonic() + 30
                    wall_ref = time.time() - time.monotonic()
                    tz = _tz_key()
                    day = dt.date.today()
                    if once:
                        return 0 if via else 1
                elif once:
                    return 1 if not now_present else 0

                if cfg is not None and time.monotonic() >= next_switch:
                    next_switch = time.monotonic() + SWITCH_SECONDS
                    try:
                        self._auto_switch(cfg)
                    except OSError as e:
                        log.warning("profile switch failed: %s", e)
                self._stop.wait(POLL_SECONDS)
        finally:
            if self.engine:
                self.engine.stop()
            if self.keys:
                self.keys.stop()
            lock.release()
        return 0


def run(system: bool = False, interval: int | None = None, once: bool = False) -> int:
    logging.basicConfig(level=logging.INFO, format="%(asctime)s %(name)s: %(message)s", datefmt="%H:%M:%S")
    return Agent(system=system, interval_min=interval).run(once=once)
