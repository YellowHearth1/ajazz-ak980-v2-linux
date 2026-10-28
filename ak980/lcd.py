"""Images and GIFs for the 240×135 screen.

Upload blob (from the vendor's DeviceDriver.exe):
  [0]           frame count
  [1..n]        delay of each frame in 2 ms units (1..255)
  [256..]       frames as RGB565 little-endian, 240×135, rows top to bottom
  0xFF padding up to a multiple of 4096 bytes
"""

from __future__ import annotations

import os
import shutil
import time
from dataclasses import dataclass, field

import numpy as np
from PIL import Image, ImageDraw, ImageEnhance, ImageFont, ImageOps, ImageSequence

from .device import LCD_HEADER, LCD_HEIGHT, LCD_MAX_FRAMES, LCD_PAGE, LCD_WIDTH
from .i18n import tr

DEFAULT_DELAY = 100
MIN_DELAY = 2
MAX_DELAY = 510
SECONDS_PER_PAGE = 0.145
IMAGE_EXTS = (".png", ".jpg", ".jpeg", ".bmp", ".gif", ".webp", ".apng")
FONT_DIR = os.path.join(os.path.dirname(__file__), "data", "fonts")
HISTORY_DIR = os.path.join(os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share")),
                           "ak980", "history")


@dataclass
class RenderOptions:
    fit: str = "fit"                  # fit | fill | stretch
    background: tuple[int, int, int] = (0, 0, 0)
    flip_h: bool = False
    flip_v: bool = False
    brightness: float = 1.0
    contrast: float = 1.0
    saturation: float = 1.0
    speed: float = 1.0
    fixed_delay: int | None = None
    trim_start: int = 0
    trim_end: int = 0
    frame_step: int = 1
    text: str = ""
    text_color: tuple[int, int, int] = (255, 255, 255)
    text_size: int = 18
    text_pos: str = "bottom-right"    # top-left | top | top-right | center | bottom-left | bottom | bottom-right
    dither: bool = True
    # Per-frame edits keyed by the source frame index
    deleted: set[int] = field(default_factory=set)
    delays: dict[int, int] = field(default_factory=dict)


@dataclass
class Source:
    """Original frames (RGBA) with their durations in ms."""
    frames: list[tuple[Image.Image, int]] = field(default_factory=list)

    @classmethod
    def load(cls, path: str, limit: int = 1000) -> Source:
        src = cls()
        files = frame_files(path) if os.path.isdir(path) else [path]
        for f in files:
            with Image.open(f) as img:
                for frame in ImageSequence.Iterator(img):
                    if len(src.frames) >= limit:
                        return src
                    d = frame.info.get("duration", img.info.get("duration", DEFAULT_DELAY)) or DEFAULT_DELAY
                    # Browsers treat 0-10 ms as "as fast as sensible"; do the same
                    if d <= 10:
                        d = DEFAULT_DELAY
                    src.frames.append((frame.convert("RGBA").copy(), int(d)))
        if not src.frames:
            raise ValueError(tr("No images in {path}", path=path))
        return src

    def select(self, o: RenderOptions, max_frames: int = LCD_MAX_FRAMES) -> list[tuple[int, int]]:
        """Source indices that survive trimming, stepping and deletion, with their delays."""
        n = len(self.frames)
        start = max(0, min(o.trim_start, n - 1))
        end = max(start + 1, min(n - o.trim_end, n))
        step = max(1, o.frame_step)
        picked = []
        for i in range(start, end, step):
            if i in o.deleted:
                continue
            if len(picked) >= max_frames:
                break
            delay = sum(self.frames[j][1] for j in range(i, min(i + step, end)))
            if o.fixed_delay:
                delay = o.fixed_delay
            delay = round(delay / max(0.05, o.speed))
            if i in o.delays:
                delay = o.delays[i]
            picked.append((i, max(MIN_DELAY, min(MAX_DELAY, delay))))
        return picked

    def available(self, o: RenderOptions) -> int:
        """How many frames the options select before the screen's frame limit is applied."""
        return len(self.select(o, max_frames=10 ** 6))

    def render_frame(self, index: int, o: RenderOptions) -> Image.Image:
        return render(self.frames[index][0], o)

    def render(self, o: RenderOptions, max_frames: int = LCD_MAX_FRAMES) -> list[tuple[Image.Image, int]]:
        return [(self.render_frame(i, o), d) for i, d in self.select(o, max_frames)]


def frame_files(directory: str) -> list[str]:
    files = [os.path.join(directory, f) for f in os.listdir(directory) if f.lower().endswith(IMAGE_EXTS)]

    def key(p: str):
        stem = os.path.splitext(os.path.basename(p))[0]
        return (0, int(stem), "") if stem.isdigit() else (1, 0, stem.lower())

    return sorted(files, key=key)


def _font(size: int) -> ImageFont.ImageFont:
    for name in (os.path.join(FONT_DIR, "FiraSansCondensed-SemiBold.ttf"), "DejaVuSans-Bold.ttf",
                 "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"):
        try:
            return ImageFont.truetype(name, size)
        except OSError:
            continue
    return ImageFont.load_default()


