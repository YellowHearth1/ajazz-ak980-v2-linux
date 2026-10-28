"""Lighting effects computed on the PC and streamed to the keyboard in real time.

Every effect works on the keys' positions from layout.json (normalised to 0..1, y grows towards
the space bar) and returns one RGB colour per key as floats in 0..1. The same code drives the
keyboard and the preview in the app, so the preview is exact.
"""

from __future__ import annotations

import colorsys
import math
from dataclasses import dataclass, field

import numpy as np


@dataclass(frozen=True)
class Param:
    id: str
    label: str                     # English, translated with tr()
    kind: str                      # color | range | choice | toggle
    default: object
    lo: int = 0
    hi: int = 100
    choices: tuple[tuple[str, str], ...] = ()


@dataclass
class Inputs:
    presses: list[int] = field(default_factory=list)   # row numbers (into the key arrays) pressed since last frame
    bands: np.ndarray | None = None                     # audio spectrum, 0..1 per band
    level: float = 0.0                                  # overall loudness 0..1
    beat: bool = False


DIRECTION_CHOICES = (("right", "To the right"), ("left", "To the left"), ("down", "Downwards"),
                     ("up", "Upwards"), ("diagonal", "Diagonal"), ("circle", "From the centre"))
PALETTES = {
    "rainbow": None,
    "fire": [(0.0, (0, 0, 0)), (0.3, (180, 10, 0)), (0.6, (255, 90, 0)), (0.85, (255, 190, 40)),
             (1.0, (255, 250, 210))],
    "ice": [(0.0, (0, 0, 0)), (0.35, (0, 40, 160)), (0.7, (40, 170, 255)), (1.0, (230, 250, 255))],
    "amber": [(0.0, (0, 0, 0)), (0.5, (200, 70, 0)), (1.0, (255, 190, 60))],
    "toxic": [(0.0, (0, 0, 0)), (0.4, (0, 120, 30)), (0.75, (120, 255, 0)), (1.0, (240, 255, 160))],
}
PALETTE_CHOICES = (("rainbow", "Rainbow"), ("fire", "Fire"), ("ice", "Ice"), ("amber", "Amber"),
                   ("toxic", "Toxic"))


def rgb(c) -> np.ndarray:
    return np.asarray(c, dtype=np.float32) / 255.0


def hsv_rows(h: np.ndarray, s: float | np.ndarray = 1.0, v: float | np.ndarray = 1.0) -> np.ndarray:
    """Vectorised HSV -> RGB for arrays of hue (0..1)."""
    h = np.mod(h, 1.0) * 6.0
    i = np.floor(h).astype(np.int32) % 6
    f = h - np.floor(h)
    s = np.broadcast_to(np.asarray(s, dtype=np.float32), h.shape)
    v = np.broadcast_to(np.asarray(v, dtype=np.float32), h.shape)
    p, q, t = v * (1 - s), v * (1 - s * f), v * (1 - s * (1 - f))
    choices = [np.stack(c, -1) for c in ((v, t, p), (q, v, p), (p, v, t), (p, q, v), (t, p, v), (v, p, q))]
    out = np.zeros(h.shape + (3,), dtype=np.float32)
    for k in range(6):
        out[i == k] = choices[k][i == k]
    return out


def palette(name: str, v: np.ndarray, hue_x: np.ndarray | None = None) -> np.ndarray:
    v = np.clip(v, 0, 1)
    stops = PALETTES.get(name)
    if stops is None:  # rainbow: hue from position, brightness from value
        h = hue_x if hue_x is not None else v
        return hsv_rows(h, 1.0, v)
    xs = np.array([s[0] for s in stops], dtype=np.float32)
    cols = np.array([s[1] for s in stops], dtype=np.float32) / 255.0
    return np.stack([np.interp(v, xs, cols[:, c]) for c in range(3)], -1).astype(np.float32)


