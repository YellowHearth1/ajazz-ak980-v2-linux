import datetime as dt

from ak980.device import Dongle, clock_payload
from ak980.keymap import KeyAction
from ak980.lighting import KeyboardOptions, LightSettings
from ak980.macro import Macro


def op(packet: bytes) -> tuple[int, int]:
    return packet[0], packet[1]


def test_clock_payload_layout():
    p = clock_payload(dt.datetime(2026, 10, 28, 13, 35, 33), slot=1)
    # SLOT 5A YY MM DD hh mm ss 00 DOW; 28 October 2026 is a Wednesday (3, Sunday = 0)
    assert p == bytes([1, 0x5A, 26, 10, 28, 13, 35, 33, 0, 3])


def test_sync_clock_sequence(keyboard, commands):
    keyboard.sync_clock(dt.datetime(2026, 10, 28, 13, 35, 33))
    sent = commands()
    assert [op(p) for p in sent] == [(4, 0x18), (4, 0x28), (0, 1), (4, 0x02)]
    assert sent[1][8] == 1                      # one data packet follows
    assert sent[2][1:11] == clock_payload(dt.datetime(2026, 10, 28, 13, 35, 33), 1)
    assert sent[2][62:64] == b"\xAA\x55"
    assert all(len(f) == 65 and f[0] == 0 for f in keyboard._cmd.features)


def test_lighting_sequence(keyboard, commands):
    keyboard.set_lighting(LightSettings(mode=7, r=1, g=2, b=3, brightness=4, speed=2, direction=3))
    sent = commands()
    assert [op(p) for p in (sent[0], sent[1], sent[3], sent[4])] == [(4, 0x18), (4, 0x13), (4, 0x02), (4, 0xF0)]
    data = sent[2]
    assert data[:4] == bytes([7, 1, 2, 3]) and data[9:12] == bytes([4, 2, 3]) and data[14:16] == b"\xAA\x55"


def test_realtime_frame(keyboard):
    keyboard.send_realtime({1: (10, 20, 30), 127: (1, 2, 3)}, gap=0)
    f = keyboard._cmd.features
    assert len(f) == 10
    assert f[0][1:3] == bytes([4, 0x20]) and f[0][9] == 8
    table = b"".join(x[1:] for x in f[1:9])
    assert table[4:8] == bytes([1, 10, 20, 30])
    assert table[127 * 4:128 * 4] == bytes([127, 1, 2, 3])
    assert f[9][1:3] == bytes([4, 0x02])


def test_keymap_and_options(keyboard, commands):
    keyboard.set_keymap(True, {5: KeyAction.parse("media:mute")})
    sent = commands()
    assert [op(p)[1] for p in sent[:2]] == [0x18, 0x27]
    assert sent[1][8] == 9
    table = b"".join(sent[2:11])
    assert table[5 * 4:5 * 4 + 4] == bytes([3, 0xE2, 0, 0]) and table[-2:] == b"\xAA\x55"
    keyboard._cmd.features.clear()
    keyboard.set_options(KeyboardOptions(disable_win=True, sleep_time=3, key_response=4))
    sent = commands()
    assert [op(p)[1] for p in sent] == [0x18, 0x17, 0x01, 0x02]
    assert sent[2][4] == 1 and sent[2][6] == 3 and sent[2][8] == 4


def test_custom_colors(keyboard, commands):
    keyboard.set_custom_colors({1: (255, 0, 0)}, brightness=3)
    sent = commands()
    assert sent[2][0] == 0x80 and sent[2][9] == 3
    i = [n for n, p in enumerate(sent) if op(p) == (4, 0x23)][0]
    assert sent[i][8] == 9
    table = b"".join(sent[i + 1:i + 10])
    assert table[4:8] == bytes([1, 255, 0, 0])


def test_macros(keyboard, commands):
    keyboard.set_macros([Macro.parse("text:a")])
    sent = commands()
    assert op(sent[0]) == (4, 0x19) and op(sent[1]) == (4, 0x15)
    pages = sent[1][8]
    assert len(sent) == 2 + pages + 1 and op(sent[-1]) == (4, 0x02)


def test_lcd_upload(keyboard):
    blob = bytes(4096 * 3)
    keyboard._lcd.replies = [b"ok"] * 3
    seen = []
    assert keyboard.upload_lcd(blob, slot=1, progress=seen.append)
    assert [len(w) for w in keyboard._lcd.writes] == [4097] * 3
    assert seen[-1] == 1.0
    cmd = [f[1:] for f in keyboard._cmd.features]
    assert op(cmd[1]) == (4, 0x72) and cmd[1][2] == 1 and cmd[1][8] == 3


def test_dongle_checksum_and_clock():
    from conftest import FakeHid

    class DongleHid(FakeHid):
        def read(self, timeout=0.3, size=4096):
            return self.writes[-1][1:] if self.writes else None

    d = object.__new__(Dongle)
    d._dev = DongleHid(output_len=33)
    assert d.sync_clock(dt.datetime(2026, 10, 28, 13, 35, 33))
    report = d._dev.writes[-1]
    assert len(report) == 33 and report[-1] == sum(report[:-1]) & 0xFF
    assert report[1:3] == bytes([0x0C, 0x10]) and report[5:7] == bytes([1, 0x5A]) and report[18:20] == b"\xAA\x55"
