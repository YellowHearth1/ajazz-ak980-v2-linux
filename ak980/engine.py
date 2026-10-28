"""Streams the active profile's PC lighting effect to the keyboard.

Runs inside the background agent, or inside the app window when no agent is running. It follows
the config file: when the profile switches to a firmware effect or per-key colours, the stream
stops and the keyboard gets the profile's own lighting back.
"""

from __future__ import annotations

import logging
import threading
import time
from typing import Callable

import numpy as np

from . import config as cfgmod
from .audio import AudioMeter
from .device import WiredKeyboard
from .effects import Effect, Inputs, create, to_bytes
from .keyinput import KeyListener
from .layout import by_code, positions
from .profiles import apply_lighting

log = logging.getLogger("ak980.engine")


class LightingEngine:
    def __init__(self, drive_device: bool = True, on_frame: Callable[[np.ndarray], None] | None = None,
                 config_path: str = cfgmod.CONFIG_PATH):
        self.drive_device = drive_device
        self.on_frame = on_frame
        self.config_path = config_path
        self.leds, self.pos = positions()
        self.row_of = {idx: row for row, idx in enumerate(self.leds)}
        self.status = "idle"          # idle | streaming | no-cable | error
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None
        self._presses: list[int] = []
        self._press_lock = threading.Lock()
        self._keys = KeyListener(self._on_key)
        self._audio = AudioMeter()
        self._kb: WiredKeyboard | None = None
        self._next_open = 0.0
        self.preview_only_profile: cfgmod.Profile | None = None

    # ------------------------------------------------------------ control

    def start(self) -> None:
        if self._thread and self._thread.is_alive():
            return
        self._stop.clear()
        self._thread = threading.Thread(target=self._run, name="ak980-engine", daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        if self._thread:
            self._thread.join(2)
            self._thread = None

    @property
    def audio_error(self) -> str | None:
        return self._audio.error

    def _on_key(self, usage: int, pressed: bool) -> None:
        if not pressed:
            return
        k = by_code(usage)
        if k is not None and k.index in self.row_of:
            with self._press_lock:
                self._presses.append(self.row_of[k.index])

    def inject_press(self, led_index: int) -> None:
        """Lets the app feed clicks on the on-screen keyboard to reactive effects."""
        if led_index in self.row_of:
            with self._press_lock:
                self._presses.append(self.row_of[led_index])

    # ------------------------------------------------------------ device

    def _device(self) -> WiredKeyboard | None:
        if not self.drive_device:
            return None
        if self._kb is None and time.monotonic() >= self._next_open:
            self._next_open = time.monotonic() + 2
            found = WiredKeyboard.find()
            if found:
                try:
                    self._kb = WiredKeyboard(*found)
                except OSError as e:
                    log.warning("cannot open the keyboard: %s", e)
                    self._kb = None
        return self._kb

    def _drop_device(self) -> None:
        if self._kb:
            self._kb.close()
        self._kb = None
        self._next_open = time.monotonic() + 2

    def _restore(self, profile: cfgmod.Profile) -> None:
        kb = self._device()
        if kb is None:
            return
        try:
            apply_lighting(kb, profile)
        except OSError as e:
            log.warning("cannot restore lighting: %s", e)
            self._drop_device()

    # ------------------------------------------------------------ loop

    def _run(self) -> None:
        cfg = cfgmod.Config.load(self.config_path)
        seen = cfgmod.mtime(self.config_path)
        effect: Effect | None = None
        effect_key = None
        streaming = False
        t0 = last = time.monotonic()
        next_check = 0.0
        try:
            while not self._stop.is_set():
                now = time.monotonic()
                if now >= next_check:
                    next_check = now + 0.25
                    m = cfgmod.mtime(self.config_path)
                    if m != seen:
                        seen = m
                        cfg = cfgmod.Config.load(self.config_path)
                profile = self.preview_only_profile or cfg.profile
                if profile.light_source != "host":
                    if streaming:
                        self._restore(profile)
                        streaming = False
                    self._set_inputs(None)
                    self.status = "idle"
                    effect = None
                    effect_key = None
                    self._stop.wait(0.25)
                    t0 = last = time.monotonic()
                    continue

                values = profile.host_values.get(profile.host_effect, {})
                key = (profile.host_effect, repr(sorted(values.items())))
                if key != effect_key:
                    effect = create(profile.host_effect, self.pos, values)
                    effect_key = key
                    self._set_inputs(effect)

                dt = max(1e-3, now - last)
                last = now
                with self._press_lock:
                    presses, self._presses = self._presses, []
                inp = Inputs(presses=presses)
                if "audio" in effect.needs and self._audio.running:
                    inp.bands, inp.level, inp.beat = self._audio.snapshot()
                colors = effect.render(now - t0, dt, inp)
                brightness = max(0, min(100, profile.host_brightness)) / 100

                if self.on_frame:
                    self.on_frame(np.clip(colors * brightness, 0, 1))
                kb = self._device()
                if kb is not None:
                    try:
                        kb.send_realtime(to_bytes(colors, self.leds, brightness))
                        streaming = True
                        self.status = "streaming"
                    except OSError as e:
                        log.warning("stream interrupted: %s", e)
                        self._drop_device()
                        self.status = "error"
                elif self.drive_device:
                    self.status = "no-cable"
                fps = max(5, min(60, cfg.settings.host_fps))
                self._stop.wait(max(0.0, 1 / fps - (time.monotonic() - now)))
        finally:
            if streaming:
                self._restore(self.preview_only_profile or cfg.profile)
            self._set_inputs(None)
            if self._kb:
                self._kb.close()
                self._kb = None
            self.status = "idle"

    def _set_inputs(self, effect: Effect | None) -> None:
        needs = effect.needs if effect else frozenset()
        if "keys" in needs:
            self._keys.start()
        else:
            self._keys.stop()
        if "audio" in needs:
            self._audio.start()
        elif self._audio.running:
            self._audio.stop()