class Effect:
    id = ""
    name = ""
    description = ""
    params: tuple[Param, ...] = ()
    needs: frozenset[str] = frozenset()

    def __init__(self, pos: np.ndarray, values: dict | None = None):
        self.pos = pos.astype(np.float32)          # (N, 2): x 0..1 left->right, y 0..1 top->bottom
        self.n = len(pos)
        self.v = {p.id: p.default for p in self.params}
        if values:
            self.v.update({k: val for k, val in values.items() if k in self.v})
        self.rng = np.random.default_rng(7)

    def projection(self, direction: str) -> np.ndarray:
        x, y = self.pos[:, 0], self.pos[:, 1]
        return {
            "right": x, "left": 1 - x, "down": y * 0.36, "up": (1 - y) * 0.36,
            "diagonal": (x + y * 0.36) / 1.36,
            "circle": np.hypot(x - 0.5, (y - 0.5) * 0.36) * 2,
        }.get(direction, x)

    def render(self, t: float, dt: float, inp: Inputs) -> np.ndarray:
        raise NotImplementedError


def _hz(speed: int, lo: float = 0.03, hi: float = 1.4) -> float:
    return lo + (hi - lo) * (max(0, min(100, speed)) / 100) ** 1.5


class Wave(Effect):
    id = "wave"
    name = "Rainbow wave"
    description = "A rainbow that flows across the keys."
    params = (
        Param("direction", "Direction", "choice", "right", choices=DIRECTION_CHOICES),
        Param("speed", "Speed", "range", 40),
        Param("size", "Wave length", "range", 100, 20, 300),
        Param("saturation", "Saturation", "range", 100),
    )

    def render(self, t, dt, inp):
        cycles = 100 / max(20, self.v["size"])
        phase = self.projection(self.v["direction"]) * cycles - t * _hz(self.v["speed"])
        return hsv_rows(phase, self.v["saturation"] / 100, 1.0)


class Drift(Effect):
    id = "drift"
    name = "Colour drift"
    description = "Two colours slowly flowing into each other."
    params = (
        Param("a", "First colour", "color", (255, 143, 31)),
        Param("b", "Second colour", "color", (40, 90, 255)),
        Param("direction", "Direction", "choice", "diagonal", choices=DIRECTION_CHOICES),
        Param("speed", "Speed", "range", 25),
        Param("size", "Wave length", "range", 120, 20, 300),
    )

    def render(self, t, dt, inp):
        cycles = 100 / max(20, self.v["size"])
        w = 0.5 + 0.5 * np.cos(2 * np.pi * (self.projection(self.v["direction"]) * cycles - t * _hz(self.v["speed"])))
        a, b = rgb(self.v["a"]), rgb(self.v["b"])
        return a[None, :] * (1 - w[:, None]) + b[None, :] * w[:, None]


class Breath(Effect):
    id = "breath"
    name = "Breathing"
    description = "The whole keyboard fades in and out."
    params = (
        Param("a", "Colour", "color", (255, 143, 31)),
        Param("alternate", "Alternate with a second colour", "toggle", False),
        Param("b", "Second colour", "color", (40, 90, 255)),
        Param("speed", "Speed", "range", 30),
        Param("floor", "Lowest brightness", "range", 5, 0, 60),
    )

    def render(self, t, dt, inp):
        hz = _hz(self.v["speed"], 0.08, 1.2)
        cycle = t * hz
        level = (1 - math.cos(2 * math.pi * cycle)) / 2
        level = self.v["floor"] / 100 + (1 - self.v["floor"] / 100) * level ** 1.6
        color = self.v["b"] if self.v["alternate"] and int(cycle) % 2 else self.v["a"]
        return np.tile(rgb(color) * level, (self.n, 1))


class Starlight(Effect):
    id = "starlight"
    name = "Starlight"
    description = "Keys light up at random and slowly fade out."
    params = (
        Param("background", "Background", "color", (6, 8, 24)),
        Param("random_hue", "Random colours", "toggle", True),
        Param("star", "Star colour", "color", (255, 220, 150)),
        Param("density", "Density", "range", 35),
        Param("fade", "Fade time", "range", 45),
    )

    def __init__(self, pos, values=None):
        super().__init__(pos, values)
        self.energy = np.zeros(self.n, np.float32)
        self.hue = self.rng.random(self.n).astype(np.float32)

    def render(self, t, dt, inp):
        rate = 0.02 + self.v["density"] / 100 * 0.9           # sparks per key per second
        spawn = self.rng.random(self.n) < rate * dt
        self.hue[spawn] = self.rng.random(int(spawn.sum()))
        self.energy[spawn] = 1.0
        tau = 0.15 + self.v["fade"] / 100 * 2.5
        self.energy *= math.exp(-dt / tau)
        star = hsv_rows(self.hue, 0.85, 1.0) if self.v["random_hue"] else np.tile(rgb(self.v["star"]), (self.n, 1))
        bg = rgb(self.v["background"])
        e = self.energy[:, None]
        return bg[None, :] * (1 - e) + star * e


