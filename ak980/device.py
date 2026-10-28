"""AJAZZ AK980 V2 protocol (SONiX firmware).

The protocol was reverse engineered from the vendor's DeviceDriver.exe in donglover69/ak980-studio
(MIT) and verified on the keyboard there; the real-time stream matches the AULA F108 Pro driver in
hcode10/OpenRGB-With-AulaF108Pro-support. Everything below was re-tested on an AK980 V2 over Linux
hidraw. docs/PROTOCOL.md has the full write-up.

Wired (0C45:8009):
  interface 3, usage page 0xFF13: 65-byte feature reports = report ID 0 + 64-byte command;
  interface 2, usage page 0xFF68: 4097-byte output reports = report ID 0 + one 4 KiB screen page.
  A transaction is 04 18 (begin) -> 04 OP ARG .. COUNT -> COUNT data packets -> 04 02 (commit) [-> 04 F0].

2.4 GHz dongle (0C45:FEFE), interface 3, usage page 0xFF60: 33-byte output reports =
  report ID 0 + command + byte sum in the last byte. The reply repeats the first three command bytes.
"""

from __future__ import annotations

import datetime as _dt
import time
from typing import Callable, Iterable, Mapping

from .hid import HidDevice, HidError, HidInfo, enumerate_hid
from .i18n import tr
from .keymap import TABLE_PACKETS as KEYMAP_PACKETS, KeyAction, encode_layer
from .lighting import MAX_LEVEL, KeyboardOptions, LightSettings
from .macro import Macro, encode_macros

VENDOR_ID = 0x0C45
WIRED_PID = 0x8009
DONGLE_PID = 0xFEFE

LCD_WIDTH = 240
LCD_HEIGHT = 135
LCD_HEADER = 256
LCD_MAX_FRAMES = 140
LCD_PAGE = 4096

REALTIME_LEDS = 128          # 8 packets × 16 entries of [index R G B]

_PREFIX = 0x04
_OP_BEGIN = 0x18
_OP_COMMIT = 0x02
_OP_APPLY = 0xF0
_OP_LIGHT = 0x13
_OP_REALTIME = 0x20
_OP_CUSTOM_COLORS = 0x23
_OP_READ_CUSTOM = 0xF5
_OP_OPTIONS = 0x17
_OP_KEYMAP_TOP = 0x11
_OP_KEYMAP_FN = 0x27
_OP_MACRO_BEGIN = 0x19
_OP_MACRO_DATA = 0x15
_OP_CLOCK = 0x28
_OP_LCD_UPLOAD = 0x72
_CUSTOM_PACKETS = 9


class NotConnected(HidError):
    pass


def clock_payload(now: _dt.datetime, slot: int) -> bytes:
    """SLOT 5A YY MM DD hh mm ss 00 DOW (Sunday = 0)."""
    return bytes([slot, 0x5A, now.year % 100, now.month, now.day, now.hour, now.minute, now.second,
                  0, (now.weekday() + 1) % 7])


