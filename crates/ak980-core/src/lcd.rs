//! Images and GIFs for the 240×135 screen.
//!
//! Upload blob (from the vendor's DeviceDriver.exe):
//!
//! ```text
//! [0]           frame count
//! [1..n]        delay of each frame in 2 ms units (1..255)
//! [256..]       frames as RGB565 little-endian, 240×135, rows top to bottom
//! 0xFF padding up to a multiple of 4096 bytes
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};
use image::codecs::gif::{GifDecoder, GifEncoder, Repeat};
use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::imageops::{self, FilterType};
use image::{AnimationDecoder, Delay, Frame, ImageReader, Rgb, RgbImage, Rgba, RgbaImage};

use crate::device::{LCD_HEADER, LCD_HEIGHT, LCD_MAX_FRAMES, LCD_PAGE, LCD_WIDTH};
use crate::error::{Error, Result};
use crate::{paths, time::LocalTime, tr};

pub const DEFAULT_DELAY: u32 = 100;
pub const MIN_DELAY: u32 = 2;
pub const MAX_DELAY: u32 = 510;
pub const SECONDS_PER_PAGE: f32 = 0.145;
pub const IMAGE_EXTS: [&str; 7] = ["png", "jpg", "jpeg", "bmp", "gif", "webp", "apng"];
const W: u32 = LCD_WIDTH as u32;
const H: u32 = LCD_HEIGHT as u32;

/// Ubuntu Bold, for captions (Ubuntu Font Licence, `ak980/data/fonts/UFL.txt`).
pub static CAPTION_FONT: &[u8] = include_bytes!("../../../ak980/data/fonts/Ubuntu-B.ttf");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fit {
    /// The whole picture, with bars.
    #[default]
    Fit,
    /// The screen filled, the picture cropped.
    Fill,
    /// The picture stretched to the screen.
    Stretch,
}

impl Fit {
    pub const ALL: [Fit; 3] = [Fit::Fit, Fit::Fill, Fit::Stretch];

    pub fn id(self) -> &'static str {
        ["fit", "fill", "stretch"][self as usize]
    }

    pub fn by_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.id() == id)
    }
}

/// The order frames play in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Order {
    #[default]
    Forward,
    Backward,
    /// Forward, then back again, without showing the turning frames twice.
    Bounce,
}

impl Order {
    pub const ALL: [Order; 3] = [Order::Forward, Order::Backward, Order::Bounce];

    pub fn id(self) -> &'static str {
        ["forward", "backward", "bounce"][self as usize]
    }

    pub fn by_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.id() == id)
    }
}

/// A colour treatment of the whole picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tone {
    #[default]
    Natural,
    Grey,
    Sepia,
    Negative,
}

impl Tone {
    pub const ALL: [Tone; 4] = [Tone::Natural, Tone::Grey, Tone::Sepia, Tone::Negative];

    pub fn id(self) -> &'static str {
        ["natural", "grey", "sepia", "negative"][self as usize]
    }

    pub fn by_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.id() == id)
    }
}

/// How long each frame of scrolling text over a still picture shows.
pub const SCROLL_DELAY: u32 = 50;

/// Where a caption goes.
pub const TEXT_POSITIONS: [&str; 7] = ["top-left", "top", "top-right", "center", "bottom-left", "bottom", "bottom-right"];

