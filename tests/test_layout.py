"""No clipped text and no page wider than the window, in both languages, at the default window size."""

import os
import sys

import pytest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "tools"))


@pytest.mark.parametrize("lang", ["en", "uk"])
def test_pages_fit(lang):
    pytest.importorskip("PyQt6.QtWidgets")
    from PyQt6.QtWidgets import QApplication

    import layout_audit
    from ak980 import gui
    from ak980.i18n import set_language

    app = QApplication.instance() or QApplication([])
    set_language(lang)
    try:
        gui.prepare(app, lang)
        from ak980.gui.window import MainWindow
        win = MainWindow()
        win.resize(1280, 860)
        win.show()
        layout_audit.settle(app)
        report = layout_audit.audit(win, app)
        win.engine.stop()
        win.close()
    finally:
        set_language("en")
        gui.number_locale("en")
    problems = [f"[{page}] {p}" for page, items in report.items() for p in items]
    assert not problems, "\n".join(problems)
