"""The keyboard itself, drawn from its real layout: the stage every page works on.

Keycaps carry two-tier legends; LED colours glow around the caps and through the legends like
shine-through keycaps; the 1.14" screen plays the selected animation. On the Screen page the
view moves in until the screen fills the stage.
"""

from __future__ import annotations

import math

from PyQt6.QtCore import QEasingCurve, QPointF, QRectF, QSize, Qt, QTimer, QVariantAnimation, pyqtSignal
from PyQt6.QtGui import (QColor, QFont, QImage, QLinearGradient, QPainter, QPainterPath, QPen, QRadialGradient,
                         QTransform)
from PyQt6.QtWidgets import QSizePolicy, QToolTip, QWidget

from ..layout import CASE_RECT, KNOB_CENTER, KNOB_RADIUS, SCREEN_RECT, LayoutKey, keys
from . import theme

LCD_ASPECT = 240 / 135


def _mix(a: QColor, b: QColor, t: float) -> QColor:
    return QColor(int(a.red() + (b.red() - a.red()) * t), int(a.green() + (b.green() - a.green()) * t),
                  int(a.blue() + (b.blue() - a.blue()) * t))


def _legend(label: str) -> tuple[str, str]:
    """("!1") -> ("!", "1"): shifted symbol on top, base below, like the real keycaps."""
    if len(label) == 2 and not label.isalnum() and label not in ("Fn",):
        return label[0], label[1]
    return "", label


