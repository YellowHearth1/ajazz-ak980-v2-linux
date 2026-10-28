"""Every interface string has a Ukrainian translation with the same placeholders."""

import ast
import pathlib
import re

from ak980 import i18n
from ak980.locale.uk import MESSAGES

ROOT = pathlib.Path(__file__).resolve().parent.parent / "ak980"


def source_strings():
    found, plurals = set(), set()
    for path in ROOT.rglob("*.py"):
        if "locale" in path.parts:
            continue
        for node in ast.walk(ast.parse(path.read_text())):
            if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
                args = node.args
                if node.func.id == "tr" and args and isinstance(args[0], ast.Constant) and isinstance(args[0].value, str):
                    found.add(args[0].value)
                if node.func.id == "plural" and len(args) >= 2 and isinstance(args[1], ast.Constant):
                    plurals.add(args[1].value)
    return found, plurals


def data_strings():
    from ak980 import effects, keymap, lighting, macro
    out = [m.name for m in lighting.LIGHT_MODES] + lighting.DIRECTIONS + lighting.SLEEP_CHOICES
    out += [n for n, _ in keymap.MEDIA_KEYS] + keymap.MOUSE_LABELS + keymap.MACRO_MODE_LABELS
    out += list(macro.MOUSE_BUTTON_LABELS.values())
    for e in effects.EFFECTS.values():
        out += [e.name, e.description] + [p.label for p in e.params] + [c[1] for p in e.params for c in p.choices]
    return out


def test_everything_is_translated():
    found, plurals = source_strings()
    missing = sorted(s for s in found | set(data_strings()) if s not in MESSAGES)
    assert not missing, missing
    for one in plurals:
        assert isinstance(MESSAGES.get(one), tuple), one


def test_placeholders_match():
    for key, value in MESSAGES.items():
        for v in value if isinstance(value, tuple) else (value,):
            assert set(re.findall(r"{(\w*)}", key)) == set(re.findall(r"{(\w*)}", v)), key


def test_plural_forms():
    i18n.set_language("uk")
    try:
        assert i18n.plural(1, "{n} frame", "{n} frames") == "1 кадр"
        assert i18n.plural(3, "{n} frame", "{n} frames") == "3 кадри"
        assert i18n.plural(11, "{n} frame", "{n} frames") == "11 кадрів"
        assert i18n.plural(22, "{n} frame", "{n} frames") == "22 кадри"
    finally:
        i18n.set_language("en")
    assert i18n.plural(1, "{n} frame", "{n} frames") == "1 frame"
    assert i18n.plural(2, "{n} frame", "{n} frames") == "2 frames"
