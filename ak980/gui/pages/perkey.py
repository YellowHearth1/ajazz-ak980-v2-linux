"""Per-key colours, painted on the keyboard and stored in it."""

from __future__ import annotations

import colorsys

from PyQt6.QtCore import Qt
from PyQt6.QtGui import QColor
from PyQt6.QtWidgets import QGridLayout

from ...i18n import plural, tr
from ...layout import keys
from ...lighting import MAX_LEVEL
from .. import theme
from ..widgets import ColorChip, ColorPicker, Form, SliderRow, button, column, heading, label
from . import Page

GROUPS = {
    "WASD": {"W", "A", "S", "D"},
    "Arrows": {"↑", "↓", "←", "→"},
    "F row": {f"F{i}" for i in range(1, 13)},
    "Numpad": {"Num", "/", "*", "-", "+", "7", "8", "9", "4", "5", "6", "1", "2", "3", "0", "."},
}


class PerKeyPage(Page):
    title = "Colours"
    icon = "colours"

    def __init__(self, win):
        super().__init__(win)
        self.colors: dict[int, tuple[int, int, int]] = {}
        self.history: list[dict] = []

        brush_col = column(heading(tr("Brush")), spacing=theme.S3, stretch=False)
        self.brush = ColorPicker((255, 143, 31))
        self.brush.setMaximumWidth(440)
        brush_col.addWidget(self.brush)
        brush_col.addWidget(label(tr("Left button paints (drag across keys), right button picks a key's colour, "
                                     "middle button clears it."), "dim", wrap=True))
        brush_col.addStretch(1)

        tools = QGridLayout()
        tools.setSpacing(theme.S2)
        actions = [
            (tr("Fill every key"), "fill", self.fill_all),
            (tr("Rainbow"), None, self.rainbow),
            (tr("Gradient"), None, self.gradient),
            (tr("Clear"), "trash", self.clear),
            (tr("Undo"), None, self.undo),
        ]
        for i, (text, icon, fn) in enumerate(actions):
            b = button(text, None, icon)
            b.clicked.connect(fn)
            tools.addWidget(b, i // 2, i % 2)
        groups = QGridLayout()
        groups.setSpacing(theme.S2)
        for i, (name, members) in enumerate(GROUPS.items()):
            b = button(tr(name))
            b.setToolTip(tr("Paint {group} with the brush", group=tr(name)))
            b.clicked.connect(lambda _=False, m=members: self.paint_group(m))
            groups.addWidget(b, i // 2, i % 2)
        form = Form()
        self.gradient_to = ColorChip((40, 90, 255))
        form.add(tr("Gradient to"), self.gradient_to)
        self.bright = SliderRow(0, MAX_LEVEL, 5)
        self.bright.changed.connect(self._brightness)
        form.add(tr("Brightness"), self.bright)
        tools_col = column(heading(tr("Tools")), tools, label(tr("Paint a group"), "label"), groups, form)

        write = column(heading(tr("Keyboard")), spacing=theme.S3, stretch=False)
        self.count = label("", "dim")
        write.addWidget(self.count)
        self.write_button = button(tr("Write to keyboard"), "primary", "upload")
        self.write_button.clicked.connect(self.write)
        write.addWidget(self.write_button)
        write.addWidget(label(tr("The colours are stored in the keyboard and replace the lighting effect. "
                                 "Keys without a colour stay dark."), "dim", wrap=True))
        write.addStretch(1)

        self.root.addLayout(self.columns(brush_col, tools_col, write, stretches=[5, 5, 3]))
        self.root.addStretch(1)
        self.refresh()

    # ------------------------------------------------------------ state

    def refresh(self) -> None:
        self.colors = dict(self.profile.custom_colors)
        self.bright.set_value(self.profile.custom_brightness)
        self._update()

    def _snapshot(self) -> None:
        self.history.append(dict(self.colors))
        del self.history[:-30]

    def _update(self) -> None:
        self.count.setText(plural(len(self.colors), "{n} key coloured", "{n} keys coloured"))
        if self.win.page() is self:
            self.stage.brightness = 0.35 + 0.65 * self.bright.value() / MAX_LEVEL
            self.stage.set_colors({i: QColor(*c) for i, c in self.colors.items()})

    def on_enter(self) -> None:
        self.stage.paint_mode = True
        self._update()

    def on_leave(self) -> None:
        self.stage.brightness = 1.0

    def _brightness(self, v: int) -> None:
        self.profile.custom_brightness = v
        self._update()

    # ------------------------------------------------------------ painting

    def on_stage_key(self, key, mouse_button) -> None:
        if mouse_button == Qt.MouseButton.RightButton:
            if key.index in self.colors:
                self.brush.set_rgb(self.colors[key.index])
            return
        self._snapshot()
        if mouse_button == Qt.MouseButton.MiddleButton:
            self.colors.pop(key.index, None)
        else:
            self.colors[key.index] = self.brush.rgb
        self._update()

    def fill_all(self) -> None:
        self._snapshot()
        self.colors = {k.index: self.brush.rgb for k in keys()}
        self._update()

    def clear(self) -> None:
        self._snapshot()
        self.colors = {}
        self._update()

    def undo(self) -> None:
        if self.history:
            self.colors = self.history.pop()
            self._update()

    def paint_group(self, labels: set[str]) -> None:
        self._snapshot()
        for k in keys():
            if k.label in labels:
                self.colors[k.index] = self.brush.rgb
        self._update()

    def _span(self):
        ks = keys()
        lo, hi = min(k.x for k in ks), max(k.x for k in ks)
        return ks, lo, hi - lo

    def rainbow(self) -> None:
        self._snapshot()
        ks, lo, span = self._span()
        for k in ks:
            r, g, b = colorsys.hsv_to_rgb(0.85 * (k.x - lo) / span, 1, 1)
            self.colors[k.index] = (int(r * 255), int(g * 255), int(b * 255))
        self._update()

    def gradient(self) -> None:
        self._snapshot()
        ks, lo, span = self._span()
        a, b = self.brush.rgb, self.gradient_to.rgb
        for k in ks:
            t = (k.x - lo) / span
            self.colors[k.index] = tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))
        self._update()

    # ------------------------------------------------------------ keyboard

    def write(self) -> None:
        p = self.profile
        p.custom_colors = dict(self.colors)
        p.custom_brightness = self.bright.value()
        p.light_source = "custom"
        self.win.save()
        colors, bright = dict(self.colors), self.bright.value()
        self.win.run(self.win.wired(lambda kb: kb.set_custom_colors(colors, bright)),
                     lambda _: self.win.notify(tr("Colours written to the keyboard")), tr("Writing colours…"))

    def connection_changed(self, state) -> None:
        self.write_button.setEnabled(state == "usb")
        self.write_button.setToolTip("" if state == "usb" else tr("Needs the USB cable"))
