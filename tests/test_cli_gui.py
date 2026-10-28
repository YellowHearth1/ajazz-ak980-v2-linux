import pytest

from ak980 import cli


def test_parser_in_both_languages():
    from ak980.i18n import set_language
    for lang in ("uk", "en"):
        set_language(lang)
        assert cli.build_parser().parse_args(["remap", "CapsLock", "key:ctrl", "--dry"]).cmd == "remap"


def test_dry_remap(capsys):
    assert cli.main(["--lang", "en", "remap", "CapsLock", "key:ctrl", "--dry"]) == 0
    out = capsys.readouterr().out
    assert "02 01 00 00" in out


def test_fx_and_profiles(capsys):
    assert cli.main(["--lang", "en", "fx", "fire", "height=80"]) == 0
    assert cli.main(["--lang", "en", "profile", "new", "Game", "--copy"]) == 0
    assert cli.main(["--lang", "en", "auto", "add", "cs2", "Game"]) == 0
    from ak980.config import Config
    c = Config.load()
    assert c.profile.light_source == "host" and c.profile.host_values["fire"]["height"] == 80
    assert [r.process for r in c.settings.rules] == ["cs2"]


def test_window_builds_every_page():
    pytest.importorskip("PyQt6.QtWidgets")
    from PyQt6.QtWidgets import QApplication
    app = QApplication.instance() or QApplication([])
    from ak980.gui import theme
    from ak980.gui.window import MainWindow
    theme.load_fonts()
    app.setStyleSheet(theme.stylesheet())
    win = MainWindow()
    for i in range(len(win.pages)):
        win.select_page(i)
        app.processEvents()
    win.engine.stop()
    win.close()
