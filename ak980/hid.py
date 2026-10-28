"""Minimal HID access through Linux hidraw, with no third-party dependencies."""

from __future__ import annotations

import errno
import fcntl
import glob
import os
import re
import select
import threading
import time
from contextlib import contextmanager
from dataclasses import dataclass

from .i18n import tr


def _ioc(direction: int, nr: int, size: int) -> int:
    return (direction << 30) | (size << 16) | (ord("H") << 8) | nr


def _hidiocsfeature(n: int) -> int:
    return _ioc(3, 0x06, n)


def _hidiocgfeature(n: int) -> int:
    return _ioc(3, 0x07, n)


@dataclass(frozen=True)
class HidInfo:
    path: str
    vendor_id: int
    product_id: int
    name: str
    interface: int
    usage_page: int
    usage: int
    # Report lengths in bytes including the report ID byte (like HIDP_CAPS on Windows)
    input_len: int
    output_len: int
    feature_len: int

    def __str__(self) -> str:
        return (f"{self.path}  {self.vendor_id:04x}:{self.product_id:04x}  if{self.interface}  "
                f"page {self.usage_page:#06x}/{self.usage:#04x}  in {self.input_len} out {self.output_len} "
                f"feat {self.feature_len}  {self.name}")


def parse_descriptor(desc: bytes) -> tuple[int, int, int, int, int]:
    """Returns (usage_page, usage, input, output, feature) of the first top-level collection."""
    usage_page = usage = 0
    first_page = first_usage = None
    report_size = report_count = 0
    bits = {0x80: 0, 0x90: 0, 0xB0: 0}
    depth = 0
    i = 0
    while i < len(desc):
        prefix = desc[i]
        if prefix == 0xFE:  # long item
            i += 3 + desc[i + 1]
            continue
        size = (0, 1, 2, 4)[prefix & 0x03]
        value = int.from_bytes(desc[i + 1:i + 1 + size], "little")
        tag = prefix & 0xFC
        i += 1 + size
        if tag == 0x04:
            usage_page = value
        elif tag == 0x08:
            usage = value
        elif tag == 0x74:
            report_size = value
        elif tag == 0x94:
            report_count = value
        elif tag == 0xA0:
            if depth == 0 and first_page is None:
                first_page, first_usage = usage_page, usage
            depth += 1
        elif tag == 0xC0:
            depth -= 1
        elif tag in bits:
            bits[tag] += report_size * report_count

    def length(b: int) -> int:
        return 0 if b == 0 else (b + 7) // 8 + 1

    return (first_page or 0, first_usage or 0, length(bits[0x80]), length(bits[0x90]), length(bits[0xB0]))


def enumerate_hid(vendor_id: int | None = None, product_id: int | None = None) -> list[HidInfo]:
    found = []
    for sys_dir in sorted(glob.glob("/sys/class/hidraw/hidraw*")):
        try:
            with open(os.path.join(sys_dir, "device", "uevent")) as f:
                uevent = dict(line.strip().split("=", 1) for line in f if "=" in line)
            with open(os.path.join(sys_dir, "device", "report_descriptor"), "rb") as f:
                desc = f.read()
        except OSError:
            continue
        m = re.match(r"[0-9A-Fa-f]+:([0-9A-Fa-f]+):([0-9A-Fa-f]+)", uevent.get("HID_ID", ""))
        if not m:
            continue
        vid, pid = int(m.group(1), 16), int(m.group(2), 16)
        if vendor_id is not None and vid != vendor_id:
            continue
        if product_id is not None and pid != product_id:
            continue
        im = re.search(r"input(\d+)$", uevent.get("HID_PHYS", ""))
        page, use, inp, out, feat = parse_descriptor(desc)
        found.append(HidInfo(
            path="/dev/" + os.path.basename(sys_dir), vendor_id=vid, product_id=pid,
            name=uevent.get("HID_NAME", ""), interface=int(im.group(1)) if im else -1,
            usage_page=page, usage=use, input_len=inp, output_len=out, feature_len=feat))
    return found


class HidError(OSError):
    pass


class PermissionProblem(HidError):
    """The device exists but this user may not open it (udev rule missing)."""


def _retry(fn, attempts: int = 4):
    """The keyboard briefly stalls (EPIPE) when it is busy; a short pause is enough."""
    for i in range(attempts):
        try:
            return fn()
        except (BrokenPipeError, TimeoutError):
            if i == attempts - 1:
                raise
            time.sleep(0.005 * (i + 1))


class HidDevice:
    def __init__(self, info: HidInfo):
        self.info = info
        self._rlock = threading.RLock()
        self._depth = 0
        try:
            self._fd = os.open(info.path, os.O_RDWR | os.O_NONBLOCK | os.O_CLOEXEC)
        except PermissionError as e:
            raise PermissionProblem(errno.EACCES, tr(
                "No access to {path}. Install the udev rule (Device page or `ak980 setup`) "
                "and reconnect the keyboard.", path=info.path)) from e

    def close(self) -> None:
        if self._fd >= 0:
            os.close(self._fd)
            self._fd = -1

    def __enter__(self) -> HidDevice:
        return self

    def __exit__(self, *exc) -> None:
        self.close()

    @contextmanager
    def locked(self):
        """Exclusive access for a whole transaction, across threads and processes.

        The app, the background agent and the system service may all talk to the keyboard; an
        advisory flock on the hidraw node keeps their begin/data/commit sequences from interleaving.
        """
        with self._rlock:
            if self._depth == 0:
                fcntl.flock(self._fd, fcntl.LOCK_EX)
            self._depth += 1
            try:
                yield
            finally:
                self._depth -= 1
                if self._depth == 0:
                    fcntl.flock(self._fd, fcntl.LOCK_UN)

    def write(self, report: bytes, timeout: float = 3.0) -> None:
        """Output report. Byte 0 is the report ID (0 for unnumbered reports)."""
        view = memoryview(bytes(report))
        while view:
            _, w, _ = select.select([], [self._fd], [], timeout)
            if not w:
                raise HidError(errno.ETIMEDOUT, tr("Timed out writing to {path}", path=self.info.path))
            try:
                n = os.write(self._fd, view)
            except BlockingIOError:
                continue
            if n == len(view):
                return
            view = view[n:]

    def read(self, timeout: float = 0.3, size: int = 4096) -> bytes | None:
        """Input report (without a report ID byte for unnumbered reports) or None on timeout."""
        r, _, _ = select.select([self._fd], [], [], timeout)
        if not r:
            return None
        try:
            return os.read(self._fd, size)
        except BlockingIOError:
            return None

    def drain(self) -> None:
        while self.read(0) is not None:
            pass

    def set_feature(self, report: bytes) -> None:
        buf = bytearray(report)
        _retry(lambda: fcntl.ioctl(self._fd, _hidiocsfeature(len(buf)), buf))

    def get_feature(self, report_id: int = 0, length: int | None = None) -> bytes:
        """Returns the report with the report ID in byte 0."""
        n = length or self.info.feature_len
        buf = bytearray(n)
        buf[0] = report_id

        def ioctl():
            return fcntl.ioctl(self._fd, _hidiocgfeature(n), buf, True)

        got = _retry(ioctl)
        return bytes(buf[:got if isinstance(got, int) and got > 0 else n])
