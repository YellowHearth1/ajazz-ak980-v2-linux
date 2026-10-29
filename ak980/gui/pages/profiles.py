"""Profiles, automatic switching for games, launcher keys."""

from __future__ import annotations

import os

from PyQt6.QtCore import Qt
from PyQt6.QtWidgets import (QComboBox, QDialog, QDialogButtonBox, QFileDialog, QHBoxLayout, QHeaderView,
                             QInputDialog, QLineEdit, QListWidget, QListWidgetItem, QTableWidget, QTableWidgetItem,
                             QVBoxLayout)

from ...agent import running_processes
from ...config import Profile, Rule, export_profile, import_profile
from ...i18n import tr
from .. import theme
from ..widgets import StatusLine, Toggle, button, heading, label
from . import Page


class RuleDialog(QDialog):
    def __init__(self, parent, profiles: list[str]):
        super().__init__(parent)
        self.setWindowTitle(tr("Switch profile for a program"))
        lay = QVBoxLayout(self)
        lay.setSpacing(theme.S3)
        lay.addWidget(label(tr("Program name as the system sees it (for Steam games often the .exe name). "
                               "Pick a running one or type it."), "dim", wrap=True))
        self.name = QLineEdit()
        self.name.setPlaceholderText("cs2, eldenring.exe, obs …")
        lay.addWidget(self.name)
        self.running = QListWidget()
        self.running.setMinimumHeight(220)
        for n in sorted(x for x in running_processes() if x and not x.startswith(("kworker", "ksoftirq", "rcu_"))):
            self.running.addItem(n)
        self.running.itemClicked.connect(lambda item: self.name.setText(item.text()))
        lay.addWidget(self.running)
        row = QHBoxLayout()
        row.addWidget(label(tr("Profile"), "label"))
        self.profile = QComboBox()
        self.profile.addItems(profiles)
        row.addWidget(self.profile, 1)
        lay.addLayout(row)
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        lay.addWidget(buttons)
        self.resize(460, 480)


