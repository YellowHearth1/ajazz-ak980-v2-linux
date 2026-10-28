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


def main(tray: bool = False) -> int:
    from PyQt6.QtNetwork import QLocalServer, QLocalSocket
    from PyQt6.QtWidgets import QApplication

    from .. import APP_ID, APP_NAME
    from ..config import peek_language
    from ..i18n import set_language

    set_language(peek_language())

    app = QApplication(sys.argv)
    app.setApplicationName("ak980")
    app.setApplicationDisplayName(APP_NAME)
    app.setDesktopFileName(APP_ID)
    app.setQuitOnLastWindowClosed(False)

    # One window per user: a second start just brings the first one forward
    name = f"ak980-gui-{os.getuid()}"
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

    from . import icons, theme
    from .window import MainWindow
    app.setStyle("Fusion")
    app.setPalette(_palette())
    theme.load_fonts()
    app.setFont(theme.font("body"))
    app.setWindowIcon(icons.app_icon())
    app.setStyleSheet(theme.stylesheet())

    win = MainWindow()
    server.newConnection.connect(lambda: (server.nextPendingConnection(), win.show_window()))
    if not (tray and win.tray):
        win.show()
    return app.exec()
