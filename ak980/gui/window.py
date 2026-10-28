"""Main window: navigation, header, the keyboard stage and the pages."""

from __future__ import annotations

import os
import time
import traceback
from typing import Callable

from PyQt6.QtCore import QObject, QRectF, QRunnable, Qt, QThreadPool, QTimer, pyqtSignal
from PyQt6.QtGui import QAction, QColor, QFont, QPainter
from PyQt6.QtWidgets import (QAbstractButton, QApplication, QComboBox, QHBoxLayout, QMainWindow, QMenu,
                             QMessageBox, QScrollArea, QStackedWidget, QSystemTrayIcon, QVBoxLayout, QWidget)

from .. import APP_NAME, APP_TITLE, permissions, runtime
from ..config import Config
from ..device import Dongle, NotConnected, WiredKeyboard, connection_state
from ..engine import LightingEngine
from ..i18n import tr
from ..layout import positions
from . import icons, theme
from .stage import KeyboardStage
from .widgets import Dot, button, label


class _Signals(QObject):
    done = pyqtSignal(object)
    failed = pyqtSignal(str)
    progress = pyqtSignal(float)


class Task(QRunnable):
    def __init__(self, fn: Callable, with_progress: bool = False):
        super().__init__()
        self.fn = fn
        self.with_progress = with_progress
        self.signals = _Signals()
        self.setAutoDelete(False)

    def run(self) -> None:
        try:
            result = self.fn(self.signals.progress.emit) if self.with_progress else self.fn()
        except (NotConnected, OSError, ValueError, PermissionError) as e:
            self.signals.failed.emit(str(e))
        except Exception:  # noqa: BLE001 - show any failure instead of losing the window
            self.signals.failed.emit(traceback.format_exc(limit=4))
        else:
            self.signals.done.emit(result)


class NavButton(QAbstractButton):
    def __init__(self, icon_name: str, text: str):
        super().__init__()
        self.icon_name = icon_name
        self.setText(text)
        self.setCheckable(True)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        self.setFixedSize(80, 62)
        self.setToolTip(text)

    def paintEvent(self, _) -> None:
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        on = self.isChecked()
        hover = self.underMouse()
        r = QRectF(self.rect()).adjusted(8, 3, -8, -3)
        if on or hover:
            # A pressed keycap under the active section
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(QColor(theme.KEYCAP if on else theme.KEYCAP_LO))
            p.drawRoundedRect(r, 8, 8)
            if on:
                p.setBrush(QColor("#1D1F24"))
                p.drawRoundedRect(QRectF(r.left(), r.bottom() - 3, r.width(), 3), 1.5, 1.5)
        color = theme.AMBER if on else (theme.INK if hover else theme.INK_DIM)
        pm = icons.pixmap(self.icon_name, color, 22)
        p.drawPixmap(int(r.center().x() - 11), int(r.top() + 8), pm)
        p.setFont(theme.font("small", 11, QFont.Weight.Medium))
        p.setPen(QColor(color))
        p.drawText(QRectF(r.left() - 6, r.top() + 33, r.width() + 12, 16), Qt.AlignmentFlag.AlignCenter,
                   p.fontMetrics().elidedText(self.text(), Qt.TextElideMode.ElideRight, int(r.width() + 10)))
        if self.hasFocus():
            p.setPen(theme.color(theme.AMBER))
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.drawRoundedRect(r.adjusted(-1, -1, 1, 1), 9, 9)

    def enterEvent(self, _) -> None:
        self.update()

    def leaveEvent(self, _) -> None:
        self.update()


class AppMark(QWidget):
    def __init__(self):
        super().__init__()
        self.setFixedSize(80, 64)

    def paintEvent(self, _) -> None:
        p = QPainter(self)
        p.drawPixmap(20, 14, icons.app_icon().pixmap(40, 40))


