"""Screen: animations for the 240×135 display and the clock."""

from __future__ import annotations

import os
import time

from PyQt6.QtCore import QRectF, QSize, Qt, QTimer, pyqtSignal
from PyQt6.QtGui import QColor, QImage, QPainter, QPen
from PyQt6.QtWidgets import (QFileDialog, QHBoxLayout, QLineEdit, QMenu, QProgressBar, QScrollArea, QSpinBox,
                             QDoubleSpinBox, QVBoxLayout, QWidget)

from ... import lcd
from ...i18n import decimal, plural, tr
from .. import icons, theme
from ..widgets import (ColorChip, ElidedLabel, Form, Segmented, SliderRow, StatusLine, Toggle, button, column, combo,
                       heading, label)
from . import Page

POSITIONS = [("top-left", "Top left"), ("top", "Top"), ("top-right", "Top right"), ("center", "Centre"),
             ("bottom-left", "Bottom left"), ("bottom", "Bottom"), ("bottom-right", "Bottom right")]


def to_qimage(img) -> QImage:
    data = img.tobytes("raw", "RGB")
    return QImage(data, img.width, img.height, img.width * 3, QImage.Format.Format_RGB888).copy()


class Timeline(QWidget):
    """The frames of the animation with their delays, like a film strip."""
    selected = pyqtSignal(int)
    delete_requested = pyqtSignal(int)

    THUMB_W, THUMB_H, GAP = 80, 45, 16

    def __init__(self):
        super().__init__()
        self.thumbs: list[QImage] = []
        self.delays: list[int] = []
        self.current = -1
        self.playing_index = -1
        self.setFixedHeight(self.THUMB_H + 28)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        self.setCursor(Qt.CursorShape.PointingHandCursor)

    def set_frames(self, thumbs: list[QImage], delays: list[int]) -> None:
        self.thumbs, self.delays = thumbs, delays
        self.current = min(self.current, len(thumbs) - 1)
        self.setMinimumWidth(len(thumbs) * (self.THUMB_W + self.GAP) + self.GAP)
        self.update()

    def sizeHint(self) -> QSize:
        return QSize(max(200, len(self.thumbs) * (self.THUMB_W + self.GAP)), self.THUMB_H + 28)

    def paintEvent(self, _) -> None:
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        mono = theme.font("mono", 10)
        for i, img in enumerate(self.thumbs):
            x = self.GAP + i * (self.THUMB_W + self.GAP)
            r = QRectF(x, 4, self.THUMB_W, self.THUMB_H)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(QColor(theme.PLATE))
            p.drawRoundedRect(r.adjusted(-2, -2, 2, 2), 5, 5)
            p.drawImage(r, img)
            if i == self.current:
                p.setPen(QPen(QColor(theme.AMBER), 2))
                p.setBrush(Qt.BrushStyle.NoBrush)
                p.drawRoundedRect(r.adjusted(-3, -3, 3, 3), 6, 6)
            elif i == self.playing_index:
                p.setPen(QPen(QColor(theme.INK_DIM), 1))
                p.setBrush(Qt.BrushStyle.NoBrush)
                p.drawRoundedRect(r.adjusted(-2, -2, 2, 2), 5, 5)
            p.setFont(mono)
            p.setPen(QColor(theme.AMBER if i == self.current else theme.INK_OFF))
            p.drawText(QRectF(x, r.bottom() + 5, self.THUMB_W / 2, 16), Qt.AlignmentFlag.AlignLeft, str(i + 1))
            p.setPen(QColor(theme.AMBER if i == self.current else theme.INK_DIM))
            p.drawText(QRectF(x + self.THUMB_W / 3, r.bottom() + 5, self.THUMB_W * 2 / 3, 16),
                       Qt.AlignmentFlag.AlignRight, f"{self.delays[i]}{tr(' ms')}")
        if not self.thumbs:
            p.setPen(QColor(theme.INK_DIM))
            p.setFont(theme.font("body"))
            p.drawText(self.rect(), Qt.AlignmentFlag.AlignCenter,
                       tr("Open a GIF, a picture or a folder of frames to see them here"))

    def index_at(self, x: float) -> int:
        i = int((x - self.GAP / 2) // (self.THUMB_W + self.GAP))
        return i if 0 <= i < len(self.thumbs) else -1

    def mousePressEvent(self, e) -> None:
        i = self.index_at(e.position().x())
        if i >= 0:
            self.current = i
            self.update()
            self.selected.emit(i)

    def keyPressEvent(self, e) -> None:
        if not self.thumbs:
            return super().keyPressEvent(e)
        if e.key() in (Qt.Key.Key_Delete, Qt.Key.Key_Backspace) and self.current >= 0:
            self.delete_requested.emit(self.current)
        elif e.key() == Qt.Key.Key_Right:
            self.current = min(len(self.thumbs) - 1, self.current + 1)
            self.selected.emit(self.current)
        elif e.key() == Qt.Key.Key_Left:
            self.current = max(0, self.current - 1)
            self.selected.emit(self.current)
        else:
            return super().keyPressEvent(e)
        self.update()


class ScreenPage(Page):
    title = "Screen"
    icon = "screen"
    stage_mode = "screen"

    def __init__(self, win):
        super().__init__(win)
        self.source: lcd.Source | None = None
        self.path = ""
        self.opts = lcd.RenderOptions()
        self.frames: list = []          # rendered (PIL image, delay)
        self.indices: list[int] = []    # source frame index of each rendered frame
        self.qframes: list[tuple[QImage, int]] = []
        self.cancel = False
        self.render_generation = 0
        self.upload_started = 0.0

        # Header row: what is loaded and how to load something else
        top = QHBoxLayout()
        top.setSpacing(theme.S2)
        b = button(tr("Open image…"), None, "image")
        b.clicked.connect(self.open_file)
        top.addWidget(b)
        b = button(tr("Open folder of frames…"), None, "folder")
        b.clicked.connect(self.open_dir)
        top.addWidget(b)
        self.history_button = button(tr("Earlier uploads"), None, "history",
                                     tr("Every animation sent to the keyboard is kept here"))
        self.history_button.clicked.connect(self.show_history)
        top.addWidget(self.history_button)
        top.addSpacing(theme.S3)
        self.info = ElidedLabel("", "dim")
        top.addWidget(self.info, 1)
        self.play_button = button("", "ghost", "pause", tr("Pause the preview"))
        self.play_button.clicked.connect(self.toggle_play)
        top.addWidget(self.play_button)
        self.root.addLayout(top)

        self.timeline = Timeline()
        self.timeline.selected.connect(self.frame_selected)
        self.timeline.delete_requested.connect(self.delete_frame)
        strip = QScrollArea()
        strip.setWidget(self.timeline)
        strip.setWidgetResizable(False)
        strip.setFixedHeight(self.timeline.height() + 14)
        strip.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAsNeeded)
        strip.setVerticalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        self.strip = strip
        self.root.addWidget(strip)

        frame_row = QHBoxLayout()
        frame_row.setSpacing(theme.S2)
        self.frame_label = label("", "label")
        frame_row.addWidget(self.frame_label)
        self.frame_delay = QSpinBox()
        self.frame_delay.setRange(lcd.MIN_DELAY, lcd.MAX_DELAY)
        self.frame_delay.setSuffix(tr(" ms"))
        self.frame_delay.setToolTip(tr("How long this frame stays on the screen"))
        self.frame_delay.editingFinished.connect(self.set_frame_delay)
        frame_row.addWidget(self.frame_delay)
        self.delete_button = button(tr("Delete frame"), "ghost", "trash")
        self.delete_button.clicked.connect(lambda: self.delete_frame(self.timeline.current))
        frame_row.addWidget(self.delete_button)
        self.restore_button = button(tr("Restore deleted frames"), "ghost")
        self.restore_button.clicked.connect(self.restore_frames)
        frame_row.addWidget(self.restore_button)
        frame_row.addStretch(1)
        self.root.addLayout(frame_row)

        # Picture
        pic = Form()
        self.fit = Segmented([("fit", tr("Fit")), ("fill", tr("Fill")), ("stretch", tr("Stretch"))], "fit")
        self.fit.setToolTip(tr("Fit keeps the whole picture, Fill crops it to cover the screen"))
        pic.add(tr("Scale"), self.fit)
        self.bg = ColorChip((0, 0, 0))
        pic.add(tr("Background"), self.bg)
        self.flip_h = Toggle(tr("Mirror"))
        self.flip_v = Toggle(tr("Upside down"))
        pic.add(None, self.flip_h)
        pic.add(None, self.flip_v)
        self.brightness = SliderRow(20, 200, 100, "%")
        self.contrast = SliderRow(20, 200, 100, "%")
        self.saturation = SliderRow(0, 200, 100, "%")
        pic.add(tr("Brightness"), self.brightness)
        pic.add(tr("Contrast"), self.contrast)
        pic.add(tr("Saturation"), self.saturation)

        # Timing
        timing = Form()
        self.speed = QDoubleSpinBox()
        self.speed.setRange(0.1, 10)
        self.speed.setSingleStep(0.25)
        self.speed.setValue(1.0)
        self.speed.setSuffix(" ×")
        timing.add(tr("Speed"), self.speed)
        self.delay = QSpinBox()
        self.delay.setRange(0, lcd.MAX_DELAY)
        self.delay.setSpecialValueText(tr("as in the file"))
        self.delay.setSuffix(tr(" ms"))
        timing.add(tr("Every frame"), self.delay)
        self.trim_start = QSpinBox()
        self.trim_end = QSpinBox()
        for w in (self.trim_start, self.trim_end):
            w.setRange(0, 9999)
        timing.add(tr("Cut frames from the start"), self.trim_start)
        timing.add(tr("Cut frames from the end"), self.trim_end)
        self.step = QSpinBox()
        self.step.setRange(1, 20)
        self.step.setToolTip(tr("Keeps a long GIF within the screen's 140 frames"))
        timing.add(tr("Keep every Nth"), self.step)

        # Text
        text = Form()
        self.text = QLineEdit()
        self.text.setPlaceholderText(tr("Optional caption"))
        text.add(tr("Caption"), self.text)
        trow = QHBoxLayout()
        self.text_color = ColorChip((255, 255, 255))
        self.text_size = QSpinBox()
        self.text_size.setRange(8, 72)
        self.text_size.setValue(18)
        self.text_size.setSuffix(" px")
        trow.addWidget(self.text_color)
        trow.addWidget(self.text_size)
        trow.addStretch()
        text.add(tr("Colour, size"), trow)
        self.text_pos = combo([(k, tr(v)) for k, v in POSITIONS], "bottom-right")
        text.add(tr("Position"), self.text_pos)

        # Upload
        up = QVBoxLayout()
        up.setContentsMargins(0, 0, 0, 0)
        up.setSpacing(theme.S2 + 2)
        self.dither = Toggle(tr("Smooth gradients"), True)
        self.dither.setToolTip(tr("Dithers the colours to the panel's 65 thousand, so gradients show no bands"))
        up.addWidget(self.dither)
        slot_row = QHBoxLayout()
        slot_row.addWidget(label(tr("Screen slot"), "label"))
        self.slot = QSpinBox()
        self.slot.setRange(1, 8)
        self.slot.setValue(self.cfg.settings.lcd_slot)
        self.slot.setToolTip(tr("The vendor app always uses slot 1"))
        slot_row.addWidget(self.slot)
        slot_row.addStretch()
        up.addLayout(slot_row)
        self.upload_button = button(tr("Upload to the screen"), "primary", "upload")
        self.upload_button.setMinimumWidth(self.upload_button.sizeHint().width())
        self.upload_button.clicked.connect(self.upload)
        up.addWidget(self.upload_button)
        self.progress = QProgressBar()
        self.progress.setRange(0, 1000)
        self.progress.hide()
        up.addWidget(self.progress)
        self.cancel_button = button(tr("Stop uploading"), "ghost")
        self.cancel_button.clicked.connect(lambda: setattr(self, "cancel", True))
        self.cancel_button.hide()
        up.addWidget(self.cancel_button)
        self.upload_note = StatusLine()
        up.addWidget(self.upload_note)
        up.addSpacing(theme.S3)
        up.addWidget(heading(tr("Clock")))
        self.clock_auto = Toggle(tr("Keep the clock in sync"), self.cfg.settings.clock_auto)
        self.clock_auto.setToolTip(tr("The background agent sets the time when the keyboard connects, after sleep "
                                      "and every hour"))
        self.clock_auto.toggled.connect(self._clock_auto)
        up.addWidget(self.clock_auto)
        b = button(tr("Sync clock now"), None, "clock")
        b.clicked.connect(self.win.sync_clock)
        up.addWidget(b)
        up.addStretch(1)

        up_holder = QWidget()
        up_holder.setLayout(up)
        up_holder.setMinimumWidth(220)
        timing_caption = column(heading(tr("Timing")), timing, spacing=theme.S3, stretch=False)
        timing_caption.addSpacing(theme.S3)
        timing_caption.addWidget(heading(tr("Caption")))
        timing_caption.addLayout(text)
        timing_caption.addStretch(1)
        self.root.addLayout(self.columns(column(heading(tr("Picture")), pic), timing_caption,
                                         column(up_holder, stretch=False), stretches=[5, 5, 3]))
        self.root.addStretch(1)

        self.rerender = QTimer(self, singleShot=True, interval=220)
        self.rerender.timeout.connect(self.render)
        self.fit.changed.connect(lambda _: self.rerender.start())
        self.text_pos.currentIndexChanged.connect(lambda _: self.rerender.start())
        for w in (self.flip_h, self.flip_v, self.dither):
            w.toggled.connect(lambda _: self.rerender.start())
        for w in (self.brightness, self.contrast, self.saturation):
            w.changed.connect(lambda _: self.rerender.start())
        for w in (self.speed, self.delay, self.trim_start, self.trim_end, self.step, self.text_size):
            w.valueChanged.connect(lambda _: self.rerender.start())
        self.text.textChanged.connect(lambda _: self.rerender.start())
        for w in (self.bg, self.text_color):
            w.changed.connect(lambda _: self.rerender.start())
        self._frame_tools(False)
        self.stage.screen_clicked.connect(self.toggle_play)
        last = self.cfg.settings.last_image
        if last and os.path.exists(last):
            QTimer.singleShot(150, lambda: self.load(last))

    # ------------------------------------------------------------ loading

    def on_enter(self) -> None:
        self.stage.set_colors({})
        self.stage.set_frames(self.qframes)

    def open_file(self) -> None:
        start = os.path.dirname(self.path) if self.path else os.path.expanduser("~")
        path, _ = QFileDialog.getOpenFileName(self, tr("Open image"), start,
                                              tr("Images") + " (*.gif *.png *.jpg *.jpeg *.bmp *.webp *.apng)")
        if path:
            self.load(path)

    def open_dir(self) -> None:
        path = QFileDialog.getExistingDirectory(self, tr("Folder of frames (0.png, 1.png, …)"), os.path.expanduser("~"))
        if path:
            self.load(path)

    def show_history(self) -> None:
        entries = lcd.history_entries()
        menu = QMenu(self)
        if not entries:
            a = menu.addAction(tr("Nothing uploaded yet"))
            a.setEnabled(False)
        for gif in entries[:30]:
            name = os.path.basename(os.path.dirname(gif))
            stamp, _, base = name.partition("_")
            when, _, rest = base.partition("_")
            a = menu.addAction(icons.icon("image"), f"{rest or 'animation'} — {stamp} {when.replace('-', ':')}")
            a.triggered.connect(lambda _=False, g=gif: self.load(g))
        menu.addSeparator()
        a = menu.addAction(icons.icon("folder"), tr("Open the folder"))
        a.triggered.connect(lambda: self._open_folder(lcd.HISTORY_DIR))
        menu.exec(self.history_button.mapToGlobal(self.history_button.rect().bottomLeft()))

    @staticmethod
    def _open_folder(path: str) -> None:
        import subprocess
        os.makedirs(path, exist_ok=True)
        subprocess.Popen(["xdg-open", path])

    def load(self, path: str) -> None:
        self.info.setText(tr("Opening {name}…", name=os.path.basename(path.rstrip("/"))))

        def done(src):
            self.source = src
            self.path = path
            self.opts.deleted.clear()
            self.opts.delays.clear()
            self.cfg.settings.last_image = path
            self.win.save()
            self.render()

        self.win.run(lambda: lcd.Source.load(path), done, device=False,
                     failed=lambda m: self.info.setText(tr("Could not open it: {error}", error=m)))

    # ------------------------------------------------------------ rendering

    def options(self) -> lcd.RenderOptions:
        o = self.opts
        o.fit = self.fit.value()
        o.background = self.bg.rgb
        o.flip_h, o.flip_v = self.flip_h.isChecked(), self.flip_v.isChecked()
        o.brightness, o.contrast = self.brightness.value() / 100, self.contrast.value() / 100
        o.saturation = self.saturation.value() / 100
        o.speed = self.speed.value()
        o.fixed_delay = self.delay.value() or None
        o.trim_start, o.trim_end, o.frame_step = self.trim_start.value(), self.trim_end.value(), self.step.value()
        o.text, o.text_color = self.text.text(), self.text_color.rgb
        o.text_size, o.text_pos = self.text_size.value(), self.text_pos.currentData()
        o.dither = self.dither.isChecked()
        return o

    def render(self) -> None:
        if not self.source:
            return
        o = self.options()
        src = self.source
        self.render_generation += 1
        gen = self.render_generation

        def work():
            picked = src.select(o)
            frames = [(src.render_frame(i, o), d) for i, d in picked]
            previews = [to_qimage(lcd.preview_565(f, o.dither)) for f, _ in frames]
            return picked, frames, previews, src.available(o)

        def done(result):
            if gen != self.render_generation:
                return
            picked, frames, previews, available = result
            self.indices = [i for i, _ in picked]
            self.frames = frames
            self.qframes = [(img, d) for img, (_, d) in zip(previews, frames)]
            thumbs = [img.scaled(Timeline.THUMB_W, Timeline.THUMB_H, Qt.AspectRatioMode.IgnoreAspectRatio,
                                 Qt.TransformationMode.SmoothTransformation) for img in previews]
            self.timeline.set_frames(thumbs, [d for _, d in frames])
            if self.win.page() is self:
                self.stage.set_frames(self.qframes)
            n = len(frames)
            loop = sum(d for _, d in frames) / 1000
            pages = lcd.page_count(n)
            text = (f"{os.path.basename(self.path.rstrip('/'))} · {plural(n, '{n} frame', '{n} frames')} · "
                    + tr("loop {s} s", s=decimal(loop)) + " · "
                    + tr("upload ≈ {s} s", s=round(pages * lcd.SECONDS_PER_PAGE)))
            self.info.setText(text)
            over = available - n
            self.upload_note.setText(tr("{n} more frames do not fit (the screen holds 140). Raise “Keep every "
                                        "Nth” or cut frames.", n=over) if over > 0 else "")
            self._frame_tools(self.timeline.current >= 0)

        self.win.run(work, done, device=False)

    # ------------------------------------------------------------ frames

    def _frame_tools(self, on: bool) -> None:
        for w in (self.frame_label, self.frame_delay, self.delete_button):
            w.setVisible(on)
        self.restore_button.setVisible(bool(self.opts.deleted))

    def frame_selected(self, i: int) -> None:
        if not (0 <= i < len(self.frames)):
            return
        self.stage.set_playing(False)
        self.stage.set_frame(i)
        self.play_button.setIcon(icons.icon("play", theme.INK, None))
        self.frame_label.setText(tr("Frame {n}", n=i + 1))
        self.frame_delay.blockSignals(True)
        self.frame_delay.setValue(self.frames[i][1])
        self.frame_delay.blockSignals(False)
        self._frame_tools(True)

    def set_frame_delay(self) -> None:
        i = self.timeline.current
        if 0 <= i < len(self.indices):
            self.opts.delays[self.indices[i]] = self.frame_delay.value()
            self.render()

    def delete_frame(self, i: int) -> None:
        if 0 <= i < len(self.indices) and len(self.indices) > 1:
            self.opts.deleted.add(self.indices[i])
            self.timeline.current = min(i, len(self.indices) - 2)
            self.render()

    def restore_frames(self) -> None:
        self.opts.deleted.clear()
        self.render()

    def toggle_play(self) -> None:
        playing = not self.stage.playing
        self.stage.set_playing(playing)
        self.play_button.setIcon(icons.icon("pause" if playing else "play", theme.INK, None))
        self.play_button.setToolTip(tr("Pause the preview") if playing else tr("Play the preview"))

    # ------------------------------------------------------------ upload and clock

    def _clock_auto(self, on: bool) -> None:
        self.cfg.settings.clock_auto = on
        self.win.save()

    def upload(self) -> None:
        if not self.frames:
            self.win.notify(tr("Open a picture first"))
            return
        frames, dither, slot, path = list(self.frames), self.dither.isChecked(), self.slot.value(), self.path
        self.cfg.settings.lcd_slot = slot
        self.win.save()
        self.cancel = False
        self.upload_button.setEnabled(False)
        self.progress.setValue(0)
        self.progress.setFormat(tr("Preparing…"))
        self.progress.show()
        self.cancel_button.show()
        self.upload_started = time.monotonic()

        def work(progress):
            lcd.save_history(frames, path)
            blob = lcd.encode_animation(frames, dither)
            return self.win.wired(lambda kb: kb.upload_lcd(blob, slot, progress=progress,
                                                           cancel=lambda: self.cancel))()

        def on_progress(p: float) -> None:
            self.progress.setValue(int(p * 1000))
            el = time.monotonic() - self.upload_started
            left = el / p - el if p > 0 else 0
            self.progress.setFormat(tr("{p}% · {s} s left", p=int(p * 100), s=round(left)))

        def finish(ok=None):
            self.progress.hide()
            self.cancel_button.hide()
            self.upload_button.setEnabled(True)
            if ok:
                self.win.notify(tr("Animation uploaded"))
            elif ok is False:
                self.win.notify(tr("Upload stopped; the screen shows what arrived so far"))

        task = self.win.run(work, finish, tr("Uploading to the screen…"), progress=on_progress)
        task.signals.failed.connect(lambda _: finish())

    def on_close(self) -> None:
        self.cancel = True
