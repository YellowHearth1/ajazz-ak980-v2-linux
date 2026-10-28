"""Lighting: the keyboard's own effects and effects computed on the PC."""

from __future__ import annotations

import math
import time

import numpy as np
from PyQt6.QtCore import QTimer
from PyQt6.QtGui import QColor
from PyQt6.QtWidgets import QHBoxLayout, QStackedWidget, QVBoxLayout, QWidget

from ... import runtime, service
from ...effects import EFFECTS, hsv_rows
from ...i18n import tr
from ...layout import positions
from ...lighting import (CAP_BRIGHTNESS, CAP_COLOR, CAP_DIRECTION2, CAP_DIRECTION4, CAP_RAINBOW, CAP_SPEED,
                         DIRECTIONS, LIGHT_MODES, MAX_LEVEL, mode_by_id)
from .. import theme
from ..widgets import (ColorChip, ColorPicker, Form, KeycapGrid, Segmented, SliderRow, Toggle, button, column,
                       combo, heading, label)
from . import Page


def firmware_preview(s, t: float, pos: np.ndarray, has_color: bool = True) -> np.ndarray:
    """A rough idea of a firmware effect; the keyboard draws the real one itself."""
    n = len(pos)
    x, y = pos[:, 0], pos[:, 1]
    m = s.mode
    if m == 0:
        return np.zeros((n, 3), np.float32)
    speed = 0.15 + s.speed / MAX_LEVEL * 0.9
    level = 0.25 + 0.75 * s.brightness / MAX_LEVEL
    base = np.array([s.r, s.g, s.b], np.float32) / 255
    if m in (10, 11, 12, 16, 18):                     # waves and rotations
        if m == 12:
            coord = np.arctan2((y - 0.5) * 0.36, x - 0.5) / math.tau
        else:
            coord = {0: x, 1: y * 0.36, 2: -y * 0.36, 3: -x}.get(s.direction, x)
        if s.rainbow or not has_color:
            return hsv_rows(coord - t * speed, 1.0, level)
        wave = 0.35 + 0.65 * (0.5 + 0.5 * np.cos(math.tau * (coord * 1.5 - t * speed)))
        return base[None, :] * wave[:, None] * level
    if m in (6, 8):                                    # colour cycles
        hue = (t * speed * 0.3) + (x * 0.0 if m == 8 else (np.sin(x * 9 + y * 5) * 0.15))
        return hsv_rows(hue, 1.0, level)
    if m == 7:                                         # breath
        b = (1 - math.cos(t * speed * math.tau)) / 2
        col = hsv_rows(np.full(n, t * 0.07), 1.0, 1.0) if s.rainbow else np.tile(base, (n, 1))
        return col * (0.05 + 0.95 * b) * level
    if m in (9, 13, 14, 15, 17, 19):                   # things moving out from the centre
        d = np.hypot(x - 0.5, (y - 0.5) * 0.36) * 2
        wave = 0.5 + 0.5 * np.cos(math.tau * (d * 2 - t * speed))
        col = hsv_rows(d - t * speed * 0.5, 1.0, 1.0) if s.rainbow else np.tile(base, (n, 1))
        return col * (0.15 + 0.85 * wave[:, None]) * level
    if m in (2, 3, 4, 5):                              # reactive and glitter: a calm stand-in
        rng = np.random.default_rng(int(t * 6))
        sparkle = rng.random(n) > (0.93 if m in (4, 5) else 1.0)
        col = hsv_rows(x, 1.0, 1.0) if s.rainbow else np.tile(base, (n, 1))
        dim = 0.25 if m == 3 else 0.55
        return col * np.where(sparkle, 1.0, dim)[:, None] * level
    col = hsv_rows(x, 1.0, 1.0) if s.rainbow else np.tile(base, (n, 1))
    return col * level


