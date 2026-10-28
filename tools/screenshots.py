"""Renders the documentation screenshots from a demo profile, without touching a keyboard.

    QT_QPA_PLATFORM=offscreen AK980_PREVIEW_ONLY=1 python tools/screenshots.py docs/screenshots

English shots go to docs/screenshots/, Ukrainian ones to docs/screenshots/uk/.
"""

from __future__ import annotations

import json
import math
import os
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PAGES = ["screen", "lighting", "lighting-pc", "colours", "keys", "macros", "profiles", "device"]


def demo_animation(path: str) -> None:
    """An original animation for the screen page: drifting light bands behind a caption."""
    import numpy as np
    from PIL import Image, ImageDraw, ImageFont
    font = ImageFont.truetype(os.path.join(ROOT, "ak980", "data", "fonts", "FiraSansCondensed-Bold.ttf"), 34)
    small = ImageFont.truetype(os.path.join(ROOT, "ak980", "data", "fonts", "FiraSansCondensed-SemiBold.ttf"), 15)
    y, x = np.mgrid[0:135, 0:240].astype(np.float32)
    frames = []
    for i in range(24):
        t = i / 24 * math.tau
        v = (np.sin(x / 31 + t) + np.sin((x + y * 1.7) / 47 - t) + np.sin(np.hypot(x - 120, y - 140) / 23 + t)) / 3
        r = 40 + 215 * np.clip(v * 0.6 + 0.55, 0, 1)
        g = 20 + 120 * np.clip(v * 0.5 + 0.35, 0, 1)
        b = 30 + 90 * np.clip(0.6 - v * 0.6, 0, 1)
        img = Image.fromarray(np.stack([r, g, b], -1).astype("uint8"), "RGB")
        d = ImageDraw.Draw(img)
        d.text((16, 34), "AK980 V2", font=font, fill=(26, 20, 8))
        d.text((18, 80), "Software for Linux", font=small, fill=(26, 20, 8))
        frames.append(img)
    frames[0].save(path, save_all=True, append_images=frames[1:], duration=70, loop=0)


def demo_config(gif: str, lang: str) -> dict:
    uk = lang == "uk"
    caps, f12, num0 = 55, 13, 104
    return {
        "version": 2,
        "active": "Основний" if uk else "Main",
        "settings": {"language": lang, "last_image": gif, "auto_switch": True,
                     "rules": [{"process": "cs2", "profile": "CS2"}, {"process": "eldenring.exe", "profile": "Elden Ring"}],
                     "launchers": [{"key": "F13", "command": "obs", "label": ""}], "default_profile": ""},
        "profiles": [
            {"name": "Основний" if uk else "Main", "light_source": "firmware",
             "lighting": {"mode": 11, "r": 255, "g": 143, "b": 31, "rainbow": False, "brightness": 5, "speed": 3,
                          "direction": 0},
             "custom_colors": {}, "host_effect": "wave", "host_values": {"wave": {"speed": 45}},
             "keymap_top": {str(caps): {"kind": "key", "mods": 0, "code": 0xE0},
                            str(f12): {"kind": "macro", "macro": 0, "macro_mode": "once"},
                            str(num0): {"kind": "key", "code": 0x68}},
             "keymap_fn": {},
             "macros": [{"name": "GG WP" if not uk else "GG WP",
                         "steps": [["down", 0xE1, 12], ["down", 0x0A, 60], ["up", 0x0A, 40], ["down", 0x0A, 55],
                                   ["up", 0x0A, 30], ["up", 0xE1, 80], ["down", 0x2C, 40], ["up", 0x2C, 50],
                                   ["down", 0x1A, 45], ["up", 0x1A, 35], ["down", 0x13, 50], ["up", 0x13, 10]]},
                        {"name": "Build order", "steps": [["down", 0x14, 30], ["up", 0x14, 30]]}],
             "options": {"sleep_time": 2, "key_response": 2}},
            {"name": "CS2", "light_source": "host", "host_effect": "reactive",
             "host_values": {"reactive": {"style": "ripple"}}, "lighting": {"mode": 1}},
            {"name": "Elden Ring", "light_source": "host", "host_effect": "fire", "lighting": {"mode": 7}},
        ],
    }


