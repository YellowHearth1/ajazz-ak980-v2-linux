"""Pages of the main window. Each one says how the keyboard stage should look while it is open."""

from __future__ import annotations

from PyQt6.QtWidgets import QHBoxLayout, QVBoxLayout, QWidget

from ...config import Profile
from .. import theme
from ..widgets import rule


class Page(QWidget):
    title = ""
    icon = ""
    stage_mode: str | None = "board"     # "board", "screen" (zoomed on the display) or None (hidden)

    def __init__(self, win):
        super().__init__()
        self.win = win
        self.setObjectName("panel")
        self.root = QVBoxLayout(self)
        self.root.setContentsMargins(theme.S4, theme.S4, theme.S4, theme.S4)
        self.root.setSpacing(theme.S4)

    @property
    def cfg(self):
        return self.win.cfg

    @property
    def profile(self) -> Profile:
        return self.win.cfg.profile

    @property
    def stage(self):
        return self.win.stage

    def columns(self, *layouts, stretches=None) -> QHBoxLayout:
        """Lays sections out side by side, separated by thin rules."""
        row = QHBoxLayout()
        row.setSpacing(theme.S4)
        for i, lay in enumerate(layouts):
            if i:
                row.addWidget(rule(vertical=True))
            lay.setContentsMargins(0, 0, 0, 0)    # or Qt's default margins shift the column down
            holder = QWidget()
            holder.setLayout(lay)
            row.addWidget(holder, (stretches or [1] * len(layouts))[i])
        return row

    # Hooks
    def on_enter(self) -> None:
        pass

    def on_leave(self) -> None:
        pass

    def on_close(self) -> None:
        pass

    def refresh(self) -> None:
        """The active profile changed: reload the controls."""

    def on_stage_key(self, key, mouse_button) -> None:
        pass

    def wants_live_colors(self) -> bool:
        return False

    def connection_changed(self, state: str | None) -> None:
        pass