class ParamEditor(QWidget):
    """Controls for a PC effect's settings, built from its parameter list."""

    def __init__(self, effect_cls, values: dict, on_change):
        super().__init__()
        form = Form(self)
        self.effect = effect_cls
        self.values = values
        self.on_change = on_change
        self.widgets = {}
        for prm in effect_cls.params:
            v = values.get(prm.id, prm.default)
            if prm.kind == "color":
                w = ColorChip(v)
                w.changed.connect(lambda c, k=prm.id: self._set(k, tuple(c)))
            elif prm.kind == "range":
                w = SliderRow(prm.lo, prm.hi, int(v))
                w.changed.connect(lambda val, k=prm.id: self._set(k, val))
            elif prm.kind == "toggle":
                w = Toggle("", bool(v))
                w.toggled.connect(lambda val, k=prm.id: self._set(k, val))
            else:
                opts = [(c, tr(t)) for c, t in prm.choices]
                if len(opts) <= 3:
                    w = Segmented(opts, v)
                    w.changed.connect(lambda val, k=prm.id: self._set(k, val))
                else:
                    w = combo(opts, v)
                    w.currentIndexChanged.connect(lambda _, k=prm.id, box=w: self._set(k, box.currentData()))
            self.widgets[prm.id] = w
            form.add(tr(prm.label), w)
        self._dependencies()

    def _set(self, key, value) -> None:
        self.values[key] = value
        self._dependencies()
        self.on_change()

    def _dependencies(self) -> None:
        """Hides settings that do nothing with the current choices."""
        v = {p.id: self.values.get(p.id, p.default) for p in self.effect.params}
        rules = {"b": v.get("alternate", True), "star": not v.get("random_hue", False),
                 "press": not v.get("rainbow", False), "speed": v.get("style", "ripple") == "ripple"
                 if self.effect.id == "reactive" else True}
        layout = self.layout()
        for key, show in rules.items():
            w = self.widgets.get(key)
            if w is None:
                continue
            w.setVisible(bool(show))
            idx = layout.indexOf(w)
            if idx >= 0:
                row, _, _, _ = layout.getItemPosition(idx)
                item = layout.itemAtPosition(row, 0)
                if item and item.widget():
                    item.widget().setVisible(bool(show))


