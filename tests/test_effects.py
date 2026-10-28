import numpy as np
import pytest

from ak980 import effects
from ak980.layout import positions


@pytest.mark.parametrize("effect_id", list(effects.EFFECTS))
def test_effect_renders(effect_id):
    leds, pos = positions()
    e = effects.create(effect_id, pos)
    for i in range(40):
        inp = effects.Inputs(presses=[3] if i % 7 == 0 else [], bands=np.linspace(0, 1, 16, dtype=np.float32),
                             level=0.6)
        out = e.render(i / 30, 1 / 30, inp)
    assert out.shape == (len(leds), 3)
    assert np.isfinite(out).all() and out.min() >= -1e-6 and out.max() <= 1 + 1e-6


def test_params_have_defaults_and_labels():
    for e in effects.EFFECTS.values():
        assert e.name and e.description
        for p in e.params:
            assert p.label and p.kind in ("color", "range", "choice", "toggle")
            if p.kind == "choice":
                assert p.default in [c[0] for c in p.choices]


def test_to_bytes():
    leds, pos = positions()
    colors = np.ones((len(leds), 3), np.float32)
    table = effects.to_bytes(colors, leds, brightness=0.5)
    assert len(table) == 512
    i = leds[0]
    assert table[i * 4:i * 4 + 4] == bytes([i, 128, 128, 128])
