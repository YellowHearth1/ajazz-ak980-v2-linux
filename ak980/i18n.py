"""Interface translations.

Source strings are English and are passed through ``tr()``. Each language is a plain dict in
``ak980/locale/<code>.py`` mapping the English string to its translation; a missing entry falls
back to English. ``tests/test_i18n.py`` checks that every ``tr("...")`` literal is translated.
"""

from __future__ import annotations

import importlib
import locale
import os

LANGUAGES = {"en": "English", "uk": "Українська"}

_lang = "en"
_catalog: dict[str, str] = {}


def _locale_conf() -> dict[str, str]:
    """LANG and friends from locale.conf: services often start without them in the environment."""
    values: dict[str, str] = {}
    for path in ("/etc/locale.conf", os.path.join(os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config")),
                                                  "locale.conf")):
        try:
            with open(path, encoding="utf-8") as f:
                for line in f:
                    key, _, value = line.strip().partition("=")
                    if key and value:
                        values[key] = value.strip('"')
        except OSError:
            continue
    return values


def detect() -> str:
    """Language from the environment (LANGUAGE, LC_ALL, LC_MESSAGES, LANG) or locale.conf, else English."""
    conf = None
    for var in ("LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"):
        value = os.environ.get(var, "")
        if not value:
            conf = _locale_conf() if conf is None else conf
            value = conf.get(var, "")
        for part in value.split(":"):
            code = part.split("_")[0].split(".")[0].lower()
            if code in LANGUAGES:
                return code
    try:
        loc = locale.getlocale()[0] or ""
    except ValueError:
        loc = ""
    code = loc.split("_")[0].lower()
    return code if code in LANGUAGES else "en"


def set_language(code: str | None) -> str:
    """Selects the interface language; ``None`` or "auto" follows the environment."""
    global _lang, _catalog
    if not code or code == "auto":
        code = detect()
    if code not in LANGUAGES:
        code = "en"
    _lang = code
    if code == "en":
        _catalog = {}
    else:
        _catalog = importlib.import_module(f"ak980.locale.{code}").MESSAGES
    return code


def language() -> str:
    return _lang


def decimal(value: float, digits: int = 1) -> str:
    """A number with the decimal separator of the interface language (1.5 / 1,5)."""
    text = f"{value:.{digits}f}"
    return text.replace(".", ",") if _lang == "uk" else text


def tr(text: str, **kwargs) -> str:
    """Translates ``text``; keyword arguments are substituted with ``str.format``."""
    out = _catalog.get(text, text)
    return out.format(**kwargs) if kwargs else out


def plural(n: int, one: str, other: str) -> str:
    """Count-dependent text, e.g. ``plural(3, "{n} frame", "{n} frames")``.

    A catalog stores the forms under the English singular as a tuple. Ukrainian uses three forms
    (1 кадр, 2 кадри, 5 кадрів) chosen by the usual Slavic rule.
    """
    forms = _catalog.get(one)
    if isinstance(forms, tuple) and len(forms) == 3:
        n10, n100 = n % 10, n % 100
        if n10 == 1 and n100 != 11:
            form = forms[0]
        elif 2 <= n10 <= 4 and not 12 <= n100 <= 14:
            form = forms[1]
        else:
            form = forms[2]
    else:
        form = one if n == 1 else other
    return form.format(n=n)