class LightingPage(Page):
    title = "Lighting"
    icon = "lighting"

    def __init__(self, win):
        super().__init__(win)
        self.leds, self.pos = positions()
        self.t0 = time.monotonic()
        self.loading = False

        top = QHBoxLayout()
        self.source = Segmented([("firmware", tr("Keyboard effects")), ("host", tr("PC effects"))])
        self.source.changed.connect(self.source_changed)
        top.addWidget(self.source)
        top.addSpacing(theme.S3)
        self.source_note = label("", "dim", wrap=True)
        top.addWidget(self.source_note, 1)
        self.root.addLayout(top)

        self.pages = QStackedWidget()
        self.root.addWidget(self.pages, 1)
        self.pages.addWidget(self._firmware_panel())
        self.pages.addWidget(self._host_panel())

        self.send_timer = QTimer(self, singleShot=True, interval=260)
        self.send_timer.timeout.connect(self.send_firmware)
        self.save_timer = QTimer(self, singleShot=True, interval=400)
        self.save_timer.timeout.connect(self.win.save)
        self.preview = QTimer(self, interval=33)
        self.preview.timeout.connect(self.preview_tick)
        self.refresh()

    # ------------------------------------------------------------ firmware effects

    def _firmware_panel(self) -> QWidget:
        w = QWidget()
        lay = QHBoxLayout(w)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(theme.S4)
        self.modes = KeycapGrid([(m.id, tr(m.name)) for m in LIGHT_MODES], columns=5)
        self.modes.changed.connect(self.firmware_changed)
        left = column(heading(tr("Effect")), self.modes,
                      label(tr("These effects run in the keyboard itself and stay after you close the app. "
                               "The preview above is approximate."), "dim", wrap=True))
        right = Form()
        self.color = ColorPicker((255, 143, 31))
        self.color.setMaximumWidth(420)
        self.color.changed.connect(lambda _: self.firmware_changed())
        self.color_label = right.add(tr("Colour"), self.color)
        self.rainbow = Toggle(tr("Rainbow instead of one colour"))
        self.rainbow.toggled.connect(lambda _: self.firmware_changed())
        right.add(None, self.rainbow)
        self.brightness = SliderRow(0, MAX_LEVEL, MAX_LEVEL)
        self.brightness.changed.connect(lambda _: self.firmware_changed())
        self.brightness_label = right.add(tr("Brightness"), self.brightness)
        self.speed = SliderRow(0, MAX_LEVEL, 3)
        self.speed.changed.connect(lambda _: self.firmware_changed())
        self.speed_label = right.add(tr("Speed"), self.speed)
        self.direction = Segmented([(i, tr(d)) for i, d in enumerate(DIRECTIONS)], 0)
        self.direction.changed.connect(lambda _: self.firmware_changed())
        self.direction_label = right.add(tr("Direction"), self.direction)
        holder_l, holder_r = QWidget(), QWidget()
        holder_l.setLayout(left)
        holder_r.setLayout(column(heading(tr("Settings")), right))
        lay.addWidget(holder_l, 6)
        lay.addWidget(holder_r, 5)
        return w

    def _caps(self) -> int:
        return mode_by_id(self.modes.value() if self.modes.value() is not None else 1).caps

    def _update_caps(self) -> None:
        c = self._caps()
        rainbow = self.rainbow.isChecked() and bool(c & CAP_RAINBOW)
        for w, on in ((self.color, c & CAP_COLOR and not rainbow), (self.color_label, c & CAP_COLOR and not rainbow),
                      (self.rainbow, c & CAP_RAINBOW), (self.brightness, c & CAP_BRIGHTNESS),
                      (self.brightness_label, c & CAP_BRIGHTNESS), (self.speed, c & CAP_SPEED),
                      (self.speed_label, c & CAP_SPEED), (self.direction, c & (CAP_DIRECTION2 | CAP_DIRECTION4)),
                      (self.direction_label, c & (CAP_DIRECTION2 | CAP_DIRECTION4))):
            if w is not None:
                w.setEnabled(bool(on))

    def firmware_changed(self, *_) -> None:
        if self.loading:
            return
        s = self.profile.lighting
        s.mode = self.modes.value() if self.modes.value() is not None else s.mode
        s.r, s.g, s.b = self.color.rgb
        s.rainbow = self.rainbow.isChecked()
        s.brightness = self.brightness.value()
        s.speed = self.speed.value()
        s.direction = self.direction.value() or 0
        self.profile.light_source = "firmware"
        self._update_caps()
        self._source_note()
        self.save_timer.start()
        self.send_timer.start()

    def send_firmware(self) -> None:
        if self.win.connection != "usb":
            self.win.notify(tr("Saved. Connect the cable to send it to the keyboard"))
            return
        s = self.profile.lighting
        self.win.run(self.win.wired(lambda kb: kb.set_lighting(s)),
                     lambda _: self.win.notify(tr("Lighting applied")), tr("Sending the lighting…"))

    # ------------------------------------------------------------ PC effects

    def _host_panel(self) -> QWidget:
        w = QWidget()
        lay = QHBoxLayout(w)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(theme.S4)
        self.effects = KeycapGrid([(e.id, tr(e.name)) for e in EFFECTS.values()], columns=4)
        self.effects.changed.connect(self.host_changed)
        self.effect_about = label("", "dim", wrap=True)
        self.host_status = label("", "dim", wrap=True)
        self.agent_button = button(tr("Keep running in the background"), None, "power",
                                   tr("Start the agent with your session, so the effect runs without this window"))
        self.agent_button.clicked.connect(self.enable_agent)
        left = column(heading(tr("Effect")), self.effects, self.effect_about, self.host_status, self.agent_button)
        holder_l = QWidget()
        holder_l.setLayout(left)
        lay.addWidget(holder_l, 6)
        right = QVBoxLayout()
        right.setSpacing(theme.S3)
        right.addWidget(heading(tr("Settings")))
        self.params_holder = QVBoxLayout()
        right.addLayout(self.params_holder)
        bright = Form()
        self.host_brightness = SliderRow(5, 100, 100, "%")
        self.host_brightness.changed.connect(self.host_brightness_changed)
        bright.add(tr("Brightness"), self.host_brightness)
        right.addLayout(bright)
        right.addStretch(1)
        holder_r = QWidget()
        holder_r.setLayout(right)
        lay.addWidget(holder_r, 5)
        self.param_editor = None
        return w

    def host_changed(self, effect_id) -> None:
        if self.loading:
            return
        p = self.profile
        p.host_effect = effect_id
        p.light_source = "host"
        self._build_params()
        self._source_note()
        self.save_timer.start()

    def _build_params(self) -> None:
        if self.param_editor is not None:
            self.param_editor.setParent(None)
            self.param_editor.deleteLater()
        p = self.profile
        cls = EFFECTS.get(p.host_effect) or next(iter(EFFECTS.values()))
        values = p.host_values.setdefault(cls.id, {prm.id: prm.default for prm in cls.params})
        for prm in cls.params:
            values.setdefault(prm.id, prm.default)
        self.param_editor = ParamEditor(cls, values, self.save_timer.start)
        self.params_holder.addWidget(self.param_editor)
        self.effect_about.setText(tr(cls.description))
        self._host_status()

    def host_brightness_changed(self, v: int) -> None:
        self.profile.host_brightness = v
        self.save_timer.start()

    def _host_status(self) -> None:
        cls = EFFECTS.get(self.profile.host_effect)
        notes = []
        agent = runtime.agent_running()
        if self.win.connection != "usb":
            notes.append(tr("PC effects need the USB cable; the preview keeps running."))
        elif agent:
            notes.append(tr("The background agent is drawing this effect on the keyboard."))
        else:
            notes.append(tr("Runs while this window is open."))
        if cls is not None and "audio" in cls.needs:
            from ...audio import available
            if not available():
                notes.append(tr("Needs parec (PulseAudio or pipewire-pulse) or pw-record to hear the music."))
            else:
                notes.append(tr("Listens to whatever the PC is playing."))
        if cls is not None and "keys" in cls.needs:
            from ...keyinput import readable
            if not readable():
                notes.append(tr("Cannot read key presses: add your user to the “input” group. Clicks on the "
                                "keyboard above still work."))
        self.host_status.setText(" ".join(notes))
        self.agent_button.setVisible(not agent)

    def enable_agent(self) -> None:
        def done(_):
            self.win.notify(tr("The background agent is on"))
            QTimer.singleShot(1500, self._host_status)
        self.win.run(lambda: service.enable(service.preferred_user_method()), done, tr("Starting the agent…"),
                     device=False)

    # ------------------------------------------------------------ common

    def source_changed(self, value) -> None:
        if self.loading:
            return
        if value == "host":
            self.pages.setCurrentIndex(1)
            self.host_changed(self.profile.host_effect)
            self.effects.set_value(self.profile.host_effect)
        else:
            self.pages.setCurrentIndex(0)
            self.firmware_changed()

    def _source_note(self) -> None:
        src = self.profile.light_source
        if src == "custom":
            self.source_note.setText(tr("Right now the keyboard shows your per-key colours (Colours page). "
                                        "Pick an effect to replace them."))
        else:
            self.source_note.setText("")

    def refresh(self) -> None:
        self.loading = True
        p = self.profile
        s = p.lighting
        self.modes.set_value(s.mode)
        self.color.set_rgb(s.rgb)
        self.rainbow.setChecked(s.rainbow)
        self.brightness.set_value(s.brightness)
        self.speed.set_value(s.speed)
        self.direction.set_value(s.direction)
        self.effects.set_value(p.host_effect)
        self.host_brightness.set_value(p.host_brightness)
        self.source.set_value("host" if p.light_source == "host" else "firmware")
        if p.light_source != "host":
            self.source.group.setExclusive(False)
            for b in self.source.group.buttons():
                b.setChecked(False)
            self.source.group.setExclusive(True)
            if p.light_source == "firmware":
                self.source.set_value("firmware")
        self.pages.setCurrentIndex(1 if p.light_source == "host" else 0)
        self._build_params()
        self._update_caps()
        self._source_note()
        self.loading = False

    def on_enter(self) -> None:
        self.stage.brightness = 1.0
        self._host_status()
        self.preview.start()

    def on_leave(self) -> None:
        self.preview.stop()

    def wants_live_colors(self) -> bool:
        return self.profile.light_source == "host"

    def preview_tick(self) -> None:
        p = self.profile
        if p.light_source == "host":
            return          # the engine sends exact frames
        if p.light_source == "custom":
            colors = {i: QColor(*c) for i, c in p.custom_colors.items()}
            self.stage.set_colors(colors)
            return
        s = p.lighting
        c = firmware_preview(s, time.monotonic() - self.t0, self.pos, bool(mode_by_id(s.mode).caps & CAP_COLOR))
        self.stage.set_colors({idx: QColor(int(r * 255), int(g * 255), int(b * 255))
                               for idx, (r, g, b) in zip(self.leds, np.clip(c, 0, 1))})

    def on_stage_key(self, key, mouse_button) -> None:
        self.win.engine.inject_press(key.index)

    def connection_changed(self, state) -> None:
        self._host_status()