def render(img: Image.Image, o: RenderOptions) -> Image.Image:
    """Scales a frame to 240×135 and applies the adjustments; returns RGB."""
    img = img.convert("RGBA")
    w, h = LCD_WIDTH, LCD_HEIGHT
    if o.fit == "stretch":
        scaled = img.resize((w, h), Image.LANCZOS)
    else:
        s = min(w / img.width, h / img.height) if o.fit == "fit" else max(w / img.width, h / img.height)
        scaled = img.resize((max(1, round(img.width * s)), max(1, round(img.height * s))), Image.LANCZOS)
    canvas = Image.new("RGBA", (w, h), tuple(o.background) + (255,))
    canvas.alpha_composite(scaled, ((w - scaled.width) // 2, (h - scaled.height) // 2))
    out = canvas.convert("RGB")
    if o.flip_h:
        out = ImageOps.mirror(out)
    if o.flip_v:
        out = ImageOps.flip(out)
    if o.brightness != 1.0:
        out = ImageEnhance.Brightness(out).enhance(o.brightness)
    if o.contrast != 1.0:
        out = ImageEnhance.Contrast(out).enhance(o.contrast)
    if o.saturation != 1.0:
        out = ImageEnhance.Color(out).enhance(o.saturation)
    if o.text:
        draw = ImageDraw.Draw(out)
        font = _font(o.text_size)
        left, top, right, bottom = draw.textbbox((0, 0), o.text, font=font)
        tw, th = right - left, bottom - top
        m = 6
        xs = {"left": m, "center": (w - tw) // 2, "right": w - tw - m}
        ys = {"top": m, "center": (h - th) // 2, "bottom": h - th - m}
        pos = o.text_pos
        vert = "top" if pos.startswith("top") else "bottom" if pos.startswith("bottom") else "center"
        horz = "left" if pos.endswith("left") else "right" if pos.endswith("right") else "center"
        x, y = xs[horz] - left, ys[vert] - top
        draw.text((x + 1, y + 1), o.text, font=font, fill=(0, 0, 0))
        draw.text((x, y), o.text, font=font, fill=tuple(o.text_color))
    return out


_BAYER4 = np.array([[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]], dtype=np.float32) / 16.0 - 0.5


def _quantize(img: Image.Image, dither: bool) -> np.ndarray:
    """Returns an (h, w) uint16 array of RGB565 values."""
    a = np.asarray(img.convert("RGB"), dtype=np.float32)
    if dither:
        # Ordered (Bayer) dithering: one quantisation step is 8 for red/blue and 4 for green
        h, w = a.shape[:2]
        t = np.tile(_BAYER4, (h // 4 + 1, w // 4 + 1))[:h, :w, None]
        a = a + t * np.array([8.0, 4.0, 8.0], dtype=np.float32)
    a = np.clip(a + 0.5, 0, 255).astype(np.uint16)
    return ((a[..., 0] >> 3) << 11) | ((a[..., 1] >> 2) << 5) | (a[..., 2] >> 3)


def to_rgb565(img: Image.Image, dither: bool = True) -> bytes:
    """RGB565 little-endian, rows top to bottom."""
    return _quantize(img, dither).astype("<u2").tobytes()


def preview_565(img: Image.Image, dither: bool = True) -> Image.Image:
    """What a frame looks like on the panel after the RGB565 conversion."""
    v = _quantize(img, dither)
    r, g, b = (v >> 11) & 0x1F, (v >> 5) & 0x3F, v & 0x1F
    rgb = np.stack([(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)], axis=-1).astype(np.uint8)
    return Image.fromarray(rgb, "RGB")


def encode_animation(frames: list[tuple[Image.Image, int]], dither: bool = True, progress=None) -> bytes:
    if not frames:
        raise ValueError(tr("No frames to upload"))
    frames = frames[:LCD_MAX_FRAMES]
    frame_bytes = LCD_WIDTH * LCD_HEIGHT * 2
    payload = LCD_HEADER + len(frames) * frame_bytes
    pages = (payload + LCD_PAGE - 1) // LCD_PAGE
    blob = bytearray(b"\xFF" * (pages * LCD_PAGE))
    blob[0] = len(frames)
    for i, (_, delay) in enumerate(frames):
        blob[1 + i] = max(1, min(255, delay // 2))
    for i, (img, _) in enumerate(frames):
        if img.size != (LCD_WIDTH, LCD_HEIGHT):
            img = render(img, RenderOptions())
        start = LCD_HEADER + i * frame_bytes
        blob[start:start + frame_bytes] = to_rgb565(img, dither)
        if progress:
            progress((i + 1) / len(frames))
    return bytes(blob)


def page_count(n_frames: int) -> int:
    n = min(n_frames, LCD_MAX_FRAMES)
    return (LCD_HEADER + n * LCD_WIDTH * LCD_HEIGHT * 2 + LCD_PAGE - 1) // LCD_PAGE


def save_history(frames: list[tuple[Image.Image, int]], source_path: str = "") -> str:
    """Keeps a copy of every upload: the keyboard cannot give an animation back."""
    stamp = time.strftime("%Y-%m-%d_%H-%M-%S")
    base = os.path.splitext(os.path.basename(source_path.rstrip("/")))[0] if source_path else "animation"
    folder = os.path.join(HISTORY_DIR, f"{stamp}_{base}")
    os.makedirs(folder, exist_ok=True)
    imgs = [f for f, _ in frames]
    imgs[0].save(os.path.join(folder, "screen.gif"), save_all=True, append_images=imgs[1:],
                 duration=[d for _, d in frames], loop=0)
    if source_path and os.path.isfile(source_path):
        shutil.copy2(source_path, os.path.join(folder, "original" + os.path.splitext(source_path)[1]))
    return folder


def history_entries() -> list[str]:
    """Saved uploads, newest first (paths to their screen.gif)."""
    if not os.path.isdir(HISTORY_DIR):
        return []
    out = []
    for name in sorted(os.listdir(HISTORY_DIR), reverse=True):
        gif = os.path.join(HISTORY_DIR, name, "screen.gif")
        if os.path.isfile(gif):
            out.append(gif)
    return out
