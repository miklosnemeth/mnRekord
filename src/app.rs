//! The app's state and flow: sources, start and stop, prompts, shortcuts, commands.
use crate::audio::{self, AudioSource};
use crate::config::*;
use crate::ipc::{self, Cmd, Hotkeys};
use crate::recorder::{self, Session, SessionConfig, Shared};
use crate::win;
use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Launch {
    pub screenshot: Option<PathBuf>,
    pub view: String,
    pub commands: Vec<Cmd>,
}

/// What the shortcut thread may reach while the window is hidden or minimized.
#[derive(Default)]
pub struct HotState {
    pub shared: Mutex<Option<Arc<Shared>>>,
    pub armed: AtomicBool,
}

pub struct Rec {
    pub session: Session,
    pub count_from: Option<Instant>,
    pub resume_at: Option<Instant>,
    pub timer_text: String,
    pub timer_at: Instant,
}

pub struct Picker {
    pub tex: egui::TextureHandle,
    pub windows: Vec<Rect>,
    pub press: Option<egui::Pos2>,
}

pub enum Mode {
    Idle,
    PreparePick(Instant),
    Picking(Picker),
}

pub enum Prompt {
    /// Delete the recording and go back (or quit); refused means carry on.
    Delete { quit: bool, was_running: bool },
    /// The name dialog was cancelled.
    SaveOrDelete(PathBuf),
    Recover(PathBuf),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Grip {
    Move,
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

/// Pixels (RGBA), width, height.
type PreviewFrame = (Vec<u8>, usize, usize);

pub struct Preview {
    pub stop: Arc<AtomicBool>,
    pub latest: Arc<Mutex<Option<PreviewFrame>>>,
    /// Set while the window is minimized: the capture thread idles.
    pub paused: Arc<AtomicBool>,
    pub tex: Option<egui::TextureHandle>,
}

pub struct App {
    pub hwnd: isize,
    pub ctx: egui::Context,
    pub settings: Settings,
    pub demo: bool,
    pub view: String,
    pub shot: Option<PathBuf>,
    pub frame_no: u32,
    pub shot_sent: bool,
    /// 0 waiting, 1 saved, 2 failed
    pub shot_state: Arc<AtomicU8>,
    pub monitors: Vec<Rect>,
    pub vscreen: Rect,
    pub cmd_tx: Sender<Cmd>,
    pub cmd_rx: Receiver<Cmd>,
    pub hotkeys: Option<Hotkeys>,
    pub hot: Arc<HotState>,
    pub system: Option<Arc<AudioSource>>,
    pub preview: Option<Preview>,
    /// The window title last sent, so it is only set when it changes.
    pub cur_title: String,
    pub demo_tex: Option<egui::TextureHandle>,
    pub mode: Mode,
    pub rec: Option<Rec>,
    pub prompt: Option<Prompt>,
    pub border: Option<win::RegionBorder>,
    pub grip: Option<(Grip, egui::Pos2, Rect)>,
    pub prefs_open: bool,
    pub prefs_tab: usize,
    pub capture_key: Option<usize>,
    pub shortcut_msg: Option<String>,
    pub notice: Option<String>,
    pub last_saved: Option<PathBuf>,
    pub saved_rect: Option<Rect>,
    /// Window placement to keep applying while decorations change.
    pub place: Option<(Rect, bool, u32)>,
    pub mask_tick: Instant,
    pub started: Instant,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, opts: Launch) -> Self {
        let hwnd = cc
            .window_handle()
            .ok()
            .and_then(|h| match h.as_raw() {
                RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
                _ => None,
            })
            .unwrap_or(0);
        crate::ui::apply_theme(&cc.egui_ctx);
        let demo = opts.screenshot.is_some();
        let (cmd_tx, cmd_rx) = channel();
        let settings = if demo { Settings::default() } else { Settings::load() };
        // Device lists come from a worker thread, so this thread's COM mode stays as the window system set it.
        let hot: Arc<HotState> = Default::default();
        let mut app = App {
            hwnd,
            ctx: cc.egui_ctx.clone(),
            settings,
            demo,
            view: opts.view.clone(),
            shot: opts.screenshot.clone(),
            frame_no: 0,
            shot_sent: false,
            shot_state: Default::default(),
            monitors: win::monitors(),
            vscreen: win::virtual_screen(),
            cmd_tx,
            cmd_rx,
            hotkeys: None,
            hot,
            system: None,
            preview: None,
            cur_title: String::new(),
            demo_tex: None,
            mode: Mode::Idle,
            rec: None,
            prompt: None,
            border: None,
            grip: None,
            prefs_open: opts.view == "prefs",
            prefs_tab: 0,
            capture_key: None,
            shortcut_msg: None,
            notice: None,
            last_saved: None,
            saved_rect: None,
            place: None,
            mask_tick: Instant::now(),
            started: Instant::now(),
        };
        // The ratio is the base: start from a region that already has it.
        if let Some((rw, rh)) = app.settings.aspect {
            let r = app.region().fit_ratio(rw, rh);
            app.settings.region = Some(r);
        }
        if !demo {
            app.border = win::RegionBorder::new();
            app.start_hotkeys();
            let (tx, c) = (app.cmd_tx.clone(), cc.egui_ctx.clone());
            ipc::serve_pipe(tx, move || c.request_repaint());
            app.check_recovery();
            for c in opts.commands {
                let _ = app.cmd_tx.send(c);
            }
            if app.prompt.is_none() {
                app.start_preview();
            }
        } else if app.view == "bar" {
            app.ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
            app.ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(760.0, 64.0)));
        }
        app
    }

    // ---- regions ----

    /// The chosen region, or the first monitor as in the original.
    pub fn region(&self) -> Rect {
        let first = self.monitors.first().copied().unwrap_or(self.vscreen);
        self.settings.region.unwrap_or(first)
    }

    pub fn set_region(&mut self, r: Rect) {
        self.settings.region = Some(r.clamped(&self.vscreen));
        self.persist();
    }

    /// Like `set_region`, but a locked ratio is kept: the largest frame of that ratio inside `r`.
    pub fn set_region_fitted(&mut self, r: Rect) {
        let r = match self.settings.aspect {
            Some((rw, rh)) => r.fit_ratio(rw, rh),
            None => r,
        };
        self.set_region(r);
    }

    pub fn set_aspect(&mut self, aspect: Option<(i32, i32)>) {
        self.settings.aspect = aspect;
        if aspect.is_some() {
            let r = self.region();
            self.set_region_fitted(r);
        } else {
            self.persist();
        }
    }

    pub fn persist(&self) {
        if !self.demo {
            self.settings.save();
        }
    }

    fn sync_border(&mut self) {
        let show = self.settings.show_region_indicator
            && self.settings.record_screen
            && !matches!(self.mode, Mode::PreparePick(_) | Mode::Picking(_))
            && !self.monitors.contains(&self.region());
        let r = self.region();
        if let Some(b) = &mut self.border {
            if show {
                b.show(r);
            } else {
                b.hide();
            }
        }
    }

    // ---- live sources ----

    fn sync_sources(&mut self) {
        if self.demo {
            return;
        }
        if self.settings.record_system && self.system.is_none() {
            self.system = Some(AudioSource::start());
        } else if !self.settings.record_system {
            if let Some(s) = self.system.take() {
                s.stop();
            }
        }
    }

    // ---- desktop preview for the stage ----

    pub fn start_preview(&mut self) {
        if self.demo || self.preview.is_some() {
            return;
        }
        let (stop, latest, paused): (Arc<AtomicBool>, Arc<Mutex<_>>, Arc<AtomicBool>) = Default::default();
        let pa = paused.clone();
        let (st, la, vs, h, ctx) = (stop.clone(), latest.clone(), self.vscreen, self.hwnd, self.ctx.clone());
        std::thread::spawn(move || {
            let w = 960.min(vs.w).max(1);
            let hh = ((w as f64 * vs.h as f64 / vs.w.max(1) as f64) as i32).max(1);
            let frame = Duration::from_millis(33); // about 30 fps
            while !st.load(Ordering::Relaxed) {
                if pa.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
                let t0 = Instant::now();
                if let Some(mut px) = win::grab_rgba(vs, w, hh) {
                    // Paint over our own window so the preview doesn't show itself.
                    let own = win::window_rect(h);
                    let (sx, sy) = (w as f64 / vs.w as f64, hh as f64 / vs.h as f64);
                    let x0 = (((own.x - vs.x) as f64 * sx) as i32).clamp(0, w);
                    let x1 = (((own.right() - vs.x) as f64 * sx) as i32).clamp(0, w);
                    let y0 = (((own.y - vs.y) as f64 * sy) as i32).clamp(0, hh);
                    let y1 = (((own.bottom() - vs.y) as f64 * sy) as i32).clamp(0, hh);
                    for y in y0..y1 {
                        for x in x0..x1 {
                            let o = ((y * w + x) * 4) as usize;
                            px[o..o + 4].copy_from_slice(&[226, 222, 214, 255]);
                        }
                    }
                    *la.lock().unwrap() = Some((px, w as usize, hh as usize));
                    ctx.request_repaint();
                }
                std::thread::sleep(frame.saturating_sub(t0.elapsed()));
            }
        });
        self.preview = Some(Preview { stop, latest, paused, tex: None });
    }

    pub fn stop_preview(&mut self) {
        if let Some(p) = self.preview.take() {
            p.stop.store(true, Ordering::Relaxed);
        }
    }

    fn update_preview_texture(&mut self) {
        let minimized = self.ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        let Some(p) = &mut self.preview else { return };
        p.paused.store(minimized, Ordering::Relaxed);
        let Some((px, w, h)) = p.latest.lock().unwrap().take() else { return };
        let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &px);
        match &mut p.tex {
            Some(t) => t.set(img, egui::TextureOptions::LINEAR),
            None => p.tex = Some(self.ctx.load_texture("desktop", img, egui::TextureOptions::LINEAR)),
        }
    }

    // ---- shortcuts ----

    fn start_hotkeys(&mut self) {
        let (hot, ctx, hwnd) = (self.hot.clone(), self.ctx.clone(), self.hwnd);
        let keys = Hotkeys::start(self.cmd_tx.clone(), move |c| {
            let shared = hot.shared.lock().unwrap().clone();
            match (c, shared) {
                (Cmd::Toggle, Some(s)) => {
                    if hot.armed.load(Ordering::Relaxed) {
                        if s.clock.running() {
                            s.pause();
                        } else {
                            s.resume();
                        }
                    }
                }
                (Cmd::Marker, Some(s)) => s.add_marker(),
                _ => win::restore_window(hwnd),
            }
            ctx.request_repaint();
        });
        self.hotkeys = Some(keys);
        self.register_shortcuts();
    }

    pub fn register_shortcuts(&self) {
        if let Some(h) = &self.hotkeys {
            let s = &self.settings;
            h.register(vec![
                (s.shortcut_record.clone(), Cmd::Toggle),
                (s.shortcut_stop.clone(), Cmd::Stop),
                (s.shortcut_marker.clone(), Cmd::Marker),
            ]);
        }
    }

    /// While a new shortcut is being typed, the keys must reach the window.
    pub fn release_shortcuts(&self) {
        if let Some(h) = &self.hotkeys {
            h.register(Vec::new());
        }
    }

    // ---- recovery ----

    fn check_recovery(&mut self) {
        let loc = self.settings.recovery_location.clone();
        if loc.is_empty() {
            return;
        }
        let dir = PathBuf::from(loc);
        if dir.join(recorder::MAIN_FILE).exists() {
            self.prompt = Some(Prompt::Recover(dir));
        } else {
            let _ = std::fs::remove_dir_all(&dir);
            self.clear_recovery();
        }
    }

    pub fn clear_recovery(&mut self) {
        self.settings.recovery_location.clear();
        self.persist();
    }

    // ---- recording ----

    pub fn can_start(&self) -> bool {
        self.settings.record_screen
    }

    pub fn start_recording(&mut self) {
        if self.rec.is_some() || self.demo || !matches!(self.mode, Mode::Idle) || self.prompt.is_some() {
            return;
        }
        if !self.can_start() {
            self.notice = Some("Turn on the screen to record.".into());
            return;
        }
        self.notice = None;
        self.prefs_open = false;
        self.sync_sources();
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let temp_dir = PathBuf::from(&self.settings.temp_folder).join(format!("{}{stamp}", recorder::TEMP_PREFIX));
        let cfg = SessionConfig {
            temp_dir: temp_dir.clone(),
            region: self.settings.record_screen.then(|| self.region()),
            fps: self.settings.max_fps,
            quality: self.settings.video_quality,
            system: self.system.clone(),
            restore_cursor: self.settings.restore_cursor,
        };
        self.settings.recovery_location = temp_dir.to_string_lossy().into_owned();
        self.persist();
        self.stop_preview();
        if self.settings.use_window_mask {
            win::mask_thread_windows(true);
        }
        let session = match Session::start(cfg) {
            Ok(s) => s,
            Err(e) => {
                win::mask_thread_windows(false);
                let _ = std::fs::remove_dir_all(&temp_dir);
                self.clear_recovery();
                eprintln!("start failed: {e}");
                self.notice = Some(format!("The recording could not start: {e}"));
                self.start_preview();
                return;
            }
        };
        *self.hot.shared.lock().unwrap() = Some(session.shared.clone());
        self.hot.armed.store(false, Ordering::Relaxed);
        self.saved_rect = Some(win::window_rect(self.hwnd));
        self.morph_to_bar();
        let now = Instant::now();
        let (count_from, resume_at) = if self.settings.show_countdown {
            (Some(now), None)
        } else {
            let wait = if self.settings.use_window_mask { self.settings.delay_ms_after_mask } else { 0 };
            (None, Some(now + Duration::from_millis(wait)))
        };
        self.rec = Some(Rec { session, count_from, resume_at, timer_text: "00:00:00".into(), timer_at: now });
    }

    fn morph_to_bar(&mut self) {
        let ppp = self.ctx.pixels_per_point();
        let first = self.monitors.first().copied().unwrap_or(self.vscreen);
        let (w, h) = ((760.0 * ppp) as i32, (64.0 * ppp) as i32);
        let r = Rect::new(first.x + (first.w - w) / 2, first.bottom() - h - (72.0 * ppp) as i32, w, h);
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        self.place = Some((r, true, 6));
    }

    pub fn restore_main_window(&mut self) {
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
        if let Some(r) = self.saved_rect.take() {
            self.place = Some((r, false, 6));
        }
        self.start_preview();
    }

    /// Ends the capture. The recording stays in the temp folder.
    fn end_session(&mut self) -> Option<(PathBuf, Result<(), String>, u64)> {
        let rec = self.rec.take()?;
        *self.hot.shared.lock().unwrap() = None;
        win::mask_thread_windows(false);
        let dir = rec.session.temp_dir.clone();
        let frames = rec.session.shared.frames.load(Ordering::Relaxed);
        let result = rec.session.finish();
        Some((dir, result, frames))
    }

    pub fn stop_recording(&mut self, force_auto_name: bool) {
        let error = self.rec.as_ref().and_then(|r| r.session.shared.error.lock().unwrap().clone());
        let Some((dir, result, frames)) = self.end_session() else { return };
        win::restore_window(self.hwnd);
        self.restore_main_window();
        if frames == 0 {
            // Stopped during the countdown: nothing was recorded.
            let _ = std::fs::remove_dir_all(&dir);
            self.clear_recovery();
            return;
        }
        if let Err(e) = result {
            self.notice = Some(format!("An error occurred in the Recorder: {e}. The recording can be recovered at the next start."));
            return;
        }
        if let Some(e) = error {
            self.notice = Some(format!("An error occurred in the Recorder: {e}. It stopped and saved the current recording."));
        }
        if self.settings.prompt_for_name && !force_auto_name {
            let picked = rfd::FileDialog::new().add_filter("MP4 video", &["mp4"]).set_file_name(auto_name()).set_directory(&self.settings.output_folder).save_file();
            match picked {
                Some(p) => self.save_to(&dir, p.with_extension("mp4")),
                None => self.prompt = Some(Prompt::SaveOrDelete(dir)),
            }
        } else {
            let folder = PathBuf::from(&self.settings.output_folder);
            if let Err(e) = std::fs::create_dir_all(&folder) {
                self.notice = Some(format!("The output folder isn't available ({e}). The recording can be recovered at the next start."));
                return;
            }
            let dest = recorder::unique_path(folder.join(auto_name()));
            self.save_to(&dir, dest);
        }
    }

    pub fn save_to(&mut self, dir: &std::path::Path, dest: PathBuf) {
        match recorder::save_files(dir, &dest) {
            Ok(_) => {
                self.clear_recovery();
                self.last_saved = Some(dest.clone());
                if self.settings.open_on_stop {
                    win::open_path(&dest);
                }
                if self.settings.exit_on_stop && self.notice.is_none() {
                    self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Err(e) => self.notice = Some(format!("The recording could not be saved ({e}). It can be recovered at the next start.")),
        }
    }

    pub fn delete_folder(&mut self, dir: &std::path::Path) {
        let _ = std::fs::remove_dir_all(dir);
        self.clear_recovery();
    }

    /// Pause and ask, as the original does for Restart and for closing.
    pub fn ask_delete(&mut self, quit: bool) {
        let Some(rec) = &self.rec else { return };
        let was_running = rec.session.shared.clock.running();
        if was_running {
            rec.session.shared.pause();
        }
        self.hot.armed.store(false, Ordering::Relaxed);
        self.prompt = Some(Prompt::Delete { quit, was_running });
    }

    pub fn answer_delete(&mut self, yes: bool, quit: bool, was_running: bool) {
        self.prompt = None;
        if yes {
            if let Some((dir, _, _)) = self.end_session() {
                self.delete_folder(&dir);
            }
            if quit {
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                self.restore_main_window();
            }
        } else if let Some(rec) = &self.rec {
            if was_running {
                rec.session.shared.resume();
            }
            self.hot.armed.store(rec.count_from.is_none(), Ordering::Relaxed);
        }
    }

    pub fn toggle_pause(&mut self) {
        if let Some(rec) = &self.rec {
            if rec.count_from.is_none() {
                if rec.session.shared.clock.running() {
                    rec.session.shared.pause();
                } else {
                    rec.session.shared.resume();
                }
            }
        }
    }

    // ---- commands from shortcuts and from a second launch ----

    fn handle(&mut self, c: Cmd) {
        match c {
            Cmd::Toggle | Cmd::Record => self.start_recording(),
            Cmd::Pause => {
                if let Some(r) = &self.rec {
                    r.session.shared.pause();
                }
            }
            Cmd::Resume => {
                if let Some(r) = &self.rec {
                    r.session.shared.resume();
                }
            }
            Cmd::Stop => self.stop_recording(false),
            Cmd::WriteToFile => {
                self.settings.exit_on_stop = true;
                self.stop_recording(true);
            }
            Cmd::Marker => {} // handled on the shortcut thread
            Cmd::SetFps(n) => {
                if SCREEN_RATES.contains(&n) {
                    self.settings.max_fps = n;
                }
            }
            Cmd::SetOutput(p) => self.settings.output_folder = p,
            Cmd::SetRegion(x, y, w, h) => self.set_region(Rect::new(x, y, w, h)),
            Cmd::SystemAudio(on) => self.set_system_audio(on),
            Cmd::Screen(on) => self.settings.record_screen = on,
            Cmd::DrawRegion => {
                if self.rec.is_none() && matches!(self.mode, Mode::Idle) {
                    self.begin_pick();
                }
            }
        }
    }

    pub fn set_system_audio(&mut self, on: bool) {
        if on && !self.demo && !std::thread::spawn(audio::has_playback_device).join().unwrap_or(false) {
            self.notice = Some("Unable to record system audio. Please connect or enable a playback device and try again.".into());
            self.settings.record_system = false;
        } else {
            self.settings.record_system = on;
        }
        self.persist();
    }

    // ---- region picking on the desktop ----

    pub fn begin_pick(&mut self) {
        if self.demo {
            return;
        }
        win::mask_thread_windows(true);
        self.mode = Mode::PreparePick(Instant::now());
    }

    fn enter_picker(&mut self) {
        let vs = self.vscreen;
        let scale = (8192.0 / vs.w.max(vs.h) as f64).min(1.0);
        let (w, h) = (((vs.w as f64 * scale) as i32).max(1), ((vs.h as f64 * scale) as i32).max(1));
        let Some(px) = win::grab_rgba(vs, w, h) else {
            win::mask_thread_windows(false);
            self.mode = Mode::Idle;
            self.notice = Some("The screen could not be read.".into());
            return;
        };
        let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
        let tex = self.ctx.load_texture("picker", img, egui::TextureOptions::LINEAR);
        let windows = win::visible_windows(windows::Win32::Foundation::HWND(self.hwnd as *mut _));
        self.stop_preview();
        self.saved_rect = Some(win::window_rect(self.hwnd));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        self.ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        self.place = Some((vs, true, 6));
        self.mode = Mode::Picking(Picker { tex, windows, press: None });
    }

    pub fn end_pick(&mut self, picked: Option<Rect>) {
        self.mode = Mode::Idle;
        win::mask_thread_windows(false);
        if let Some(r) = picked {
            self.set_region_fitted(r);
        }
        self.restore_main_window();
    }

    // ---- every frame, also while hidden ----

    fn tick(&mut self, ctx: &egui::Context) {
        while let Ok(c) = self.cmd_rx.try_recv() {
            self.handle(c);
        }
        if let Some((r, top, n)) = self.place {
            win::place_window(self.hwnd, r, top);
            self.place = (n > 1).then_some((r, top, n - 1));
        }
        if let Mode::PreparePick(since) = self.mode {
            if since.elapsed().as_millis() as u64 >= self.settings.delay_ms_after_mask {
                self.enter_picker();
            }
        }
        self.sync_border();
        if self.rec.is_none() {
            self.sync_sources();
        }
        self.update_preview_texture();
        self.tick_recording();
        self.sync_title(ctx);

        // Closing while recording: pause and ask, as the original does.
        if ctx.input(|i| i.viewport().close_requested()) && self.rec.is_some() && self.prompt.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.ask_delete(true);
        }

        if self.demo {
            self.tick_screenshot(ctx);
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(Duration::from_millis(if self.rec.is_some() { 100 } else { 50 }));
        }
    }

    /// The title shows the recording state, so the taskbar tells it even when the window is minimized.
    fn sync_title(&mut self, ctx: &egui::Context) {
        let title = match &self.rec {
            Some(r) if r.session.shared.clock.running() => format!("● {}  {APP_NAME}", r.timer_text),
            Some(r) => format!("Paused {}  {APP_NAME}", r.timer_text),
            None => APP_NAME.to_string(),
        };
        if title != self.cur_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.cur_title = title;
        }
    }

    fn tick_recording(&mut self) {
        let mut stop = false;
        let mask = self.settings.use_window_mask;
        let auto_min = self.settings.auto_minimize;
        let ctx = self.ctx.clone();
        let hot = self.hot.clone();
        if let Some(rec) = &mut self.rec {
            let shared = &rec.session.shared;
            if shared.error.lock().unwrap().is_some() {
                stop = true;
            }
            if let Some(from) = rec.count_from {
                // Wait, then each number; the recording resumes at zero.
                let total = COUNT_DELAY + COUNT_START as f32 * COUNT_STEP;
                if from.elapsed().as_secs_f32() >= total && self.prompt.is_none() {
                    rec.count_from = None;
                    shared.resume();
                    hot.armed.store(true, Ordering::Relaxed);
                    if auto_min {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                    }
                }
            }
            if rec.resume_at.is_some_and(|t| Instant::now() >= t) {
                rec.resume_at = None;
                shared.resume();
                hot.armed.store(true, Ordering::Relaxed);
                if auto_min {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
            }
            if rec.timer_at.elapsed().as_secs_f32() >= TIMER_REFRESH {
                rec.timer_at = Instant::now();
                let t = shared.clock.elapsed().as_secs();
                rec.timer_text = format!("{:02}:{:02}:{:02}", t / 3600, t / 60 % 60, t % 60);
            }
            // New windows (countdown) have to be hidden as well.
            if mask && self.mask_tick.elapsed().as_millis() > 250 {
                self.mask_tick = Instant::now();
                win::mask_thread_windows(true);
            }
        }
        if stop {
            self.stop_recording(false);
        }
    }

    fn tick_screenshot(&mut self, ctx: &egui::Context) {
        self.frame_no += 1;
        let Some(path) = self.shot.clone() else { return };
        if self.frame_no >= 120 && !self.shot_sent {
            self.shot_sent = true;
            let state = self.shot_state.clone();
            let c = ctx.clone();
            ctx.request_screenshot(move |img: Arc<egui::ColorImage>| {
                let [w, h] = img.size;
                let bytes: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
                if let Some(d) = path.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                let ok = image::RgbaImage::from_raw(w as u32, h as u32, bytes)
                    .map(|i| i.save_with_format(&path, image::ImageFormat::Png).is_ok())
                    .unwrap_or(false);
                state.store(if ok { 1 } else { 2 }, Ordering::Relaxed);
                c.request_repaint();
            });
        }
        let state = self.shot_state.load(Ordering::Relaxed);
        if state != 0 {
            std::process::exit(if state == 1 { 0 } else { 1 });
        }
        if self.started.elapsed().as_secs() > 60 {
            std::process::exit(2);
        }
    }
}

const TIMER_REFRESH: f32 = 0.5;

fn auto_name() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    recorder::generated_name((t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond))
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist();
    }
}