#[derive(Clone, Debug, PartialEq)]
pub struct RenderOptions {
    pub fit: Fit,
    pub background: [u8; 3],
    /// Quarter turns clockwise, 0..=3.
    pub rotate: u8,
    /// 1 shows the whole picture; 2 its middle half, and so on.
    pub zoom: f32,
    /// Which part a zoomed picture shows: -1 the left or top edge, 1 the right or bottom edge.
    pub pan: (f32, f32),
    /// Scale without smoothing, for pixel art.
    pub sharp: bool,
    pub flip_h: bool,
    pub flip_v: bool,
    pub tone: Tone,
    /// Turns every colour's hue by this many degrees.
    pub hue: i32,
    pub order: Order,
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub speed: f32,
    pub fixed_delay: Option<u32>,
    pub trim_start: usize,
    pub trim_end: usize,
    pub frame_step: usize,
    pub text: String,
    pub text_color: [u8; 3],
    pub text_size: u32,
    pub text_pos: String,
    /// The caption runs across the screen from right to left, this many pixels a frame.
    pub text_scroll: Option<u32>,
    /// A dark outline instead of a shadow, for text over busy pictures.
    pub text_outline: bool,
    pub dither: bool,
    /// Per-frame edits, keyed by the source frame index.
    pub deleted: BTreeSet<usize>,
    pub delays: BTreeMap<usize, u32>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            fit: Fit::Fit,
            background: [0, 0, 0],
            rotate: 0,
            zoom: 1.0,
            pan: (0.0, 0.0),
            sharp: false,
            flip_h: false,
            flip_v: false,
            tone: Tone::Natural,
            hue: 0,
            order: Order::Forward,
            brightness: 1.0,
            contrast: 1.0,
            saturation: 1.0,
            speed: 1.0,
            fixed_delay: None,
            trim_start: 0,
            trim_end: 0,
            frame_step: 1,
            text: String::new(),
            text_color: [255, 255, 255],
            text_size: 18,
            text_pos: "bottom-right".into(),
            text_scroll: None,
            text_outline: false,
            dither: true,
            deleted: BTreeSet::new(),
            delays: BTreeMap::new(),
        }
    }
}

/// Original frames with their durations in ms.
#[derive(Clone, Debug, Default)]
pub struct Source {
    pub frames: Vec<(RgbaImage, u32)>,
}

fn frame_delay(d: Delay) -> u32 {
    let (num, den) = d.numer_denom_ms();
    let ms = num.checked_div(den).unwrap_or(0);
    // Browsers treat 0-10 ms as "as fast as sensible"; do the same
    if ms <= 10 { DEFAULT_DELAY } else { ms }
}

fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| IMAGE_EXTS.contains(&e.to_lowercase().as_str()))
}

/// Images in a folder: numbered files in number order, then the rest by name.
pub fn frame_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| is_image(p)).collect())
        .unwrap_or_default();
    files.sort_by_key(|p| {
        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        match stem.parse::<u64>() {
            Ok(n) => (0, n, String::new()),
            Err(_) => (1, 0, stem.to_lowercase()),
        }
    });
    files
}

fn decode_error(path: &Path, e: impl std::fmt::Display) -> Error {
    Error::invalid(format!("{}: {e}", path.display()))
}

fn push_frames(out: &mut Vec<(RgbaImage, u32)>, frames: Vec<Frame>, limit: usize) {
    for f in frames {
        if out.len() >= limit {
            break;
        }
        let delay = frame_delay(f.delay());
        out.push((f.into_buffer(), delay));
    }
}

impl Source {
    /// A GIF, an animated PNG or WebP, a still picture or a folder of frames.
    pub fn load(path: &Path, limit: usize) -> Result<Self> {
        let mut src = Source::default();
        let files = if path.is_dir() { frame_files(path) } else { vec![path.to_path_buf()] };
        for f in files {
            if src.frames.len() >= limit {
                break;
            }
            let ext = f.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            let reader = || File::open(&f).map(BufReader::new).map_err(|e| decode_error(&f, e));
            let left = limit - src.frames.len();
            match ext.as_str() {
                "gif" => {
                    let d = GifDecoder::new(reader()?).map_err(|e| decode_error(&f, e))?;
                    let frames = d.into_frames().take(left).collect::<Result<Vec<_>, _>>().map_err(|e| decode_error(&f, e))?;
                    push_frames(&mut src.frames, frames, limit);
                    continue;
                }
                "png" | "apng" => {
                    let d = PngDecoder::new(reader()?).map_err(|e| decode_error(&f, e))?;
                    if d.is_apng().unwrap_or(false) {
                        let frames = d.apng().map_err(|e| decode_error(&f, e))?.into_frames().take(left)
                            .collect::<Result<Vec<_>, _>>().map_err(|e| decode_error(&f, e))?;
                        push_frames(&mut src.frames, frames, limit);
                        continue;
                    }
                }
                "webp" => {
                    let d = WebPDecoder::new(reader()?).map_err(|e| decode_error(&f, e))?;
                    if d.has_animation() {
                        let frames = d.into_frames().take(left).collect::<Result<Vec<_>, _>>()
                            .map_err(|e| decode_error(&f, e))?;
                        push_frames(&mut src.frames, frames, limit);
                        continue;
                    }
                }
                _ => {}
            }
            let img = ImageReader::open(&f)
                .map_err(|e| decode_error(&f, e))?
                .with_guessed_format()
                .map_err(|e| decode_error(&f, e))?
                .decode()
                .map_err(|e| decode_error(&f, e))?;
            src.frames.push((img.to_rgba8(), DEFAULT_DELAY));
        }
        if src.frames.is_empty() {
            return Err(Error::invalid(tr!("No images in {path}", path = path.display())));
        }
        Ok(src)
    }