class KeyboardStage(QWidget):
    key_clicked = pyqtSignal(object, object)     # LayoutKey, Qt.MouseButton
    screen_clicked = pyqtSignal()

    def __init__(self):
        super().__init__()
        self.keys: tuple[LayoutKey, ...] = keys()
        self.colors: dict[int, QColor] = {}
        self.brightness = 1.0
        self.captions: dict[int, str] = {}
        self.selected: set[int] = set()
        self.hover: int | None = None
        self.paint_mode = False
        self.interactive = True
        self.animations = True
        self.zoom = 0.0
        self.frames: list[tuple[QImage, int]] = []
        self.frame_index = 0
        self.playing = True
        self._last_painted: int | None = None
        self._anim = QVariantAnimation(self)
        self._anim.setDuration(420)
        self._anim.setEasingCurve(QEasingCurve.Type.OutCubic)
        self._anim.valueChanged.connect(self._set_zoom)
        self._timer = QTimer(self)
        self._timer.setSingleShot(True)
        self._timer.timeout.connect(self._next_frame)
        self.setMouseTracking(True)
        self.setMinimumHeight(220)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)
        self.setFocusPolicy(Qt.FocusPolicy.NoFocus)

    def sizeHint(self) -> QSize:
        return QSize(1000, 330)

    # ------------------------------------------------------------ state

    def set_colors(self, colors: dict[int, QColor] | None) -> None:
        self.colors = colors or {}
        self.update()

    def set_frames(self, frames: list[tuple[QImage, int]]) -> None:
        self.frames = frames
        self.frame_index = 0
        self._timer.stop()
        if frames and self.playing:
            self._timer.start(max(20, frames[0][1]))
        self.update()

    def set_frame(self, index: int) -> None:
        if self.frames:
            self.frame_index = index % len(self.frames)
            self.update()

    def set_playing(self, playing: bool) -> None:
        self.playing = playing
        if playing and self.frames:
            self._timer.start(max(20, self.frames[self.frame_index][1]))
        else:
            self._timer.stop()

    def _next_frame(self) -> None:
        if not self.frames or not self.playing:
            return
        self.frame_index = (self.frame_index + 1) % len(self.frames)
        self.update()
        self._timer.start(max(20, self.frames[self.frame_index][1]))

    def focus_screen(self, on: bool) -> None:
        target = 1.0 if on else 0.0
        if not self.animations or not self.isVisible():
            self._anim.stop()
            self._set_zoom(target)
            return
        self._anim.stop()
        self._anim.setStartValue(self.zoom)
        self._anim.setEndValue(target)
        self._anim.start()

    def _set_zoom(self, z) -> None:
        self.zoom = float(z)
        self.update()

    # ------------------------------------------------------------ geometry

    def _fit(self, rect: QRectF, pad: float) -> tuple[float, float, float]:
        w, h = self.width() - 2 * pad, self.height() - 2 * pad
        s = min(w / rect.width(), h / rect.height())
        ox = pad + (w - rect.width() * s) / 2 - rect.left() * s
        oy = pad + (h - rect.height() * s) / 2 - rect.top() * s
        return s, ox, oy

    def _transform(self) -> QTransform:
        board = QRectF(*CASE_RECT).adjusted(-6, -6, 6, 10)
        sx, sy, sw, sh = SCREEN_RECT
        screen = self._panel_rect().adjusted(-1.5, -1.5, 1.5, 1.5)
        s0, x0, y0 = self._fit(board, 18)
        s1, x1, y1 = self._fit(screen, 10)
        z = self.zoom * self.zoom * (3 - 2 * self.zoom)
        # Interpolate the scale geometrically so the move feels even
        s = s0 * (s1 / s0) ** z
        # Keep the screen centre on a straight path between both views
        cx0, cy0 = x0 + (sx + sw / 2) * s0, y0 + (sy + sh / 2) * s0
        cx1, cy1 = x1 + (sx + sw / 2) * s1, y1 + (sy + sh / 2) * s1
        cx, cy = cx0 + (cx1 - cx0) * z, cy0 + (cy1 - cy0) * z
        t = QTransform()
        t.translate(cx - (sx + sw / 2) * s, cy - (sy + sh / 2) * s)
        t.scale(s, s)
        return t

    @staticmethod
    def _cap_rect(k: LayoutKey) -> QRectF:
        return QRectF(k.x, k.y, k.w, k.h).adjusted(1.4, 1.4, -1.4, -1.4)

    def key_at(self, pos: QPointF) -> LayoutKey | None:
        inv, ok = self._transform().inverted()
        if not ok:
            return None
        p = inv.map(pos)
        for k in self.keys:
            if QRectF(k.x, k.y, k.w, k.h).contains(p):
                return k
        return None

    def _screen_contains(self, pos: QPointF) -> bool:
        inv, ok = self._transform().inverted()
        return ok and QRectF(*SCREEN_RECT).contains(inv.map(pos))

    # ------------------------------------------------------------ painting

    def paintEvent(self, _) -> None:
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        p.setRenderHint(QPainter.RenderHint.TextAntialiasing)
        self._paint_desk(p)
        t = self._transform()
        self._paint_shadow(p, t)
        p.setTransform(t)
        self._paint_case(p)
        self._paint_glow(p, t)
        p.setTransform(t)
        for k in self.keys:
            self._paint_key(p, k)
        self._paint_knob(p)
        self._paint_screen(p, t)
        if self.zoom > 0.01:
            self._paint_focus_veil(p, t)

    def _paint_desk(self, p: QPainter) -> None:
        g = QLinearGradient(0, 0, 0, self.height())
        g.setColorAt(0, QColor("#1D1E23"))
        g.setColorAt(1, QColor(theme.DESK))
        p.fillRect(self.rect(), g)

    def _paint_shadow(self, p: QPainter, t: QTransform) -> None:
        r = t.mapRect(QRectF(*CASE_RECT))
        small = QImage(max(1, self.width() // 8), max(1, self.height() // 8), QImage.Format.Format_ARGB32_Premultiplied)
        small.fill(0)
        sp = QPainter(small)
        sp.setRenderHint(QPainter.RenderHint.Antialiasing)
        sp.setPen(Qt.PenStyle.NoPen)
        sp.setBrush(QColor(0, 0, 0, 170))
        sp.drawRoundedRect(QRectF(r.left() / 8 + 1, r.top() / 8 + 2.5, r.width() / 8 - 2, r.height() / 8), 3, 3)
        sp.end()
        p.save()
        p.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform)
        p.drawImage(QRectF(0, 0, self.width(), self.height()), small)
        p.restore()

    def _paint_case(self, p: QPainter) -> None:
        r = QRectF(*CASE_RECT)
        g = QLinearGradient(0, r.top(), 0, r.bottom())
        g.setColorAt(0, QColor("#2C2F37"))
        g.setColorAt(1, QColor("#22242A"))
        p.setPen(QPen(QColor("#3B3F49"), 1.0))
        p.setBrush(g)
        p.drawRoundedRect(r, 14, 14)
        p.setPen(QPen(QColor(255, 255, 255, 18), 1.0))
        p.drawLine(QPointF(r.left() + 16, r.top() + 1), QPointF(r.right() - 16, r.top() + 1))
        plate = QRectF(18, 32, 765, 248)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(theme.PLATE))
        p.drawRoundedRect(plate, 6, 6)

    def _paint_glow(self, p: QPainter, t: QTransform) -> None:
        if not self.colors:
            return
        f = 6
        small = QImage(max(1, self.width() // f), max(1, self.height() // f), QImage.Format.Format_ARGB32_Premultiplied)
        small.fill(0)
        sp = QPainter(small)
        sp.setRenderHint(QPainter.RenderHint.Antialiasing)
        sp.setPen(Qt.PenStyle.NoPen)
        st = QTransform(t)
        st *= QTransform.fromScale(1 / f, 1 / f)
        sp.setTransform(st)
        for k in self.keys:
            c = self.colors.get(k.index)
            if c is None:
                continue
            c = QColor(c)
            c.setAlphaF(min(1.0, 0.95 * self.brightness))
            sp.setBrush(c)
            sp.drawRoundedRect(QRectF(k.x, k.y, k.w, k.h).adjusted(-2.5, -2.5, 2.5, 2.5), 6, 6)
        sp.end()
        p.save()
        p.resetTransform()
        p.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform)
        p.setClipPath(t.map(self._plate_path()))
        p.drawImage(QRectF(0, 0, small.width() * f, small.height() * f), small)
        p.restore()

    @staticmethod
    def _plate_path() -> QPainterPath:
        path = QPainterPath()
        path.addRoundedRect(QRectF(18, 32, 765, 248), 6, 6)
        return path

    def _paint_key(self, p: QPainter, k: LayoutKey) -> None:
        r = self._cap_rect(k)
        led = self.colors.get(k.index)
        hover = k.index == self.hover and self.interactive
        side = QColor("#26292F")
        top = QColor(theme.KEYCAP_HI if hover else "#30333B")
        if led is not None:
            lit = _mix(QColor("#000000"), led, self.brightness)
            side = _mix(side, lit, 0.10)
            top = _mix(top, lit, 0.06)
        p.setPen(QPen(QColor("#1A1B1F"), 0.8))
        p.setBrush(side)
        p.drawRoundedRect(r, 4.5, 4.5)
        face = r.adjusted(2.6, 1.6, -2.6, -4.2)
        g = QLinearGradient(0, face.top(), 0, face.bottom())
        g.setColorAt(0, top.lighter(108))
        g.setColorAt(1, top)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(g)
        p.drawRoundedRect(face, 3.2, 3.2)

        if led is not None:
            ink = _mix(QColor(theme.INK), _mix(QColor("#000000"), led, max(0.25, self.brightness)), 0.72)
        else:
            ink = QColor(theme.INK_DIM)
        upper, lower = _legend(k.label)
        caption = self.captions.get(k.index)
        f = QFont(theme.FONT_BODY)
        f.setWeight(QFont.Weight.Medium)
        size = 8.6 if len(lower) <= 2 else 7.2 if len(lower) <= 5 else 6.2
        f.setPixelSize(10)
        f.setPointSizeF(size * 0.75)
        p.setFont(f)
        p.setPen(ink)
        text_area = face.adjusted(2.5, 1.5, -2.5, -(7 if caption else 1.5))
        if upper:
            p.drawText(QRectF(text_area.left(), text_area.top(), text_area.width(), text_area.height() / 2),
                       Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter, upper)
            p.drawText(QRectF(text_area.left(), text_area.center().y(), text_area.width(), text_area.height() / 2),
                       Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter, lower)
        else:
            align = Qt.AlignmentFlag.AlignCenter if len(lower) <= 2 or k.w < 45 else (
                Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter)
            p.drawText(text_area, align, lower)
        if caption:
            cf = QFont(theme.FONT_BODY)
            cf.setWeight(QFont.Weight.DemiBold)
            cf.setPointSizeF(4.8)
            p.setFont(cf)
            p.setPen(QColor(theme.AMBER))
            cap_rect = QRectF(face.left() + 1, face.bottom() - 7.2, face.width() - 2, 6.6)
            p.drawText(cap_rect, Qt.AlignmentFlag.AlignCenter,
                       p.fontMetrics().elidedText(caption, Qt.TextElideMode.ElideRight, int(cap_rect.width() * 1.2)))
        if k.index in self.selected:
            p.setPen(QPen(QColor(theme.AMBER), 2.0 / max(0.1, p.transform().m11()) * 1.0))
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.drawRoundedRect(r.adjusted(-1.2, -1.2, 1.2, 1.2), 5.5, 5.5)

    def _paint_knob(self, p: QPainter) -> None:
        cx, cy = KNOB_CENTER
        r = KNOB_RADIUS
        g = QRadialGradient(QPointF(cx - r * 0.35, cy - r * 0.4), r * 1.4)
        g.setColorAt(0, QColor("#4A4E58"))
        g.setColorAt(1, QColor("#25272D"))
        p.setPen(QPen(QColor("#15161A"), 1.0))
        p.setBrush(g)
        p.drawEllipse(QPointF(cx, cy), r, r)
        p.setPen(QPen(QColor(255, 255, 255, 26), 0.6))
        for i in range(36):
            a = i * math.tau / 36
            p.drawLine(QPointF(cx + math.cos(a) * (r - 2.6), cy + math.sin(a) * (r - 2.6)),
                       QPointF(cx + math.cos(a) * (r - 0.6), cy + math.sin(a) * (r - 0.6)))
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor("#2E3138"))
        p.drawEllipse(QPointF(cx, cy), r - 4.5, r - 4.5)
        p.setBrush(QColor(theme.INK_DIM))
        p.drawEllipse(QPointF(cx, cy - r + 7.5), 1.2, 1.2)

    def _panel_rect(self) -> QRectF:
        sx, sy, sw, sh = SCREEN_RECT
        inner = QRectF(sx, sy, sw, sh).adjusted(4, 4, -4, -4)
        w = min(inner.width(), inner.height() * LCD_ASPECT)
        h = w / LCD_ASPECT
        return QRectF(inner.center().x() - w / 2, inner.center().y() - h / 2, w, h)

    def _paint_screen(self, p: QPainter, t: QTransform) -> None:
        bezel = QRectF(*SCREEN_RECT)
        p.setPen(QPen(QColor("#3A3E48"), 0.8))
        p.setBrush(QColor("#0C0D0F"))
        p.drawRoundedRect(bezel, 3, 3)
        panel = self._panel_rect()
        p.save()
        p.resetTransform()
        target = t.mapRect(panel)
        img = self.frames[self.frame_index][0] if self.frames else None
        if img is None:
            p.fillRect(target, QColor("#050506"))
        else:
            scale = target.width() / img.width()
            p.setRenderHint(QPainter.RenderHint.SmoothPixmapTransform, scale < 2.5)
            p.drawImage(target, img)
            if scale >= 2.6:
                # The panel's pixel grid, visible this close
                p.setPen(QPen(QColor(0, 0, 0, 80 if scale > 4 else 50), 1))
                for col in range(1, img.width()):
                    x = target.left() + col * scale
                    p.drawLine(QPointF(x, target.top()), QPointF(x, target.bottom()))
                for row in range(1, img.height()):
                    y = target.top() + row * scale
                    p.drawLine(QPointF(target.left(), y), QPointF(target.right(), y))
        p.restore()

    def _paint_focus_veil(self, p: QPainter, t: QTransform) -> None:
        p.save()
        p.resetTransform()
        veil = QPainterPath()
        veil.addRect(QRectF(self.rect()))
        hole = QPainterPath()
        hole.addRoundedRect(t.mapRect(QRectF(*SCREEN_RECT)), 6, 6)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(12, 13, 15, int(150 * self.zoom)))
        p.drawPath(veil.subtracted(hole))
        p.restore()

    # ------------------------------------------------------------ input

    def mousePressEvent(self, e) -> None:
        if not self.interactive:
            return
        pos = e.position()
        if self.zoom > 0.5 or self._screen_contains(pos):
            self.screen_clicked.emit()
            return
        k = self.key_at(pos)
        if k:
            self._last_painted = k.index
            self.key_clicked.emit(k, e.button())

    def mouseMoveEvent(self, e) -> None:
        if not self.interactive or self.zoom > 0.5:
            return
        k = self.key_at(e.position())
        idx = k.index if k else None
        if idx != self.hover:
            self.hover = idx
            self.update()
            if k:
                text = k.label + (f" → {self.captions[k.index]}" if k.index in self.captions else "")
                QToolTip.showText(e.globalPosition().toPoint(), text, self)
            else:
                QToolTip.hideText()
        if self.paint_mode and e.buttons() & Qt.MouseButton.LeftButton and k and k.index != self._last_painted:
            self._last_painted = k.index
            self.key_clicked.emit(k, Qt.MouseButton.LeftButton)

    def leaveEvent(self, _) -> None:
        self.hover = None
        self.update()

    def hideEvent(self, _) -> None:
        self._timer.stop()

    def showEvent(self, _) -> None:
        if self.playing and self.frames:
            self._timer.start(max(20, self.frames[self.frame_index][1]))
