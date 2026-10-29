"""Reference outputs of the Python implementation, for checking the Rust port byte for byte.

The Python 0.0.1 code is the one verified on an AK980 V2 (docs/PROTOCOL.md). This script builds a
profile that uses every kind of setting, saves it the way config.json stores it, and records the
exact reports every keyboard operation sends for it, plus screen and effect encodings.
crates/ak980-core/tests/parity.rs replays all of it through the Rust code.

    python tools/golden.py        # rewrites crates/ak980-core/tests/golden.json
"""

from __future__ import annotations

import contextlib
import datetime as dt
import hashlib
import json
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)
os.environ["LANGUAGE"] = "en"

import numpy as np  # noqa: E402
from PIL import Image  # noqa: E402

from ak980 import effects, lcd  # noqa: E402
from ak980.config import Config, Profile  # noqa: E402
from ak980.device import Dongle, WiredKeyboard  # noqa: E402
from ak980.keymap import MACRO_MODES, MEDIA_KEYS, MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_WIN, MOUSE_FUNCS, \
    KeyAction, key_code  # noqa: E402
from ak980.layout import positions  # noqa: E402
from ak980.lighting import LIGHT_MODES, SIDE_LIGHT_MODES, KeyboardOptions, LightSettings  # noqa: E402
from ak980.macro import Macro, MacroStep  # noqa: E402

OUT = os.path.join(ROOT, "crates", "ak980-core", "tests", "golden.json")
DETERMINISTIC_EFFECTS = ["wave", "drift", "breath", "reactive", "spectrum", "pulse"]


class FakeHid:
    """Records reports and answers like the keyboard; the same behaviour as tests/conftest.py."""

    def __init__(self, output_len: int = 65):
        self.features: list[bytes] = []
        self.writes: list[bytes] = []
        self.info = type("Info", (), {"output_len": output_len, "path": "/dev/fake"})()

    def set_feature(self, report):
        self.features.append(bytes(report))

    def get_feature(self, report_id=0, length=65):
        last = self.features[-1] if self.features else bytes(65)
        reply = bytearray(last[:65].ljust(65, b"\0"))
        reply[4] = 1
        return bytes(reply)

    def write(self, report, timeout=3.0):
        self.writes.append(bytes(report))

    def read(self, timeout=0.3, size=4096):
        return None

    def drain(self):
        pass

    @contextlib.contextmanager
    def locked(self):
        yield

    def close(self):
        pass


def recorded(fn) -> dict:
    kb = object.__new__(WiredKeyboard)
    kb._cmd, kb._lcd, kb.delay = FakeHid(), FakeHid(4097), 0
    fn(kb)
    return {"features": [f.hex() for f in kb._cmd.features], "writes": [w.hex() for w in kb._cmd.writes]}


def recorded_dongle(fn) -> dict:
    d = object.__new__(Dongle)
    d._dev = FakeHid(33)
    fn(d)
    return {"features": [], "writes": [w.hex() for w in d._dev.writes]}


def parity_profile() -> Profile:
    mouse = list(MOUSE_FUNCS)
    media = [usage for _, usage in list(MEDIA_KEYS)[:3]]
    top = {
        5: KeyAction(kind="off"),
        37: KeyAction(kind="key", mods=MOD_CTRL | MOD_SHIFT, code=key_code("T")),
        55: KeyAction(kind="key", code=key_code("LeftCtrl")),
        60: KeyAction(kind="key", mods=MOD_WIN | MOD_ALT, code=0),
        70: KeyAction(kind="media", usage=media[0]),
        71: KeyAction(kind="media", usage=media[2]),
        90: KeyAction(kind="multi", keys=[key_code("A"), key_code("B"), key_code("C")]),
    }
    for i, mode in enumerate(MACRO_MODES):
        top[100 + i] = KeyAction(kind="macro", macro=i % 2, macro_mode=mode, macro_count=1 + i)
    fn_layer = {10 + i: KeyAction(kind="mouse", mouse=m) for i, m in enumerate(mouse)}
    fn_layer[30] = KeyAction(kind="key", mods=MOD_SHIFT, code=key_code("F13"))
    macros = [
        Macro("GG WP", Macro.text_steps("gg wp", 25)),
        Macro("Reopen tab", Macro.combo_steps("Ctrl+Shift+T", 15)),
        Macro("Click", [MacroStep("mdown", 1, 40), MacroStep("mup", 1, 12), MacroStep("mdown", 4, 300),
                        MacroStep("mup", 4, 10)]),
        Macro("Empty", []),
        Macro("Long pause", [MacroStep("down", key_code("Space"), 65535), MacroStep("up", key_code("Space"))]),
    ]
    return Profile(
        name="Parity", light_source="custom",
        lighting=LightSettings(mode=2, r=0xFF, g=0xD2, b=0x00, brightness=4, speed=1, direction=2),
        custom_colors={0: (255, 0, 0), 37: (255, 210, 0), 55: (1, 2, 3), 99: (0, 0, 255)},
        custom_brightness=3, host_effect="fire", host_values={"fire": {"height": 80}}, host_brightness=55,
        keymap_top=top, keymap_fn=fn_layer, macros=macros,
        options=KeyboardOptions(disable_alt_tab=True, disable_win=True, fn_switch=True, sleep_time=0,
                                key_response=4),
    )