    /// Source indices that survive trimming, stepping and deletion, with their delays.
    pub fn select(&self, o: &RenderOptions, max_frames: usize) -> Vec<(usize, u32)> {
        let n = self.frames.len();
        if n == 0 {
            return Vec::new();
        }
        let start = o.trim_start.min(n - 1);
        let end = n.saturating_sub(o.trim_end).min(n).max(start + 1);
        let step = o.frame_step.max(1);
        let mut picked = Vec::new();
        for i in (start..end).step_by(step) {
            if o.deleted.contains(&i) {
                continue;
            }
            if picked.len() >= max_frames {
                break;
            }
            let mut delay: f32 = (i..(i + step).min(end)).map(|j| self.frames[j].1 as f32).sum();
            if let Some(fixed) = o.fixed_delay.filter(|d| *d > 0) {
                delay = fixed as f32;
            }
            let mut delay = (delay / o.speed.max(0.05)).round() as u32;
            if let Some(d) = o.delays.get(&i) {
                delay = *d;
            }
            picked.push((i, delay.clamp(MIN_DELAY, MAX_DELAY)));
        }
        match o.order {
            Order::Forward => {}
            Order::Backward => picked.reverse(),
            Order::Bounce => {
                let back: Vec<(usize, u32)> = picked.iter().rev().skip(1).take(picked.len().saturating_sub(2)).copied().collect();
                picked.extend(back);
                picked.truncate(max_frames);
            }
        }
        if let Some(px) = o.text_scroll.filter(|_| !o.text.trim().is_empty() && !picked.is_empty()) {
            // Enough frames for the text to cross the whole screen once; a still picture repeats
            let needed = (W as f32 + caption_width(o)) / px.max(1) as f32;
            let total = (needed.ceil() as usize).max(picked.len()).min(max_frames);
            let still = n == 1;
            picked = (0..total).map(|k| {
                let (i, d) = picked[k % picked.len()];
                (i, if still { o.fixed_delay.filter(|d| *d > 0).unwrap_or(SCROLL_DELAY) } else { d })
            }).collect();
        }
        picked
    }

    /// How many frames the options select before the screen's frame limit is applied.
    pub fn available(&self, o: &RenderOptions) -> usize {
        self.select(o, usize::MAX).len()
    }

    /// Frame `k` of `picked` (what `select` gave): its source frame, with a scrolling caption where
    /// it is at that moment.
    pub fn render_frame(&self, picked: &[(usize, u32)], k: usize, o: &RenderOptions) -> RgbImage {
        let progress = (o.text_scroll.is_some() && picked.len() > 1).then(|| k as f32 / picked.len() as f32);
        render_at(&self.frames[picked[k].0].0, o, progress)
    }

    pub fn render(&self, o: &RenderOptions, max_frames: usize) -> Vec<(RgbImage, u32)> {
        let picked = self.select(o, max_frames);
        (0..picked.len()).map(|k| (self.render_frame(&picked, k, o), picked[k].1)).collect()
    }
}

/// Grey value as Pillow computes it (ITU-R 601-2 luma).
fn luma(p: [u8; 3]) -> u32 {
    (p[0] as u32 * 19595 + p[1] as u32 * 38470 + p[2] as u32 * 7471 + 0x8000) >> 16
}

fn blend(a: f32, b: f32, f: f32) -> u8 {
    (a + (b - a) * f).round().clamp(0.0, 255.0) as u8
}

/// Scales a frame to 240×135 and applies the adjustments.
pub fn render(img: &RgbaImage, o: &RenderOptions) -> RgbImage {
    render_at(img, o, None)
}

