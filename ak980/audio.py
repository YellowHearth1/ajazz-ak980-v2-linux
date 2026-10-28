"""What the PC is playing, as a spectrum for the music effects.

Records the default output's monitor with ``parec`` (PulseAudio and PipeWire's pulse server) or
``pw-record`` (plain PipeWire). No audio library is needed; the samples arrive on a pipe.
"""

from __future__ import annotations

import shutil
import subprocess
import threading
import time

import numpy as np

RATE = 48000
CHUNK = 1024                 # ~21 ms per analysis window
BANDS = 16


def _default_monitor() -> str | None:
    if not shutil.which("pactl"):
        return None
    try:
        sink = subprocess.run(["pactl", "get-default-sink"], capture_output=True, text=True, timeout=2).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return None
    return sink + ".monitor" if sink else None


def capture_command() -> list[str] | None:
    if shutil.which("parec"):
        cmd = ["parec", "--raw", "--format=float32le", f"--rate={RATE}", "--channels=1", "--latency-msec=20"]
        monitor = _default_monitor()
        return cmd + ([f"--device={monitor}"] if monitor else [])
    if shutil.which("pw-record"):
        return ["pw-record", "-P", "{ stream.capture.sink=true }", "--format=f32", f"--rate={RATE}",
                "--channels=1", "-"]
    return None


def available() -> bool:
    return capture_command() is not None


class AudioMeter:
    """Runs the recorder in the background and keeps the latest spectrum."""

    def __init__(self, bands: int = BANDS):
        self.n = bands
        self.bands = np.zeros(bands, np.float32)
        self.level = 0.0
        self.beat = False
        self.error: str | None = None
        self._peak = np.full(bands, 1e-3, np.float32)
        self._avg_energy = 0.0
        self._proc: subprocess.Popen | None = None
        self._thread: threading.Thread | None = None
        self._stop = threading.Event()
        self._lock = threading.Lock()
        freqs = np.fft.rfftfreq(CHUNK, 1 / RATE)
        edges = np.geomspace(40, 16000, bands + 1)
        self._bins = [np.where((freqs >= lo) & (freqs < hi))[0] for lo, hi in zip(edges[:-1], edges[1:])]
        self._window = np.hanning(CHUNK).astype(np.float32)

    @property
    def running(self) -> bool:
        return self._thread is not None and self._thread.is_alive()

    def start(self) -> bool:
        if self.running:
            return True
        cmd = capture_command()
        if cmd is None:
            self.error = "no-recorder"
            return False
        self._stop.clear()
        try:
            self._proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                          stdin=subprocess.DEVNULL, bufsize=0)
        except OSError as e:
            self.error = str(e)
            return False
        self._thread = threading.Thread(target=self._run, name="ak980-audio", daemon=True)
        self._thread.start()
        return True

    def stop(self) -> None:
        self._stop.set()
        if self._proc:
            self._proc.terminate()
            try:
                self._proc.wait(1)
            except subprocess.TimeoutExpired:
                self._proc.kill()
            self._proc = None
        if self._thread:
            self._thread.join(1)
            self._thread = None
        self.bands[:] = 0
        self.level = 0.0

    def snapshot(self) -> tuple[np.ndarray, float, bool]:
        with self._lock:
            beat, self.beat = self.beat, False
            return self.bands.copy(), self.level, beat

    def _run(self) -> None:
        need = CHUNK * 4
        buf = b""
        stream = self._proc.stdout
        while not self._stop.is_set():
            data = stream.read(need - len(buf))
            if not data:
                time.sleep(0.05)
                if self._proc is None or self._proc.poll() is not None:
                    self.error = "recorder-exited"
                    return
                continue
            buf += data
            if len(buf) < need:
                continue
            samples = np.frombuffer(buf, dtype="<f4")
            buf = b""
            self._analyse(samples)

    def _analyse(self, samples: np.ndarray) -> None:
        spec = np.abs(np.fft.rfft(samples * self._window))
        raw = np.array([spec[b].mean() if len(b) else 0.0 for b in self._bins], np.float32)
        raw = np.log1p(raw * 4)
        # Automatic gain per band: follow the peak, let it relax slowly
        self._peak = np.maximum(self._peak * 0.995, raw)
        norm = np.clip(raw / np.maximum(self._peak, 0.3), 0, 1)
        energy = float(np.sqrt(np.mean(samples * samples)))
        with self._lock:
            # Fast rise, slower fall, so the bars do not flicker
            up = norm > self.bands
            self.bands[up] = norm[up] * 0.7 + self.bands[up] * 0.3
            self.bands[~up] = self.bands[~up] * 0.82 + norm[~up] * 0.18
            self.level = min(1.0, energy * 5)
            self._avg_energy = self._avg_energy * 0.95 + energy * 0.05
            if energy > self._avg_energy * 1.6 and energy > 0.02:
                self.beat = True