class MainWindow(QMainWindow):
    frame_ready = pyqtSignal(object)

    def __init__(self):
        super().__init__()
        self.cfg = Config.load()
        self.setWindowTitle(APP_TITLE)
        self.setWindowIcon(icons.app_icon())
        self.resize(1280, 860)
        self.setMinimumSize(1080, 720)
        self.device_pool = QThreadPool(self)
        self.device_pool.setMaxThreadCount(1)     # one keyboard operation at a time
        self.work_pool = QThreadPool(self)
        self.tasks: set[Task] = set()
        self.busy_count = 0
        self.battery: int | None = None
        self.battery_at = 0.0
        self.connection: str | None = "unknown"
        self.last_clock_sync: float | None = None
        self.leds, _ = positions()
        self._quitting = False
        # For screenshots and demos: render everything, send nothing to the keyboard
        self.preview_only = bool(os.environ.get("AK980_PREVIEW_ONLY"))

        central = QWidget()
        central.setObjectName("desk")
        self.setCentralWidget(central)
        outer = QHBoxLayout(central)
        outer.setContentsMargins(0, 0, 0, 0)
        outer.setSpacing(0)

        rail = QWidget()
        rail.setFixedWidth(88)
        rail_lay = QVBoxLayout(rail)
        rail_lay.setContentsMargins(4, 6, 4, 12)
        rail_lay.setSpacing(2)
        rail_lay.addWidget(AppMark(), 0, Qt.AlignmentFlag.AlignHCenter)
        rail_lay.addSpacing(6)
        self.nav: list[NavButton] = []
        self.rail_lay = rail_lay
        outer.addWidget(rail)

        right = QVBoxLayout()
        right.setContentsMargins(0, 0, 0, 0)
        right.setSpacing(0)
        outer.addLayout(right, 1)
        right.addWidget(self._build_header())

        self.banner = QWidget()
        bl = QHBoxLayout(self.banner)
        bl.setContentsMargins(theme.S4, theme.S2, theme.S4, theme.S2)
        self.banner_text = label("", "warn")
        bl.addWidget(self.banner_text, 1)
        self.banner_button = button(tr("Allow access"), "primary", "shield")
        self.banner_button.clicked.connect(self.fix_permissions)
        bl.addWidget(self.banner_button)
        self.banner.setStyleSheet("background: #2A2416;")
        self.banner.hide()
        right.addWidget(self.banner)

        self.stage = KeyboardStage()
        self.stage.animations = self.cfg.settings.animations
        self.stage.key_clicked.connect(self._stage_key)
        right.addWidget(self.stage, 5)

        self.stack = QStackedWidget()
        self.stack.setObjectName("panel")
        right.addWidget(self.stack, 6)
        self.right = right

        self.pages = []
        self._build_pages()

        self.frame_ready.connect(self._show_frame)
        self.engine = LightingEngine(drive_device=self._engine_should_drive(),
                                     on_frame=lambda c: self.frame_ready.emit(c))
        self.engine.preview_only_profile = self.cfg.profile
        self.engine.start()
        self._engine_drives = self.engine.drive_device

        self.poll = QTimer(self, interval=2000)
        self.poll.timeout.connect(self.update_connection)
        self.poll.start()
        self.toast_timer = QTimer(self, singleShot=True, interval=4500)
        self.toast_timer.timeout.connect(lambda: self.toast.setText(""))
        self.tray: QSystemTrayIcon | None = None
        self._setup_tray()
        self.select_page(0)
        self.update_connection()
        QTimer.singleShot(0, self.nav[0].setFocus)

    # ------------------------------------------------------------ building

    def _build_header(self) -> QWidget:
        head = QWidget()
        head.setFixedHeight(60)
        lay = QHBoxLayout(head)
        lay.setContentsMargins(theme.S4, theme.S2, theme.S4, 0)
        lay.setSpacing(theme.S3)
        title = label("AK980 V2", "title")
        lay.addWidget(title)
        lay.addSpacing(theme.S2)
        self.profile_box = QComboBox()
        self.profile_box.setMinimumWidth(170)
        self.profile_box.setToolTip(tr("Active profile"))
        self.profile_box.activated.connect(self._profile_chosen)
        lay.addWidget(self.profile_box)
        self.toast = label("", "dim")
        lay.addWidget(self.toast, 1, Qt.AlignmentFlag.AlignCenter)
        self.busy = label("", "dim")
        lay.addWidget(self.busy)
        self.conn_dot = Dot()
        lay.addWidget(self.conn_dot)
        self.conn_label = label("")
        lay.addWidget(self.conn_label)
        lay.addSpacing(theme.S2)
        self.clock_label = label("", "dim")
        lay.addWidget(self.clock_label)
        self.sync_button = button(tr("Sync clock"), "ghost", "clock", tr("Set the screen clock to the system time"))
        self.sync_button.clicked.connect(self.sync_clock)
        lay.addWidget(self.sync_button)
        self.refresh_profiles()
        return head

    def _build_pages(self) -> None:
        from .pages.device import DevicePage
        from .pages.keys import KeysPage
        from .pages.lighting import LightingPage
        from .pages.macros import MacrosPage
        from .pages.perkey import PerKeyPage
        from .pages.profiles import ProfilesPage
        from .pages.screen import ScreenPage
        for cls in (ScreenPage, LightingPage, PerKeyPage, KeysPage, MacrosPage, ProfilesPage, DevicePage):
            page = cls(self)
            self.pages.append(page)
            scroll = QScrollArea()
            scroll.setWidgetResizable(True)
            scroll.setFrameShape(QScrollArea.Shape.NoFrame)
            scroll.setWidget(page)
            self.stack.addWidget(scroll)
            nb = NavButton(cls.icon, tr(cls.title))
            nb.clicked.connect(lambda _=False, i=len(self.nav): self.select_page(i))
            self.nav.append(nb)
            self.rail_lay.addWidget(nb, 0, Qt.AlignmentFlag.AlignHCenter)
        self.rail_lay.addStretch(1)

    def _setup_tray(self) -> None:
        if not QSystemTrayIcon.isSystemTrayAvailable():
            return
        self.tray = QSystemTrayIcon(icons.app_icon(), self)
        menu = QMenu()
        a = QAction(tr("Show window"), self)
        a.triggered.connect(self.show_window)
        menu.addAction(a)
        a = QAction(tr("Sync clock"), self)
        a.triggered.connect(self.sync_clock)
        menu.addAction(a)
        self.tray_profiles = menu.addMenu(tr("Profile"))
        self.tray_profiles.aboutToShow.connect(self._fill_tray_profiles)
        menu.addSeparator()
        a = QAction(tr("Quit"), self)
        a.triggered.connect(self.quit)
        menu.addAction(a)
        self.tray.setContextMenu(menu)
        self.tray.activated.connect(
            lambda r: self.show_window() if r == QSystemTrayIcon.ActivationReason.Trigger else None)
        self.tray.setToolTip(APP_NAME)
        self.tray.show()

    def _fill_tray_profiles(self) -> None:
        self.tray_profiles.clear()
        for name in self.cfg.names():
            a = self.tray_profiles.addAction(name)
            a.setCheckable(True)
            a.setChecked(name == self.cfg.active)
            a.triggered.connect(lambda _=False, n=name: self.switch_profile(n))

    # ------------------------------------------------------------ pages and stage

    def page(self):
        return self.pages[self.stack.currentIndex()]

    def select_page(self, index: int) -> None:
        old = self.pages[self.stack.currentIndex()] if self.stack.count() else None
        if old is not None and old is not self.pages[index]:
            old.on_leave()
        self.stack.setCurrentIndex(index)
        for i, b in enumerate(self.nav):
            b.setChecked(i == index)
        page = self.pages[index]
        mode = page.stage_mode
        self.stage.setVisible(mode is not None)
        # The screen page needs more room for its controls than the keyboard pages
        self.right.setStretchFactor(self.stage, 4 if mode == "screen" else 5)
        self.right.setStretchFactor(self.stack, 7 if mode == "screen" else 6)
        self.stage.paint_mode = False
        self.stage.selected = set()
        self.stage.captions = {}
        self.stage.focus_screen(mode == "screen")
        page.on_enter()
        self.stage.update()

    def _stage_key(self, key, mouse_button) -> None:
        self.page().on_stage_key(key, mouse_button)

    def _show_frame(self, colors) -> None:
        page = self.page()
        if not page.wants_live_colors():
            return
        self.stage.set_colors({idx: QColor(int(r * 255), int(g * 255), int(b * 255))
                               for idx, (r, g, b) in zip(self.leds, colors)})

    # ------------------------------------------------------------ tasks

    def run(self, fn: Callable, done: Callable | None = None, busy: str = "", device: bool = True,
            progress: Callable[[float], None] | None = None, failed: Callable[[str], None] | None = None) -> Task:
        task = Task(fn, with_progress=progress is not None)
        self.tasks.add(task)
        if busy:
            self.busy_count += 1
            self.busy.setText(busy)

        def finish(*_):
            self.tasks.discard(task)
            if busy:
                self.busy_count -= 1
                if self.busy_count <= 0:
                    self.busy_count = 0
                    self.busy.setText("")

        if progress is not None:
            task.signals.progress.connect(progress)
        if done:
            task.signals.done.connect(done)
        task.signals.done.connect(finish)
        task.signals.failed.connect(finish)
        task.signals.failed.connect(failed or self.error)
        (self.device_pool if device else self.work_pool).start(task)
        return task

    def _engine_should_drive(self) -> bool:
        return not self.preview_only and not runtime.agent_running()

    def wired(self, fn: Callable[[WiredKeyboard], object]) -> Callable[[], object]:
        if self.preview_only:
            return lambda: None

        def work():
            with WiredKeyboard.open() as kb:
                return fn(kb)
        return work

    def error(self, message: str) -> None:
        QMessageBox.warning(self, APP_NAME, message)

    def notify(self, text: str) -> None:
        self.toast.setText(text)
        self.toast_timer.start()

    def save(self) -> None:
        self.cfg.save()

    # ------------------------------------------------------------ profiles

    def refresh_profiles(self) -> None:
        self.profile_box.blockSignals(True)
        self.profile_box.clear()
        for name in self.cfg.names():
            self.profile_box.addItem(name)
        self.profile_box.setCurrentText(self.cfg.active)
        self.profile_box.blockSignals(False)

    def _profile_chosen(self, index: int) -> None:
        self.switch_profile(self.profile_box.itemText(index))

    def switch_profile(self, name: str, apply: bool = True) -> None:
        if self.cfg.get(name) is None:
            return
        self.cfg.active = name
        self.save()
        self.engine.preview_only_profile = self.cfg.profile
        self.refresh_profiles()
        for page in self.pages:
            page.refresh()
        self.page().on_enter()
        if apply and connection_state() == "usb":
            from ..profiles import apply_profile
            profile = self.cfg.profile
            self.run(self.wired(lambda kb: apply_profile(kb, profile)),
                     lambda _: self.notify(tr("Profile “{name}” is on the keyboard", name=name)),
                     tr("Applying the profile…"))

    # ------------------------------------------------------------ device state

    def update_connection(self) -> None:
        state = connection_state()
        if state != self.connection:
            self.connection = state
            for page in self.pages:
                page.connection_changed(state)
        if state == "usb":
            self.conn_dot.set_color(theme.OK)
            self.conn_label.setText(tr("USB cable"))
            self.conn_label.setToolTip(tr("Every setting is available"))
        elif state == "2.4g":
            self.conn_dot.set_color(theme.WARN)
            text = tr("2.4 GHz")
            if self.battery is not None:
                text += f" · {self.battery}%"
            self.conn_label.setText(text)
            self.conn_label.setToolTip(tr("Through the dongle the keyboard only takes the clock and reports its "
                                          "battery. Connect the cable for everything else."))
            if time.monotonic() - self.battery_at > 30:
                self.battery_at = time.monotonic()
                self.run(self._read_battery, self._battery_read, failed=lambda _: None)
        else:
            self.conn_dot.set_color(theme.INK_OFF)
            self.conn_label.setText(tr("Not connected"))
            self.conn_label.setToolTip(tr("Connect the AK980 with the USB cable or its 2.4 GHz dongle"))
        access = permissions.accessible()
        if access is False:
            self.banner_text.setText(tr("This user may not use the keyboard yet. Allow access once "
                                        "(asks for your password)."))
            self.banner.show()
        else:
            self.banner.hide()
        drive = self._engine_should_drive()
        if drive != self._engine_drives:
            self._engine_drives = drive
            self.engine.stop()
            self.engine.drive_device = drive
            self.engine.start()
        if self.last_clock_sync:
            self.clock_label.setText(tr("Clock synced {time}", time=time.strftime(
                "%H:%M", time.localtime(self.last_clock_sync))))
        if self.tray:
            self.tray.setToolTip(f"{APP_NAME} — {self.conn_label.text()}")

    @staticmethod
    def _read_battery():
        info = Dongle.find()
        if not info:
            return None
        with Dongle(info) as d:
            return d.read_battery()

    def _battery_read(self, level) -> None:
        self.battery = level
        self.update_connection()

    def sync_clock(self) -> None:
        slot = self.cfg.settings.clock_slot

        def work():
            from ..agent import sync_clock_once
            via = sync_clock_once(slot)
            if not via:
                raise NotConnected(tr("The keyboard was not found or did not answer. In 2.4 GHz mode it has to "
                                      "be awake."))
            return via

        def done(via):
            self.last_clock_sync = time.time()
            self.notify(tr("Clock synced over {link}", link=via))
            self.update_connection()

        self.run(work, done, tr("Syncing the clock…"))

    def fix_permissions(self) -> None:
        self.run(permissions.install_rule, lambda _: (self.notify(tr("Access allowed")), self.update_connection()),
                 tr("Waiting for authorisation…"), device=False)

    # ------------------------------------------------------------ window

    def show_window(self) -> None:
        self.showNormal()
        self.raise_()
        self.activateWindow()

    def quit(self) -> None:
        self._quitting = True
        self.close()

    def closeEvent(self, e) -> None:
        if self.cfg.settings.close_to_tray and self.tray and not self._quitting:
            e.ignore()
            self.hide()
            return
        for page in self.pages:
            page.on_close()
        self.engine.stop()
        self.device_pool.waitForDone(15000)
        if self.tray:
            self.tray.hide()
        e.accept()
        QApplication.quit()