class WiredKeyboard:
    """The keyboard on a USB cable: every setting is available."""

    connection = "usb"

    def __init__(self, command: HidInfo, lcd: HidInfo | None):
        self._cmd = HidDevice(command)
        try:
            self._lcd = HidDevice(lcd) if lcd else None
        except Exception:
            self._cmd.close()
            raise
        # Pause before each configuration command, as in the vendor driver (<cmd_delaytime value="35"/>)
        self.delay = 0.035

    @staticmethod
    def find() -> tuple[HidInfo, HidInfo | None] | None:
        devs = enumerate_hid(VENDOR_ID, WIRED_PID)
        cmd = next((d for d in devs if d.usage_page == 0xFF13 and d.feature_len >= 65), None)
        if cmd is None:
            return None
        lcd = next((d for d in devs if d.usage_page == 0xFF68 and d.output_len >= LCD_PAGE + 1), None)
        return cmd, lcd

    @classmethod
    def open(cls) -> WiredKeyboard:
        found = cls.find()
        if found is None:
            if Dongle.find():
                raise NotConnected(tr("This needs the USB cable: connect the keyboard with the cable and "
                                      "switch it to wired mode."))
            raise NotConnected(tr("AK980 not found on USB"))
        return cls(*found)

    def close(self) -> None:
        self._cmd.close()
        if self._lcd:
            self._lcd.close()

    def __enter__(self) -> WiredKeyboard:
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    def locked(self):
        return self._cmd.locked()

    # ------------------------------------------------------------ transport

    def send_packet(self, packet: bytes) -> bytes:
        """Sends a 64-byte command and reads the echo (byte 4 is a status)."""
        report = bytes([0]) + bytes(packet[:64]).ljust(64, b"\0")
        with self._cmd.locked():
            time.sleep(self.delay)
            self._cmd.set_feature(report)
            time.sleep(self.delay)
            return self._cmd.get_feature(0, 65)

    def _send_noack(self, packet: bytes, delay: float | None = None) -> None:
        report = bytes([0]) + bytes(packet[:64]).ljust(64, b"\0")
        with self._cmd.locked():
            time.sleep(self.delay if delay is None else delay)
            self._cmd.set_feature(report)

    def _command(self, op: int, arg: int = 0, count: int = 0, extra: Mapping[int, int] | None = None) -> bytes:
        p = bytearray(64)
        p[0] = _PREFIX
        p[1] = op
        p[2] = arg
        p[8] = count & 0xFF
        p[9] = (count >> 8) & 0xFF
        for i, v in (extra or {}).items():
            p[i] = v
        return self.send_packet(p)

    def _begin(self) -> None:
        self._command(_OP_BEGIN)

    def _commit(self) -> None:
        self._command(_OP_COMMIT)

    def _apply(self, ack: bool = False) -> None:
        p = bytes([_PREFIX, _OP_APPLY])
        if ack:
            self.send_packet(p)
        else:
            self._send_noack(p)

    # ------------------------------------------------------------ clock and screen

    def sync_clock(self, now: _dt.datetime | None = None, slot: int = 1) -> bool:
        """Sets the screen clock. ``slot`` is the screen page the vendor driver sends with the time."""
        now = now or _dt.datetime.now()
        with self.locked():
            self._begin()
            self._command(_OP_CLOCK, extra={8: 1})
            p = bytearray(64)
            p[1:11] = clock_payload(now, slot)
            p[62] = 0xAA
            p[63] = 0x55
            self.send_packet(p)
            self._commit()
        return True

    def upload_lcd(self, blob: bytes, slot: int = 1,
                   progress: Callable[[float], None] | None = None,
                   cancel: Callable[[], bool] | None = None) -> bool:
        """Uploads an animation (see lcd.encode_animation), about 145 ms per 4 KiB page.

        Returns False when ``cancel`` stopped it; the keyboard keeps the pages written so far.
        """
        if self._lcd is None:
            raise HidError(tr("The screen interface (0xFF68) was not found"))
        if len(blob) % LCD_PAGE:
            raise ValueError("blob size must be a multiple of 4096")
        pages = len(blob) // LCD_PAGE
        done = True
        with self.locked():
            self._lcd.drain()
            self._begin()
            self._command(_OP_LCD_UPLOAD, slot, pages)
            for i in range(pages):
                if cancel and cancel():
                    done = False
                    break
                self._lcd.write(b"\0" + blob[i * LCD_PAGE:(i + 1) * LCD_PAGE])
                self._lcd.read(0.3)  # the keyboard acknowledges every flash write
                if progress:
                    progress((i + 1) / pages)
            self._commit()
        return done

    # ------------------------------------------------------------ lighting

    def set_lighting(self, s: LightSettings) -> None:
        with self.locked():
            self._begin()
            self._command(_OP_LIGHT, count=1)
            self._send_noack(s.to_packet())
            self._commit()
            self._apply()

    def set_custom_colors(self, colors: Mapping[int, tuple[int, int, int]], brightness: int = MAX_LEVEL) -> None:
        """Static per-key colours stored in the keyboard: {LED index: (R, G, B)}; the rest are black."""
        mode = bytearray(64)
        mode[0] = 0x80
        mode[9] = max(0, min(MAX_LEVEL, brightness))
        mode[14] = 0xAA
        mode[15] = 0x55
        table = bytearray(_CUSTOM_PACKETS * 64)
        for idx, (r, g, b) in colors.items():
            if 0 <= idx and idx * 4 + 3 < len(table) - 2:
                table[idx * 4:idx * 4 + 4] = bytes((idx, r, g, b))
        table[-2:] = b"\xAA\x55"
        with self.locked():
            self._begin()
            self._command(_OP_LIGHT, count=1)
            self._send_noack(mode)
            self._commit()
            self._apply()
            self._begin()
            self._command(_OP_CUSTOM_COLORS, count=_CUSTOM_PACKETS)
            for i in range(_CUSTOM_PACKETS):
                self._send_noack(table[i * 64:(i + 1) * 64])
            self._commit()
            self._apply()

    def send_realtime(self, colors: Mapping[int, tuple[int, int, int]] | bytes, gap: float = 0.002) -> None:
        """One frame of the real-time stream (nothing is written to flash).

        ``colors`` is {LED index: (R, G, B)} or a ready 512-byte table of [index R G B] entries.
        The keyboard shows the frame until the next one, or until a configuration command
        replaces it. Sending faster than ~1 ms per report makes the keyboard stall the bus.
        """
        if isinstance(colors, (bytes, bytearray)):
            table = bytes(colors).ljust(REALTIME_LEDS * 4, b"\0")
        else:
            buf = bytearray(REALTIME_LEDS * 4)
            for idx, (r, g, b) in colors.items():
                if 0 <= idx < REALTIME_LEDS:
                    buf[idx * 4:idx * 4 + 4] = bytes((idx, r, g, b))
            table = bytes(buf)
        dev = self._cmd
        with dev.locked():
            head = bytearray(65)
            head[1] = _PREFIX
            head[2] = _OP_REALTIME
            head[9] = 8
            dev.set_feature(head)
            dev.get_feature(0, 65)
            for i in range(8):
                time.sleep(gap)
                dev.set_feature(b"\0" + table[i * 64:(i + 1) * 64])
            time.sleep(gap)
            tail = bytearray(65)
            tail[1] = _PREFIX
            tail[2] = _OP_COMMIT
            dev.set_feature(tail)
            dev.get_feature(0, 65)

    def set_side_light(self, mode: int) -> None:
        p = bytearray(64)
        p[0] = mode
        p[14] = 0xAA
        p[15] = 0x55
        with self.locked():
            self._begin()
            self._command(_OP_LIGHT, count=1)
            self._send_noack(p)
            self._commit()
            self._apply()

    def read_custom_table(self, max_packets: int = _CUSTOM_PACKETS) -> bytes:
        """Reads the per-key colour table ([idx R G B] × 144) without changing anything.

        On the AK980 V2 this returns a table that does not follow animated effects, so it is only
        offered as a diagnostic (`ak980 custom --read`).
        """
        with self.locked():
            time.sleep(0.01)
            cmd = bytearray(65)
            cmd[1] = _PREFIX
            cmd[2] = _OP_READ_CUSTOM
            self._cmd.set_feature(cmd)
            time.sleep(0.005)
            echo = None
            for _ in range(10):
                try:
                    echo = self._cmd.get_feature(0, 65)
                    break
                except OSError:
                    time.sleep(0.005)
            out = bytearray()
            if echo is not None and echo[4] == 1:
                got = reads = 0
                while got < max_packets and reads < max_packets + 2:
                    reads += 1
                    time.sleep(0.003)
                    t0 = time.monotonic()
                    try:
                        r = self._cmd.get_feature(0, 65)
                    except OSError:
                        break
                    if time.monotonic() - t0 > 0.36:
                        break
                    if r[1] == _PREFIX and r[2] == _OP_READ_CUSTOM:
                        continue
                    out += r[1:65]
                    got += 1
            self._commit()
            return bytes(out)

    # ------------------------------------------------------------ options, keys, macros

    def set_options(self, o: KeyboardOptions) -> None:
        with self.locked():
            self._begin()
            self._command(_OP_OPTIONS, 1, count=1)
            self._send_noack(o.to_packet())
            self._commit()

    def set_keymap(self, fn_layer: bool, actions: Mapping[int, KeyAction]) -> None:
        """Replaces a whole layer: keys missing from ``actions`` go back to their default."""
        table = encode_layer(actions)
        with self.locked():
            self._begin()
            self._command(_OP_KEYMAP_FN if fn_layer else _OP_KEYMAP_TOP, count=KEYMAP_PACKETS)
            for i in range(KEYMAP_PACKETS):
                self._send_noack(table[i * 64:(i + 1) * 64])
            self._commit()
            self._apply(ack=True)

    def set_macros(self, macros: Iterable[Macro]) -> None:
        data, pages = encode_macros(list(macros))
        with self.locked():
            self._command(_OP_MACRO_BEGIN)
            self._command(_OP_MACRO_DATA, count=pages)
            time.sleep(0.03)
            for i in range(pages):
                self._send_noack(data[i * 64:(i + 1) * 64])
            time.sleep(0.03)
            self._commit()