class Reactive(Effect):
    id = "reactive"
    name = "Reactive"
    description = "Keys respond to typing: the pressed key, a ripple or a cross lights up."
    needs = frozenset({"keys"})
    params = (
        Param("style", "Style", "choice", "ripple",
              choices=(("key", "Pressed key"), ("ripple", "Ripple"), ("cross", "Cross"))),
        Param("idle", "Idle colour", "color", (12, 14, 22)),
        Param("rainbow", "Rainbow", "toggle", True),
        Param("press", "Press colour", "color", (255, 143, 31)),
        Param("fade", "Fade time", "range", 40),
        Param("speed", "Ripple speed", "range", 50),
    )

    def __init__(self, pos, values=None):
        super().__init__(pos, values)
        self.energy = np.zeros(self.n, np.float32)
        self.color = np.zeros((self.n, 3), np.float32)
        self.ripples: list[tuple[float, float, float, float]] = []   # x, y, start time, hue
        self.t = 0.0
        self.hue = 0.0

    def _press_color(self) -> np.ndarray:
        if self.v["rainbow"]:
            self.hue = (self.hue + 0.13) % 1.0
            return np.array(colorsys.hsv_to_rgb(self.hue, 1, 1), np.float32)
        return rgb(self.v["press"])

    def render(self, t, dt, inp):
        self.t = t
        tau = 0.08 + self.v["fade"] / 100 * 1.5
        style = self.v["style"]
        x, y = self.pos[:, 0], self.pos[:, 1] * 0.36
        for i in inp.presses:
            c = self._press_color()
            if style == "key":
                self.energy[i] = 1.0
                self.color[i] = c
            elif style == "cross":
                hit = (np.abs(self.pos[:, 1] - self.pos[i, 1]) < 0.08) | (np.abs(x - x[i]) < 0.03)
                self.energy[hit] = 1.0
                self.color[hit] = c
            else:
                self.ripples.append((float(x[i]), float(y[i]), t, self.hue if self.v["rainbow"] else -1.0))
        self.energy *= math.exp(-dt / tau)
        out = np.tile(rgb(self.v["idle"]), (self.n, 1))
        if style == "ripple":
            speed = 0.15 + self.v["speed"] / 100 * 1.2
            alive = []
            for rx, ry, t0, hue in self.ripples[-24:]:
                age = t - t0
                life = math.exp(-age / (tau * 1.5))
                if life < 0.02:
                    continue
                alive.append((rx, ry, t0, hue))
                d = np.hypot(x - rx, y - ry)
                ring = np.exp(-((d - speed * age) ** 2) / 0.0025) * life
                c = (np.array(colorsys.hsv_to_rgb(hue, 1, 1), np.float32) if hue >= 0 else rgb(self.v["press"]))
                out = out * (1 - ring[:, None]) + c[None, :] * ring[:, None]
            self.ripples = alive
            return out
        e = self.energy[:, None]
        return out * (1 - e) + self.color * e


class Fire(Effect):
    id = "fire"
    name = "Fire"
    description = "Flames rising from the space bar."
    params = (
        Param("palette", "Colours", "choice", "fire", choices=tuple(c for c in PALETTE_CHOICES if c[0] != "rainbow")),
        Param("height", "Flame height", "range", 55),
        Param("speed", "Speed", "range", 45),
    )

    def __init__(self, pos, values=None):
        super().__init__(pos, values)
        self.grid = np.random.default_rng(11).random((64, 64)).astype(np.float32)

    def _noise(self, u: np.ndarray, v: np.ndarray) -> np.ndarray:
        g = self.grid
        u0, v0 = np.floor(u).astype(int), np.floor(v).astype(int)
        fu, fv = u - u0, v - v0
        fu, fv = fu * fu * (3 - 2 * fu), fv * fv * (3 - 2 * fv)
        a = g[u0 % 64, v0 % 64]
        b = g[(u0 + 1) % 64, v0 % 64]
        c = g[u0 % 64, (v0 + 1) % 64]
        d = g[(u0 + 1) % 64, (v0 + 1) % 64]
        return (a * (1 - fu) + b * fu) * (1 - fv) + (c * (1 - fu) + d * fu) * fv

    def render(self, t, dt, inp):
        x, y = self.pos[:, 0], self.pos[:, 1]
        sp = 0.4 + self.v["speed"] / 100 * 3.0
        n = 0.65 * self._noise(x * 9, y * 3 + t * sp) + 0.35 * self._noise(x * 19 + 7, y * 6 + t * sp * 1.7)
        height = 0.25 + self.v["height"] / 100 * 0.9
        heat = np.clip(n * 1.25 - (1 - y) / height + 0.45, 0, 1)
        return palette(self.v["palette"], heat)


