"""The app's own line icons (24 px grid, 1.7 px stroke), drawn for this keyboard."""

from __future__ import annotations

import os
from functools import lru_cache

from PyQt6.QtCore import QByteArray, QRectF, Qt
from PyQt6.QtGui import QIcon, QPainter, QPixmap
from PyQt6.QtSvg import QSvgRenderer

from . import theme

_PATHS = {
    # Navigation: each one is drawn from the part of the keyboard it controls
    "screen": '<rect x="2.5" y="6" width="12.5" height="8.5" rx="1.5"/><circle cx="19.2" cy="10.2" r="2.6"/>'
              '<path d="M2.5 18.5h19"/>',
    "lighting": '<circle cx="12" cy="12" r="3.2"/><path d="M12 3v2.4M12 18.6V21M3 12h2.4M18.6 12H21'
                'M5.6 5.6l1.7 1.7M16.7 16.7l1.7 1.7M5.6 18.4l1.7-1.7M16.7 7.3l1.7-1.7"/>',
    "colours": '<rect x="3" y="6.5" width="5" height="5" rx="1"/><rect x="9.5" y="6.5" width="5" height="5" rx="1" '
               'fill="currentColor"/><rect x="16" y="6.5" width="5" height="5" rx="1"/><rect x="3" y="13" width="5" '
               'height="5" rx="1"/><rect x="9.5" y="13" width="5" height="5" rx="1"/><rect x="16" y="13" width="5" '
               'height="5" rx="1" fill="currentColor"/>',
    "keys": '<rect x="3" y="3" width="9" height="9" rx="2"/><rect x="12" y="12" width="9" height="9" rx="2"/>'
            '<path d="M15.5 4.5H17a3 3 0 0 1 3 3v1M8.5 19.5H7a3 3 0 0 1-3-3v-1"/><path d="M18.3 7.2L20 8.9l1.6-1.7'
            'M5.7 16.8L4 15.1l-1.6 1.7"/>',
    "macros": '<circle cx="6.5" cy="12" r="3" fill="currentColor"/><path d="M12.5 7h8.5M12.5 12h8.5M12.5 17h5.5"/>',
    "profiles": '<path d="M12 3.5l8.5 4.3-8.5 4.3-8.5-4.3z"/><path d="M3.5 12.2l8.5 4.3 8.5-4.3"/>'
                '<path d="M3.5 16.5l8.5 4.3 8.5-4.3"/>',
    "device": '<path d="M4 7h10M18 7h2M4 17h4M12 17h8"/><circle cx="16" cy="7" r="2"/><circle cx="10" cy="17" r="2"/>',
    # Actions
    "clock": '<circle cx="12" cy="12" r="8"/><path d="M12 7.5V12l3 2"/>',
    "upload": '<path d="M12 15V4M7.5 8.5L12 4l4.5 4.5"/><path d="M4 15v3.5A1.5 1.5 0 0 0 5.5 20h13a1.5 1.5 0 0 0 '
              '1.5-1.5V15"/>',
    "download": '<path d="M12 4v11M7.5 10.5L12 15l4.5-4.5"/><path d="M4 15v3.5A1.5 1.5 0 0 0 5.5 20h13a1.5 1.5 0 0 0 '
                '1.5-1.5V15"/>',
    "image": '<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><circle cx="9" cy="10" r="1.6"/>'
             '<path d="M4 17l5-4.5 4 3.5 3-2.5 4 3.5"/>',
    "folder": '<path d="M3 7.5A1.5 1.5 0 0 1 4.5 6H9l2 2h8.5A1.5 1.5 0 0 1 21 9.5v8a1.5 1.5 0 0 1-1.5 1.5h-15'
              'A1.5 1.5 0 0 1 3 17.5z"/>',
    "history": '<path d="M4.5 12a7.5 7.5 0 1 0 2.2-5.3"/><path d="M4.5 4.5v4h4"/><path d="M12 8.5V12l2.5 2"/>',
    "play": '<path d="M8 5.5v13l10-6.5z" fill="currentColor"/>',
    "pause": '<path d="M8.5 5.5v13M15.5 5.5v13"/>',
    "record": '<circle cx="12" cy="12" r="6" fill="currentColor"/>',
    "stop": '<rect x="7" y="7" width="10" height="10" rx="1.5" fill="currentColor"/>',
    "trash": '<path d="M4 7h16M10 11v6M14 11v6M6 7l1 12.5A1.5 1.5 0 0 0 8.5 21h7a1.5 1.5 0 0 0 1.5-1.5L18 7'
             'M9 7V4.5h6V7"/>',
    "plus": '<path d="M12 5v14M5 12h14"/>',
    "copy": '<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5.5A1.5 1.5 0 0 0 14.5 4h-9A1.5 1.5 0 0 0 '
            '4 5.5v9A1.5 1.5 0 0 0 5.5 16H8"/>',
    "edit": '<path d="M4 20h4L19 9l-4-4L4 16z"/><path d="M13.5 6.5l4 4"/>',
    "dropper": '<path d="M13.5 7.5l3 3M5 19l.8-3.7 8.7-8.8 3 3-8.8 8.7z"/><path d="M15 5l1.5-1.5a2.1 2.1 0 0 1 3 3L18 8"/>',
    "fill": '<path d="M5 11.5L11.5 5l7 7-6.5 6.5a1.5 1.5 0 0 1-2.1 0L5 13.6a1.5 1.5 0 0 1 0-2.1z"/>'
            '<path d="M5 12h13" /><path d="M20 15.5s1.5 2 1.5 3a1.5 1.5 0 0 1-3 0c0-1 1.5-3 1.5-3z" fill="currentColor"/>',
    "usb": '<path d="M12 3v14"/><path d="M9.5 5.5L12 3l2.5 2.5"/><circle cx="12" cy="19" r="2"/>'
           '<path d="M12 12.5l-4-2V8M12 14.5l4-2V10"/><rect x="15" y="8" width="2" height="2" fill="currentColor"/>'
           '<circle cx="8" cy="7.5" r="1" fill="currentColor"/>',
    "wireless": '<path d="M4.5 9.5a10.5 10.5 0 0 1 15 0M7.8 12.8a6 6 0 0 1 8.4 0"/>'
                '<circle cx="12" cy="16.5" r="1.5" fill="currentColor"/>',
    "unplugged": '<path d="M7 12h3M14 12h3M10 9v6M14 9v6M4 12h3M17 12h3"/><path d="M5 19L19 5"/>',
    "link": '<path d="M14 4h6v6M20 4l-9 9M18 14v4.5A1.5 1.5 0 0 1 16.5 20h-11A1.5 1.5 0 0 1 4 18.5v-11A1.5 1.5 0 0 1 '
            '5.5 6H10"/>',
    "check": '<path d="M5 12.5l4.5 4.5L19 7.5"/>',
    "up": '<path d="M6 14.5l6-6 6 6"/>',
    "down": '<path d="M6 9.5l6 6 6-6"/>',
    "warning": '<path d="M12 4l9 16H3z"/><path d="M12 10v4.5M12 17v.5"/>',
    "shield": '<path d="M12 3l7 3v5.5c0 4.3-3 7.7-7 9.5-4-1.8-7-5.2-7-9.5V6z"/><path d="M9 12l2 2 4-4"/>',
    "music": '<path d="M9 17V6l10-2v11"/><circle cx="6.5" cy="17" r="2.5"/><circle cx="16.5" cy="15" r="2.5"/>',
    "chip": '<rect x="6" y="6" width="12" height="12" rx="2"/><path d="M9 3v3M15 3v3M9 18v3M15 18v3M3 9h3M3 15h3'
            'M18 9h3M18 15h3"/>',
    "monitor": '<rect x="3" y="4.5" width="18" height="12" rx="1.5"/><path d="M8.5 20h7M12 16.5V20"/>',
    "power": '<path d="M12 3.5v8"/><path d="M7 6.5a7.5 7.5 0 1 0 10 0"/>',
    "language": '<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.5 2.3 3.5 5.2 3.5 8.5s-1 6.2-3.5 8.5'
                'c-2.5-2.3-3.5-5.2-3.5-8.5s1-6.2 3.5-8.5z"/>',
}


