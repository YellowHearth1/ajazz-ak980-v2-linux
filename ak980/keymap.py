"""Key remapping (opcode 0x11 for the base layer, 0x27 for the Fn layer).

Every key is a 4-byte entry at its key index (= the LED index in layout.json):
  00 00 00 00        factory default
  05 03 00 00        disabled
  02 MODS CODE 00    HID key with modifiers (1 Ctrl, 2 Shift, 4 Alt, 8 Win)
  01 01 BTN 00       mouse button (1 left, 2 right, 4 middle, 3 double click, 10 button 4, 08 button 5)
  01 03 DIR 00       wheel (01 up, FF down)
  03 LO HI 00        consumer usage (media / launch keys)
  06 SLOT MODE COUNT macro
  07 K1 K2 K3        up to three keys at once
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Mapping

from .i18n import tr

TABLE_BYTES = 576
TABLE_PACKETS = 9
MAX_INDEX = 142

MOD_CTRL, MOD_SHIFT, MOD_ALT, MOD_WIN = 0x01, 0x02, 0x04, 0x08


def _build_hid_keys() -> list[tuple[str, int]]:
    keys = [(chr(ord("A") + i), 0x04 + i) for i in range(26)]
    keys += [(str(i), 0x1D + i) for i in range(1, 10)] + [("0", 0x27)]
    keys += [("Enter", 0x28), ("Esc", 0x29), ("Backspace", 0x2A), ("Tab", 0x2B), ("Space", 0x2C),
             ("-", 0x2D), ("=", 0x2E), ("[", 0x2F), ("]", 0x30), ("\\", 0x31), (";", 0x33), ("'", 0x34),
             ("`", 0x35), (",", 0x36), (".", 0x37), ("/", 0x38), ("CapsLock", 0x39)]
    keys += [(f"F{i}", 0x39 + i) for i in range(1, 13)]
    keys += [("PrintScreen", 0x46), ("ScrollLock", 0x47), ("Pause", 0x48), ("Insert", 0x49), ("Home", 0x4A),
             ("PageUp", 0x4B), ("Delete", 0x4C), ("End", 0x4D), ("PageDown", 0x4E), ("Right", 0x4F),
             ("Left", 0x50), ("Down", 0x51), ("Up", 0x52), ("NumLock", 0x53), ("Num/", 0x54), ("Num*", 0x55),
             ("Num-", 0x56), ("Num+", 0x57), ("NumEnter", 0x58)]
    keys += [(f"Num{i}", 0x58 + i) for i in range(1, 10)] + [("Num0", 0x62), ("Num.", 0x63), ("Menu", 0x65)]
    keys += [(f"F{i}", 0x68 + i - 13) for i in range(13, 25)]
    keys += [("LeftCtrl", 0xE0), ("LeftShift", 0xE1), ("LeftAlt", 0xE2), ("LeftWin", 0xE3),
             ("RightCtrl", 0xE4), ("RightShift", 0xE5), ("RightAlt", 0xE6), ("RightWin", 0xE7),
             ("Fn", 0xAF)]
    return keys


HID_KEYS: list[tuple[str, int]] = _build_hid_keys()
_BY_NAME = {n.lower(): c for n, c in HID_KEYS}
_BY_NAME.update({
    "return": 0x28, "escape": 0x29, "bksp": 0x2A, "del": 0x4C, "ins": 0x49, "pgup": 0x4B, "pgdn": 0x4E,
    "prtsc": 0x46, "scrlk": 0x47, "caps": 0x39, "app": 0x65, "ctrl": 0xE0, "shift": 0xE1, "alt": 0xE2,
    "win": 0xE3, "gui": 0xE3, "super": 0xE3,
})

# Linux input keycode (linux/input-event-codes.h) -> HID usage. Qt's nativeScanCode() is this plus 8.
LINUX_TO_HID: dict[int, int] = {
    1: 0x29, 2: 0x1E, 3: 0x1F, 4: 0x20, 5: 0x21, 6: 0x22, 7: 0x23, 8: 0x24, 9: 0x25, 10: 0x26, 11: 0x27,
    12: 0x2D, 13: 0x2E, 14: 0x2A, 15: 0x2B, 16: 0x14, 17: 0x1A, 18: 0x08, 19: 0x15, 20: 0x17, 21: 0x1C,
    22: 0x18, 23: 0x0C, 24: 0x12, 25: 0x13, 26: 0x2F, 27: 0x30, 28: 0x28, 29: 0xE0, 30: 0x04, 31: 0x16,
    32: 0x07, 33: 0x09, 34: 0x0A, 35: 0x0B, 36: 0x0D, 37: 0x0E, 38: 0x0F, 39: 0x33, 40: 0x34, 41: 0x35,
    42: 0xE1, 43: 0x31, 44: 0x1D, 45: 0x1B, 46: 0x06, 47: 0x19, 48: 0x05, 49: 0x11, 50: 0x10, 51: 0x36,
    52: 0x37, 53: 0x38, 54: 0xE5, 55: 0x55, 56: 0xE2, 57: 0x2C, 58: 0x39, 59: 0x3A, 60: 0x3B, 61: 0x3C,
    62: 0x3D, 63: 0x3E, 64: 0x3F, 65: 0x40, 66: 0x41, 67: 0x42, 68: 0x43, 69: 0x53, 70: 0x47, 71: 0x5F,
    72: 0x60, 73: 0x61, 74: 0x56, 75: 0x5C, 76: 0x5D, 77: 0x5E, 78: 0x57, 79: 0x59, 80: 0x5A, 81: 0x5B,
    82: 0x62, 83: 0x63, 86: 0x64, 87: 0x44, 88: 0x45, 96: 0x58, 97: 0xE4, 98: 0x54, 99: 0x46, 100: 0xE6,
    102: 0x4A, 103: 0x52, 104: 0x4B, 105: 0x50, 106: 0x4F, 107: 0x4D, 108: 0x51, 109: 0x4E, 110: 0x49,
    111: 0x4C, 119: 0x48, 125: 0xE3, 126: 0xE7, 127: 0x65,
    **{183 + i: 0x68 + i for i in range(12)},      # F13..F24
}

MEDIA_KEYS: list[tuple[str, int]] = [
    ("Play/Pause", 0xCD), ("Stop", 0xB7), ("Previous track", 0xB6), ("Next track", 0xB5),
    ("Volume up", 0xE9), ("Volume down", 0xEA), ("Mute", 0xE2),
    ("Calculator", 0x192), ("Mail", 0x18A), ("Media player", 0x183), ("My Computer", 0x194),
    ("Browser home", 0x223), ("Browser refresh", 0x227), ("Browser stop", 0x226), ("Browser back", 0x224),
    ("Browser forward", 0x225), ("Browser favorites", 0x22A), ("Browser search", 0x221),
]
_MEDIA_ALIASES = {
    "play": 0xCD, "playpause": 0xCD, "stop": 0xB7, "prev": 0xB6, "previous": 0xB6, "next": 0xB5,
    "volup": 0xE9, "voldown": 0xEA, "mute": 0xE2, "calc": 0x192, "calculator": 0x192, "mail": 0x18A,
    "player": 0x183, "computer": 0x194, "home": 0x223, "refresh": 0x227, "back": 0x224,
    "forward": 0x225, "favorites": 0x22A, "search": 0x221,
}

MOUSE_FUNCS = ["left", "right", "middle", "double", "btn4", "btn5", "wheelup", "wheeldown"]
MOUSE_LABELS = ["Left button", "Right button", "Middle button", "Double click", "Button 4", "Button 5",
                "Wheel up", "Wheel down"]
_MOUSE_BYTES = {
    "left": (0x01, 0x01), "right": (0x01, 0x02), "middle": (0x01, 0x04), "double": (0x01, 0x03),
    "btn5": (0x01, 0x08), "btn4": (0x01, 0x10), "wheelup": (0x03, 0x01), "wheeldown": (0x03, 0xFF),
}
MACRO_MODES = ["once", "repeat", "toggle", "hold", "click"]
MACRO_MODE_LABELS = ["Play once", "Play N times", "Until pressed again", "While held", "Click to play"]

# Spare keys the firmware can send but no keyboard key produces; the agent turns them into
# "launch a program / open a link" actions.
LAUNCHER_KEYS = [f"F{i}" for i in range(13, 25)]


def key_code(name: str) -> int:
    n = name.strip().lower()
    if n in _BY_NAME:
        return _BY_NAME[n]
    if n.startswith("0x"):
        return int(n, 16)
    raise ValueError(tr("Unknown key: {name}", name=name))


def key_name(code: int) -> str:
    for n, c in HID_KEYS:
        if c == code:
            return n
    return f"0x{code:02X}"


def media_usage(name: str) -> int:
    n = name.strip().lower().replace(" ", "")
    if n in _MEDIA_ALIASES:
        return _MEDIA_ALIASES[n]
    for label, u in MEDIA_KEYS:
        if label.lower().replace(" ", "") == n:
            return u
    if n.startswith("0x"):
        return int(n, 16)
    raise ValueError(tr("Unknown media key: {name}", name=name))


def media_name(usage: int) -> str:
    for n, u in MEDIA_KEYS:
        if u == usage:
            return tr(n)
    return f"Consumer 0x{usage:03X}"


@dataclass
class KeyAction:
    kind: str = "default"          # default | off | key | mouse | media | macro | multi
    mods: int = 0
    code: int = 0
    mouse: str = "left"
    usage: int = 0
    macro: int = 0
    macro_mode: str = "once"
    macro_count: int = 1
    keys: list[int] = field(default_factory=list)

    def to_entry(self) -> bytes:
        e = bytearray(4)
        if self.kind == "off":
            e[0:2] = b"\x05\x03"
        elif self.kind == "key":
            e[0] = 0x02
            if self.mods == 0 and 0xE0 <= self.code <= 0xE7:
                e[1] = 1 << (self.code - 0xE0)  # a bare modifier goes out as its bit, like the vendor driver
            else:
                e[1] = self.mods & 0xFF
                e[2] = self.code
        elif self.kind == "mouse":
            e[0] = 0x01
            e[1], e[2] = _MOUSE_BYTES[self.mouse]
        elif self.kind == "media":
            e[0] = 0x03
            e[1] = self.usage & 0xFF
            e[2] = self.usage >> 8
        elif self.kind == "macro":
            e[0] = 0x06
            e[1] = max(0, min(99, self.macro))
            e[2] = MACRO_MODES.index(self.macro_mode)
            e[3] = max(1, min(255, self.macro_count)) if self.macro_mode == "repeat" else 0
        elif self.kind == "multi":
            e[0] = 0x07
            for i, k in enumerate(self.keys[:3]):
                e[1 + i] = k
        return bytes(e)

    def describe(self, macro_names: list[str] | None = None) -> str:
        if self.kind == "default":
            return tr("Default")
        if self.kind == "off":
            return tr("Disabled")
        if self.kind == "key":
            parts = [n for bit, n in ((MOD_CTRL, "Ctrl"), (MOD_SHIFT, "Shift"), (MOD_ALT, "Alt"), (MOD_WIN, "Win"))
                     if self.mods & bit]
            if self.code:
                parts.append(key_name(self.code))
            return "+".join(parts) or tr("(nothing)")
        if self.kind == "mouse":
            return tr(MOUSE_LABELS[MOUSE_FUNCS.index(self.mouse)])
        if self.kind == "media":
            return media_name(self.usage)
        if self.kind == "macro":
            name = (macro_names[self.macro] if macro_names and self.macro < len(macro_names)
                    else tr("Macro {n}", n=self.macro))
            extra = {"repeat": f" ×{self.macro_count}"}.get(self.macro_mode, "")
            return name + extra
        if self.kind == "multi":
            return "+".join(key_name(k) for k in self.keys)
        return "?"

    def to_spec(self) -> str:
        if self.kind in ("default", "off"):
            return self.kind
        if self.kind == "key":
            mods = [n for bit, n in ((MOD_CTRL, "ctrl"), (MOD_SHIFT, "shift"), (MOD_ALT, "alt"), (MOD_WIN, "win"))
                    if self.mods & bit]
            return "key:" + "+".join(mods + ([key_name(self.code)] if self.code else []))
        if self.kind == "mouse":
            return "mouse:" + self.mouse
        if self.kind == "media":
            return f"media:0x{self.usage:x}"
        if self.kind == "macro":
            s = f"macro:{self.macro}:{self.macro_mode}"
            return s + (f":{self.macro_count}" if self.macro_mode == "repeat" else "")
        return "multi:" + ",".join(key_name(k) for k in self.keys)

    @classmethod
    def parse(cls, spec: str) -> KeyAction:
        """default | off | key:[ctrl+][shift+][alt+][win+]NAME | mouse:left|right|middle|double|btn4|btn5|
        wheelup|wheeldown | media:NAME | macro:N[:once|repeat:COUNT|toggle|hold|click] | multi:A,B,C"""
        s = spec.strip()
        if s.lower() in ("default", "def"):
            return cls()
        if s.lower() in ("off", "disabled", "none"):
            return cls(kind="off")
        kind, _, rest = s.partition(":")
        kind = kind.lower()
        if kind == "key":
            # Everything before the last "+" is a modifier; "key:ctrl" on its own is the Ctrl key
            parts = [x.strip() for x in rest.split("+") if x.strip()]
            mod_bits = {"ctrl": MOD_CTRL, "shift": MOD_SHIFT, "alt": MOD_ALT, "win": MOD_WIN, "super": MOD_WIN}
            mods = 0
            for p in parts[:-1]:
                if p.lower() not in mod_bits:
                    raise ValueError(tr("Unknown modifier: {name}", name=p))
                mods |= mod_bits[p.lower()]
            if len(parts) > 1 and parts[-1].lower() in mod_bits:
                return cls(kind="key", mods=mods | mod_bits[parts[-1].lower()], code=0)
            return cls(kind="key", mods=mods, code=key_code(parts[-1]) if parts else 0)
        if kind == "mouse":
            if rest.lower() not in _MOUSE_BYTES:
                raise ValueError(tr("Unknown mouse action: {name}", name=rest))
            return cls(kind="mouse", mouse=rest.lower())
        if kind == "media":
            return cls(kind="media", usage=media_usage(rest))
        if kind == "macro":
            parts = rest.split(":")
            a = cls(kind="macro", macro=int(parts[0]))
            if len(parts) > 1:
                if parts[1].lower() not in MACRO_MODES:
                    raise ValueError(tr("Unknown macro mode: {name}", name=parts[1]))
                a.macro_mode = parts[1].lower()
                if a.macro_mode == "repeat":
                    a.macro_count = int(parts[2]) if len(parts) > 2 else 2
            return a
        if kind == "multi":
            return cls(kind="multi", keys=[key_code(k) for k in rest.split(",") if k.strip()][:3])
        raise ValueError(tr("Unknown key action: {spec}", spec=spec))

    def to_dict(self) -> dict:
        return asdict(self)

    @classmethod
    def from_dict(cls, d: dict) -> KeyAction:
        return cls(**{k: v for k, v in d.items() if k in cls.__dataclass_fields__})


def encode_layer(actions: Mapping[int, KeyAction]) -> bytes:
    t = bytearray(TABLE_BYTES)
    for idx, action in actions.items():
        if 1 <= idx <= MAX_INDEX and action is not None:
            t[idx * 4:idx * 4 + 4] = action.to_entry()
    t[574] = 0xAA
    t[575] = 0x55
    return bytes(t)