class ProfilesPage(Page):
    title = "Profiles"
    icon = "profiles"
    stage_mode = None

    def __init__(self, win):
        super().__init__(win)
        left = QVBoxLayout()
        left.setSpacing(theme.S3)
        left.addWidget(heading(tr("Profiles")))
        left.addWidget(label(tr("A profile holds the lighting, per-key colours, both key layers, macros and "
                                "keyboard options. Switching sends it to the keyboard."), "dim", wrap=True))
        self.list = QListWidget()
        self.list.itemDoubleClicked.connect(lambda _: self.use())
        left.addWidget(self.list, 1)
        row = QHBoxLayout()
        row.setSpacing(theme.S1)
        for icon_name, tip, fn in (("plus", tr("New profile"), self.new), ("copy", tr("Duplicate"), self.duplicate),
                                   ("edit", tr("Rename"), self.rename), ("trash", tr("Delete"), self.delete),
                                   ("download", tr("Import…"), self.import_), ("upload", tr("Export…"), self.export)):
            b = button("", None, icon_name, tip)
            b.clicked.connect(fn)
            row.addWidget(b)
        row.addStretch()
        left.addLayout(row)
        self.use_button = button(tr("Use this profile"), "primary", "check")
        self.use_button.clicked.connect(self.use)
        left.addWidget(self.use_button)

        mid = QVBoxLayout()
        mid.setSpacing(theme.S3)
        mid.addWidget(heading(tr("Games and programs")))
        self.auto = Toggle(tr("Switch profiles when a program starts"), self.cfg.settings.auto_switch)
        self.auto.toggled.connect(self._auto)
        mid.addWidget(self.auto)
        self.rules = QTableWidget(0, 2)
        self.rules.setHorizontalHeaderLabels([tr("Program"), tr("Profile")])
        self.rules.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.Stretch)
        self.rules.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        self.rules.verticalHeader().setVisible(False)
        self.rules.verticalHeader().setDefaultSectionSize(38)
        self.rules.setMinimumHeight(170)
        mid.addWidget(self.rules, 1)
        row = QHBoxLayout()
        b = button(tr("Add a program…"), None, "plus")
        b.clicked.connect(self.add_rule)
        row.addWidget(b)
        b = button(tr("Remove"), "ghost", "trash")
        b.clicked.connect(self.remove_rule)
        row.addWidget(b)
        row.addStretch()
        mid.addLayout(row)
        mid.addWidget(label(tr("After the program exits"), "label", wrap=True))
        self.default = QComboBox()
        self.default.currentIndexChanged.connect(self._default)
        mid.addWidget(self.default)
        self.agent_note = StatusLine()
        mid.addWidget(self.agent_note)

        right = QVBoxLayout()
        right.setSpacing(theme.S3)
        right.addWidget(heading(tr("Launcher keys")))
        right.addWidget(label(tr("Keys set to “Program or link” on the Keys page. The background agent runs them."),
                              "dim", wrap=True))
        self.launchers = QTableWidget(0, 2)
        self.launchers.setHorizontalHeaderLabels([tr("Code"), tr("Runs")])
        self.launchers.horizontalHeader().setSectionResizeMode(0, QHeaderView.ResizeMode.ResizeToContents)
        self.launchers.horizontalHeader().setSectionResizeMode(1, QHeaderView.ResizeMode.Stretch)
        self.launchers.verticalHeader().setVisible(False)
        self.launchers.setMinimumHeight(170)
        self.launchers.itemChanged.connect(self._launcher_edited)
        right.addWidget(self.launchers, 1)

        self.root.addLayout(self.columns(left, mid, right, stretches=[4, 5, 4]), 1)
        self.refresh()

    # ------------------------------------------------------------ profiles

    def refresh(self) -> None:
        self.list.clear()
        for name in self.cfg.names():
            item = QListWidgetItem(name)
            if name == self.cfg.active:
                item.setForeground(theme.color(theme.AMBER))
                item.setToolTip(tr("Active profile"))
                f = item.font()
                f.setBold(True)
                item.setFont(f)
            self.list.addItem(item)
        self.list.setCurrentRow(self.cfg.names().index(self.cfg.active))
        self._rules()
        self._launchers()

    def selected(self) -> str | None:
        item = self.list.currentItem()
        return item.text() if item else None

    def _changed(self) -> None:
        self.win.save()
        self.win.refresh_profiles()
        self.refresh()

    def new(self) -> None:
        name, ok = QInputDialog.getText(self, tr("New profile"), tr("Name"), text=self.cfg.unique_name(tr("Profile")))
        if ok:
            self.cfg.add(Profile(name=name.strip() or tr("Profile")))
            self._changed()

    def duplicate(self) -> None:
        p = self.cfg.get(self.selected() or "")
        if p:
            self.cfg.add(p.copy(p.name))
            self._changed()

    def rename(self) -> None:
        old = self.selected()
        if not old:
            return
        name, ok = QInputDialog.getText(self, tr("Rename"), tr("Name"), text=old)
        if ok and name.strip() and name.strip() != old:
            try:
                self.cfg.rename(old, name)
            except ValueError as e:
                self.win.error(str(e))
                return
            self._changed()

    def delete(self) -> None:
        name = self.selected()
        if not name:
            return
        try:
            self.cfg.remove(name)
        except ValueError as e:
            self.win.error(str(e))
            return
        self.win.engine.preview_only_profile = self.cfg.profile
        self._changed()

    def import_(self) -> None:
        path, _ = QFileDialog.getOpenFileName(self, tr("Import profile"), os.path.expanduser("~"),
                                              tr("AK980 profiles") + " (*.json)")
        if path:
            try:
                p = self.cfg.add(import_profile(path))
            except (OSError, ValueError) as e:
                self.win.error(str(e))
                return
            self._changed()
            self.win.notify(tr("Profile imported: {name}", name=p.name))

    def export(self) -> None:
        p = self.cfg.get(self.selected() or "")
        if not p:
            return
        path, _ = QFileDialog.getSaveFileName(self, tr("Export profile"),
                                              os.path.expanduser(f"~/{p.name}.ak980.json"),
                                              tr("AK980 profiles") + " (*.json)")
        if path:
            export_profile(p, path)
            self.win.notify(tr("Profile exported"))

    def use(self) -> None:
        name = self.selected()
        if name:
            self.win.switch_profile(name)
            self.refresh()

    # ------------------------------------------------------------ automatic switching

    def _rules(self) -> None:
        s = self.cfg.settings
        self.rules.setRowCount(len(s.rules))
        for row, r in enumerate(s.rules):
            self.rules.setItem(row, 0, QTableWidgetItem(r.process))
            box = QComboBox()
            box.addItems(self.cfg.names())
            box.setCurrentText(r.profile)
            box.currentTextChanged.connect(lambda name, rule=r: (setattr(rule, "profile", name), self.win.save()))
            self.rules.setCellWidget(row, 1, box)
        self.default.blockSignals(True)
        self.default.clear()
        self.default.addItem(tr("Go back to the previous profile"), "")
        for name in self.cfg.names():
            self.default.addItem(name, name)
        self.default.setCurrentIndex(max(0, self.default.findData(s.default_profile)))
        self.default.blockSignals(False)
        from ... import runtime
        self.agent_note.setText("" if runtime.agent_running() else
                                tr("Switching is done by the background agent, which is not running. Turn it on "
                                   "on the Device page."))

    def _auto(self, on: bool) -> None:
        self.cfg.settings.auto_switch = on
        self.win.save()

    def _default(self, _) -> None:
        self.cfg.settings.default_profile = self.default.currentData() or ""
        self.win.save()

    def add_rule(self) -> None:
        dlg = RuleDialog(self, self.cfg.names())
        if dlg.exec() and dlg.name.text().strip():
            process = dlg.name.text().strip()
            s = self.cfg.settings
            s.rules = [r for r in s.rules if r.process.lower() != process.lower()]
            s.rules.append(Rule(process, dlg.profile.currentText()))
            self.win.save()
            self._rules()

    def remove_rule(self) -> None:
        rows = sorted({i.row() for i in self.rules.selectedIndexes()}, reverse=True)
        if not rows and self.rules.currentRow() >= 0:
            rows = [self.rules.currentRow()]
        for r in rows:
            if 0 <= r < len(self.cfg.settings.rules):
                self.cfg.settings.rules.pop(r)
        self.win.save()
        self._rules()

    # ------------------------------------------------------------ launchers

    def _launchers(self) -> None:
        self.launchers.blockSignals(True)
        items = self.cfg.settings.launchers
        self.launchers.setRowCount(len(items))
        for row, x in enumerate(items):
            code = QTableWidgetItem(x.key)
            code.setFlags(code.flags() & ~Qt.ItemFlag.ItemIsEditable)
            self.launchers.setItem(row, 0, code)
            self.launchers.setItem(row, 1, QTableWidgetItem(x.command))
        self.launchers.blockSignals(False)

    def _launcher_edited(self, item) -> None:
        if item.column() == 1 and 0 <= item.row() < len(self.cfg.settings.launchers):
            self.cfg.settings.launchers[item.row()].command = item.text().strip()
            self.win.save()

    def on_enter(self) -> None:
        self.refresh()
