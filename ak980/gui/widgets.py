"""Controls shared by every page."""

from __future__ import annotations

from typing import Iterable, Sequence

from PyQt6.QtCore import QPointF, QRectF, QSize, Qt, pyqtSignal
from PyQt6.QtGui import QColor, QImage, QLinearGradient, QMouseEvent, QPainter, QPen
from PyQt6.QtWidgets import (QAbstractButton, QButtonGroup, QComboBox, QFrame, QGridLayout, QHBoxLayout, QLabel,
                             QLineEdit, QPushButton, QSizePolicy, QSlider, QVBoxLayout, QWidget)

from ..i18n import tr
from . import icons, theme


def label(text: str = "", role: str | None = None, wrap: bool = False) -> QLabel:
    w = QLabel(text)
    if role:
        w.setProperty("role", role)
    w.setWordWrap(wrap)
    return w


def button(text: str = "", kind: str | None = None, icon: str | None = None, tip: str | None = None) -> QPushButton:
    b = QPushButton(text)
    if kind:
        b.setProperty("kind", kind)
    if icon:
        color = theme.ON_AMBER if kind == "primary" else theme.INK
        b.setIcon(icons.icon(icon, color, None))
        b.setIconSize(QSize(16, 16))
    if tip:
        b.setToolTip(tip)
    b.setCursor(Qt.CursorShape.PointingHandCursor)
    return b


def rule(vertical: bool = False) -> QFrame:
    f = QFrame()
    f.setProperty("role", "rule")
    if vertical:
        f.setFixedWidth(1)
    else:
        f.setFixedHeight(1)
    return f


def heading(text: str) -> QLabel:
    return label(text, "heading")


class Toggle(QAbstractButton):
    """An on/off switch with its label; Space toggles it."""

    def __init__(self, text: str = "", checked: bool = False):
        super().__init__()
        self.setCheckable(True)
        self.setChecked(checked)
        self.setText(text)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        self.setSizePolicy(QSizePolicy.Policy.Preferred, QSizePolicy.Policy.Fixed)

    def sizeHint(self) -> QSize:
        fm = self.fontMetrics()
        return QSize(38 + (fm.horizontalAdvance(self.text()) + 10 if self.text() else 0), 24)

    def paintEvent(self, _) -> None:
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        on = self.isChecked()
        enabled = self.isEnabled()
        track = QRectF(1, 4, 34, 18)
        p.setPen(QPen(theme.color(theme.AMBER_LO if on else theme.EDGE), 1))
        p.setBrush(theme.color(theme.AMBER if on else theme.PLATE, 1.0 if enabled else 0.4))
        p.drawRoundedRect(track, 9, 9)
        knob_x = track.right() - 16 if on else track.left() + 2
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(theme.color(theme.ON_AMBER if on else theme.INK_DIM, 1.0 if enabled else 0.5))
        p.drawEllipse(QRectF(knob_x, track.top() + 2, 14, 14))
        if self.hasFocus():
            p.setPen(QPen(theme.color(theme.AMBER), 1.5))
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.drawRoundedRect(track.adjusted(-2, -2, 2, 2), 11, 11)
        if self.text():
            p.setPen(theme.color(theme.INK if enabled else theme.INK_OFF))
            p.setFont(self.font())
            p.drawText(QRectF(44, 0, self.width() - 44, self.height()),
                       Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft, self.text())

    def hitButton(self, pos) -> bool:
        return self.rect().contains(pos)


class Segmented(QWidget):
    """A row of mutually exclusive choices."""
    changed = pyqtSignal(object)

    def __init__(self, options: Sequence[tuple[object, str]], value: object = None):
        super().__init__()
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(0)
        self.group = QButtonGroup(self)
        self.group.setExclusive(True)
        self.values = [v for v, _ in options]
        for i, (v, text) in enumerate(options):
            b = QPushButton(text)
            b.setCheckable(True)
            b.setCursor(Qt.CursorShape.PointingHandCursor)
            b.setProperty("segment", "only" if len(options) == 1 else "first" if i == 0 else
                          "last" if i == len(options) - 1 else "middle")
            self.group.addButton(b, i)
            lay.addWidget(b)
        self.set_value(value if value is not None else self.values[0])
        self.group.idClicked.connect(lambda i: self.changed.emit(self.values[i]))

    def value(self) -> object:
        i = self.group.checkedId()
        return self.values[i] if i >= 0 else None

    def set_value(self, v: object) -> None:
        if v in self.values:
            self.group.button(self.values.index(v)).setChecked(True)