/// The part of `img` a zoom shows: `1 / zoom` of each side, moved by `pan` within what is left over.
fn zoomed(img: &RgbaImage, zoom: f32, pan: (f32, f32)) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    let zoom = zoom.clamp(1.0, 8.0);
    let (cw, ch) = (((iw as f32 / zoom).round() as u32).max(1), ((ih as f32 / zoom).round() as u32).max(1));
    let x = ((iw - cw) as f32 * (pan.0.clamp(-1.0, 1.0) + 1.0) / 2.0).round() as u32;
    let y = ((ih - ch) as f32 * (pan.1.clamp(-1.0, 1.0) + 1.0) / 2.0).round() as u32;
    imageops::crop_imm(img, x, y, cw, ch).to_image()
}

/// Turns the hue of `p` by `degrees`, keeping its brightness (a rotation about the grey axis).
fn turn_hue(p: [u8; 3], degrees: f32) -> [u8; 3] {
    let (c, s) = (degrees.to_radians().cos(), degrees.to_radians().sin());
    let k = (1.0 - c) / 3.0;
    let r3 = (1.0f32 / 3.0).sqrt() * s;
    let m = [[c + k, k - r3, k + r3], [k + r3, c + k, k - r3], [k - r3, k + r3, c + k]];
    let v = p.map(|x| x as f32);
    let ch = |row: [f32; 3]| (row[0] * v[0] + row[1] * v[1] + row[2] * v[2]).round().clamp(0.0, 255.0) as u8;
    [ch(m[0]), ch(m[1]), ch(m[2])]
}

/// Like `render`; `scroll` (0..1) is how far a scrolling caption has gone across.
pub fn render_at(img: &RgbaImage, o: &RenderOptions, scroll: Option<f32>) -> RgbImage {
    let turned;
    let img = match o.rotate % 4 {
        1 => { turned = imageops::rotate90(img); &turned }
        2 => { turned = imageops::rotate180(img); &turned }
        3 => { turned = imageops::rotate270(img); &turned }
        _ => img,
    };
    let cropped;
    let img = if o.zoom > 1.0 { cropped = zoomed(img, o.zoom, o.pan); &cropped } else { img };
    let filter = if o.sharp { FilterType::Nearest } else { FilterType::Lanczos3 };
    let (iw, ih) = img.dimensions();
    let scaled = match o.fit {
        Fit::Stretch => imageops::resize(img, W, H, filter),
        fit => {
            let (sx, sy) = (W as f32 / iw as f32, H as f32 / ih as f32);
            let s = if fit == Fit::Fit { sx.min(sy) } else { sx.max(sy) };
            let nw = ((iw as f32 * s).round() as u32).max(1);
            let nh = ((ih as f32 * s).round() as u32).max(1);
            imageops::resize(img, nw, nh, filter)
        }
    };
    let [br, bg, bb] = o.background;
    let mut canvas = RgbaImage::from_pixel(W, H, Rgba([br, bg, bb, 255]));
    let ox = (W as i64 - scaled.width() as i64).div_euclid(2);
    let oy = (H as i64 - scaled.height() as i64).div_euclid(2);
    imageops::overlay(&mut canvas, &scaled, ox, oy);
    let mut out = RgbImage::from_fn(W, H, |x, y| {
        let p = canvas.get_pixel(x, y).0;
        Rgb([p[0], p[1], p[2]])
    });
    if o.flip_h {
        imageops::flip_horizontal_in_place(&mut out);
    }
    if o.flip_v {
        imageops::flip_vertical_in_place(&mut out);
    }
    if o.brightness != 1.0 {
        for p in out.pixels_mut() {
            for c in &mut p.0 {
                *c = blend(0.0, *c as f32, o.brightness);
            }
        }
    }
    if o.contrast != 1.0 {
        let mean = (out.pixels().map(|p| luma(p.0) as f64).sum::<f64>() / (W * H) as f64 + 0.5).floor() as f32;
        for p in out.pixels_mut() {
            for c in &mut p.0 {
                *c = blend(mean, *c as f32, o.contrast);
            }
        }
    }
    if o.saturation != 1.0 {
        for p in out.pixels_mut() {
            let grey = luma(p.0) as f32;
            for c in &mut p.0 {
                *c = blend(grey, *c as f32, o.saturation);
            }
        }
    }
    match o.tone {
        Tone::Natural => {}
        Tone::Grey => {
            for p in out.pixels_mut() {
                let g = luma(p.0) as u8;
                p.0 = [g, g, g];
            }
        }
        Tone::Sepia => {
            for p in out.pixels_mut() {
                let [r, g, b] = p.0.map(|c| c as f32);
                let ch = |a: f32, bb: f32, c: f32| (r * a + g * bb + b * c).round().clamp(0.0, 255.0) as u8;
                p.0 = [ch(0.393, 0.769, 0.189), ch(0.349, 0.686, 0.168), ch(0.272, 0.534, 0.131)];
            }
        }
        Tone::Negative => {
            for p in out.pixels_mut() {
                p.0 = p.0.map(|c| 255 - c);
            }
        }
    }
    if o.hue % 360 != 0 {
        for p in out.pixels_mut() {
            p.0 = turn_hue(p.0, o.hue as f32);
        }
    }
    if !o.text.is_empty() {
        draw_caption(&mut out, o, scroll);
    }
    out
}

