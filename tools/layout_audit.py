"""Finds clipped text and widgets that do not fit, on every page, without looking at pixels.

    QT_QPA_PLATFORM=offscreen python tools/layout_audit.py [WIDTHxHEIGHT ...] [--lang uk|en]

Checks labels, buttons, combo and spin boxes, line edits and toggles for clipped text, table rows
whose cell widgets are taller than the row, children sticking out of their parent, pages that
need horizontal scrolling, container widgets that keep Qt's default layout margins (their
content then sits lower and further right than the columns next to them), and empty labels that
still take up a row. Runs the demo profile from tools/screenshots.py in preview mode (nothing
is sent to a keyboard) and exits with status 1 when something does not fit.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SIZES = ["1080x720", "1280x860", "1920x1040"]


def check(root, win) -> list[str]:
    from PyQt6.QtCore import QMargins, Qt
    from PyQt6.QtWidgets import (QAbstractSpinBox, QComboBox, QLabel, QLineEdit, QPushButton, QScrollArea,
                                 QStackedWidget, QStyle, QTableWidget, QWidget)
    from PyQt6.QtWidgets import QAbstractButton
    from ak980.gui.widgets import ElidedLabel, Toggle

    def name(w) -> str:
        for attr in ("text", "currentText", "placeholderText"):
            fn = getattr(w, attr, None)
            if callable(fn):
                try:
                    text = fn()
                except TypeError:
                    continue
                if text:
                    return f"{type(w).__name__}({text[:40]!r})"
        return type(w).__name__

    def shown(w) -> bool:
        return w.isVisibleTo(win) and w.width() > 0 and w.height() > 0

    problems = []
    for w in [root] + root.findChildren(QWidget):
        if not shown(w):
            continue
        fm = w.fontMetrics()
        if isinstance(w, QAbstractButton) and not isinstance(w, QPushButton) and w.hasHeightForWidth():
            if w.minimumSizeHint().width() > w.width() + 1:
                problems.append(f"too narrow {name(w)}: needs {w.minimumSizeHint().width()}px, has {w.width()}px")
            elif w.height() + 1 < w.heightForWidth(w.width()):
                problems.append(f"too short {name(w)}: needs {w.heightForWidth(w.width())}px, has {w.height()}px")
        if isinstance(w, ElidedLabel):
            pass
        elif isinstance(w, QLabel):
            pm = w.pixmap()
            text = w.text()
            if (not text and (pm is None or pm.isNull()) and type(w) is QLabel and w.width() > 4
                    and w.height() > 4 and not box_stretch(w.parentWidget().layout(), w)):
                near = next((c.text() for c in w.parentWidget().findChildren(QLabel) if c.text()), "")
                problems.append(f"empty label near {near[:30]!r} takes {w.width()}x{w.height()}px")
            if (not w.wordWrap() and w.textFormat() != Qt.TextFormat.RichText and text and "<" not in text
                    and (pm is None or pm.isNull())):
                need = max(fm.horizontalAdvance(line) for line in text.split("\n"))
                have = w.contentsRect().width() - 2 * w.margin()
                if need > have + 1:
                    problems.append(f"clipped label {name(w)}: needs {need}px, has {have}px")
        elif isinstance(w, Toggle):
            pass
        elif isinstance(w, QPushButton):
            if w.hasHeightForWidth():
                if w.height() + 1 < w.heightForWidth(w.width()):
                    problems.append(f"button too short {name(w)}")
            elif w.text() and w.sizeHint().width() > w.width() + 1:
                problems.append(f"clipped button {name(w)}: needs {w.sizeHint().width()}px, has {w.width()}px")
        elif isinstance(w, QComboBox):
            need = fm.horizontalAdvance(w.currentText()) + 38
            if need > w.width() + 1:
                problems.append(f"clipped combo {name(w)}: needs {need}px, has {w.width()}px")
        elif isinstance(w, QAbstractSpinBox):
            need = fm.horizontalAdvance(w.text()) + 40
            if need > w.width() + 1:
                problems.append(f"clipped spin box {name(w)}: needs {need}px, has {w.width()}px")
        elif isinstance(w, QLineEdit) and not w.text() and w.placeholderText():
            need = fm.horizontalAdvance(w.placeholderText()) + 20
            if need > w.width() + 1:
                problems.append(f"clipped placeholder {name(w)}: needs {need}px, has {w.width()}px")
        if isinstance(w, QTableWidget):
            for row in range(w.rowCount()):
                for col in range(w.columnCount()):
                    cell = w.cellWidget(row, col)
                    if cell is not None and cell.sizeHint().height() > w.rowHeight(row) + 1:
                        problems.append(f"table row {row} of {name(w)} is {w.rowHeight(row)}px, its "
                                        f"{type(cell).__name__} needs {cell.sizeHint().height()}px")
        lay = w.layout()
        if type(w) is QWidget and lay is not None and any(p.isAncestorOf(w) for p in win.pages):
            m = w.style().pixelMetric(QStyle.PixelMetric.PM_LayoutLeftMargin, None, w)
            if m > 0 and lay.contentsMargins() == QMargins(m, m, m, m):
                inside = next((c.text() for c in w.findChildren(QLabel) if c.text()), "")
                problems.append(f"container with {inside[:30]!r} keeps the default {m}px layout margins")
        if isinstance(w, QScrollArea) and w.widget() is not None and w.widget().objectName() == "panel" \
                and w.horizontalScrollBar().isVisible():
            problems.append(f"page needs horizontal scrolling ({w.widget().minimumSizeHint().width()}px wide)")
        parent = w.parentWidget()
        if (parent is not None and shown(parent) and parent.objectName() != "qt_scrollarea_viewport"
                and not isinstance(parent, (QScrollArea, QStackedWidget)) and w.window() is win):
            g = w.geometry()
            if g.right() > parent.width() + 2 or g.bottom() > parent.height() + 2:
                problems.append(f"{name(w)} sticks out of {name(parent)} by "
                                f"{max(g.right() - parent.width(), g.bottom() - parent.height())}px")
    return problems


def box_stretch(layout, w) -> int:
    """The stretch factor ``w`` has in ``layout`` or a layout nested in it (0 if none)."""
    from PyQt6.QtWidgets import QBoxLayout
    if layout is None:
        return 0
    for i in range(layout.count()):
        item = layout.itemAt(i)
        if item.widget() is w:
            return layout.stretch(i) if isinstance(layout, QBoxLayout) else 0
        if item.layout() is not None and (found := box_stretch(item.layout(), w)):
            return found
    return 0


def settle(app) -> None:
    for _ in range(8):
        app.processEvents()


def audit(win, app) -> dict[str, list[str]]:
    from ak980.layout import by_index
    report: dict[str, list[str]] = {}
    for i, page in enumerate(("screen", "lighting", "colours", "keys", "macros", "profiles", "device")):
        win.select_page(i)
        settle(app)
        report[page] = check(win.centralWidget(), win)
    lighting = win.pages[1]
    win.select_page(1)
    lighting.source.set_value("host")
    lighting.source_changed("host")
    for effect in ("wave", "drift", "breath", "starlight", "reactive", "fire", "spectrum", "pulse"):
        lighting.effects.set_value(effect)
        lighting.host_changed(effect)
        settle(app)
        report[f"lighting/{effect}"] = check(win.centralWidget(), win)
    keys_page = win.pages[3]
    win.select_page(3)
    keys_page.select(by_index(55))
    for kind in ("key", "mouse", "media", "macro", "multi", "launch", "off"):
        keys_page.kind.set_value(kind)
        keys_page.editors.setCurrentIndex(keys_page.editor_index[kind])
        settle(app)
        report[f"keys/{kind}"] = check(win.centralWidget(), win)
    return report


def run_child(size: str, lang: str) -> int:
    from PyQt6.QtWidgets import QApplication
    sys.path.insert(0, ROOT)
    from ak980 import gui
    from ak980.i18n import set_language
    set_language(lang)
    app = QApplication(sys.argv[:1])
    gui.prepare(app, lang)
    from ak980.gui.window import MainWindow
    win = MainWindow()
    w, h = (int(x) for x in size.split("x"))
    win.resize(w, h)
    win.show()
    settle(app)
    report = audit(win, app)
    win.engine.stop()
    win.close()
    bad = {k: v for k, v in report.items() if v}
    print(f"== {lang} {size}: {sum(len(v) for v in bad.values())} problem(s)")
    for page, problems in bad.items():
        for p in problems:
            print(f"   [{page}] {p}")
    return 1 if bad else 0


def main() -> int:
    args = sys.argv[1:]
    langs = ["uk", "en"]
    if "--lang" in args:
        i = args.index("--lang")
        langs = [args[i + 1]]
        args = args[:i] + args[i + 2:]
    sizes = args or SIZES
    sys.path.insert(0, HERE)
    from screenshots import demo_animation, demo_config
    status = 0
    with tempfile.TemporaryDirectory() as tmp:
        gif = os.path.join(tmp, "demo.gif")
        demo_animation(gif)
        for lang in langs:
            home = os.path.join(tmp, lang)
            os.makedirs(os.path.join(home, "config", "ak980"))
            with open(os.path.join(home, "config", "ak980", "config.json"), "w", encoding="utf-8") as f:
                json.dump(demo_config(gif, lang), f, ensure_ascii=False)
            for size in sizes:
                env = dict(os.environ, XDG_CONFIG_HOME=os.path.join(home, "config"),
                           XDG_DATA_HOME=os.path.join(home, "data"), AK980_PREVIEW_ONLY="1",
                           AK980_AUDIT_CHILD=f"{size}:{lang}",
                           QT_QPA_PLATFORM=os.environ.get("QT_QPA_PLATFORM", "offscreen"), PYTHONPATH=ROOT)
                status |= subprocess.run([sys.executable, os.path.abspath(__file__)], env=env).returncode
    return status


if __name__ == "__main__":
    child = os.environ.get("AK980_AUDIT_CHILD")
    if child:
        size, lang = child.split(":")
        sys.exit(run_child(size, lang))
    sys.exit(main())