class Dongle:
    """The keyboard in 2.4 GHz mode through its dongle: battery level and clock."""

    connection = "2.4g"

    def __init__(self, info: HidInfo):
        self._dev = HidDevice(info)

    @staticmethod
    def find() -> HidInfo | None:
        devs = [d for d in enumerate_hid(VENDOR_ID, DONGLE_PID) if d.output_len >= 33 and d.input_len >= 33]
        devs.sort(key=lambda d: (d.usage_page != 0xFF60, d.interface != 3))
        return devs[0] if devs else None

    @classmethod
    def open(cls) -> Dongle:
        info = cls.find()
        if info is None:
            raise NotConnected(tr("AK980 2.4 GHz dongle not found"))
        return cls(info)

    def close(self) -> None:
        self._dev.close()

    def __enter__(self) -> Dongle:
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    def locked(self):
        return self._dev.locked()

    def request(self, command: bytes, wait: bool = True) -> bytes | None:
        n = self._dev.info.output_len
        report = bytearray(n)
        report[1:1 + min(len(command), n - 2)] = command[:n - 2]
        report[-1] = sum(report[:-1]) & 0xFF
        with self._dev.locked():
            self._dev.drain()
            self._dev.write(bytes(report), timeout=0.5)
            if not wait:
                return None
            for _ in range(20):
                r = self._dev.read(0.05)
                if r is None:
                    continue
                # hidraw drops the report ID of unnumbered reports; Windows keeps it
                data = r[1:] if len(r) == n else r
                if data[:3] == bytes(report[1:4]):
                    return data
        return None

    def read_battery(self) -> int | None:
        # After a while idle the first request only wakes the radio link
        reply = self.request(b"\x20\x01") or self.request(b"\x20\x01")
        if reply is None or len(reply) < 4 or reply[3] == 0:
            return None
        return min(reply[3], 100)

    def sync_clock(self, now: _dt.datetime | None = None, slot: int = 1) -> bool:
        """0C 10 00 00 SLOT 5A YY MM DD hh mm ss 00 DOW .. .. .. AA 55 (vendor driver, wireless variant)."""
        now = now or _dt.datetime.now()
        cmd = bytearray(19)
        cmd[0] = 0x0C
        cmd[1] = 0x10
        cmd[4:14] = clock_payload(now, slot)
        cmd[17] = 0xAA
        cmd[18] = 0x55
        return self.request(bytes(cmd)) is not None


def connection_state() -> str | None:
    """"usb", "2.4g" or None. Reads sysfs only, so it is cheap enough to poll."""
    if WiredKeyboard.find():
        return "usb"
    if Dongle.find():
        return "2.4g"
    return None


def open_any() -> WiredKeyboard | Dongle:
    """The cable wins: every feature is available through it."""
    if WiredKeyboard.find():
        return WiredKeyboard.open()
    if Dongle.find():
        return Dongle.open()
    raise NotConnected(tr("AJAZZ AK980 not found (neither on USB nor through the 2.4 GHz dongle)"))
