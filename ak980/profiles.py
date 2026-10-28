"""Sending a profile to the keyboard."""

from __future__ import annotations

from typing import Callable

from .config import Profile
from .device import WiredKeyboard
from .i18n import tr

PARTS = ("lighting", "options", "macros", "keys")


def apply_lighting(kb: WiredKeyboard, p: Profile) -> None:
    """What the keyboard shows on its own: the firmware effect or the stored per-key colours.

    For a PC effect this is what remains when the stream stops.
    """
    if p.light_source == "custom":
        kb.set_custom_colors(p.custom_colors, p.custom_brightness)
    else:
        kb.set_lighting(p.lighting)


def apply_profile(kb: WiredKeyboard, p: Profile, parts=PARTS,
                  step: Callable[[str], None] | None = None) -> None:
    with kb.locked():
        if "lighting" in parts:
            step and step(tr("Lighting"))
            apply_lighting(kb, p)
        if "options" in parts:
            step and step(tr("Keyboard options"))
            kb.set_options(p.options)
        if "macros" in parts:
            step and step(tr("Macros"))
            kb.set_macros(p.macros)
        if "keys" in parts:
            step and step(tr("Key layers"))
            kb.set_keymap(False, p.keymap_top)
            kb.set_keymap(True, p.keymap_fn)
