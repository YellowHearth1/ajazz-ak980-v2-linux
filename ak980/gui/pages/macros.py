"""Macros: recorded or typed sequences stored in the keyboard."""

from __future__ import annotations

import time

from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtWidgets import (QAbstractItemView, QHBoxLayout, QHeaderView, QInputDialog, QListWidget, QMenu,
                             QProgressBar, QPushButton, QSpinBox, QTableWidget, QTableWidgetItem, QVBoxLayout)

from ...i18n import plural, tr
from ...keymap import LINUX_TO_HID
from ...macro import BUDGET_BYTES, MAX_DELAY, MIN_DELAY, MOUSE_BUTTON_LABELS, Macro, MacroStep, encode_macros, used_bytes
from .. import icons, theme
from ..widgets import Toggle, button, heading, label
from . import Page


class Recorder(QPushButton):
    """While it is pressed in, key presses and releases become macro steps."""
    step = pyqtSignal(object)

    def __init__(self):
        super().__init__(tr("Record"))
        self.setIcon(icons.icon("record", "#FF6B5E", None))
        self.setCheckable(True)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.with_timing = True
        self.pending: MacroStep | None = None
        self.last = 0.0
        self.toggled.connect(self._toggled)

    def _toggled(self, on: bool) -> None:
        self.setText(tr("Stop recording") if on else tr("Record"))
        self.setIcon(icons.icon("stop" if on else "record", "#FF6B5E", None))
        if on:
            self.grabKeyboard()
        else:
            self.releaseKeyboard()
            if self.pending:
                self.step.emit(self.pending)
                self.pending = None
        self.last = 0.0

    def _event(self, e, action: str) -> None:
        if e.isAutoRepeat():
            return
        code = LINUX_TO_HID.get(e.nativeScanCode() - 8, 0)
        if not code:
            return
        now = time.monotonic()
        if self.pending:
            if self.with_timing and self.last:
                self.pending.delay = max(MIN_DELAY, min(MAX_DELAY, int((now - self.last) * 1000)))
            self.step.emit(self.pending)
        self.last = now
        self.pending = MacroStep(action, code)

    def keyPressEvent(self, e) -> None:
        if self.isChecked():
            self._event(e, "down")
        else:
            super().keyPressEvent(e)

    def keyReleaseEvent(self, e) -> None:
        if self.isChecked():
            self._event(e, "up")
        else:
            super().keyReleaseEvent(e)


