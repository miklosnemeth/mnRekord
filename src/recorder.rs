//! One recording: a pausable clock, capture threads, the audio mixer and the writers.
use crate::audio::AudioSource;
use crate::config::{Rect, LOW_DISK_BYTES, VIDEO_BPP};
use crate::encode::*;
use crate::screen::{CursorStyle, ScreenGrabber};
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

/// The file the packets are going to. A long recording is split into parts: the target is switched
/// and `offset` (clock ticks at the switch) makes the new file start at zero.
struct Target {
    tx: std::sync::mpsc::SyncSender<Packet>,
    offset: i64,
}

pub struct Out(Mutex<Target>);

impl Out {
    fn new(tx: std::sync::mpsc::SyncSender<Packet>) -> Out {
        Out(Mutex::new(Target { tx, offset: 0 }))
    }
    fn switch(&self, tx: std::sync::mpsc::SyncSender<Packet>, offset: i64) {
        *self.0.lock().unwrap() = Target { tx, offset };
    }
    fn video(&self, nv12: Vec<u8>, time: i64, duration: i64, sh: &Shared) {
        let t = self.0.lock().unwrap();
        match t.tx.try_send(Packet::Video { nv12, time: (time - t.offset).max(0), duration }) {
            Ok(()) => {
                sh.frames.fetch_add(1, Ordering::Relaxed);
            }
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                sh.dropped.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => sh.fail("Video Write Failed".into()),
        }
    }
    fn audio(&self, pcm: Vec<u8>, time: i64, duration: i64) -> bool {
        let t = self.0.lock().unwrap();
        t.tx.send(Packet::Audio { pcm, time: (time - t.offset).max(0), duration }).is_ok()
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
    /// Where the recorded region is now, when it follows a window.
    pub live_region: Mutex<Option<Rect>>,
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
    pub cursor: CursorStyle,
    pub hardware: bool,
    pub follow: Option<Follow>,
    /// Start a new file after this many seconds of recording (0 = one file).
    pub split_seconds: u32,
}

/// A recording whose region keeps its place relative to a window.
#[derive(Clone, Copy)]
pub struct Follow {
    pub hwnd: isize,
    /// Region position minus window position, taken when the recording starts.
    pub offset: (i32, i32),
}

pub struct Session {
    pub shared: Arc<Shared>,
    pub temp_dir: PathBuf,
    threads: Vec<std::thread::JoinHandle<()>>,
    writers: Arc<Mutex<Vec<Writer>>>,
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
        let writers = Arc::new(Mutex::new(Vec::new()));
        let mut out = None;

        if let Some(region) = cfg.region {
            let region = region.even();
            let spec = VideoSpec { width: region.w as u32, height: region.h as u32, fps: cfg.fps, bits_per_pixel: bpp, quality: cfg.quality, hardware: cfg.hardware };
            let w = Writer::spawn(&cfg.temp_dir.join(MAIN_FILE), spec.clone(), has_audio)?;
            let target = Arc::new(Out::new(w.tx.clone()));
            writers.lock().unwrap().push(w);
            let (o, sh, fps, follow) = (target.clone(), shared.clone(), cfg.fps, cfg.follow);
            let grabber = ScreenGrabber::new(region, cfg.cursor, cfg.follow.is_some())?;
            threads.push(std::thread::spawn(move || screen_loop(grabber, region, fps, follow, o, sh)));
            if cfg.split_seconds > 0 {
                let (o, sh, w, dir) = (target.clone(), shared.clone(), writers.clone(), cfg.temp_dir.clone());
                let secs = cfg.split_seconds as u64;
                threads.push(std::thread::spawn(move || split_loop(o, sh, w, dir, spec, has_audio, secs)));
            }
            out = Some(target);
        }
        let Some(primary) = out else { return Err("Invalid Input".into()) };
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
        let writers: Vec<Writer> = std::mem::take(&mut *self.writers.lock().unwrap());
        for w in writers {
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

fn screen_loop(mut g: ScreenGrabber, region: Rect, fps: u32, follow: Option<Follow>, out: Arc<Out>, sh: Arc<Shared>) {
    let vs = win::virtual_screen();
    let mut at = (region.x, region.y);
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
        if let Some(f) = follow {
            if let Some(fr) = win::window_frame(f.hwnd) {
                let x = (fr.x + f.offset.0).clamp(vs.x, (vs.right() - region.w).max(vs.x));
                let y = (fr.y + f.offset.1).clamp(vs.y, (vs.bottom() - region.h).max(vs.y));
                if (x, y) != at {
                    at = (x, y);
                    g.move_to(x, y);
                    *sh.live_region.lock().unwrap() = Some(Rect::new(x, y, region.w, region.h));
                }
            }
        }
        let bgra = g.frame();
        if !sh.clock.running() {
            continue;
        }
        let time = sh.clock.ticks();
        let mut nv12 = Vec::new();
        bgra_to_nv12(bgra, w * 4, w, h, &mut nv12);
        out.video(nv12, time, duration, &sh);
    }
}

/// Starts a new file every `secs` of recorded time. The old file is closed at once, so a crash
/// later only costs the part being written.
fn split_loop(out: Arc<Out>, sh: Arc<Shared>, writers: Arc<Mutex<Vec<Writer>>>, dir: PathBuf, spec: VideoSpec, audio: bool, secs: u64) {
    let mut part = 1;
    let mut started = Duration::ZERO;
    while !sh.stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(200));
        if !sh.clock.running() || sh.clock.elapsed() < started + Duration::from_secs(secs) {
            continue;
        }
        part += 1;
        match Writer::spawn(&dir.join(part_file(part)), spec.clone(), audio) {
            Ok(w) => {
                started = sh.clock.elapsed();
                out.switch(w.tx.clone(), sh.clock.ticks());
                let mut list = writers.lock().unwrap();
                if let Some(old) = list.last() {
                    let _ = old.tx.send(Packet::Finish);
                }
                list.push(w);
            }
            Err(e) => {
                sh.fail(format!("Could not start the next part: {e}"));
                return;
            }
        }
    }
}