/// How wide the caption is, in pixels.
fn caption_width(o: &RenderOptions) -> f32 {
    let Ok(font) = FontRef::try_from_slice(CAPTION_FONT) else { return 0.0 };
    let (_, [left, _, right, _]) = layout_text(&font, &o.text, o.text_size.max(6) as f32);
    right - left
}

/// Laid-out glyphs of `text` and their ink box (min x, min y, max x, max y) at the origin.
fn layout_text(font: &FontRef<'_>, text: &str, size: f32) -> (Vec<ab_glyph::Glyph>, [f32; 4]) {
    let px = size * font.height_unscaled() / font.units_per_em().unwrap_or(1000.0);
    let scaled = font.as_scaled(PxScale::from(px));
    let mut glyphs = Vec::new();
    let mut caret = 0.0f32;
    let mut prev = None;
    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(p) = prev {
            caret += scaled.kern(p, id);
        }
        glyphs.push(id.with_scale_and_position(px, point(caret, scaled.ascent())));
        caret += scaled.h_advance(id);
        prev = Some(id);
    }
    let mut bbox = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for g in &glyphs {
        if let Some(o) = font.outline_glyph(g.clone()) {
            let b = o.px_bounds();
            bbox = [bbox[0].min(b.min.x), bbox[1].min(b.min.y), bbox[2].max(b.max.x), bbox[3].max(b.max.y)];
        }
    }
    if bbox[0] > bbox[2] {
        bbox = [0.0; 4];
    }
    (glyphs, bbox)
}

fn draw_glyphs(img: &mut RgbImage, font: &FontRef<'_>, glyphs: &[ab_glyph::Glyph], dx: f32, dy: f32, color: [u8; 3]) {
    for g in glyphs {
        let mut g = g.clone();
        g.position = point(g.position.x + dx, g.position.y + dy);
        let Some(o) = font.outline_glyph(g) else { continue };
        let b = o.px_bounds();
        o.draw(|x, y, cov| {
            let (px, py) = (b.min.x as i32 + x as i32, b.min.y as i32 + y as i32);
            if px < 0 || py < 0 || px >= W as i32 || py >= H as i32 {
                return;
            }
            let p = img.get_pixel_mut(px as u32, py as u32);
            let a = cov.clamp(0.0, 1.0);
            for (c, t) in p.0.iter_mut().zip(color) {
                *c = (*c as f32 * (1.0 - a) + t as f32 * a).round() as u8;
            }
        });
    }
}

fn draw_caption(img: &mut RgbImage, o: &RenderOptions, scroll: Option<f32>) {
    let Ok(font) = FontRef::try_from_slice(CAPTION_FONT) else { return };
    let (glyphs, [left, top, right, bottom]) = layout_text(&font, &o.text, o.text_size.max(6) as f32);
    let (tw, th) = (right - left, bottom - top);
    let m = 6.0;
    let pos = o.text_pos.as_str();
    let vert = if pos.starts_with("top") { 0 } else if pos.starts_with("bottom") { 2 } else { 1 };
    let horz = if pos.ends_with("left") { 0 } else if pos.ends_with("right") { 2 } else { 1 };
    let x = match scroll {
        // From just off the right edge to just off the left one
        Some(t) => (W as f32 - t * (W as f32 + tw)).round(),
        None => [m, ((W as f32 - tw) / 2.0).floor(), W as f32 - tw - m][horz],
    } - left;
    let y = [m, ((H as f32 - th) / 2.0).floor(), H as f32 - th - m][vert] - top;
    if o.text_outline {
        for (dx, dy) in [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (-1.0, 0.0), (1.0, 0.0), (-1.0, 1.0), (0.0, 1.0), (1.0, 1.0)] {
            draw_glyphs(img, &font, &glyphs, x + dx, y + dy, [0, 0, 0]);
        }
    } else {
        draw_glyphs(img, &font, &glyphs, x + 1.0, y + 1.0, [0, 0, 0]);
    }
    draw_glyphs(img, &font, &glyphs, x, y, o.text_color);
}

