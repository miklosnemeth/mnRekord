//! Application settings and defaults.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const APP_NAME: &str = "mnRekord";
pub const SCREEN_RATES: [u32; 4] = [15, 24, 30, 60];
pub const MIN_REGION: i32 = 16;
pub const COUNT_START: i32 = 3;
pub const COUNT_DELAY: f32 = 0.5;
pub const COUNT_STEP: f32 = 1.0;
/// Nominal bits per pixel and frame announced to the encoder; the quality setting decides the real rate.
pub const VIDEO_BPP: f64 = 0.1;
pub const AUDIO_BYTES_PER_SEC: u32 = 20_000; // 160 kbit/s
pub const DEFAULT_QUALITY: u32 = 75;
pub const BORDER_PX: i32 = 3;
pub const LOW_DISK_BYTES: u64 = 0x12c0_0000; // 300 MiB
pub const MAGNIFIER_ZOOM: f32 = 5.0; // 100 px view of 20 px
pub const MAGNIFIER_SIZE: f32 = 100.0;

/// (name, long side) of the sizes offered when the ratio is locked to 16:9 or 9:16.
pub const SIZE_TIERS: [(&str, i32); 5] = [("360p", 640), ("HD", 1280), ("FHD", 1920), ("QHD", 2560), ("4K UHD", 3840)];

/// (name, width, height) of the sizes offered for a free ratio.
pub const PRESETS: [(&str, i32, i32); 7] = [
    ("AD", 600, 315),
    ("SD", 640, 480),
    ("HD", 1280, 720),
    ("FHD", 1920, 1080),
    ("2K", 2048, 1080),
    ("QHD", 2560, 1440),
    ("4K UHD", 3840, 2160),
];

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }
    pub fn intersect(&self, o: &Rect) -> Option<Rect> {
        let (x, y) = (self.x.max(o.x), self.y.max(o.y));
        let (r, b) = (self.right().min(o.right()), self.bottom().min(o.bottom()));
        (r > x && b > y).then(|| Rect::new(x, y, r - x, b - y))
    }
    /// Keep the rectangle inside `bounds` at no less than the minimum side.
    pub fn clamped(&self, bounds: &Rect) -> Rect {
        let w = self.w.clamp(MIN_REGION, bounds.w.max(MIN_REGION));
        let h = self.h.clamp(MIN_REGION, bounds.h.max(MIN_REGION));
        let x = self.x.clamp(bounds.x, (bounds.right() - w).max(bounds.x));
        let y = self.y.clamp(bounds.y, (bounds.bottom() - h).max(bounds.y));
        Rect::new(x, y, w, h)
    }
    /// Largest rectangle of ratio `rw:rh` that fits inside this one, centred in it.
    pub fn fit_ratio(&self, rw: i32, rh: i32) -> Rect {
        if (self.w * rh - self.h * rw).abs() <= rw.max(rh) {
            return *self;
        }
        let (w, h) = if self.w * rh > self.h * rw {
            (((self.h as i64 * rw as i64) / rh as i64) as i32, self.h)
        } else {
            (self.w, ((self.w as i64 * rh as i64) / rw as i64) as i32)
        };
        Rect::new(self.x + (self.w - w) / 2, self.y + (self.h - h) / 2, w.max(1), h.max(1))
    }
    /// H.264 needs even sides.
    pub fn even(&self) -> Rect {
        Rect::new(self.x, self.y, self.w & !1, self.h & !1)
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Shortcut {
    pub key: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}
impl Shortcut {
    pub fn plain(key: &str) -> Self {
        Self { key: key.into(), ctrl: false, alt: false, shift: false }
    }
    pub fn ctrl(key: &str) -> Self {
        Self { key: key.into(), ctrl: true, alt: false, shift: false }
    }
    pub fn label(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s += "Ctrl+";
        }
        if self.alt {
            s += "Alt+";
        }
        if self.shift {
            s += "Shift+";
        }
        s + &self.key
    }
    /// Windows virtual-key code.
    pub fn vk(&self) -> Option<u32> {
        let k = self.key.as_str();
        if let Some(n) = k.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
            return (1..=24).contains(&n).then_some(0x70 + n - 1);
        }
        let mut c = k.chars();
        match (c.next(), c.next()) {
            (Some(ch), None) if ch.is_ascii_uppercase() || ch.is_ascii_digit() => Some(ch as u32),
            _ => None,
        }
    }
}

/// The language of the interface.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Language {
    /// Follow the Windows display language.
    #[default]
    Auto,
    English,
    Hungarian,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    pub show_countdown: bool,
    pub use_window_mask: bool,
    pub restore_cursor: bool,
    /// Draw the mouse pointer into the recording.
    pub show_cursor: bool,
    /// Halo around the pointer and a ring on clicks.
    pub highlight_cursor: bool,
    pub exit_on_stop: bool,
    pub open_on_stop: bool,
    pub show_region_indicator: bool,
    pub auto_minimize: bool,
    pub prompt_for_name: bool,
    /// File name without extension; `{date}`, `{time}` and `{window}` are filled in.
    pub name_template: String,
    /// Keep the region in place relative to the window it was picked from.
    pub follow_window: bool,
    pub output_folder: String,
    pub temp_folder: String,
    pub shortcut_record: Shortcut,
    pub shortcut_stop: Shortcut,
    pub shortcut_marker: Shortcut,
    pub max_fps: u32,
    /// H.264 quality 1..100 for the encoder's quality-based rate control.
    pub video_quality: u32,
    /// Start a new file after this many seconds of recording (0 = one file).
    pub split_seconds: u32,
    /// Use a hardware (GPU) video encoder when available.
    pub hardware_encoder: bool,
    pub record_screen: bool,
    pub record_system: bool,
    /// Record only the sound of the window the region was picked from.
    pub window_sound_only: bool,
    pub region: Option<Rect>,
    /// Locked width:height ratio of the region (None = free).
    pub aspect: Option<(i32, i32)>,
    pub delay_ms_after_mask: u64,
    pub recovery_location: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::Auto,
            show_countdown: true,
            use_window_mask: true,
            restore_cursor: true,
            show_cursor: true,
            highlight_cursor: true,
            exit_on_stop: true,
            open_on_stop: true,
            show_region_indicator: true,
            auto_minimize: false,
            prompt_for_name: false,
            name_template: "Rec {date} {time}".into(),
            follow_window: false,
            output_folder: default_output().to_string_lossy().into_owned(),
            temp_folder: std::env::temp_dir().to_string_lossy().into_owned(),
            shortcut_record: Shortcut::plain("F9"),
            shortcut_stop: Shortcut::plain("F10"),
            shortcut_marker: Shortcut::ctrl("M"),
            max_fps: 30,
            video_quality: DEFAULT_QUALITY,
            split_seconds: 0,
            hardware_encoder: true,
            record_screen: true,
            record_system: true,
            window_sound_only: false,
            region: None,
            aspect: Some((16, 9)),
            delay_ms_after_mask: 200,
            recovery_location: String::new(),
        }
    }
}

pub fn default_output() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    home.join("Documents").join(APP_NAME)
}

fn settings_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join(APP_NAME).join("settings.json")
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(settings_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn save(&self) {
        let p = settings_path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, s);
        }
    }
}
