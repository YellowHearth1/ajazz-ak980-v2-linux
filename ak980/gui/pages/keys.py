"""Keys: remapping on the base layer and the Fn layer."""

from __future__ import annotations

from PyQt6.QtCore import Qt, pyqtSignal
from PyQt6.QtWidgets import (QComboBox, QHBoxLayout, QLineEdit, QListWidget, QListWidgetItem, QPushButton,
                             QSpinBox, QStackedWidget, QVBoxLayout, QWidget)

from ...config import Launcher
from ...i18n import tr
from ...keymap import (HID_KEYS, LAUNCHER_KEYS, LINUX_TO_HID, MACRO_MODE_LABELS, MACRO_MODES, MEDIA_KEYS, MOD_ALT,
                       MOD_CTRL, MOD_SHIFT, MOD_WIN, MOUSE_FUNCS, MOUSE_LABELS, KeyAction, key_code, key_name)
from ...layout import by_index
from .. import theme
from ..widgets import Form, KeycapGrid, Segmented, Toggle, button, column, combo, heading, label
from . import Page

KINDS = [("default", "Default"), ("key", "Key or shortcut"), ("mouse", "Mouse"), ("media", "Media"),
         ("macro", "Macro"), ("multi", "Several keys"), ("launch", "Program or link"), ("off", "Disabled")]


class KeyCapture(QPushButton):
    """Press it, then press a key: reports that key's HID usage (layout independent)."""
    captured = pyqtSignal(int)

    def __init__(self):
        super().__init__(tr("Press a key…"))
        self.setCheckable(True)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.toggled.connect(lambda on: self.setText(tr("Waiting for a key…") if on else tr("Press a key…")))

    def keyPressEvent(self, e) -> None:
        if self.isChecked() and not e.isAutoRepeat():
            code = LINUX_TO_HID.get(e.nativeScanCode() - 8, 0)
            self.setChecked(False)
            if code:
                self.captured.emit(code)
            return
        super().keyPressEvent(e)


def key_combo(value: int = 0, empty: str | None = None) -> QComboBox:
    c = QComboBox()
    if empty is not None:
        c.addItem(empty, 0)
    for n, code in HID_KEYS:
        c.addItem(n, code)
    i = c.findData(value)
    c.setCurrentIndex(max(0, i))
    c.setMaxVisibleItems(18)
    return c