const BAYER4: [[f32; 4]; 4] = [[0.0, 8.0, 2.0, 10.0], [12.0, 4.0, 14.0, 6.0], [3.0, 11.0, 1.0, 9.0], [15.0, 7.0, 13.0, 5.0]];

/// RGB565 values, rows top to bottom. Ordered (Bayer) dithering adds up to half a quantisation step:
/// 8 for red and blue, 4 for green.
pub fn quantize(img: &RgbImage, dither: bool) -> Vec<u16> {
    let mut out = Vec::with_capacity((img.width() * img.height()) as usize);
    for (x, y, p) in img.enumerate_pixels() {
        let t = if dither { BAYER4[(y % 4) as usize][(x % 4) as usize] / 16.0 - 0.5 } else { 0.0 };
        let ch = |v: u8, step: f32| -> u16 { ((v as f32 + t * step) + 0.5).clamp(0.0, 255.0) as u16 };
        let (r, g, b) = (ch(p[0], 8.0), ch(p[1], 4.0), ch(p[2], 8.0));
        out.push(((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3));
    }
    out
}

/// RGB565 little-endian, rows top to bottom.
pub fn to_rgb565(img: &RgbImage, dither: bool) -> Vec<u8> {
    quantize(img, dither).into_iter().flat_map(u16::to_le_bytes).collect()
}

/// What a frame looks like on the panel after the RGB565 conversion.
pub fn preview_565(img: &RgbImage, dither: bool) -> RgbImage {
    let v = quantize(img, dither);
    RgbImage::from_fn(img.width(), img.height(), |x, y| {
        let c = v[(y * img.width() + x) as usize];
        let (r, g, b) = (((c >> 11) & 0x1F) as u8, ((c >> 5) & 0x3F) as u8, (c & 0x1F) as u8);
        Rgb([(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)])
    })
}

pub fn page_count(n_frames: usize) -> usize {
    let n = n_frames.min(LCD_MAX_FRAMES);
    (LCD_HEADER + n * LCD_WIDTH * LCD_HEIGHT * 2).div_ceil(LCD_PAGE)
}

pub fn encode_animation(frames: &[(RgbImage, u32)], dither: bool, progress: &mut dyn FnMut(f32)) -> Result<Vec<u8>> {
    if frames.is_empty() {
        return Err(Error::invalid(tr!("No frames to upload")));
    }
    let frames = &frames[..frames.len().min(LCD_MAX_FRAMES)];
    let frame_bytes = LCD_WIDTH * LCD_HEIGHT * 2;
    let pages = page_count(frames.len());
    let mut blob = vec![0xFFu8; pages * LCD_PAGE];
    blob[0] = frames.len() as u8;
    for (i, (_, delay)) in frames.iter().enumerate() {
        blob[1 + i] = (delay / 2).clamp(1, 255) as u8;
    }
    for (i, (img, _)) in frames.iter().enumerate() {
        let owned;
        let img = if img.dimensions() == (W, H) {
            img
        } else {
            let rgba = image::DynamicImage::ImageRgb8(img.clone()).to_rgba8();
            owned = render(&rgba, &RenderOptions::default());
            &owned
        };
        let start = LCD_HEADER + i * frame_bytes;
        blob[start..start + frame_bytes].copy_from_slice(&to_rgb565(img, dither));
        progress((i + 1) as f32 / frames.len() as f32);
    }
    Ok(blob)
}

/// Writes frames as an animated GIF.
pub fn save_gif(frames: &[(RgbImage, u32)], path: &Path) -> Result<()> {
    let file = File::create(path)?;
    let mut enc = GifEncoder::new_with_speed(file, 10);
    enc.set_repeat(Repeat::Infinite).map_err(|e| Error::invalid(e.to_string()))?;
    let gif_frames = frames.iter().map(|(img, d)| {
        let rgba = image::DynamicImage::ImageRgb8(img.clone()).to_rgba8();
        Frame::from_parts(rgba, 0, 0, Delay::from_numer_denom_ms(*d, 1))
    });
    enc.encode_frames(gif_frames).map_err(|e| Error::invalid(e.to_string()))?;
    Ok(())
}

/// Keeps a copy of every upload: the keyboard cannot give an animation back.
pub fn save_history(frames: &[(RgbImage, u32)], source: Option<&Path>) -> Result<PathBuf> {
    let base = source
        .and_then(|p| p.file_stem().or_else(|| p.file_name()))
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "animation".into());
    let folder = paths::history_dir().join(format!("{}_{base}", LocalTime::now().file_stamp()));
    fs::create_dir_all(&folder)?;
    save_gif(frames, &folder.join("screen.gif"))?;
    if let Some(src) = source.filter(|p| p.is_file()) {
        let ext = src.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        fs::copy(src, folder.join(format!("original{ext}")))?;
    }
    Ok(folder)
}

