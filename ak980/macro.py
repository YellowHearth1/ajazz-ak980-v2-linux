"""Macros (opcodes 0x19 + 0x15).

Memory image: a directory of 100 × 4 bytes (offset LE16 + 00 00; FF FF FF FF for a macro without
steps), then from byte 400 one record per macro: [event count LE16][6 × 00][events]. Every step
becomes two 4-byte events: the action [00 00 CODE TYPE] (TYPE B0 key down, 30 key up, 90 mouse
down, 10 mouse up) and the pause after it [ms LE16 00 50] with ms >= 10. The keyboard receives
PAGES × 64 bytes with AA 55 at the end.
"""

from __future__ import annotations

from dataclasses import dataclass, field

from .i18n import tr
from .keymap import key_code, key_name

MAX_MACROS = 100
DIRECTORY_BYTES = 400
MIN_DELAY = 10
MAX_DELAY = 0xFFFF
BUDGET_BYTES = 0xA8C  # vendor limit: (steps + macros) × 8

_TYPES = {"down": 0xB0, "up": 0x30, "mdown": 0x90, "mup": 0x10}
_MOUSE = {"left": 1, "right": 2, "middle": 4}
MOUSE_BUTTON_LABELS = {1: "Left button", 2: "Right button", 4: "Middle button"}


@dataclass
class MacroStep:
    action: str          # down | up | mdown | mup
    code: int            # HID key usage, or mouse button mask (1 left, 2 right, 4 middle)
    delay: int = MIN_DELAY

    @property
    def pressed(self) -> bool:
        return self.action in ("down", "mdown")

    def target(self) -> str:
        if self.action in ("down", "up"):
            return key_name(self.code)
        return tr(MOUSE_BUTTON_LABELS.get(self.code, "Button {n}"), n=self.code)

    def describe(self) -> str:
        verb = tr("press") if self.pressed else tr("release")
        return tr("{verb} {target}, wait {ms} ms", verb=verb, target=self.target(), ms=self.delay)

    def to_list(self) -> list:
        return [self.action, self.code, self.delay]


@dataclass
class Macro:
    name: str = "Macro"
    steps: list[MacroStep] = field(default_factory=list)

    def to_dict(self) -> dict:
        return {"name": self.name, "steps": [s.to_list() for s in self.steps]}

    @classmethod
    def from_dict(cls, d: dict) -> Macro:
        return cls(d.get("name", "Macro"), [MacroStep(*s) for s in d.get("steps", [])])

    def duration_ms(self) -> int:
        return sum(s.delay for s in self.steps)

    @staticmethod
    def text_steps(text: str, delay: int = MIN_DELAY) -> list[MacroStep]:
        """Text in the US layout: Shift is wrapped around characters that need it."""
        steps = []
        for ch in text.replace("\r\n", "\n"):
            r = _char_to_hid(ch)
            if r is None:
                continue
            shift, code = r
            if shift:
                steps.append(MacroStep("down", 0xE1, delay))
            steps += [MacroStep("down", code, delay), MacroStep("up", code, delay)]
            if shift:
                steps.append(MacroStep("up", 0xE1, delay))
        return steps

    @staticmethod
    def combo_steps(combo: str, delay: int = MIN_DELAY) -> list[MacroStep]:
        codes = [key_code(p) for p in combo.split("+") if p.strip()]
        return ([MacroStep("down", c, delay) for c in codes] +
                [MacroStep("up", c, delay) for c in reversed(codes)])

    @classmethod
    def parse(cls, spec: str, name: str | None = None) -> Macro:
        """text:hello | combo:ctrl+shift+esc | steps:down:A,wait:50,up:A,tap:B,click:left"""
        kind, _, rest = spec.partition(":")
        m = cls(name or spec)
        kind = kind.lower()
        if kind == "text":
            m.steps = cls.text_steps(rest)
        elif kind == "combo":
            m.steps = cls.combo_steps(rest)
        elif kind == "steps":
            for part in (p.strip() for p in rest.split(",") if p.strip()):
                op, _, arg = part.partition(":")
                op = op.lower()
                if op in ("down", "up"):
                    m.steps.append(MacroStep(op, key_code(arg)))
                elif op == "tap":
                    c = key_code(arg)
                    m.steps += [MacroStep("down", c), MacroStep("up", c)]
                elif op == "click":
                    b = _MOUSE.get(arg.lower())
                    if b is None:
                        raise ValueError(tr("Unknown mouse button: {name}", name=arg))
                    m.steps += [MacroStep("mdown", b), MacroStep("mup", b)]
                elif op == "wait":
                    if not m.steps:
                        raise ValueError(tr("“wait” must follow a step"))
                    m.steps[-1].delay = int(arg)
                else:
                    raise ValueError(tr("Unknown macro step: {step}", step=part))
        else:
            raise ValueError(tr("A macro is text:..., combo:... or steps:..."))
        return m


def used_bytes(macros: list[Macro]) -> int:
    return (sum(len(m.steps) for m in macros) + len(macros)) * 8


def encode_macros(macros: list[Macro]) -> tuple[bytes, int]:
    if len(macros) > MAX_MACROS:
        raise ValueError(tr("The keyboard holds at most {n} macros", n=MAX_MACROS))
    if used_bytes(macros) > BUDGET_BYTES:
        raise ValueError(tr("Macros need {used} of {total} bytes of keyboard memory; remove some steps",
                            used=used_bytes(macros), total=BUDGET_BYTES))
    img = bytearray(DIRECTORY_BYTES + BUDGET_BYTES + 128)
    offset = DIRECTORY_BYTES
    for i, m in enumerate(macros):
        if not m.steps:
            img[i * 4:i * 4 + 4] = b"\xFF\xFF\xFF\xFF"
            continue
        events = len(m.steps) * 2
        img[i * 4:i * 4 + 2] = offset.to_bytes(2, "little")
        img[offset:offset + 2] = events.to_bytes(2, "little")
        p = offset + 8
        for s in m.steps:
            img[p + 2] = s.code
            img[p + 3] = _TYPES[s.action]
            ms = max(MIN_DELAY, min(MAX_DELAY, s.delay))
            img[p + 4:p + 6] = ms.to_bytes(2, "little")
            img[p + 7] = 0x50
            p += 8
        offset = p
    pages = offset // 64 + (1 if offset % 64 == 0 else 2)
    data = bytearray(pages * 64)
    data[:offset] = img[:offset]
    data[-2:] = b"\xAA\x55"
    return bytes(data), pages


def _char_to_hid(ch: str) -> tuple[bool, int] | None:
    if "a" <= ch <= "z":
        return False, 0x04 + ord(ch) - ord("a")
    if "A" <= ch <= "Z":
        return True, 0x04 + ord(ch) - ord("A")
    if "1" <= ch <= "9":
        return False, 0x1E + ord(ch) - ord("1")
    plain = {"0": 0x27, "\n": 0x28, "\t": 0x2B, " ": 0x2C, "-": 0x2D, "=": 0x2E, "[": 0x2F, "]": 0x30,
             "\\": 0x31, ";": 0x33, "'": 0x34, "`": 0x35, ",": 0x36, ".": 0x37, "/": 0x38}
    if ch in plain:
        return False, plain[ch]
    shifted = ")!@#$%^&*(_+{}|:\"~<>?"
    base = "0123456789-=[]\\;'`,./"
    i = shifted.find(ch)
    if i >= 0:
        r = _char_to_hid(base[i])
        return (True, r[1]) if r else None
    return None