def svg(name: str, color: str) -> bytes:
    body = _PATHS[name].replace("currentColor", color)
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="{color}" '
            f'stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">{body}</svg>').encode()


@lru_cache(maxsize=512)
def pixmap(name: str, color: str = theme.INK, size: int = 20, ratio: float = 2.0) -> QPixmap:
    px = int(size * ratio)
    pm = QPixmap(px, px)
    pm.fill(Qt.GlobalColor.transparent)
    p = QPainter(pm)
    p.setRenderHint(QPainter.RenderHint.Antialiasing)
    QSvgRenderer(QByteArray(svg(name, color))).render(p, QRectF(0, 0, px, px))
    p.end()
    pm.setDevicePixelRatio(ratio)
    return pm


def icon(name: str, color: str = theme.INK, active: str | None = theme.AMBER,
         disabled: str = theme.INK_OFF, size: int = 20) -> QIcon:
    ic = QIcon()
    ic.addPixmap(pixmap(name, color, size), QIcon.Mode.Normal, QIcon.State.Off)
    ic.addPixmap(pixmap(name, disabled, size), QIcon.Mode.Disabled, QIcon.State.Off)
    if active:
        ic.addPixmap(pixmap(name, active, size), QIcon.Mode.Normal, QIcon.State.On)
        ic.addPixmap(pixmap(name, active, size), QIcon.Mode.Active, QIcon.State.On)
    return ic


APP_ICON = os.path.join(os.path.dirname(os.path.dirname(__file__)), "data",
                        "io.github.yellowhearth1.AjazzAK980V2.svg")


@lru_cache(maxsize=1)
def app_icon() -> QIcon:
    ic = QIcon()
    renderer = QSvgRenderer(APP_ICON)
    for size in (16, 22, 24, 32, 48, 64, 128, 256):
        pm = QPixmap(size, size)
        pm.fill(Qt.GlobalColor.transparent)
        p = QPainter(pm)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        renderer.render(p, QRectF(0, 0, size, size))
        p.end()
        ic.addPixmap(pm)
    return ic