def paint_demo_colours(page) -> None:
    from ak980.layout import keys
    colours = {}
    for k in keys():
        colours[k.index] = (24, 30, 46)
        if k.label in ("W", "A", "S", "D"):
            colours[k.index] = (255, 143, 31)
        elif k.label in ("↑", "↓", "←", "→"):
            colours[k.index] = (40, 150, 255)
        elif k.label.startswith("F") and k.label[1:].isdigit():
            colours[k.index] = (255, 60, 90)
        elif k.label in ("Esc",):
            colours[k.index] = (255, 40, 30)
    page.colors = colours
    page._update()


def shoot(out_dir: str, lang: str) -> None:
    from PyQt6.QtCore import QTimer
    from PyQt6.QtWidgets import QApplication

    sys.path.insert(0, ROOT)
    from ak980 import gui
    from ak980.gui import theme
    from ak980.i18n import set_language
    set_language(lang)
    app = QApplication(sys.argv[:1])
    app.setStyle("Fusion")
    app.setPalette(gui._palette())
    theme.load_fonts()
    app.setFont(theme.font("body"))
    app.setStyleSheet(theme.stylesheet())
    from ak980.gui.window import MainWindow
    win = MainWindow()
    win.resize(1440, 900)
    win.show()
    os.makedirs(out_dir, exist_ok=True)
    steps = list(enumerate(PAGES))

    def prepare(name: str) -> None:
        index = {"screen": 0, "lighting": 1, "lighting-pc": 1, "colours": 2, "keys": 3, "macros": 4,
                 "profiles": 5, "device": 6}[name]
        win.select_page(index)
        page = win.pages[index]
        if name == "lighting":
            page.source.set_value("firmware")
            page.source_changed("firmware")
        elif name == "lighting-pc":
            page.source.set_value("host")
            page.source_changed("host")
            page.effects.set_value("wave")
            page.host_changed("wave")
        elif name == "colours":
            paint_demo_colours(page)
        elif name == "keys":
            from ak980.layout import by_index
            page.select(by_index(55))
        elif name == "macros":
            page.list.setCurrentRow(0)
        win.toast.setText("")
        win.centralWidget().setFocus()

    def step(i: int = 0) -> None:
        if i >= len(steps):
            win.engine.stop()
            app.quit()
            return
        _, name = steps[i]
        prepare(name)
        def capture() -> None:
            win.toast.setText("")
            win.grab().save(os.path.join(out_dir, f"{name}.png"))
            step(i + 1)

        QTimer.singleShot(1500 if i == 0 else 900, capture)

    QTimer.singleShot(1800, step)
    app.exec()


def main() -> int:
    out = os.path.abspath(sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "docs", "screenshots"))
    if len(sys.argv) > 2:          # child process: one language
        shoot(out, sys.argv[2])
        return 0
    with tempfile.TemporaryDirectory() as tmp:
        gif = os.path.join(tmp, "demo.gif")
        demo_animation(gif)
        for lang, sub in (("en", ""), ("uk", "uk")):
            home = os.path.join(tmp, lang)
            os.makedirs(os.path.join(home, "config", "ak980"))
            with open(os.path.join(home, "config", "ak980", "config.json"), "w", encoding="utf-8") as f:
                json.dump(demo_config(gif, lang), f, ensure_ascii=False)
            env = dict(os.environ, XDG_CONFIG_HOME=os.path.join(home, "config"),
                       XDG_DATA_HOME=os.path.join(home, "data"), AK980_PREVIEW_ONLY="1",
                       QT_QPA_PLATFORM=os.environ.get("QT_QPA_PLATFORM", "offscreen"), PYTHONPATH=ROOT)
            subprocess.run([sys.executable, __file__, os.path.join(out, sub), lang], env=env, check=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