/// Saved uploads, newest first (paths to their screen.gif).
pub fn history_entries() -> Vec<PathBuf> {
    let mut names: Vec<PathBuf> = fs::read_dir(paths::history_dir())
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    names.sort();
    names.reverse();
    names.into_iter().map(|d| d.join("screen.gif")).filter(|g| g.is_file()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_layout() {
        let red = RgbImage::from_pixel(W, H, Rgb([255, 0, 0]));
        let blob = encode_animation(&[(red.clone(), 100), (red, 40)], false, &mut |_| {}).unwrap();
        assert_eq!(blob.len() % 4096, 0);
        assert_eq!(&blob[..3], &[2, 50, 20]);
        assert_eq!(&blob[256..258], &0xF800u16.to_le_bytes());
        assert_eq!(page_count(2), blob.len() / 4096);
        assert_eq!(blob[blob.len() - 1], 0xFF, "padding");
    }

    #[test]
    fn preview_round_trip() {
        let img = RgbImage::from_pixel(W, H, Rgb([0, 255, 0]));
        assert_eq!(preview_565(&img, false).get_pixel(10, 10).0, [0, 255, 0]);
    }

    #[test]
    fn frame_selection() {
        let src = Source {
            frames: (0..10).map(|i| (RgbaImage::from_pixel(10, 10, Rgba([i * 20, 0, 0, 255])), 50)).collect(),
        };
        let mut o = RenderOptions { frame_step: 2, trim_start: 1, ..Default::default() };
        let picked = src.select(&o, LCD_MAX_FRAMES);
        assert_eq!(picked.iter().map(|p| p.0).collect::<Vec<_>>(), [1, 3, 5, 7, 9]);
        assert_eq!(picked[0].1, 100, "two 50 ms frames merged");
        o.deleted.insert(3);
        o.delays.insert(5, 300);
        assert_eq!(&src.select(&o, LCD_MAX_FRAMES)[1..3], &[(5, 300), (7, 100)]);
    }

    #[test]
    fn render_fits_and_captions() {
        let tall = RgbaImage::from_pixel(10, 100, Rgba([255, 255, 255, 255]));
        let out = render(&tall, &RenderOptions::default());
        assert_eq!(out.dimensions(), (W, H));
        assert_eq!(out.get_pixel(0, 0).0, [0, 0, 0], "bars at the sides");
        assert_eq!(out.get_pixel(120, 67).0, [255, 255, 255]);
        let mut o = RenderOptions { text: "12:34".into(), text_pos: "top-left".into(), ..Default::default() };
        o.background = [0, 0, 255];
        let black = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 255, 0]));
        let cap = render(&black, &o);
        let white = cap.pixels().filter(|p| p.0 == [255, 255, 255]).count();
        assert!(white > 30, "the caption is drawn ({white} px)");
        assert!(cap.get_pixel(200, 120).0 == [0, 0, 255], "only in its corner");
    }

    #[test]
    fn turning_zooming_and_tones() {
        // A picture wider than tall, red on the left half and blue on the right
        let img = RgbaImage::from_fn(200, 100, |x, _| if x < 100 { Rgba([255, 0, 0, 255]) } else { Rgba([0, 0, 255, 255]) });
        let stretch = RenderOptions { fit: Fit::Stretch, dither: false, ..RenderOptions::default() };
        let turned = render(&img, &RenderOptions { rotate: 1, ..stretch.clone() });
        assert_eq!(turned.get_pixel(120, 5).0, [255, 0, 0], "a quarter turn clockwise: the left half goes to the top");
        assert_eq!(turned.get_pixel(120, 130).0, [0, 0, 255]);
        let right = render(&img, &RenderOptions { zoom: 2.0, pan: (1.0, 0.0), ..stretch.clone() });
        assert!(right.pixels().all(|p| p.0 == [0, 0, 255]), "zoomed into the right edge: only blue");
        let left = render(&img, &RenderOptions { zoom: 2.0, pan: (-1.0, 0.0), ..stretch.clone() });
        assert!(left.pixels().all(|p| p.0 == [255, 0, 0]));
        assert_eq!(render(&img, &RenderOptions { tone: Tone::Negative, ..stretch.clone() }).get_pixel(5, 5).0, [0, 255, 255]);
        let grey = render(&img, &RenderOptions { tone: Tone::Grey, ..stretch.clone() }).get_pixel(5, 5).0;
        assert!(grey[0] == grey[1] && grey[1] == grey[2]);
        let turned_hue = render(&img, &RenderOptions { hue: 120, ..stretch.clone() }).get_pixel(5, 5).0;
        assert!(turned_hue[1] > 200 && turned_hue[0] < 60, "red turned by 120° is green: {turned_hue:?}");
        let tiny = RgbaImage::from_fn(2, 1, |x, _| if x == 0 { Rgba([0, 0, 0, 255]) } else { Rgba([255, 255, 255, 255]) });
        let sharp = render(&tiny, &RenderOptions { sharp: true, ..stretch.clone() });
        assert!(sharp.pixels().all(|p| p.0 == [0, 0, 0] || p.0 == [255, 255, 255]), "no smoothing between pixels");
    }

    #[test]
    fn play_order_and_scrolling_text() {
        let frames = (0..5).map(|i| (RgbaImage::from_pixel(4, 4, Rgba([i * 40, 0, 0, 255])), 100)).collect();
        let src = Source { frames };
        let order = |o: Order| src.select(&RenderOptions { order: o, ..RenderOptions::default() }, 140)
            .iter().map(|(i, _)| *i).collect::<Vec<_>>();
        assert_eq!(order(Order::Backward), vec![4, 3, 2, 1, 0]);
        assert_eq!(order(Order::Bounce), vec![0, 1, 2, 3, 4, 3, 2, 1], "the turning frames once");

        let still = Source { frames: vec![(RgbaImage::from_pixel(240, 135, Rgba([0, 0, 0, 255])), 100)] };
        let o = RenderOptions { text: "Hello".into(), text_scroll: Some(6), text_size: 24, dither: false,
                                ..RenderOptions::default() };
        let picked = still.select(&o, 140);
        let expected = ((240.0 + caption_width(&o)) / 6.0).ceil() as usize;
        assert_eq!(picked.len(), expected, "enough frames to cross the screen");
        assert!(picked.iter().all(|(i, d)| *i == 0 && *d == SCROLL_DELAY));
        // The text moves to the left from frame to frame
        let lit = |k: usize| {
            let img = still.render_frame(&picked, k, &o);
            (0..240).find(|&x| (0..135).any(|y| img.get_pixel(x, y).0[0] > 128))
        };
        let (a, b) = (lit(expected / 3), lit(expected / 2));
        assert!(a.is_some() && b.is_some() && b < a, "{a:?} {b:?}");
        assert_eq!(still.select(&RenderOptions { text_scroll: Some(6), ..RenderOptions::default() }, 140).len(), 1,
                   "no text, nothing to scroll");
    }

    #[test]
    fn gif_round_trip() {
        let dir = std::env::temp_dir().join(format!("ak980-lcd-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.gif");
        let frames: Vec<(RgbImage, u32)> =
            (0..3).map(|i| (RgbImage::from_pixel(W, H, Rgb([i * 80, 0, 0])), 120)).collect();
        save_gif(&frames, &path).unwrap();
        let src = Source::load(&path, 1000).unwrap();
        assert_eq!(src.frames.len(), 3);
        assert_eq!(src.frames[0].1, 120);
        let _ = fs::remove_dir_all(&dir);
    }
}
