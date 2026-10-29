"""Design tokens, fonts and the application stylesheet. docs/DESIGN.md explains the choices."""

from __future__ import annotations

import os

from PyQt6.QtGui import QColor, QFont, QFontDatabase

# Colours: the keyboard's own materials. The window is the desk, panels are the case,
# controls are keycaps, text is legend white; one amber signal marks action, selection and focus.
DESK = "#191A1E"
CASE = "#212328"
PLATE = "#1B1C20"
KEYCAP = "#2B2E35"
KEYCAP_HI = "#363A44"
KEYCAP_LO = "#24262C"
EDGE = "#383C46"
INK = "#ECE7DF"
INK_DIM = "#A39F98"
INK_OFF = "#6F6C67"          # disabled text only
AMBER = "#FF8F1F"
AMBER_HI = "#FFA548"
AMBER_LO = "#D9730F"
ON_AMBER = "#1A1408"
OK = "#5BD08F"
WARN = "#F2C14E"
ERROR = "#FF6B5E"

FONT_BODY = "Ubuntu"
FONT_DISPLAY = "Ubuntu"
FONT_MONO = "Ubuntu Mono"
# Used when the plain Ubuntu family is missing but a patched one is installed
FALLBACK = {"Ubuntu": ["Ubuntu Nerd Font", "Ubuntu Nerd Font Propo"],
            "Ubuntu Mono": ["UbuntuMono Nerd Font", "UbuntuMono Nerd Font Mono", "DejaVu Sans Mono"]}
FONT_DIR = os.path.join(os.path.dirname(os.path.dirname(__file__)), "data", "fonts")

# Type scale (pixels)
SIZE_BODY = 14
SIZE_SMALL = 13
SIZE_TITLE = 22
SIZE_HEADING = 17
SIZE_FIGURE = 30

# Spacing scale (pixels)
S1, S2, S3, S4, S5 = 4, 8, 12, 20, 32


def assets() -> str:
    """Small SVGs the stylesheet refers to (Qt style sheets only take files for arrow images)."""
    base = os.path.join(os.environ.get("XDG_CACHE_HOME", os.path.expanduser("~/.cache")), "ak980", "qss")
    os.makedirs(base, exist_ok=True)
    shapes = {"up": "M2 7l4-4 4 4", "down": "M2 4l4 4 4-4"}
    for name, d in shapes.items():
        for state, colour in (("", INK_DIM), ("-hover", INK), ("-off", INK_OFF)):
            path = os.path.join(base, f"chevron-{name}{state}.svg")
            svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 11" width="12" height="11">'
                   f'<path d="{d}" fill="none" stroke="{colour}" stroke-width="1.6" stroke-linecap="round" '
                   f'stroke-linejoin="round"/></svg>')
            try:
                with open(path) as f:
                    if f.read() == svg:
                        continue
            except OSError:
                pass
            with open(path, "w") as f:
                f.write(svg)
    return base.replace(os.sep, "/")


def color(hex_value: str, alpha: float = 1.0) -> QColor:
    c = QColor(hex_value)
    c.setAlphaF(alpha)
    return c


def families(name: str) -> list[str]:
    return [name] + FALLBACK.get(name, [])


def css_family(name: str) -> str:
    return ", ".join(f'"{f}"' for f in families(name)) + (", monospace" if "Mono" in name else ", sans-serif")


def load_fonts() -> None:
    """Uses the system's Ubuntu fonts when installed, the bundled copy otherwise."""
    have = set(QFontDatabase.families())
    if {FONT_BODY, FONT_MONO} <= have:
        return
    if os.path.isdir(FONT_DIR):
        for name in sorted(os.listdir(FONT_DIR)):
            if name.endswith(".ttf"):
                QFontDatabase.addApplicationFont(os.path.join(FONT_DIR, name))


