"""Every test runs with its own home directories and without a display."""

import os
import sys
import tempfile

_tmp = tempfile.mkdtemp(prefix="ak980-tests-")
for var in ("XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_STATE_HOME", "XDG_RUNTIME_DIR"):
    path = os.path.join(_tmp, var.lower())
    os.makedirs(path, mode=0o700, exist_ok=True)
    os.environ[var] = path
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
os.environ["AK980_PREVIEW_ONLY"] = "1"
os.environ["LANGUAGE"] = "en"
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import contextlib  # noqa: E402

import pytest  # noqa: E402


class FakeHid:
    """Records what the protocol code sends and answers like the keyboard does."""

    def __init__(self, output_len: int = 65):
        self.features: list[bytes] = []
        self.writes: list[bytes] = []
        self.info = type("Info", (), {"output_len": output_len, "path": "/dev/fake"})()
        self.replies: list[bytes] = []

    def set_feature(self, report):
        self.features.append(bytes(report))

    def get_feature(self, report_id=0, length=65):
        last = self.features[-1] if self.features else bytes(65)
        reply = bytearray(last[:65].ljust(65, b"\0"))
        reply[4] = 1
        return bytes(reply)

    def write(self, report, timeout=3.0):
        self.writes.append(bytes(report))

    def read(self, timeout=0.3, size=4096):
        return self.replies.pop(0) if self.replies else None

    def drain(self):
        pass

    @contextlib.contextmanager
    def locked(self):
        yield

    def close(self):
        pass


@pytest.fixture
def keyboard():
    from ak980.device import WiredKeyboard
    kb = object.__new__(WiredKeyboard)
    kb._cmd = FakeHid()
    kb._lcd = FakeHid(4097)
    kb.delay = 0
    return kb


@pytest.fixture
def commands(keyboard):
    """The 64-byte command payloads sent so far (report ID stripped)."""
    return lambda: [f[1:] for f in keyboard._cmd.features]
