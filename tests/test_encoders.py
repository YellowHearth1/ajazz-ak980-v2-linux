import pytest
from PIL import Image

from ak980 import lcd
from ak980.keymap import KeyAction, encode_layer
from ak980.macro import BUDGET_BYTES, Macro, encode_macros, used_bytes


@pytest.mark.parametrize("spec, entry", [
    ("key:ctrl+shift+a", "02 03 04 00"),
    ("key:ctrl", "02 01 00 00"),
    ("key:win+l", "02 08 0f 00"),
    ("mouse:wheeldown", "01 03 ff 00"),
    ("mouse:btn4", "01 01 10 00"),
    ("media:volup", "03 e9 00 00"),
    ("media:calc", "03 92 01 00"),
    ("macro:2:repeat:5", "06 02 01 05"),
    ("macro:1:hold", "06 01 03 00"),
    ("multi:a,b,c", "07 04 05 06"),
    ("off", "05 03 00 00"),
    ("default", "00 00 00 00"),
])
def test_key_entries(spec, entry):
    a = KeyAction.parse(spec)
    assert a.to_entry().hex(" ") == entry
    assert KeyAction.parse(a.to_spec()).to_entry() == a.to_entry()


def test_layer_table():
    t = encode_layer({1: KeyAction.parse("off"), 200: KeyAction.parse("off")})
    assert len(t) == 576 and t[4:8] == bytes([5, 3, 0, 0]) and t[-2:] == b"\xAA\x55"


def test_macro_image():
    data, pages = encode_macros([Macro.parse("text:Hi"), Macro("empty")])
    assert len(data) == pages * 64 and data[-2:] == b"\xAA\x55"
    assert data[0:2] == (400).to_bytes(2, "little")          # first macro starts after the directory
    assert data[4:8] == b"\xFF\xFF\xFF\xFF"                   # a macro without steps
    assert data[400] == 12                                    # 6 steps -> 12 events
    assert data[408 + 3] == 0xB0 and data[408 + 7] == 0x50    # key down, then its pause


def test_macro_budget():
    big = Macro("big", Macro.text_steps("x" * 400))
    assert used_bytes([big]) > BUDGET_BYTES
    with pytest.raises(ValueError):
        encode_macros([big])


def test_lcd_blob():
    red = Image.new("RGB", (240, 135), (255, 0, 0))
    blob = lcd.encode_animation([(red, 100), (red, 40)], dither=False)
    assert len(blob) % 4096 == 0
    assert blob[0] == 2 and blob[1] == 50 and blob[2] == 20
    assert blob[256:258] == (0xF800).to_bytes(2, "little")
    assert lcd.page_count(2) == len(blob) // 4096


def test_lcd_preview_roundtrip():
    img = Image.new("RGB", (240, 135), (0, 255, 0))
    assert lcd.preview_565(img, dither=False).getpixel((10, 10)) == (0, 255, 0)


def test_lcd_frame_selection(tmp_path):
    frames = [Image.new("RGB", (10, 10), (i * 20, 0, 0)) for i in range(10)]
    path = tmp_path / "a.gif"
    frames[0].save(path, save_all=True, append_images=frames[1:], duration=50, loop=0)
    src = lcd.Source.load(str(path))
    o = lcd.RenderOptions(frame_step=2, trim_start=1)
    picked = src.select(o)
    assert [i for i, _ in picked] == [1, 3, 5, 7, 9]
    assert picked[0][1] == 100                                # two 50 ms frames merged
    o.deleted.add(3)
    o.delays[5] = 300
    assert [(i, d) for i, d in src.select(o)][1:3] == [(5, 300), (7, 100)]