def font(role: str = "body", size: int | None = None, weight: QFont.Weight | None = None) -> QFont:
    family, default_size, default_weight = {
        "body": (FONT_BODY, SIZE_BODY, QFont.Weight.Normal),
        "small": (FONT_BODY, SIZE_SMALL, QFont.Weight.Normal),
        "label": (FONT_BODY, SIZE_SMALL, QFont.Weight.Medium),
        "title": (FONT_DISPLAY, SIZE_TITLE, QFont.Weight.Bold),
        "heading": (FONT_DISPLAY, SIZE_HEADING, QFont.Weight.Medium),
        "figure": (FONT_DISPLAY, SIZE_FIGURE, QFont.Weight.Bold),
        "mono": (FONT_MONO, SIZE_BODY, QFont.Weight.Normal),
        "legend": (FONT_BODY, 11, QFont.Weight.Medium),
    }[role]
    f = QFont()
    f.setFamilies(families(family))
    f.setPixelSize(size or default_size)
    f.setWeight(weight or default_weight)
    f.setHintingPreference(QFont.HintingPreference.PreferNoHinting)
    return f


def stylesheet() -> str:
    a = assets()
    return f"""
* {{ outline: none; }}
QWidget {{
    color: {INK};
    font-family: {css_family(FONT_BODY)};
    font-size: {SIZE_BODY}px;
    selection-background-color: {AMBER};
    selection-color: {ON_AMBER};
}}
QMainWindow, #desk {{ background: {DESK}; }}
#panel, QDialog, #panel QScrollArea, #panel QScrollArea > QWidget > QWidget {{ background: {CASE}; }}
QScrollArea {{ border: none; background: transparent; }}
QToolTip {{
    background: {KEYCAP}; color: {INK}; border: 1px solid {EDGE};
    padding: 5px 8px; border-radius: 4px;
}}
QLabel[role="dim"] {{ color: {INK_DIM}; }}
QLabel[role="label"] {{ color: {INK_DIM}; font-size: {SIZE_SMALL}px; font-weight: 500; }}
QLabel[role="heading"] {{ font-family: {css_family(FONT_DISPLAY)}; font-size: {SIZE_HEADING}px; font-weight: 500; }}
QLabel[role="title"] {{ font-family: {css_family(FONT_DISPLAY)}; font-size: {SIZE_TITLE}px; font-weight: 700; }}
QLabel[role="mono"] {{ font-family: {css_family(FONT_MONO)}; font-size: {SIZE_BODY}px; color: {INK_DIM}; }}
QLabel[role="warn"] {{ color: {WARN}; }}
QLabel[role="error"] {{ color: {ERROR}; }}
QLabel:disabled {{ color: {INK_OFF}; }}
QFrame[role="rule"] {{ background: {EDGE}; border: none; }}

QPushButton {{
    background: {KEYCAP}; color: {INK};
    border: 1px solid {EDGE}; border-bottom-color: #2A2D35;
    border-radius: 6px; padding: 7px 14px; min-height: 18px;
}}
QPushButton:hover {{ background: {KEYCAP_HI}; }}
QPushButton:pressed {{ background: {KEYCAP_LO}; padding-top: 8px; padding-bottom: 6px; }}
QPushButton:focus {{ border-color: {AMBER}; }}
QPushButton:disabled {{ color: {INK_OFF}; background: {KEYCAP_LO}; border-color: {KEYCAP}; }}
QPushButton:checked {{ background: {KEYCAP_HI}; color: {AMBER}; border-color: {AMBER}; }}
QPushButton[kind="primary"] {{
    background: {AMBER}; color: {ON_AMBER}; border: 1px solid {AMBER_LO};
    font-weight: 600; padding: 8px 18px;
}}
QPushButton[kind="primary"]:hover {{ background: {AMBER_HI}; }}
QPushButton[kind="primary"]:pressed {{ background: {AMBER_LO}; }}
QPushButton[kind="primary"]:focus {{ border-color: {INK}; }}
QPushButton[kind="primary"]:disabled {{ background: #5A4128; color: #2A2116; border-color: #5A4128; }}
QPushButton[kind="ghost"], QPushButton[kind="danger"] {{ background: transparent; border-color: transparent;
    color: {INK_DIM}; padding: 6px 10px; }}
QPushButton[kind="ghost"]:hover {{ background: {KEYCAP}; color: {INK}; }}
QPushButton[kind="danger"]:hover {{ background: {KEYCAP}; color: {ERROR}; }}
QPushButton[kind="ghost"]:focus, QPushButton[kind="danger"]:focus {{ border-color: {AMBER}; }}

QPushButton[segment] {{ border-radius: 0; padding: 6px 12px; margin: 0; }}
QPushButton[segment="first"] {{ border-top-left-radius: 6px; border-bottom-left-radius: 6px; }}
QPushButton[segment="last"] {{ border-top-right-radius: 6px; border-bottom-right-radius: 6px; }}
QPushButton[segment="only"] {{ border-radius: 6px; }}
QPushButton[segment]:checked {{ background: {AMBER}; color: {ON_AMBER}; border-color: {AMBER_LO}; font-weight: 600; }}

QLineEdit, QSpinBox, QDoubleSpinBox, QComboBox, QPlainTextEdit {{
    background: {PLATE}; border: 1px solid {EDGE}; border-radius: 5px;
    padding: 5px 8px; min-height: 18px;
}}
QLineEdit:focus, QSpinBox:focus, QDoubleSpinBox:focus, QComboBox:focus, QPlainTextEdit:focus {{
    border-color: {AMBER};
}}
QLineEdit:disabled, QSpinBox:disabled, QComboBox:disabled {{ color: {INK_OFF}; }}
QLineEdit[role="mono"] {{ font-family: {css_family(FONT_MONO)}; font-size: {SIZE_BODY}px; }}
QSpinBox::up-button, QSpinBox::down-button, QDoubleSpinBox::up-button, QDoubleSpinBox::down-button {{
    width: 18px; border: none; background: transparent; subcontrol-origin: border;
}}
QSpinBox::up-button, QDoubleSpinBox::up-button {{ subcontrol-position: top right; margin-top: 2px; }}
QSpinBox::down-button, QDoubleSpinBox::down-button {{ subcontrol-position: bottom right; margin-bottom: 2px; }}
QSpinBox::up-arrow, QDoubleSpinBox::up-arrow {{ image: url({a}/chevron-up.svg); width: 10px; height: 9px; }}
QSpinBox::down-arrow, QDoubleSpinBox::down-arrow {{ image: url({a}/chevron-down.svg); width: 10px; height: 9px; }}
QSpinBox::up-arrow:hover, QDoubleSpinBox::up-arrow:hover {{ image: url({a}/chevron-up-hover.svg); }}
QSpinBox::down-arrow:hover, QDoubleSpinBox::down-arrow:hover {{ image: url({a}/chevron-down-hover.svg); }}
QSpinBox::up-arrow:disabled, QSpinBox::up-arrow:off, QDoubleSpinBox::up-arrow:disabled {{
    image: url({a}/chevron-up-off.svg);
}}
QSpinBox::down-arrow:disabled, QSpinBox::down-arrow:off, QDoubleSpinBox::down-arrow:disabled {{
    image: url({a}/chevron-down-off.svg);
}}
QComboBox::drop-down {{ border: none; width: 24px; }}
QComboBox::down-arrow {{ image: url({a}/chevron-down.svg); width: 10px; height: 9px; }}
QComboBox::down-arrow:hover {{ image: url({a}/chevron-down-hover.svg); }}
QComboBox::down-arrow:disabled {{ image: url({a}/chevron-down-off.svg); }}
QComboBox QAbstractItemView {{
    background: {KEYCAP}; border: 1px solid {EDGE}; selection-background-color: {KEYCAP_HI};
    selection-color: {AMBER}; padding: 4px; outline: none;
}}

QSlider::groove:horizontal {{ height: 4px; background: {KEYCAP}; border-radius: 2px; }}
QSlider::sub-page:horizontal {{ background: {AMBER}; border-radius: 2px; }}
QSlider::sub-page:horizontal:disabled {{ background: {EDGE}; }}
QSlider::handle:horizontal {{
    background: {INK}; width: 14px; height: 14px; margin: -6px 0; border-radius: 7px;
    border: 2px solid {CASE};
}}
QSlider::handle:horizontal:hover {{ background: #FFFFFF; }}
QSlider::handle:horizontal:focus {{ background: {AMBER}; }}
QSlider::handle:horizontal:disabled {{ background: {INK_OFF}; }}

QListWidget, QTreeWidget, QTableWidget {{
    background: {PLATE}; border: 1px solid {EDGE}; border-radius: 6px; padding: 4px;
    alternate-background-color: {PLATE};
}}
QListWidget::item, QTreeWidget::item {{ padding: 6px 8px; border-radius: 4px; }}
QListWidget::item:hover, QTreeWidget::item:hover {{ background: {KEYCAP}; }}
QListWidget::item:selected, QTreeWidget::item:selected {{ background: {KEYCAP_HI}; color: {AMBER}; }}
QTableWidget {{ gridline-color: {KEYCAP}; }}
QTableWidget QSpinBox, QTableWidget QComboBox {{
    background: transparent; border: 1px solid transparent; border-radius: 4px; padding: 1px 6px; min-height: 0;
}}
QTableWidget QSpinBox:hover, QTableWidget QComboBox:hover {{ border-color: {EDGE}; }}
QTableWidget QSpinBox:focus, QTableWidget QComboBox:focus {{ border-color: {AMBER}; }}
QTableWidget::item {{ padding: 4px 6px; }}
QTableWidget::item:selected {{ background: {KEYCAP_HI}; color: {AMBER}; }}
QHeaderView::section {{
    background: {CASE}; color: {INK_DIM}; border: none; border-bottom: 1px solid {EDGE};
    padding: 6px; font-size: {SIZE_SMALL}px;
}}
QTableCornerButton::section {{ background: {CASE}; border: none; }}

QProgressBar {{
    background: {PLATE}; border: 1px solid {EDGE}; border-radius: 5px; height: 10px;
    text-align: center; color: {INK}; font-size: {SIZE_SMALL}px;
}}
QProgressBar::chunk {{ background: {AMBER}; border-radius: 4px; }}

QScrollBar:vertical {{ background: transparent; width: 10px; margin: 2px; }}
QScrollBar::handle:vertical {{ background: {KEYCAP_HI}; border-radius: 3px; min-height: 30px; }}
QScrollBar:horizontal {{ background: transparent; height: 10px; margin: 2px; }}
QScrollBar::handle:horizontal {{ background: {KEYCAP_HI}; border-radius: 3px; min-width: 30px; }}
QScrollBar::add-line, QScrollBar::sub-line {{ width: 0; height: 0; }}
QScrollBar::add-page, QScrollBar::sub-page {{ background: transparent; }}

QMenu {{ background: {KEYCAP}; border: 1px solid {EDGE}; padding: 4px; }}
QMenu::item {{ padding: 6px 22px 6px 12px; border-radius: 4px; }}
QMenu::item:selected {{ background: {KEYCAP_HI}; color: {AMBER}; }}
QMenu::separator {{ height: 1px; background: {EDGE}; margin: 4px 6px; }}

QMessageBox, QInputDialog, QColorDialog, QFileDialog {{ background: {CASE}; }}
QCheckBox {{ spacing: 8px; }}
QCheckBox::indicator {{
    width: 16px; height: 16px; border-radius: 4px; border: 1px solid {EDGE}; background: {PLATE};
}}
QCheckBox::indicator:checked {{ background: {AMBER}; border-color: {AMBER_LO}; }}
QCheckBox:focus {{ color: {AMBER}; }}
"""