/// Name of the file of part `n` in the temp folder (part 1 is the main file).
pub fn part_file(n: u32) -> String {
    if n <= 1 {
        MAIN_FILE.to_string()
    } else {
        format!("cs_recording_part{n}.mp4")
    }
}

/// Follows the recording clock, so silence from either source never shifts the timeline.
fn mixer_loop(sources: Vec<Arc<AudioSource>>, out: Arc<Out>, sh: Arc<Shared>) {
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
        if !out.audio(pcm, time, duration) {
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
    let parts: Vec<PathBuf> = (2..).map(|n| temp.join(part_file(n))).take_while(|p| p.exists()).collect();
    if parts.is_empty() {
        move_file(&temp.join(MAIN_FILE), dest)?;
        saved.push(dest.to_path_buf());
    } else {
        // A split recording: name-part1.mp4, name-part2.mp4, ...
        for (i, from) in std::iter::once(temp.join(MAIN_FILE)).chain(parts).enumerate() {
            let to = PathBuf::from(format!("{stem}-part{}.mp4", i + 1));
            move_file(&from, &to)?;
            saved.push(to);
        }
    }
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

/// A file name from the template: `{date}` 2026-11-07, `{time}` 14-05-09, `{window}` the title of the picked window.
pub fn file_name(template: &str, now: (u16, u16, u16, u16, u16, u16), window: &str) -> String {
    let (y, mo, d, h, mi, s) = now;
    let window = if window.trim().is_empty() { "screen" } else { window.trim() };
    let name = template
        .replace("{date}", &format!("{y}-{mo:02}-{d:02}"))
        .replace("{time}", &format!("{h:02}-{mi:02}-{s:02}"))
        .replace("{window}", window);
    let clean: String = name.chars().map(|c| if r#"\/:*?"<>|"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    let clean = clean.trim().trim_end_matches('.').trim();
    let clean: String = clean.chars().take(120).collect();
    format!("{}.mp4", if clean.is_empty() { "Recording" } else { &clean })
}

/// `name.ext` or, when taken, `name (1).ext`, `name (2).ext`...
pub fn unique_ext(p: PathBuf) -> PathBuf {
    if !p.exists() {
        return p;
    }
    let stem = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
    let ext = p.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = p.parent().map(Path::to_path_buf).unwrap_or_default();
    (1..).map(|i| dir.join(format!("{stem} ({i}).{ext}"))).find(|c| !c.exists()).unwrap_or(p)
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

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: (u16, u16, u16, u16, u16, u16) = (2026, 11, 7, 14, 5, 9);

    #[test]
    fn file_name_fills_the_tokens() {
        assert_eq!(file_name("Rec {date} {time}", NOW, ""), "Rec 2026-11-07 14-05-09.mp4");
        assert_eq!(file_name("{window} {date}", NOW, "Notes"), "Notes 2026-11-07.mp4");
        assert_eq!(file_name("{window}", NOW, "  "), "screen.mp4");
    }

    #[test]
    fn file_name_is_a_valid_file_name() {
        assert_eq!(file_name("{window}", NOW, "a/b:c*d?"), "a_b_c_d_.mp4");
        assert_eq!(file_name("", NOW, ""), "Recording.mp4");
        assert!(file_name(&"x".repeat(500), NOW, "").len() <= 124);
    }
}