class Spectrum(Effect):
    id = "spectrum"
    name = "Music: spectrum"
    description = "An audio spectrum analyser across the keyboard, from what is playing on the PC."
    needs = frozenset({"audio"})
    params = (
        Param("palette", "Colours", "choice", "rainbow", choices=PALETTE_CHOICES),
        Param("style", "Style", "choice", "bars", choices=(("bars", "Bars"), ("glow", "Glow"))),
        Param("sensitivity", "Sensitivity", "range", 55),
    )

    def render(self, t, dt, inp):
        x, y = self.pos[:, 0], self.pos[:, 1]
        if inp.bands is None or not len(inp.bands):
            return np.zeros((self.n, 3), np.float32)
        nb = len(inp.bands)
        gain = 0.4 + self.v["sensitivity"] / 100 * 1.6
        level = np.clip(inp.bands[np.clip((x * nb).astype(int), 0, nb - 1)] * gain, 0, 1)
        if self.v["style"] == "glow":
            v = level
        else:
            height = 1 - y                                   # 0 at the space bar, 1 at the F row
            v = np.clip((level - height) * 6 + 1, 0, 1)
        if self.v["palette"] == "rainbow":
            return hsv_rows(x * 0.85, 1.0, v)
        return palette(self.v["palette"], v * (0.35 + 0.65 * (1 - y if self.v["style"] == "bars" else level)))


class Pulse(Effect):
    id = "pulse"
    name = "Music: pulse"
    description = "The keyboard breathes with the loudness of the music."
    needs = frozenset({"audio"})
    params = (
        Param("a", "Quiet colour", "color", (40, 10, 120)),
        Param("b", "Loud colour", "color", (255, 143, 31)),
        Param("sensitivity", "Sensitivity", "range", 55),
        Param("from_centre", "Spread from the centre", "toggle", True),
    )

    def __init__(self, pos, values=None):
        super().__init__(pos, values)
        self.smooth = 0.0

    def render(self, t, dt, inp):
        gain = 0.5 + self.v["sensitivity"] / 100 * 2.0
        target = min(1.0, inp.level * gain)
        k = 1 - math.exp(-dt / (0.04 if target > self.smooth else 0.25))
        self.smooth += (target - self.smooth) * k
        lvl = self.smooth
        a, b = rgb(self.v["a"]), rgb(self.v["b"])
        if self.v["from_centre"]:
            d = np.hypot(self.pos[:, 0] - 0.5, (self.pos[:, 1] - 0.55) * 0.36) * 2.1
            m = np.clip((lvl * 1.15 - d) * 3 + 1, 0, 1)[:, None]
        else:
            m = np.full((self.n, 1), lvl, np.float32)
        base = a * (0.25 + 0.5 * lvl)
        return base[None, :] * (1 - m) + b[None, :] * m


EFFECTS: dict[str, type[Effect]] = {e.id: e for e in (Wave, Drift, Breath, Starlight, Reactive, Fire, Spectrum, Pulse)}


def create(effect_id: str, pos: np.ndarray, values: dict | None = None) -> Effect:
    return EFFECTS.get(effect_id, Wave)(pos, values)


def to_bytes(colors: np.ndarray, leds: list[int], brightness: float = 1.0) -> bytes:
    """Packs per-key colours into the 512-byte real-time table ([index R G B] × 128)."""
    c = np.clip(colors * (brightness * 255.0) + 0.5, 0, 255).astype(np.uint8)
    table = bytearray(512)
    for row, idx in enumerate(leds):
        if 0 <= idx < 128:
            table[idx * 4] = idx
            table[idx * 4 + 1:idx * 4 + 4] = c[row].tobytes()
    return bytes(table)
