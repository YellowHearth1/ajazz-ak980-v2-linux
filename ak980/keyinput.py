"""Key presses on the AK980, read from its /dev/input event devices.

Used by the reactive lighting effects and by launcher keys (F13–F24 → run a program). Reading
needs the ``input`` group (most desktop users are in it) or the udev rule's access control list.
"""

from __future__ import annotations

import glob
import os
import select
import struct
import threading
import time
from typing import Callable

from .keymap import LINUX_TO_HID

_EVENT = struct.Struct("llHHi")
EV_KEY = 1
NAME_HINTS = ("AK980",)


def keyboard_event_devices() -> list[str]:
    paths = []
    for path in sorted(glob.glob("/sys/class/input/event*")):
        try:
            with open(os.path.join(path, "device", "name")) as f:
                name = f.read().strip()
        except OSError:
            continue
        if any(h in name for h in NAME_HINTS):
            paths.append("/dev/input/" + os.path.basename(path))
    return paths


def readable() -> bool:
    devs = keyboard_event_devices()
    return bool(devs) and all(os.access(p, os.R_OK) for p in devs)


class KeyListener:
    """Calls ``callback(hid_usage, pressed)`` for every key press and release (no auto-repeat)."""

    def __init__(self, callback: Callable[[int, bool], None]):
        self.callback = callback
        self._fds: dict[str, int] = {}
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    def start(self) -> None:
        if self._thread and self._thread.is_alive():
            return
        self._stop.clear()
        self._thread = threading.Thread(target=self._run, name="ak980-keys", daemon=True)
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        if self._thread:
            self._thread.join(1)
            self._thread = None
        for fd in self._fds.values():
            os.close(fd)
        self._fds.clear()

    def _rescan(self) -> None:
        present = set(keyboard_event_devices())
        for path in list(self._fds):
            if path not in present:
                os.close(self._fds.pop(path))
        for path in present - set(self._fds):
            try:
                self._fds[path] = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_CLOEXEC)
            except OSError:
                pass

    def _run(self) -> None:
        next_scan = 0.0
        while not self._stop.is_set():
            if time.monotonic() >= next_scan:
                self._rescan()
                next_scan = time.monotonic() + 3
            fds = list(self._fds.values())
            if not fds:
                self._stop.wait(1)
                continue
            ready, _, _ = select.select(fds, [], [], 0.5)
            for fd in ready:
                try:
                    data = os.read(fd, _EVENT.size * 64)
                except (BlockingIOError, InterruptedError):
                    continue
                except OSError:
                    next_scan = 0  # the device went away; rescan on the next turn
                    continue
                for off in range(0, len(data) - _EVENT.size + 1, _EVENT.size):
                    _, _, etype, code, value = _EVENT.unpack_from(data, off)
                    if etype == EV_KEY and value in (0, 1):
                        usage = LINUX_TO_HID.get(code)
                        if usage:
                            self.callback(usage, value == 1)
