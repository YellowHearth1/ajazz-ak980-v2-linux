"""Firmware lighting effects (opcode 0x13) and keyboard options (opcode 0x17)."""

from __future__ import annotations

from dataclasses import asdict, dataclass

MAX_LEVEL = 5

# What an effect responds to (the config_func column in the vendor driver's database)
CAP_BRIGHTNESS = 0x01
CAP_SPEED = 0x02
CAP_DIRECTION2 = 0x04
CAP_DIRECTION4 = 0x08
CAP_COLOR = 0x10
CAP_RAINBOW = 0x20


@dataclass(frozen=True)
class LightMode:
    id: int
    name: str        # English name from the vendor's language/1033.lan; translated with tr()
    caps: int


# Firmware order
LIGHT_MODES: list[LightMode] = [
    LightMode(1, "Static", 0x31),
    LightMode(2, "Single on", 0x33),
    LightMode(3, "Single off", 0x33),
    LightMode(4, "Glittering", 0x33),
    LightMode(5, "Fluttering", 0x33),
    LightMode(6, "Colourful", 0x03),
    LightMode(7, "Breath", 0x33),
    LightMode(8, "Spectrum", 0x03),
    LightMode(9, "Outward", 0x33),
    LightMode(10, "Scrolling", 0x3B),
    LightMode(11, "Rolling", 0x37),
    LightMode(12, "Rotating", 0x37),
    LightMode(13, "Explode", 0x33),
    LightMode(14, "Launch", 0x33),
    LightMode(15, "Ripples", 0x33),
    LightMode(16, "Flowing", 0x37),
    LightMode(17, "Pulsating", 0x33),
    LightMode(18, "Tilt", 0x37),
    LightMode(19, "Shuttle", 0x33),
    LightMode(0, "Off", 0),
]

DIRECTIONS = ["Left", "Down", "Up", "Right"]

SIDE_LIGHT_MODES = {
    "flowing": 0x20, "red": 0x21, "yellow": 0x22, "green": 0x23, "iceblue": 0x24,
    "blue": 0x25, "pink": 0x26, "white": 0x27, "neon": 0x28, "off": 0x29,
}

# Factory defaults from the vendor's config.xml (<light default_mode="11" default_brightness="5" default_speed="3"/>)
FACTORY_MODE = 11


def mode_by_id(mode_id: int) -> LightMode:
    return next((m for m in LIGHT_MODES if m.id == mode_id), LIGHT_MODES[0])


def mode_by_name(name: str) -> LightMode:
    from .i18n import tr
    key = name.strip().lower().replace(" ", "").replace("-", "")
    for m in LIGHT_MODES:
        if key in (str(m.id), m.name.lower().replace(" ", ""), tr(m.name).lower().replace(" ", "")):
            return m
    raise ValueError(tr("Unknown lighting effect: {name}", name=name))


@dataclass
class LightSettings:
    mode: int = FACTORY_MODE
    r: int = 0xFF
    g: int = 0x8F
    b: int = 0x1F
    rainbow: bool = False
    brightness: int = MAX_LEVEL
    speed: int = 3
    direction: int = 0

    def to_packet(self) -> bytes:
        """MODE R G B .. .. .. .. RAINBOW BRIGHT SPEED DIR .. .. AA 55"""
        p = bytearray(64)
        p[0] = self.mode
        if self.mode != 0:
            p[1], p[2], p[3] = self.r, self.g, self.b
            p[8] = 1 if self.rainbow else 0
            p[9] = max(0, min(MAX_LEVEL, self.brightness))
            p[10] = max(0, min(MAX_LEVEL, self.speed))
            p[11] = self.direction & 0xFF
        p[14] = 0xAA
        p[15] = 0x55
        return bytes(p)

    @property
    def rgb(self) -> tuple[int, int, int]:
        return self.r, self.g, self.b

    def to_dict(self) -> dict:
        return asdict(self)

    @classmethod
    def from_dict(cls, d: dict) -> LightSettings:
        return cls(**{k: v for k, v in d.items() if k in cls.__dataclass_fields__})


SLEEP_CHOICES = ["Never", "1 min", "5 min", "30 min"]


@dataclass
class KeyboardOptions:
    disable_alt_tab: bool = False
    disable_alt_f4: bool = False
    disable_win: bool = False
    fn_switch: bool = False
    sleep_time: int = 2      # 0 never, 1 = 1 min, 2 = 5 min, 3 = 30 min
    key_response: int = 2    # debounce level 1..5

    def to_packet(self) -> bytes:
        p = bytearray(64)
        p[1] = 0x01
        p[2] = int(self.disable_alt_tab)
        p[3] = int(self.disable_alt_f4)
        p[4] = int(self.disable_win)
        p[5] = int(self.fn_switch)
        p[6] = self.sleep_time
        p[8] = self.key_response
        p[62] = 0xAA
        p[63] = 0x55
        return bytes(p)

    def to_dict(self) -> dict:
        return asdict(self)

    @classmethod
    def from_dict(cls, d: dict) -> KeyboardOptions:
        return cls(**{k: v for k, v in d.items() if k in cls.__dataclass_fields__})