class MacrosPage(Page):
    title = "Macros"
    icon = "macros"
    stage_mode = None

    def __init__(self, win):
        super().__init__(win)
        left = QVBoxLayout()
        left.setSpacing(theme.S3)
        left.addWidget(heading(tr("Macros")))
        self.list = QListWidget()
        self.list.currentRowChanged.connect(self.show_macro)
        left.addWidget(self.list, 1)
        row = QHBoxLayout()
        row.setSpacing(theme.S1)
        for icon_name, tip, fn in (("plus", tr("New macro"), self.add), ("edit", tr("Rename"), self.rename),
                                   ("copy", tr("Duplicate"), self.duplicate), ("trash", tr("Delete"), self.remove)):
            b = button("", None, icon_name, tip)
            b.clicked.connect(fn)
            row.addWidget(b)
        row.addStretch()
        up = button("", "ghost", "up", tr("Move up"))
        up.clicked.connect(lambda: self.move(-1))
        down = button("", "ghost", "down", tr("Move down"))
        down.clicked.connect(lambda: self.move(1))
        row.addWidget(up)
        row.addWidget(down)
        left.addLayout(row)
        self.memory = QProgressBar()
        self.memory.setRange(0, BUDGET_BYTES)
        left.addWidget(self.memory)
        left.addWidget(label(tr("A macro's number is its slot in the keyboard; keys refer to it on the Keys page."),
                             "dim", wrap=True))

        right = QVBoxLayout()
        right.setSpacing(theme.S3)
        self.steps_title = heading(tr("Steps"))
        right.addWidget(self.steps_title)
        rec = QHBoxLayout()
        self.recorder = Recorder()
        self.recorder.step.connect(self.add_step)
        rec.addWidget(self.recorder)
        self.timing = Toggle(tr("Keep my real pauses"), True)
        self.timing.toggled.connect(lambda on: setattr(self.recorder, "with_timing", on))
        rec.addWidget(self.timing)
        rec.addStretch()
        for text, fn in ((tr("Type text…"), self.add_text), (tr("Shortcut…"), self.add_combo)):
            b = button(text)
            b.clicked.connect(fn)
            rec.addWidget(b)
        self.click_button = button(tr("Mouse click"))
        self.click_button.clicked.connect(self.add_click)
        rec.addWidget(self.click_button)
        right.addLayout(rec)

        self.table = QTableWidget(0, 3)
        self.table.setHorizontalHeaderLabels([tr("Action"), tr("Key or button"), tr("Then wait, ms")])
        self.table.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.ResizeToContents)
        self.table.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        self.table.horizontalHeader().setSectionResizeMode(2, QHeaderView.ResizeMode.ResizeToContents)
        self.table.verticalHeader().setVisible(False)
        self.table.setSelectionBehavior(QAbstractItemView.SelectionBehavior.SelectRows)
        self.table.setEditTriggers(QAbstractItemView.EditTrigger.NoEditTriggers)
        self.table.setMinimumHeight(260)
        right.addWidget(self.table, 1)

        edit = QHBoxLayout()
        edit.addWidget(label(tr("Pause after the selected steps"), "label"))
        self.delay = QSpinBox()
        self.delay.setRange(MIN_DELAY, MAX_DELAY)
        self.delay.setSuffix(" ms")
        self.delay.setValue(50)
        edit.addWidget(self.delay)
        b = button(tr("Set"))
        b.clicked.connect(self.set_delay)
        edit.addWidget(b)
        b = button(tr("Delete steps"), "ghost", "trash")
        b.clicked.connect(self.delete_steps)
        edit.addWidget(b)
        edit.addStretch()
        self.write_button = button(tr("Write macros to keyboard"), "primary", "upload")
        self.write_button.clicked.connect(self.write)
        edit.addWidget(self.write_button)
        right.addLayout(edit)

        self.root.addLayout(self.columns(left, right, stretches=[3, 7]), 1)
        self.refresh()

    # ------------------------------------------------------------ list

    @property
    def macros(self) -> list[Macro]:
        return self.profile.macros

    def macro(self) -> Macro | None:
        r = self.list.currentRow()
        return self.macros[r] if 0 <= r < len(self.macros) else None

    def refresh(self, select: int | None = None) -> None:
        cur = self.list.currentRow() if select is None else select
        self.list.blockSignals(True)
        self.list.clear()
        for i, m in enumerate(self.macros):
            self.list.addItem(f"{i}  {m.name}")
        self.list.blockSignals(False)
        if self.macros:
            self.list.setCurrentRow(max(0, min(cur, len(self.macros) - 1)))
        else:
            from PyQt6.QtWidgets import QListWidgetItem
            item = QListWidgetItem(tr("No macros yet. Press + or Record."))
            item.setFlags(Qt.ItemFlag.NoItemFlags)
            self.list.addItem(item)
        self.show_macro()

    def _memory(self) -> None:
        used = used_bytes(self.macros)
        self.memory.setValue(min(used, BUDGET_BYTES))
        self.memory.setFormat(tr("Keyboard memory: {used} of {total} bytes", used=used, total=BUDGET_BYTES))

    def _changed(self) -> None:
        self._memory()
        self.win.save()
        r = self.list.currentRow()
        m = self.macro()
        if m and self.list.item(r):
            self.list.item(r).setText(f"{r}  {m.name}")

    def show_macro(self, *_) -> None:
        m = self.macro()
        self.steps_title.setText(tr("Steps of “{name}”", name=m.name) if m else tr("Create a macro to start"))
        self.table.setRowCount(0)
        if m:
            self.table.setRowCount(len(m.steps))
            for row, s in enumerate(m.steps):
                act = QTableWidgetItem(tr("press") if s.pressed else tr("release"))
                act.setForeground(theme.color(theme.AMBER if s.pressed else theme.INK_DIM))
                self.table.setItem(row, 0, act)
                self.table.setItem(row, 1, QTableWidgetItem(s.target()))
                spin = QSpinBox()
                spin.setRange(MIN_DELAY, MAX_DELAY)
                spin.setValue(s.delay)
                spin.setFrame(False)
                spin.valueChanged.connect(lambda v, step=s: (setattr(step, "delay", v), self.win.save()))
                self.table.setCellWidget(row, 2, spin)
            total = m.duration_ms()
            self.steps_title.setText(tr("Steps of “{name}”", name=m.name) + "  ·  " +
                                     plural(len(m.steps), "{n} step", "{n} steps") + f"  ·  {total / 1000:.2f} s")
        self._memory()

    def add(self) -> None:
        name, ok = QInputDialog.getText(self, tr("New macro"), tr("Name"),
                                        text=tr("Macro {n}", n=len(self.macros)))
        if ok:
            self.macros.append(Macro(name.strip() or tr("Macro {n}", n=len(self.macros))))
            self.win.save()
            self.refresh(len(self.macros) - 1)

    def rename(self) -> None:
        m = self.macro()
        if m:
            name, ok = QInputDialog.getText(self, tr("Rename"), tr("Name"), text=m.name)
            if ok and name.strip():
                m.name = name.strip()
                self._changed()
                self.show_macro()

    def duplicate(self) -> None:
        m = self.macro()
        if m:
            self.macros.append(Macro.from_dict({**m.to_dict(), "name": m.name + " 2"}))
            self.win.save()
            self.refresh(len(self.macros) - 1)

    def remove(self) -> None:
        r = self.list.currentRow()
        if 0 <= r < len(self.macros):
            self.macros.pop(r)
            self.win.save()
            self.refresh()

    def move(self, d: int) -> None:
        r = self.list.currentRow()
        if 0 <= r < len(self.macros) and 0 <= r + d < len(self.macros):
            self.macros[r], self.macros[r + d] = self.macros[r + d], self.macros[r]
            self.win.save()
            self.refresh(r + d)

    # ------------------------------------------------------------ steps

    def _ensure(self) -> Macro:
        if self.macro() is None:
            self.macros.append(Macro(tr("Macro {n}", n=len(self.macros))))
            self.refresh(len(self.macros) - 1)
        return self.macro()

    def add_step(self, s: MacroStep) -> None:
        self._ensure().steps.append(s)
        self._changed()
        self.show_macro()
        self.table.scrollToBottom()

    def add_text(self) -> None:
        text, ok = QInputDialog.getMultiLineText(self, tr("Type text"),
                                                 tr("Text to type (Latin letters, US layout):"))
        if ok and text:
            self._ensure().steps += Macro.text_steps(text)
            self._changed()
            self.show_macro()

    def add_combo(self) -> None:
        text, ok = QInputDialog.getText(self, tr("Shortcut"), tr("For example ctrl+shift+esc"))
        if ok and text:
            try:
                steps = Macro.combo_steps(text)
            except ValueError as e:
                self.win.error(str(e))
                return
            self._ensure().steps += steps
            self._changed()
            self.show_macro()

    def add_click(self) -> None:
        menu = QMenu(self)
        for mask, name in MOUSE_BUTTON_LABELS.items():
            a = menu.addAction(tr(name))
            a.triggered.connect(lambda _=False, b=mask: self._click(b))
        menu.exec(self.click_button.mapToGlobal(self.click_button.rect().bottomLeft()))

    def _click(self, mask: int) -> None:
        self._ensure().steps += [MacroStep("mdown", mask), MacroStep("mup", mask)]
        self._changed()
        self.show_macro()

    def _selected_rows(self) -> list[int]:
        return sorted({i.row() for i in self.table.selectedIndexes()})

    def set_delay(self) -> None:
        m = self.macro()
        if m:
            for r in self._selected_rows():
                m.steps[r].delay = self.delay.value()
            self._changed()
            self.show_macro()

    def delete_steps(self) -> None:
        m = self.macro()
        if m:
            for r in reversed(self._selected_rows()):
                m.steps.pop(r)
            self._changed()
            self.show_macro()

    def write(self) -> None:
        macros = list(self.macros)
        try:
            encode_macros(macros)
        except ValueError as e:
            self.win.error(str(e))
            return
        self.win.run(self.win.wired(lambda kb: kb.set_macros(macros)),
                     lambda _: self.win.notify(plural(len(macros), "{n} macro written", "{n} macros written")),
                     tr("Writing macros…"))

    def connection_changed(self, state) -> None:
        self.write_button.setEnabled(state == "usb")
        self.write_button.setToolTip("" if state == "usb" else tr("Needs the USB cable"))

