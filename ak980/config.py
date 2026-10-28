"""Settings and profiles (~/.config/ak980/config.json).

A profile is everything the keyboard can hold plus the PC-side lighting: lighting source and
effect, per-key colours, both key layers, macros and keyboard options. Global settings (clock,
language, auto-switching rules, launcher keys) live next to the profiles.
"""

from __future__ import annotations

import copy
import json
import os
from dataclasses import dataclass, field

from .i18n import tr
from .keymap import KeyAction
from .lighting import KeyboardOptions, LightSettings
from .macro import Macro

CONFIG_DIR = os.path.join(os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config")), "ak980")
CONFIG_PATH = os.path.join(CONFIG_DIR, "config.json")
SYSTEM_CONFIG = "/etc/ak980/agent.conf"
VERSION = 2

LIGHT_SOURCES = ("firmware", "custom", "host")


def _rgb_map(d: dict) -> dict[int, tuple[int, int, int]]:
    return {int(k): tuple(v) for k, v in d.items()}


def _tuplify(values: dict) -> dict:
    return {k: tuple(v) if isinstance(v, list) else v for k, v in values.items()}


def default_name() -> str:
    return tr("Main")


@dataclass
class Profile:
    name: str = field(default_factory=default_name)
    light_source: str = "firmware"            # firmware effect | custom per-key colours | host (PC) effect
    lighting: LightSettings = field(default_factory=LightSettings)
    custom_colors: dict[int, tuple[int, int, int]] = field(default_factory=dict)
    custom_brightness: int = 5
    host_effect: str = "wave"
    host_values: dict[str, dict] = field(default_factory=dict)
    host_brightness: int = 100
    keymap_top: dict[int, KeyAction] = field(default_factory=dict)
    keymap_fn: dict[int, KeyAction] = field(default_factory=dict)
    macros: list[Macro] = field(default_factory=list)
    options: KeyboardOptions = field(default_factory=KeyboardOptions)

    def to_dict(self) -> dict:
        return {
            "name": self.name,
            "light_source": self.light_source,
            "lighting": self.lighting.to_dict(),
            "custom_colors": {str(k): list(v) for k, v in self.custom_colors.items()},
            "custom_brightness": self.custom_brightness,
            "host_effect": self.host_effect,
            "host_values": {k: {kk: list(vv) if isinstance(vv, tuple) else vv for kk, vv in v.items()}
                            for k, v in self.host_values.items()},
            "host_brightness": self.host_brightness,
            "keymap_top": {str(k): v.to_dict() for k, v in self.keymap_top.items() if v.kind != "default"},
            "keymap_fn": {str(k): v.to_dict() for k, v in self.keymap_fn.items() if v.kind != "default"},
            "macros": [m.to_dict() for m in self.macros],
            "options": self.options.to_dict(),
        }

    @classmethod
    def from_dict(cls, d: dict) -> Profile:
        p = cls(name=d.get("name") or default_name())
        p.light_source = d.get("light_source", "firmware") if d.get("light_source") in LIGHT_SOURCES else "firmware"
        p.lighting = LightSettings.from_dict(d.get("lighting", {}))
        p.custom_colors = _rgb_map(d.get("custom_colors", {}))
        p.custom_brightness = int(d.get("custom_brightness", 5))
        p.host_effect = d.get("host_effect", "wave")
        p.host_values = {k: _tuplify(v) for k, v in d.get("host_values", {}).items()}
        p.host_brightness = int(d.get("host_brightness", 100))
        p.keymap_top = {int(k): KeyAction.from_dict(v) for k, v in d.get("keymap_top", {}).items()}
        p.keymap_fn = {int(k): KeyAction.from_dict(v) for k, v in d.get("keymap_fn", {}).items()}
        p.macros = [Macro.from_dict(m) for m in d.get("macros", [])]
        p.options = KeyboardOptions.from_dict(d.get("options", {}))
        return p

    def copy(self, name: str | None = None) -> Profile:
        p = copy.deepcopy(self)
        if name:
            p.name = name
        return p


@dataclass
class Rule:
    process: str          # process name as in /proc/<pid>/comm or the executable's file name
    profile: str


@dataclass
class Launcher:
    key: str              # F13..F24
    command: str          # shell command or URL
    label: str = ""


@dataclass
class Settings:
    language: str = "auto"
    clock_auto: bool = True
    clock_slot: int = 1
    sync_interval_min: int = 60
    lcd_slot: int = 1
    last_image: str = ""
    auto_switch: bool = False
    default_profile: str = ""
    rules: list[Rule] = field(default_factory=list)
    launchers: list[Launcher] = field(default_factory=list)
    host_fps: int = 30
    animations: bool = True
    close_to_tray: bool = False

    def to_dict(self) -> dict:
        d = {k: getattr(self, k) for k in self.__dataclass_fields__ if k not in ("rules", "launchers")}
        d["rules"] = [vars(r) for r in self.rules]
        d["launchers"] = [vars(x) for x in self.launchers]
        return d

    @classmethod
    def from_dict(cls, d: dict) -> Settings:
        s = cls(**{k: v for k, v in d.items() if k in cls.__dataclass_fields__ and k not in ("rules", "launchers")})
        s.rules = [Rule(**r) for r in d.get("rules", []) if r.get("process") and r.get("profile")]
        s.launchers = [Launcher(**x) for x in d.get("launchers", []) if x.get("key") and x.get("command")]
        return s


@dataclass
class Config:
    settings: Settings = field(default_factory=Settings)
    profiles: list[Profile] = field(default_factory=lambda: [Profile()])
    active: str = ""
    path: str = CONFIG_PATH

    def __post_init__(self) -> None:
        if not self.active and self.profiles:
            self.active = self.profiles[0].name

    # ------------------------------------------------------------ profiles

    @property
    def profile(self) -> Profile:
        p = self.get(self.active)
        if p is None:
            p = self.profiles[0]
            self.active = p.name
        return p

    def get(self, name: str) -> Profile | None:
        return next((p for p in self.profiles if p.name == name), None)

    def names(self) -> list[str]:
        return [p.name for p in self.profiles]

    def unique_name(self, base: str) -> str:
        if self.get(base) is None:
            return base
        n = 2
        while self.get(f"{base} {n}") is not None:
            n += 1
        return f"{base} {n}"

    def add(self, profile: Profile) -> Profile:
        profile.name = self.unique_name(profile.name.strip() or tr("Profile"))
        self.profiles.append(profile)
        return profile

    def remove(self, name: str) -> None:
        if len(self.profiles) <= 1:
            raise ValueError(tr("The last profile cannot be deleted"))
        self.profiles = [p for p in self.profiles if p.name != name]
        if self.active == name:
            self.active = self.profiles[0].name
        self.settings.rules = [r for r in self.settings.rules if r.profile != name]
        if self.settings.default_profile == name:
            self.settings.default_profile = ""

    def rename(self, old: str, new: str) -> str:
        p = self.get(old)
        if p is None:
            raise ValueError(tr("No profile named “{name}”", name=old))
        new = new.strip()
        if not new:
            raise ValueError(tr("A profile needs a name"))
        if new != old and self.get(new) is not None:
            raise ValueError(tr("A profile named “{name}” already exists", name=new))
        p.name = new
        if self.active == old:
            self.active = new
        for r in self.settings.rules:
            if r.profile == old:
                r.profile = new
        if self.settings.default_profile == old:
            self.settings.default_profile = new
        return new

    # ------------------------------------------------------------ files

    @classmethod
    def load(cls, path: str = CONFIG_PATH) -> Config:
        c = cls(path=path)
        try:
            with open(path, encoding="utf-8") as f:
                d = json.load(f)
        except (OSError, ValueError):
            return c
        if "profiles" not in d:
            return cls._migrate_v1(d, path)
        c.settings = Settings.from_dict(d.get("settings", {}))
        c.profiles = [Profile.from_dict(p) for p in d.get("profiles", [])] or [Profile()]
        c.active = d.get("active", c.profiles[0].name)
        return c

    @classmethod
    def _migrate_v1(cls, d: dict, path: str) -> Config:
        """Version 1 kept one flat profile next to the settings."""
        c = cls(path=path)
        p = Profile.from_dict({**d, "name": default_name()})
        if d.get("custom_colors") and not d.get("lighting"):
            p.light_source = "custom"
        c.profiles = [p]
        c.active = p.name
        c.settings = Settings.from_dict({k: d[k] for k in ("clock_slot", "lcd_slot", "sync_interval_min",
                                                           "last_image") if k in d})
        try:
            c.save()        # migrate once, so every process sees the same profile names
        except OSError:
            pass
        return c

    def to_dict(self) -> dict:
        return {"version": VERSION, "active": self.active, "settings": self.settings.to_dict(),
                "profiles": [p.to_dict() for p in self.profiles]}

    def save(self, path: str | None = None) -> None:
        path = path or self.path
        os.makedirs(os.path.dirname(path), exist_ok=True)
        tmp = path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(self.to_dict(), f, ensure_ascii=False, indent=2)
        os.replace(tmp, path)


def peek_language(path: str = CONFIG_PATH) -> str:
    """The language setting, read before anything else so new profiles get translated names."""
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f).get("settings", {}).get("language", "auto")
    except (OSError, ValueError, AttributeError):
        return "auto"


def mtime(path: str = CONFIG_PATH) -> float:
    try:
        return os.stat(path).st_mtime
    except OSError:
        return 0.0


def load_system() -> Settings:
    """Settings for the system service (/etc/ak980/agent.conf, simple key = value lines)."""
    s = Settings()
    try:
        with open(SYSTEM_CONFIG, encoding="utf-8") as f:
            for line in f:
                line = line.split("#", 1)[0].strip()
                if "=" not in line:
                    continue
                key, value = (x.strip() for x in line.split("=", 1))
                if key in ("clock_slot", "sync_interval_min") and value.isdigit():
                    setattr(s, key, int(value))
    except OSError:
        pass
    return s


def export_profile(p: Profile, path: str) -> None:
    with open(path, "w", encoding="utf-8") as f:
        json.dump({"ak980_profile": 1, **p.to_dict()}, f, ensure_ascii=False, indent=2)


def import_profile(path: str) -> Profile:
    with open(path, encoding="utf-8") as f:
        d = json.load(f)
    if not isinstance(d, dict) or "ak980_profile" not in d:
        raise ValueError(tr("{path} is not an exported AK980 profile", path=os.path.basename(path)))
    return Profile.from_dict(d)