class SliderRow(QWidget):
    """Slider with its value printed next to it."""
    changed = pyqtSignal(int)

    def __init__(self, lo: int, hi: int, value: int, suffix: str = "", step: int = 1):
        super().__init__()
        lay = QHBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(theme.S2)
        self.slider = QSlider(Qt.Orientation.Horizontal)
        self.slider.setRange(lo, hi)
        self.slider.setSingleStep(step)
        self.slider.setPageStep(max(step, (hi - lo) // 10))
        self.slider.setValue(value)
        self.suffix = suffix
        self.value_label = label("", "mono")
        self.value_label.setMinimumWidth(42)
        self.value_label.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
        lay.addWidget(self.slider, 1)
        lay.addWidget(self.value_label)
        self.slider.valueChanged.connect(self._changed)
        self._show(value)

    def _show(self, v: int) -> None:
        self.value_label.setText(f"{v}{self.suffix}")

    def _changed(self, v: int) -> None:
        self._show(v)
        self.changed.emit(v)

    def value(self) -> int:
        return self.slider.value()

    def set_value(self, v: int) -> None:
        self.slider.blockSignals(True)
        self.slider.setValue(v)
        self.slider.blockSignals(False)
        self._show(v)


class _SVPlane(QWidget):
    picked = pyqtSignal(float, float)

    def __init__(self):
        super().__init__()
        self.hue = 0.0
        self.s = 1.0
        self.v = 1.0
        self._img: QImage | None = None
        self.setMinimumSize(150, 110)
        self.setCursor(Qt.CursorShape.CrossCursor)

    def set_hue(self, h: float) -> None:
        if abs(h - self.hue) > 1e-4:
            self.hue = h
            self._img = None
            self.update()

    def resizeEvent(self, e) -> None:
        self._img = None

    def paintEvent(self, _) -> None:
        w, h = self.width(), self.height()
        if self._img is None or self._img.size() != self.size():
            img = QImage(w, h, QImage.Format.Format_RGB32)
            p = QPainter(img)
            hue = QColor.fromHsvF(self.hue, 1, 1)
            g = QLinearGradient(0, 0, w, 0)
            g.setColorAt(0, QColor(255, 255, 255))
            g.setColorAt(1, hue)
            p.fillRect(0, 0, w, h, g)
            g2 = QLinearGradient(0, 0, 0, h)
            g2.setColorAt(0, QColor(0, 0, 0, 0))
            g2.setColorAt(1, QColor(0, 0, 0, 255))
            p.fillRect(0, 0, w, h, g2)
            p.end()
            self._img = img
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.drawImage(0, 0, self._img)
        x, y = self.s * (w - 1), (1 - self.v) * (h - 1)
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.setPen(QPen(QColor(0, 0, 0, 160), 3))
        p.drawEllipse(QPointF(x, y), 6, 6)
        p.setPen(QPen(QColor(255, 255, 255), 1.5))
        p.drawEllipse(QPointF(x, y), 6, 6)

    def _pick(self, e: QMouseEvent) -> None:
        s = min(1.0, max(0.0, e.position().x() / max(1, self.width() - 1)))
        v = 1 - min(1.0, max(0.0, e.position().y() / max(1, self.height() - 1)))
        self.s, self.v = s, v
        self.update()
        self.picked.emit(s, v)

    def mousePressEvent(self, e) -> None:
        self._pick(e)

    def mouseMoveEvent(self, e) -> None:
        if e.buttons() & Qt.MouseButton.LeftButton:
            self._pick(e)


class _HueStrip(QWidget):
    picked = pyqtSignal(float)

    def __init__(self):
        super().__init__()
        self.hue = 0.0
        self.setFixedHeight(14)
        self.setCursor(Qt.CursorShape.PointingHandCursor)

    def paintEvent(self, _) -> None:
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        g = QLinearGradient(0, 0, self.width(), 0)
        for i in range(7):
            g.setColorAt(i / 6, QColor.fromHsvF((i / 6) % 1.0, 1, 1))
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(g)
        p.drawRoundedRect(QRectF(0, 3, self.width(), 8), 4, 4)
        x = self.hue * (self.width() - 12) + 6
        p.setBrush(QColor.fromHsvF(self.hue, 1, 1))
        p.setPen(QPen(QColor(255, 255, 255), 2))
        p.drawEllipse(QPointF(x, 7), 6, 6)

    def _pick(self, e) -> None:
        self.hue = min(0.999, max(0.0, (e.position().x() - 6) / max(1, self.width() - 12)))
        self.update()
        self.picked.emit(self.hue)

    def mousePressEvent(self, e) -> None:
        self._pick(e)

    def mouseMoveEvent(self, e) -> None:
        if e.buttons() & Qt.MouseButton.LeftButton:
            self._pick(e)


PRESET_COLORS = [(255, 143, 31), (255, 40, 30), (255, 210, 0), (60, 220, 90), (0, 200, 255), (40, 90, 255),
                 (160, 70, 255), (255, 60, 170), (255, 255, 255)]


class ColorPicker(QWidget):
    """Saturation/value plane, hue strip, hex field and a row of presets, always visible."""
    changed = pyqtSignal(tuple)

    def __init__(self, rgb: tuple[int, int, int] = (255, 143, 31), compact: bool = False):
        super().__init__()
        lay = QVBoxLayout(self)
        lay.setContentsMargins(0, 0, 0, 0)
        lay.setSpacing(theme.S2)
        self.plane = _SVPlane()
        if compact:
            self.plane.setMinimumSize(140, 84)
        self.hue = _HueStrip()
        lay.addWidget(self.plane, 1)
        lay.addWidget(self.hue)
        row = QHBoxLayout()
        row.setSpacing(theme.S1)
        self.hex = QLineEdit()
        self.hex.setProperty("role", "mono")
        self.hex.setMaxLength(7)
        self.hex.setFixedWidth(84)
        self.hex.setToolTip(tr("Colour as #RRGGBB"))
        row.addWidget(self.hex)
        row.addSpacing(theme.S1)
        for c in PRESET_COLORS:
            row.addWidget(self._swatch(c))
        row.addStretch()
        lay.addLayout(row)
        self.rgb = tuple(rgb)
        self.set_rgb(rgb)
        self.plane.picked.connect(self._sv)
        self.hue.picked.connect(self._h)
        self.hex.editingFinished.connect(self._hex)

    def _swatch(self, c) -> QPushButton:
        b = QPushButton()
        b.setFixedSize(18, 18)
        b.setCursor(Qt.CursorShape.PointingHandCursor)
        b.setToolTip("#%02X%02X%02X" % c)
        b.setStyleSheet(f"QPushButton {{ background: rgb{tuple(c)}; border: 1px solid {theme.EDGE}; "
                        f"border-radius: 4px; padding: 0; min-height: 0; }}"
                        f"QPushButton:hover, QPushButton:focus {{ border: 2px solid {theme.INK}; }}")
        b.clicked.connect(lambda: self._emit(c))
        return b

    def set_rgb(self, rgb) -> None:
        self.rgb = tuple(int(x) for x in rgb)
        c = QColor(*self.rgb)
        h, s, v = c.hsvHueF(), c.hsvSaturationF(), c.valueF()
        if h >= 0:
            self.hue.hue = h
            self.plane.set_hue(h)
        self.plane.s, self.plane.v = s, v
        self.plane.update()
        self.hue.update()
        self.hex.setText("#%02X%02X%02X" % self.rgb)

    def _emit(self, rgb) -> None:
        self.set_rgb(rgb)
        self.changed.emit(self.rgb)

    def _sv(self, s: float, v: float) -> None:
        c = QColor.fromHsvF(self.hue.hue, s, v)
        self.rgb = (c.red(), c.green(), c.blue())
        self.hex.setText("#%02X%02X%02X" % self.rgb)
        self.changed.emit(self.rgb)

    def _h(self, h: float) -> None:
        self.plane.set_hue(h)
        self._sv(self.plane.s, self.plane.v)

    def _hex(self) -> None:
        t = self.hex.text().strip().lstrip("#")
        if len(t) == 6:
            try:
                self._emit((int(t[0:2], 16), int(t[2:4], 16), int(t[4:6], 16)))
                return
            except ValueError:
                pass
        self.hex.setText("#%02X%02X%02X" % self.rgb)


class ColorChip(QPushButton):
    """A colour swatch; clicking it opens a small picker."""
    changed = pyqtSignal(tuple)

    def __init__(self, rgb=(255, 143, 31)):
        super().__init__()
        self.setFixedSize(46, 26)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.rgb = tuple(rgb)
        self._style()
        self.clicked.connect(self._open)

    def _style(self) -> None:
        self.setToolTip("#%02X%02X%02X" % self.rgb)
        self.setStyleSheet(f"QPushButton {{ background: rgb{self.rgb}; border: 1px solid {theme.EDGE}; "
                           f"border-radius: 5px; padding: 0; min-height: 0; }}"
                           f"QPushButton:hover, QPushButton:focus {{ border: 2px solid {theme.INK}; }}")

    def set_rgb(self, rgb) -> None:
        self.rgb = tuple(int(x) for x in rgb)
        self._style()

    def _open(self) -> None:
        pop = QFrame(self, Qt.WindowType.Popup)
        pop.setStyleSheet(f"QFrame {{ background: {theme.KEYCAP}; border: 1px solid {theme.EDGE}; "
                          f"border-radius: 8px; }}")
        lay = QVBoxLayout(pop)
        lay.setContentsMargins(theme.S3, theme.S3, theme.S3, theme.S3)
        picker = ColorPicker(self.rgb, compact=True)
        picker.setFixedWidth(300)
        picker.changed.connect(lambda c: (self.set_rgb(c), self.changed.emit(self.rgb)))
        lay.addWidget(picker)
        pop.adjustSize()
        pos = self.mapToGlobal(self.rect().bottomLeft())
        pop.move(pos.x(), pos.y() + 4)
        pop.show()


class KeycapGrid(QWidget):
    """Choices drawn as keycaps, e.g. the lighting effects."""
    changed = pyqtSignal(object)

    def __init__(self, options: Sequence[tuple[object, str]], columns: int = 4, value: object = None,
                 min_height: int = 38):
        super().__init__()
        grid = QGridLayout(self)
        grid.setContentsMargins(0, 0, 0, 0)
        grid.setSpacing(6)
        self.group = QButtonGroup(self)
        self.values = [v for v, _ in options]
        for i, (v, text) in enumerate(options):
            b = QPushButton(text)
            b.setCheckable(True)
            b.setProperty("keycap", "true")
            b.setMinimumHeight(min_height)
            b.setCursor(Qt.CursorShape.PointingHandCursor)
            b.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Fixed)
            self.group.addButton(b, i)
            grid.addWidget(b, i // columns, i % columns)
        if value is not None:
            self.set_value(value)
        self.group.idClicked.connect(lambda i: self.changed.emit(self.values[i]))

    def set_value(self, v) -> None:
        if v in self.values:
            self.group.button(self.values.index(v)).setChecked(True)

    def value(self):
        i = self.group.checkedId()
        return self.values[i] if i >= 0 else None


class Form(QGridLayout):
    """Two-column form: dim label on the left, control on the right."""

    def __init__(self, parent: QWidget | None = None):
        super().__init__(parent)
        self.setContentsMargins(0, 0, 0, 0)
        self.setHorizontalSpacing(theme.S3)
        self.setVerticalSpacing(theme.S2 + 2)
        self.setColumnStretch(1, 1)
        self._row = 0

    def add(self, text: str | None, widget: QWidget | QHBoxLayout) -> QLabel | None:
        lab = None
        if text:
            lab = label(text, "label")
            lab.setAlignment(Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignVCenter)
            self.addWidget(lab, self._row, 0)
        if isinstance(widget, QWidget):
            self.addWidget(widget, self._row, 1)
        else:
            self.addLayout(widget, self._row, 1)
        self._row += 1
        return lab

    def full(self, widget: QWidget) -> None:
        self.addWidget(widget, self._row, 0, 1, 2)
        self._row += 1


def combo(options: Iterable[tuple[object, str]], value=None) -> QComboBox:
    c = QComboBox()
    for v, text in options:
        c.addItem(text, v)
    if value is not None:
        i = c.findData(value)
        if i >= 0:
            c.setCurrentIndex(i)
    return c


def column(*widgets, spacing: int = theme.S3, stretch: bool = True) -> QVBoxLayout:
    lay = QVBoxLayout()
    lay.setContentsMargins(0, 0, 0, 0)
    lay.setSpacing(spacing)
    for w in widgets:
        if isinstance(w, QWidget):
            lay.addWidget(w)
        else:
            lay.addLayout(w)
    if stretch:
        lay.addStretch(1)
    return lay


class Dot(QWidget):
    """A small status light."""

    def __init__(self, color: str = theme.INK_OFF):
        super().__init__()
        self.setFixedSize(10, 10)
        self._color = color

    def set_color(self, color: str) -> None:
        self._color = color
        self.update()

    def paintEvent(self, _) -> None:
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(theme.color(self._color))
        p.drawEllipse(QRectF(1, 1, 8, 8))