class KeysPage(Page):
    title = "Keys"
    icon = "keys"

    def __init__(self, win):
        super().__init__(win)
        self.key = None
        self.loading = False

        top = QHBoxLayout()
        self.layer = Segmented([(False, tr("Base layer")), (True, tr("Fn layer"))], False)
        self.layer.setToolTip(tr("The Fn layer is what a key does while Fn is held"))
        self.layer.changed.connect(lambda _: self.refresh())
        top.addWidget(self.layer)
        top.addSpacing(theme.S3)
        self.hint = label(tr("Click a key on the keyboard above."), "dim")
        top.addWidget(self.hint, 1)
        self.root.addLayout(top)

        edit = QVBoxLayout()
        edit.setSpacing(theme.S3)
        self.key_title = heading(tr("No key selected"))
        edit.addWidget(self.key_title)
        self.kind = KeycapGrid([(k, tr(t)) for k, t in KINDS], columns=4, min_height=34)
        self.kind.changed.connect(self.kind_changed)
        edit.addWidget(self.kind)
        self.editors = QStackedWidget()
        self.editor_index = {}
        for kind, _ in KINDS:
            self.editor_index[kind] = self.editors.addWidget(self._editor(kind))
        edit.addWidget(self.editors)
        self.bytes_label = label("", "mono")
        edit.addWidget(self.bytes_label)
        edit.addStretch(1)
        self.edit_box = QWidget()
        self.edit_box.setLayout(edit)
        self.edit_box.setEnabled(False)

        side = QVBoxLayout()
        side.setSpacing(theme.S3)
        side.addWidget(heading(tr("Changed keys")))
        self.changed_list = QListWidget()
        self.changed_list.setMinimumHeight(160)
        self.changed_list.itemClicked.connect(self._list_clicked)
        side.addWidget(self.changed_list, 1)
        row = QHBoxLayout()
        self.clear_button = button(tr("Reset key"), "ghost")
        self.clear_button.clicked.connect(self.reset_key)
        row.addWidget(self.clear_button)
        self.reset_button = button(tr("Reset layer"), "ghost")
        self.reset_button.setProperty("kind", "danger")
        self.reset_button.clicked.connect(self.reset_layer)
        row.addWidget(self.reset_button)
        row.addStretch()
        side.addLayout(row)
        self.write_button = button(tr("Write layer to keyboard"), "primary", "upload")
        self.write_button.clicked.connect(self.write)
        side.addWidget(self.write_button)
        side.addWidget(label(tr("Writing replaces the whole layer, so it always sends every change on it."),
                             "dim", wrap=True))
        side_w = QWidget()
        side_w.setLayout(side)

        self.root.addLayout(self.columns(column(self.edit_box, stretch=False), column(side_w, stretch=False),
                                         stretches=[7, 4]))
        self.refresh()

    # ------------------------------------------------------------ editors

    def _editor(self, kind: str) -> QWidget:
        w = QWidget()
        form = Form(w)
        if kind == "key":
            mods = QHBoxLayout()
            self.mod_toggles = {}
            for bit, name in ((MOD_CTRL, "Ctrl"), (MOD_SHIFT, "Shift"), (MOD_ALT, "Alt"), (MOD_WIN, "Win")):
                t = Toggle(name)
                t.toggled.connect(self.changed)
                self.mod_toggles[bit] = t
                mods.addWidget(t)
            mods.addStretch()
            form.add(tr("Hold"), mods)
            row = QHBoxLayout()
            self.key_box = key_combo(0, tr("(only the modifiers)"))
            self.key_box.currentIndexChanged.connect(self.changed)
            row.addWidget(self.key_box, 1)
            cap = KeyCapture()
            cap.captured.connect(lambda c: self.key_box.setCurrentIndex(max(0, self.key_box.findData(c))))
            row.addWidget(cap)
            form.add(tr("Key"), row)
        elif kind == "mouse":
            self.mouse_box = combo([(f, tr(t)) for f, t in zip(MOUSE_FUNCS, MOUSE_LABELS)])
            self.mouse_box.currentIndexChanged.connect(self.changed)
            form.add(tr("Action"), self.mouse_box)
        elif kind == "media":
            self.media_box = combo([(u, tr(n)) for n, u in MEDIA_KEYS])
            self.media_box.currentIndexChanged.connect(self.changed)
            form.add(tr("Action"), self.media_box)
        elif kind == "macro":
            self.macro_box = QComboBox()
            self.macro_box.currentIndexChanged.connect(self.changed)
            form.add(tr("Macro"), self.macro_box)
            self.macro_mode = combo([(m, tr(t)) for m, t in zip(MACRO_MODES, MACRO_MODE_LABELS)])
            self.macro_mode.currentIndexChanged.connect(self.changed)
            form.add(tr("Play"), self.macro_mode)
            self.macro_count = QSpinBox()
            self.macro_count.setRange(1, 255)
            self.macro_count.valueChanged.connect(self.changed)
            self.macro_count_label = form.add(tr("Times"), self.macro_count)
            self.no_macros = label(tr("No macros yet: record one on the Macros page."), "dim")
            form.full(self.no_macros)
        elif kind == "multi":
            row = QHBoxLayout()
            self.multi = [key_combo(0, "—") for _ in range(3)]
            for c in self.multi:
                c.currentIndexChanged.connect(self.changed)
                row.addWidget(c)
            form.add(tr("Keys"), row)
        elif kind == "launch":
            self.command = QLineEdit()
            self.command.setPlaceholderText(tr("e.g. steam, obs or https://…"))
            self.command.editingFinished.connect(self.changed)
            form.add(tr("Run"), self.command)
            form.full(label(tr("The key sends a spare F13–F24 code, and the background agent runs the program or "
                               "opens the link. It works while the agent runs."), "dim", wrap=True))
        elif kind == "off":
            form.full(label(tr("The key does nothing."), "dim"))
        else:
            form.full(label(tr("The key does what is printed on it."), "dim"))
        return w

    # ------------------------------------------------------------ state

    @property
    def fn(self) -> bool:
        return bool(self.layer.value())

    def table(self) -> dict:
        return self.profile.keymap_fn if self.fn else self.profile.keymap_top

    def _launcher_for(self, action: KeyAction) -> Launcher | None:
        if action.kind != "key" or action.mods:
            return None
        name = key_name(action.code)
        return next((x for x in self.cfg.settings.launchers if x.key == name), None)

    def describe(self, action: KeyAction) -> str:
        launcher = self._launcher_for(action)
        if launcher:
            return tr("Run: {cmd}", cmd=launcher.command)
        return action.describe([m.name for m in self.profile.macros])

    def refresh(self) -> None:
        self.macro_box.blockSignals(True)
        self.macro_box.clear()
        for i, m in enumerate(self.profile.macros):
            self.macro_box.addItem(f"{i}: {m.name}", i)
        self.macro_box.blockSignals(False)
        self.no_macros.setVisible(not self.profile.macros)
        self.changed_list.clear()
        for fn_layer, table in ((False, self.profile.keymap_top), (True, self.profile.keymap_fn)):
            for idx, action in sorted(table.items()):
                k = by_index(idx)
                if k is None or action.kind == "default":
                    continue
                item = QListWidgetItem(f"{'Fn + ' if fn_layer else ''}{k.label}  →  {self.describe(action)}")
                item.setData(Qt.ItemDataRole.UserRole, (fn_layer, idx))
                self.changed_list.addItem(item)
        if not self.changed_list.count():
            item = QListWidgetItem(tr("Every key does what is printed on it"))
            item.setFlags(Qt.ItemFlag.NoItemFlags)
            self.changed_list.addItem(item)
        if self.key is not None:
            self.select(self.key)
        self._captions()

    def _captions(self) -> None:
        if self.win.page() is self:
            self.stage.captions = {i: self.describe(a) for i, a in self.table().items() if a.kind != "default"}
            self.stage.selected = {self.key.index} if self.key else set()
            self.stage.update()

    def on_enter(self) -> None:
        self.stage.set_colors({})
        self._captions()

    def on_stage_key(self, key, mouse_button) -> None:
        self.select(key)

    def _list_clicked(self, item) -> None:
        data = item.data(Qt.ItemDataRole.UserRole)
        if not data:
            return
        fn_layer, idx = data
        self.layer.set_value(fn_layer)
        k = by_index(idx)
        if k:
            self.select(k)
        self._captions()

    def select(self, key) -> None:
        self.key = key
        self.edit_box.setEnabled(True)
        self.key_title.setText(("Fn + " if self.fn else "") + key.label)
        a = self.table().get(key.index, KeyAction())
        launcher = self._launcher_for(a)
        self.loading = True
        kind = "launch" if launcher else a.kind
        self.kind.set_value(kind)
        self.editors.setCurrentIndex(self.editor_index[kind])
        for bit, t in self.mod_toggles.items():
            t.setChecked(bool(a.mods & bit))
        self.key_box.setCurrentIndex(max(0, self.key_box.findData(a.code if a.kind == "key" else 0)))
        self.mouse_box.setCurrentIndex(max(0, self.mouse_box.findData(a.mouse)))
        self.media_box.setCurrentIndex(max(0, self.media_box.findData(a.usage)))
        self.macro_box.setCurrentIndex(max(0, self.macro_box.findData(a.macro)))
        self.macro_mode.setCurrentIndex(max(0, self.macro_mode.findData(a.macro_mode)))
        self.macro_count.setValue(a.macro_count)
        for i, c in enumerate(self.multi):
            c.setCurrentIndex(max(0, c.findData(a.keys[i] if i < len(a.keys) else 0)))
        self.command.setText(launcher.command if launcher else "")
        self.loading = False
        self._show_repeat()
        self.bytes_label.setText(tr("Sent as {hex}", hex=a.to_entry().hex(" ")))
        self._captions()

    def _show_repeat(self) -> None:
        repeat = self.macro_mode.currentData() == "repeat"
        self.macro_count.setVisible(repeat)
        if self.macro_count_label:
            self.macro_count_label.setVisible(repeat)

    def kind_changed(self, kind) -> None:
        self.editors.setCurrentIndex(self.editor_index[kind])
        self.changed()

    def current(self) -> KeyAction | None:
        kind = self.kind.value()
        a = KeyAction(kind=kind if kind != "launch" else "key")
        if kind == "key":
            a.mods = sum(bit for bit, t in self.mod_toggles.items() if t.isChecked())
            a.code = self.key_box.currentData() or 0
        elif kind == "mouse":
            a.mouse = self.mouse_box.currentData()
        elif kind == "media":
            a.usage = self.media_box.currentData()
        elif kind == "macro":
            if not self.profile.macros:
                return None
            a.macro = self.macro_box.currentData() or 0
            a.macro_mode = self.macro_mode.currentData()
            a.macro_count = self.macro_count.value()
        elif kind == "multi":
            a.keys = [c.currentData() for c in self.multi if c.currentData()]
        elif kind == "launch":
            command = self.command.text().strip()
            if not command:
                return None
            fkey = self._launcher_key(command)
            if fkey is None:
                self.win.error(tr("All twelve launcher keys (F13–F24) are in use."))
                return None
            a.code = key_code(fkey)
        return a

    def _launcher_key(self, command: str) -> str | None:
        """Reuses this key's launcher slot, or takes the first free F13–F24."""
        launchers = self.cfg.settings.launchers
        old = self._launcher_for(self.table().get(self.key.index, KeyAction()))
        if old:
            old.command = command
            return old.key
        used = {x.key for x in launchers}
        for name in LAUNCHER_KEYS:
            if name not in used:
                launchers.append(Launcher(name, command))
                return name
        return None

    def _drop_unused_launchers(self) -> None:
        used = set()
        for p in self.cfg.profiles:
            for table in (p.keymap_top, p.keymap_fn):
                for a in table.values():
                    if a.kind == "key" and not a.mods:
                        used.add(key_name(a.code))
        self.cfg.settings.launchers = [x for x in self.cfg.settings.launchers if x.key in used]

    def changed(self, *_) -> None:
        if self.loading or self.key is None:
            return
        self._show_repeat()
        a = self.current()
        if a is None:
            return
        if a.kind == "default":
            self.table().pop(self.key.index, None)
        else:
            self.table()[self.key.index] = a
        self._drop_unused_launchers()
        self.win.save()
        self.bytes_label.setText(tr("Sent as {hex} — not on the keyboard until you write the layer",
                                    hex=a.to_entry().hex(" ")))
        self.refresh()

    def reset_key(self) -> None:
        if self.key is not None:
            self.table().pop(self.key.index, None)
            self._drop_unused_launchers()
            self.win.save()
            self.refresh()

    def reset_layer(self) -> None:
        self.table().clear()
        self._drop_unused_launchers()
        self.win.save()
        self.refresh()
        self.write()

    def write(self) -> None:
        fn, table = self.fn, dict(self.table())
        self.win.run(self.win.wired(lambda kb: kb.set_keymap(fn, table)),
                     lambda _: self.win.notify(tr("Fn layer written") if fn else tr("Base layer written")),
                     tr("Writing keys…"))

    def connection_changed(self, state) -> None:
        self.write_button.setEnabled(state == "usb")
        self.write_button.setToolTip("" if state == "usb" else tr("Needs the USB cable"))