def gradient(flip: bool = False) -> Image.Image:
    x = np.arange(240, dtype=np.uint16)[None, :].repeat(135, 0)
    y = np.arange(135, dtype=np.uint16)[:, None].repeat(240, 1)
    rgb = np.stack([x * 255 // 239, y * 255 // 134, (x + y) % 256], axis=-1).astype(np.uint8)
    img = Image.fromarray(rgb, "RGB")
    return img.transpose(Image.Transpose.FLIP_LEFT_RIGHT) if flip else img


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def effect_frames(leds, pos) -> list[dict]:
    out = []
    bands = np.linspace(0.0, 1.0, 16, dtype=np.float32)
    for effect_id in DETERMINISTIC_EFFECTS:
        e = effects.create(effect_id, pos, {})
        frames = []
        for i, t in enumerate((0.0, 0.5, 1.25)):
            inp = effects.Inputs(presses=[3, 10] if i == 0 else [], bands=bands, level=0.6, beat=(i == 1))
            frames.append(effects.to_bytes(e.render(t, 0.5, inp), leds, 0.8).hex())
        out.append({"effect": effect_id, "frames": frames})
    return out


def main() -> int:
    profile = parity_profile()
    cfg = Config(path=os.devnull)
    cfg.profiles = [profile]
    cfg.active = profile.name
    leds, pos = positions()
    pos = np.asarray(pos, dtype=np.float32)

    ops = [
        {"op": "set_lighting", "settings": profile.lighting.to_dict(), **recorded(lambda kb: kb.set_lighting(profile.lighting))},
        {"op": "set_custom_colors", **recorded(lambda kb: kb.set_custom_colors(profile.custom_colors, profile.custom_brightness))},
        {"op": "set_options", "options": profile.options.to_dict(), **recorded(lambda kb: kb.set_options(profile.options))},
        {"op": "set_keymap", "fn_layer": False, **recorded(lambda kb: kb.set_keymap(False, profile.keymap_top))},
        {"op": "set_keymap", "fn_layer": True, **recorded(lambda kb: kb.set_keymap(True, profile.keymap_fn))},
        {"op": "set_macros", **recorded(lambda kb: kb.set_macros(profile.macros))},
    ]
    for m in LIGHT_MODES:
        s = LightSettings(mode=m.id, r=(m.id * 37) % 256, g=(m.id * 91) % 256, b=255 - m.id, rainbow=m.id % 3 == 0,
                          brightness=m.id % 6, speed=(m.id + 2) % 6, direction=m.id % 4)
        ops.append({"op": "set_lighting", "settings": s.to_dict(), **recorded(lambda kb, s=s: kb.set_lighting(s))})
    for o in (KeyboardOptions(), KeyboardOptions(disable_alt_f4=True, sleep_time=3, key_response=1)):
        ops.append({"op": "set_options", "options": o.to_dict(), **recorded(lambda kb, o=o: kb.set_options(o))})
    for mode in list(SIDE_LIGHT_MODES.values())[:3]:
        ops.append({"op": "set_side_light", "mode": mode, **recorded(lambda kb, mode=mode: kb.set_side_light(mode))})
    for when, slot in ((dt.datetime(2026, 10, 29, 15, 7, 32), 1), (dt.datetime(2027, 1, 3, 0, 0, 59), 3)):
        stamp = [when.year, when.month, when.day, when.hour, when.minute, when.second]
        ops.append({"op": "sync_clock", "time": stamp, "slot": slot,
                    **recorded(lambda kb, w=when, s=slot: kb.sync_clock(w, s))})
        ops.append({"op": "dongle_sync_clock", "time": stamp, "slot": slot,
                    **recorded_dongle(lambda d, w=when, s=slot: d.sync_clock(w, s))})
    ops.append({"op": "dongle_battery", **recorded_dongle(lambda d: d.request(b"\x20\x01"))})
    wave = effects.create("wave", pos, {})
    table = effects.to_bytes(wave.render(0.3, 0.3, effects.Inputs()), leds, 1.0)
    ops.append({"op": "send_realtime", "table": table.hex(), **recorded(lambda kb: kb.send_realtime(table, 0))})

    frames = [(gradient(), 100), (Image.new("RGB", (240, 135), (255, 0, 0)), 50), (gradient(True), 510)]
    screen = {
        "rgb565_dither": sha(lcd.to_rgb565(gradient(), True)),
        "rgb565_plain": sha(lcd.to_rgb565(gradient(), False)),
        "animation": sha(lcd.encode_animation(frames, True)),
        "animation_len": len(lcd.encode_animation(frames, True)),
        "page_count": [lcd.page_count(n) for n in (1, 2, 24, 140)],
    }

    golden = {
        "comment": "Generated by tools/golden.py from the Python implementation; do not edit by hand.",
        "config": cfg.to_dict(),
        "ops": ops,
        "screen": screen,
        "effects": effect_frames(leds, pos),
    }
    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(golden, f, ensure_ascii=False, indent=1)
        f.write("\n")
    print(f"{OUT}: {len(ops)} operations, {len(golden['effects'])} effects")
    return 0


if __name__ == "__main__":
    sys.exit(main())
