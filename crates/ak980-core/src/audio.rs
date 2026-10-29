//! What the PC is playing, as a spectrum for the music effects.
//!
//! Records the default output's monitor with `parec` (PulseAudio and PipeWire's pulse server) or
//! `pw-record` (plain PipeWire). No audio library is needed; the samples arrive on a pipe.

use std::f32::consts::PI;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};

use crate::service::{run, which};

pub const RATE: usize = 48000;
/// About 21 ms per analysis window.
pub const CHUNK: usize = 1024;
pub const BANDS: usize = 16;

fn default_monitor() -> Option<String> {
    which("pactl")?;
    let sink = run(&["pactl", "get-default-sink"], Duration::from_secs(2)).stdout.trim().to_string();
    (!sink.is_empty()).then(|| format!("{sink}.monitor"))
}

/// The recorder to run, or None when neither parec nor pw-record is installed.
pub fn capture_command() -> Option<Vec<String>> {
    let rate = format!("--rate={RATE}");
    if which("parec").is_some() {
        let mut cmd: Vec<String> = ["parec", "--raw", "--format=float32le", &rate, "--channels=1", "--latency-msec=20"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        if let Some(monitor) = default_monitor() {
            cmd.push(format!("--device={monitor}"));
        }
        return Some(cmd);
    }
    if which("pw-record").is_some() {
        return Some(
            ["pw-record", "-P", "{ stream.capture.sink=true }", "--format=f32", &rate, "--channels=1", "-"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );
    }
    None
}

pub fn available() -> bool {
    capture_command().is_some()
}

/// Turns windows of samples into smoothed, self-levelling spectrum bands, loudness and beats.
pub struct Analyser {
    pub bands: Vec<f32>,
    pub level: f32,
    pub beat: bool,
    peak: Vec<f32>,
    avg_energy: f32,
    bins: Vec<Vec<usize>>,
    window: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
}

impl Analyser {
    pub fn new(bands: usize) -> Self {
        // Logarithmic band edges from 40 Hz to 16 kHz over the FFT's bin frequencies
        let edges: Vec<f64> = (0..=bands).map(|i| 40.0 * (16000.0f64 / 40.0).powf(i as f64 / bands as f64)).collect();
        let freq = |k: usize| k as f64 * RATE as f64 / CHUNK as f64;
        let bins = (0..bands)
            .map(|b| (0..=CHUNK / 2).filter(|&k| freq(k) >= edges[b] && freq(k) < edges[b + 1]).collect())
            .collect();
        let window = (0..CHUNK).map(|n| 0.5 - 0.5 * (2.0 * PI * n as f32 / (CHUNK - 1) as f32).cos()).collect();
        Self {
            bands: vec![0.0; bands],
            level: 0.0,
            beat: false,
            peak: vec![1e-3; bands],
            avg_energy: 0.0,
            bins,
            window,
            fft: FftPlanner::new().plan_fft_forward(CHUNK),
        }
    }

    /// Takes one window of CHUNK samples.
    pub fn analyse(&mut self, samples: &[f32]) {
        let mut buf: Vec<Complex32> =
            samples.iter().zip(&self.window).map(|(s, w)| Complex32::new(s * w, 0.0)).collect();
        buf.resize(CHUNK, Complex32::new(0.0, 0.0));
        self.fft.process(&mut buf);
        let spec: Vec<f32> = buf[..=CHUNK / 2].iter().map(|c| c.norm()).collect();
        let energy = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt();
        for (b, bins) in self.bins.iter().enumerate() {
            let mean = if bins.is_empty() { 0.0 } else { bins.iter().map(|&k| spec[k]).sum::<f32>() / bins.len() as f32 };
            let raw = (mean * 4.0).ln_1p();
            // Automatic gain per band: follow the peak, let it relax slowly
            self.peak[b] = (self.peak[b] * 0.995).max(raw);
            let norm = (raw / self.peak[b].max(0.3)).clamp(0.0, 1.0);
            // Fast rise, slower fall, so the bars do not flicker
            self.bands[b] = if norm > self.bands[b] {
                norm * 0.7 + self.bands[b] * 0.3
            } else {
                self.bands[b] * 0.82 + norm * 0.18
            };
        }
        self.level = (energy * 5.0).min(1.0);
        self.avg_energy = self.avg_energy * 0.95 + energy * 0.05;
        if energy > self.avg_energy * 1.6 && energy > 0.02 {
            self.beat = true;
        }
    }
}

#[derive(Default)]
struct Shared {
    bands: Vec<f32>,
    level: f32,
    beat: bool,
    error: Option<String>,
}

/// Runs the recorder in the background and keeps the latest spectrum.
pub struct AudioMeter {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    child: Option<Child>,
    thread: Option<JoinHandle<()>>,
}

impl Default for AudioMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioMeter {
    pub fn new() -> Self {
        let shared = Shared { bands: vec![0.0; BANDS], ..Shared::default() };
        Self { shared: Arc::new(Mutex::new(shared)), stop: Arc::default(), child: None, thread: None }
    }

    pub fn running(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    /// "no-recorder", "recorder-exited" or a start error, once something went wrong.
    pub fn error(&self) -> Option<String> {
        self.shared.lock().unwrap().error.clone()
    }

    pub fn start(&mut self) -> bool {
        if self.running() {
            return true;
        }
        let Some(cmd) = capture_command() else {
            self.shared.lock().unwrap().error = Some("no-recorder".into());
            return false;
        };
        let mut child = match Command::new(&cmd[0]).args(&cmd[1..]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() {
            Ok(c) => c,
            Err(e) => {
                self.shared.lock().unwrap().error = Some(e.to_string());
                return false;
            }
        };
        let Some(mut stream) = child.stdout.take() else { return false };
        self.child = Some(child);
        self.stop.store(false, Ordering::Relaxed);
        let (shared, stop) = (self.shared.clone(), self.stop.clone());
        self.thread = std::thread::Builder::new()
            .name("ak980-audio".into())
            .spawn(move || {
                let mut analyser = Analyser::new(BANDS);
                let mut bytes = vec![0u8; CHUNK * 4];
                while !stop.load(Ordering::Relaxed) {
                    if stream.read_exact(&mut bytes).is_err() {
                        if !stop.load(Ordering::Relaxed) {
                            shared.lock().unwrap().error = Some("recorder-exited".into());
                        }
                        return;
                    }
                    let samples: Vec<f32> =
                        bytes.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
                    analyser.analyse(&samples);
                    let mut s = shared.lock().unwrap();
                    s.bands.clone_from(&analyser.bands);
                    s.level = analyser.level;
                    s.beat |= std::mem::take(&mut analyser.beat);
                }
            })
            .ok();
        true
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(mut child) = self.child.take() {
            // SAFETY: SIGTERM to our own child, by the PID its handle holds
            unsafe { libc::kill(child.id() as i32, libc::SIGTERM) };
            let deadline = Instant::now() + Duration::from_secs(1);
            while child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join(); // the pipe closed with the recorder, so the read returns
        }
        let mut s = self.shared.lock().unwrap();
        s.bands.iter_mut().for_each(|b| *b = 0.0);
        s.level = 0.0;
    }

    /// Bands, loudness and whether a beat came since the last snapshot.
    pub fn snapshot(&self) -> (Vec<f32>, f32, bool) {
        let mut s = self.shared.lock().unwrap();
        let beat = std::mem::take(&mut s.beat);
        (s.bands.clone(), s.level, beat)
    }
}

impl Drop for AudioMeter {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, amp: f32) -> Vec<f32> {
        (0..CHUNK).map(|n| amp * (2.0 * PI * freq * n as f32 / RATE as f32).sin()).collect()
    }

    #[test]
    fn a_tone_lands_in_its_band() {
        // Every band levels itself against its own peak, so after one window the tone's band (and a
        // neighbour the window leaks into) is high while distant bands stay near zero
        let mut a = Analyser::new(BANDS);
        a.analyse(&sine(1000.0, 0.5));
        let edges: Vec<f32> = (0..=BANDS).map(|i| 40.0 * 400f32.powf(i as f32 / BANDS as f32)).collect();
        let band = (0..BANDS).find(|&b| edges[b] <= 1000.0 && 1000.0 < edges[b + 1]).unwrap();
        assert!(a.bands[band] > 0.5, "{:?}", a.bands);
        assert!(a.bands[0] < 0.1 && a.bands[BANDS - 1] < 0.1, "{:?}", a.bands);
        assert!(a.level > 0.5 && a.beat);
    }
}
