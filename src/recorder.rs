//! One recording: a pausable clock, capture threads, the audio mixer and the writers.
use crate::audio::AudioSource;
use crate::config::{Rect, LOW_DISK_BYTES, VIDEO_BPP};
use crate::encode::*;
use crate::screen::ScreenGrabber;
use crate::win;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const TEMP_PREFIX: &str = "trecRec";
pub const MAIN_FILE: &str = "cs_recording.mp4";
pub const MARKER_FILE: &str = "markers.txt";

#[derive(Default)]
struct ClockState {
    done: Duration,
    since: Option<Instant>,
}

#[derive(Default)]
pub struct Clock(Mutex<ClockState>);
impl Clock {
    pub fn run(&self) {
        let mut c = self.0.lock().unwrap();
        if c.since.is_none() {
            c.since = Some(Instant::now());
        }
    }
    pub fn pause(&self) {
        let mut c = self.0.lock().unwrap();
        if let Some(s) = c.since.take() {
            c.done += s.elapsed();
        }
    }
    pub fn running(&self) -> bool {
        self.0.lock().unwrap().since.is_some()
    }
    pub fn elapsed(&self) -> Duration {
        let c = self.0.lock().unwrap();
        c.done + c.since.map(|s| s.elapsed()).unwrap_or_default()
    }
    fn ticks(&self) -> i64 {
        (self.elapsed().as_nanos() / 100) as i64
    }
}

/// What other threads (shortcuts, the command pipe) may touch while recording.
#[derive(Default)]
pub struct Shared {
    pub clock: Clock,
    pub stop: AtomicBool,
    pub markers: Mutex<Vec<Duration>>,
    pub error: Mutex<Option<String>>,
    pub frames: AtomicU64,
    pub dropped: AtomicU64,
    pub pause_cursor: Mutex<Option<(i32, i32)>>,
    pub restore_cursor: AtomicBool,
    pub screen_on: AtomicBool,
}

impl Shared {
    pub fn fail(&self, msg: String) {
        let mut e = self.error.lock().unwrap();
        if e.is_none() {
            *e = Some(msg);
        }
    }
    pub fn pause(&self) {
        self.clock.pause();
        *self.pause_cursor.lock().unwrap() = Some(win::cursor_pos());
    }
    pub fn resume(&self) {
        if self.restore_cursor.load(Ordering::Relaxed) && self.screen_on.load(Ordering::Relaxed) {
            if let Some((x, y)) = self.pause_cursor.lock().unwrap().take() {
                win::set_cursor_pos(x, y);
            }
        }
        self.clock.run();
    }
    pub fn add_marker(&self) {
        // The original only adds markers when the screen is being recorded.
        if self.screen_on.load(Ordering::Relaxed) {
            self.markers.lock().unwrap().push(self.clock.elapsed());
        }
    }
}

pub struct SessionConfig {
    pub temp_dir: PathBuf,
    pub region: Option<Rect>,
    pub fps: u32,
    pub quality: u32,
    pub system: Option<Arc<AudioSource>>,
    pub restore_cursor: bool,
}

pub struct Session {
    pub shared: Arc<Shared>,
    pub temp_dir: PathBuf,
    threads: Vec<std::thread::JoinHandle<()>>,
    writers: Vec<Writer>,
    audio: Vec<Arc<AudioSource>>,
}

