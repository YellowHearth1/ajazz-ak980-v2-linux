"""The app window (PyQt6)."""

from __future__ import annotations

import os
import sys


def _palette():
    from PyQt6.QtGui import QColor, QPalette
    from . import theme
    pal = QPalette()
    roles = {
        QPalette.ColorRole.Window: theme.CASE, QPalette.ColorRole.WindowText: theme.INK,
        QPalette.ColorRole.Base: theme.PLATE, QPalette.ColorRole.AlternateBase: theme.KEYCAP_LO,
        QPalette.ColorRole.Text: theme.INK, QPalette.ColorRole.Button: theme.KEYCAP,
        QPalette.ColorRole.ButtonText: theme.INK, QPalette.ColorRole.Highlight: theme.AMBER,
        QPalette.ColorRole.HighlightedText: theme.ON_AMBER, QPalette.ColorRole.ToolTipBase: theme.KEYCAP,
        QPalette.ColorRole.ToolTipText: theme.INK, QPalette.ColorRole.Link: theme.AMBER,
        QPalette.ColorRole.PlaceholderText: theme.INK_OFF, QPalette.ColorRole.Mid: theme.EDGE,
        QPalette.ColorRole.Dark: theme.DESK, QPalette.ColorRole.Light: theme.KEYCAP_HI,
    }
    for role, value in roles.items():
        pal.setColor(role, QColor(value))
    for role in (QPalette.ColorRole.Text, QPalette.ColorRole.WindowText, QPalette.ColorRole.ButtonText):
        pal.setColor(QPalette.ColorGroup.Disabled, role, QColor(theme.INK_OFF))
    return pal


def number_locale(lang: str) -> None:
    """Numbers in spin boxes follow the interface language, not only the system locale."""
    from PyQt6.QtCore import QLocale
    QLocale.setDefault(QLocale(QLocale.Language.Ukrainian if lang == "uk" else QLocale.Language.English,
                               QLocale.Country.Ukraine if lang == "uk" else QLocale.Country.UnitedStates))


def prepare(app, lang: str) -> None:
    """Style, palette, fonts and number format; the screenshot and layout tools use it too."""
    from . import theme
    number_locale(lang)
    app.setStyle("Fusion")
    app.setPalette(_palette())
    theme.load_fonts()
    app.setFont(theme.font("body"))
    app.setStyleSheet(theme.stylesheet())


def main(tray: bool = False) -> int:
    from PyQt6.QtNetwork import QLocalServer, QLocalSocket
    from PyQt6.QtWidgets import QApplication

    from .. import APP_ID, APP_NAME
    from ..config import peek_language
    from ..i18n import set_language

    lang = set_language(peek_language())

    app = QApplication(sys.argv)
    app.setApplicationName("ak980")
    app.setApplicationDisplayName(APP_NAME)
    app.setDesktopFileName(APP_ID)
    app.setQuitOnLastWindowClosed(False)

    # One window per user: a second start just brings the first one forward
    from ..runtime import runtime_dir
    name = os.path.join(runtime_dir(), "ak980-gui.sock")
    probe = QLocalSocket()
    probe.connectToServer(name)
    if probe.waitForConnected(300):
        probe.write(b"show")
        probe.flush()
        probe.waitForBytesWritten(300)
        return 0
    QLocalServer.removeServer(name)
    server = QLocalServer()
    server.listen(name)

    from . import icons
    from .window import MainWindow
    prepare(app, lang)
    app.setWindowIcon(icons.app_icon())

    win = MainWindow()
    server.newConnection.connect(lambda: (server.nextPendingConnection(), win.show_window()))
    if not (tray and win.tray):
        win.show()
    return app.exec()
