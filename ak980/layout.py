"""The AK980 V2 key layout (from the vendor's layouts/rgb-keyboard.xml, via ak980-studio)."""

from __future__ import annotations

import json
import os
from dataclasses import dataclass
from functools import lru_cache

from .i18n import tr
from .keymap import key_code, key_name

DATA = os.path.join(os.path.dirname(__file__), "data")

# Where the 1.14" screen and the knob sit (above the numpad), in layout units.
SCREEN_RECT = (648, 36, 72, 41)       # x, y, w, h of the bezel
KNOB_CENTER = (757.5, 56.5)
KNOB_RADIUS = 17
CASE_RECT = (8, 20, 785, 268)


@dataclass(frozen=True)
class LayoutKey:
    label: str
    index: int      # LED index = slot in the remap tables
    code: int       # default HID usage
    x: int
    y: int
    w: int
    h: int

    @property
    def id(self) -> str:
        """Unique name for the CLI (from the HID usage: LeftShift, Num7, ...)."""
        return key_name(self.code)

    @property
    def center(self) -> tuple[float, float]:
        return self.x + self.w / 2, self.y + self.h / 2


@lru_cache(maxsize=1)
def layout() -> tuple[int, int, tuple[LayoutKey, ...]]:
    with open(os.path.join(DATA, "layout.json"), encoding="utf-8") as f:
        d = json.load(f)
    keys = tuple(LayoutKey(k["name"], k["led"], k["code"], k["x"], k["y"], k["w"], k["h"]) for k in d["keys"])
    return d["width"], d["height"], keys


def keys() -> tuple[LayoutKey, ...]:
    return layout()[2]


def by_index(index: int) -> LayoutKey | None:
    return next((k for k in keys() if k.index == index), None)


def by_code(code: int) -> LayoutKey | None:
    return next((k for k in keys() if k.code == code), None)


def find_key(name: str) -> LayoutKey:
    ks = keys()
    if name.isdigit():
        for k in ks:
            if k.index == int(name):
                return k
    low = name.lower()
    for k in ks:
        if k.id.lower() == low:
            return k
    for k in ks:
        if k.label.lower() == low:
            return k
    try:
        code = key_code(name)
    except ValueError:
        code = -1
    for k in ks:
        if k.code == code:
            return k
    raise ValueError(tr("There is no key “{name}” on the AK980 (see `ak980 keys`)", name=name))


@lru_cache(maxsize=1)
def positions():
    """(LED indices, (N, 2) array of key centres normalised to 0..1) for the effects."""
    import numpy as np
    ks = keys()
    xs = np.array([k.center[0] for k in ks], np.float32)
    ys = np.array([k.center[1] for k in ks], np.float32)
    pos = np.stack([(xs - xs.min()) / (xs.max() - xs.min()), (ys - ys.min()) / (ys.max() - ys.min())], -1)
    return [k.index for k in ks], pos