impl Session {
    /// Starts paused; call `shared.resume()` to begin (the countdown does this).
    pub fn start(cfg: SessionConfig) -> Result<Session, String> {
        let shared = Arc::new(Shared::default());
        shared.restore_cursor.store(cfg.restore_cursor, Ordering::Relaxed);
        shared.screen_on.store(cfg.region.is_some(), Ordering::Relaxed);
        std::fs::create_dir_all(&cfg.temp_dir).map_err(|e| format!("Failed to Set Temporary Folder to Write ({e})"))?;
        let bpp = VIDEO_BPP;
        let has_audio = cfg.system.is_some();
        let mut threads = Vec::new();
        let mut writers = Vec::new();
        let mut audio_tx = None;

        if let Some(region) = cfg.region {
            let region = region.even();
            let spec = VideoSpec { width: region.w as u32, height: region.h as u32, fps: cfg.fps, bits_per_pixel: bpp, quality: cfg.quality };
            let w = Writer::spawn(&cfg.temp_dir.join(MAIN_FILE), spec, has_audio)?;
            audio_tx = Some(w.tx.clone());
            let (tx, sh, fps) = (w.tx.clone(), shared.clone(), cfg.fps);
            writers.push(w);
            let grabber = ScreenGrabber::new(region)?;
            threads.push(std::thread::spawn(move || screen_loop(grabber, region, fps, tx, sh)));
        }
        let Some(primary) = audio_tx else { return Err("Invalid Input".into()) };
        let audio: Vec<_> = cfg.system.iter().cloned().collect();
        if has_audio {
            for a in &audio {
                a.queue.lock().unwrap().clear();
                a.capture_on.store(true, Ordering::Relaxed);
            }
            let (sources, sh) = (audio.clone(), shared.clone());
            threads.push(std::thread::spawn(move || mixer_loop(sources, primary, sh)));
        }
        // Low disk watch, like the original's "Not Enough Space On Disk".
        let (sh, dir) = (shared.clone(), cfg.temp_dir.clone());
        threads.push(std::thread::spawn(move || {
            while !sh.stop.load(Ordering::Relaxed) {
                if win::free_disk(&dir).is_some_and(|f| f < LOW_DISK_BYTES) {
                    sh.fail("Not Enough Space On Disk".into());
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }));
        Ok(Session { shared, temp_dir: cfg.temp_dir, threads, writers, audio })
    }

    /// Stops everything and closes the files. The recording stays in the temp folder.
    pub fn finish(self) -> Result<(), String> {
        self.shared.clock.pause();
        self.shared.stop.store(true, Ordering::Relaxed);
        for t in self.threads {
            let _ = t.join();
        }
        for a in &self.audio {
            a.capture_on.store(false, Ordering::Relaxed);
        }
        let mut result = Ok(());
        for w in self.writers {
            let _ = w.tx.send(Packet::Finish);
            drop(w.tx);
            if let Ok(Err(e)) = w.handle.join() {
                result = Err(e);
            }
        }
        let markers = self.shared.markers.lock().unwrap();
        if !markers.is_empty() {
            let mut text = String::new();
            for (i, m) in markers.iter().enumerate() {
                let t = m.as_secs_f64();
                text += &format!("{:02}:{:02}:{:06.3}\tMarker {}\n", (t / 3600.0) as u32, (t / 60.0) as u32 % 60, t % 60.0, i + 1);
            }
            let _ = std::fs::write(self.temp_dir.join(MARKER_FILE), text);
        }
        result
    }
}

fn screen_loop(mut g: ScreenGrabber, region: Rect, fps: u32, tx: std::sync::mpsc::SyncSender<Packet>, sh: Arc<Shared>) {
    let step = Duration::from_secs_f64(1.0 / fps as f64);
    let duration = 10_000_000 / fps as i64;
    let mut next = Instant::now();
    let (w, h) = (region.w as usize, region.h as usize);
    while !sh.stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        if now < next {
            std::thread::sleep(next - now);
        }
        next += step;
        if next < Instant::now() {
            next = Instant::now() + step; // fell behind; don't try to catch up
        }
        let bgra = g.frame();
        if !sh.clock.running() {
            continue;
        }
        let time = sh.clock.ticks();
        let mut nv12 = Vec::new();
        bgra_to_nv12(bgra, w * 4, w, h, &mut nv12);
        send_video(&tx, nv12, time, duration, &sh);
    }
}

fn send_video(tx: &std::sync::mpsc::SyncSender<Packet>, nv12: Vec<u8>, time: i64, duration: i64, sh: &Shared) {
    match tx.try_send(Packet::Video { nv12, time, duration }) {
        Ok(()) => {
            sh.frames.fetch_add(1, Ordering::Relaxed);
        }
        Err(std::sync::mpsc::TrySendError::Full(_)) => {
            sh.dropped.fetch_add(1, Ordering::Relaxed);
        }
        Err(_) => sh.fail("Video Write Failed".into()),
    }
}

/// Follows the recording clock, so silence from either source never shifts the timeline.
fn mixer_loop(sources: Vec<Arc<AudioSource>>, tx: std::sync::mpsc::SyncSender<Packet>, sh: Arc<Shared>) {
    let channels = AUDIO_CHANNELS as u64;
    let rate = AUDIO_RATE as u64;
    let mut written = 0u64; // frames
    let mut mix: Vec<f32> = Vec::new();
    let mut was_running = false;
    while !sh.stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(20));
        let running = sh.clock.running();
        if !running || !was_running {
            for s in &sources {
                s.queue.lock().unwrap().clear();
            }
        }
        was_running = running;
        if !running {
            continue;
        }
        let target = sh.clock.elapsed().as_micros() as u64 * rate / 1_000_000;
        if target <= written {
            continue;
        }
        let frames = (target - written) as usize;
        mix.clear();
        mix.resize(frames * channels as usize, 0.0);
        for s in &sources {
            let mut q = s.queue.lock().unwrap();
            let take = (frames * channels as usize).min(q.len());
            for (d, x) in mix.iter_mut().zip(q.drain(..take)) {
                *d += x;
            }
        }
        let mut pcm = Vec::with_capacity(mix.len() * 2);
        for x in &mix {
            pcm.extend_from_slice(&((x.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        let time = (written * 10_000_000 / rate) as i64;
        let duration = (frames as u64 * 10_000_000 / rate) as i64;
        if tx.send(Packet::Audio { pcm, time, duration }).is_err() {
            sh.fail("Audio Write Failed".into());
            return;
        }
        written = target;
    }
}

/// Moves a finished recording from the temp folder to its place, with its companions.
pub fn save_files(temp: &Path, dest: &Path) -> Result<Vec<PathBuf>, String> {
    let stem = dest.with_extension("");
    let stem = stem.to_string_lossy();
    let mut saved = Vec::new();
    move_file(&temp.join(MAIN_FILE), dest)?;
    saved.push(dest.to_path_buf());
    let marks = temp.join(MARKER_FILE);
    if marks.exists() {
        let to = PathBuf::from(format!("{stem}.markers.txt"));
        move_file(&marks, &to)?;
        saved.push(to);
    }
    let _ = std::fs::remove_dir_all(temp);
    Ok(saved)
}

fn move_file(from: &Path, to: &Path) -> Result<(), String> {
    if to.exists() {
        std::fs::remove_file(to).map_err(|e| e.to_string())?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).map_err(|e| e.to_string())?;
    std::fs::remove_file(from).map_err(|e| e.to_string())
}

/// "Rec " + date, with dashes between the parts: the original's naming pattern.
pub fn generated_name(now: (u16, u16, u16, u16, u16, u16)) -> String {
    let (y, mo, d, h, mi, s) = now;
    let (h12, ap) = (if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "AM" } else { "PM" });
    format!("Rec {mo}-{d}-{y}-{h12}-{mi:02}-{s:02}-{ap}.mp4")
}

pub fn unique_path(p: PathBuf) -> PathBuf {
    if !p.exists() {
        return p;
    }
    let stem = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let dir = p.parent().map(Path::to_path_buf).unwrap_or_default();
    for i in 1.. {
        let c = dir.join(format!("{stem} ({i}).mp4"));
        if !c.exists() {
            return c;
        }
    }
    p
}
