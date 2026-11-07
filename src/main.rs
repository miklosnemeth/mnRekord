#![windows_subsystem = "windows"]
mod app;
mod audio;
mod config;
mod encode;
mod gif_export;
mod i18n;
mod ipc;
mod recorder;
mod screen;
mod tray;
mod ui;
mod win;

use eframe::egui;
use windows::Win32::UI::HiDpi::*;

fn main() -> eframe::Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    // `mnrekord --gif in.mp4 out.gif`: make a GIF from a recording and exit.
    let raw: Vec<String> = std::env::args().collect();
    if let Some(i) = raw.iter().position(|a| a == "--gif") {
        if let (Some(src), Some(dest)) = (raw.get(i + 1), raw.get(i + 2)) {
            let r = gif_export::export(std::path::Path::new(src), std::path::Path::new(dest));
            let _ = std::fs::write(format!("{dest}.log"), match r {
                Ok(()) => "ok".to_string(),
                Err(e) => e,
            });
        }
        return Ok(());
    }
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut opts = app::Launch::default();
    if let Some(i) = args.iter().position(|a| a == "--screenshot") {
        opts.screenshot = args.get(i + 1).map(std::path::PathBuf::from);
        args.drain(i..(i + 2).min(args.len()));
    }
    if let Some(i) = args.iter().position(|a| a == "--lang") {
        opts.lang = match args.get(i + 1).map(String::as_str) {
            Some("hu") => Some(config::Language::Hungarian),
            Some("en") => Some(config::Language::English),
            _ => None,
        };
        args.drain(i..(i + 2).min(args.len()));
    }
    if let Some(i) = args.iter().position(|a| a == "--view") {
        opts.view = args.get(i + 1).cloned().unwrap_or_default();
        args.drain(i..(i + 2).min(args.len()));
    }
    if opts.screenshot.is_none() && !ipc::claim_single_instance() {
        // Hand over to the running copy; try for a moment in case it's between requests.
        for _ in 0..20 {
            if ipc::send_to_first(&args) {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        return Ok(());
    }
    opts.commands = ipc::parse_args(&args);
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(config::APP_NAME)
            .with_inner_size([1080.0, 700.0])
            .with_min_inner_size([560.0, 60.0])
            .with_icon(window_icon()),
        ..Default::default()
    };
    eframe::run_native(config::APP_NAME, native, Box::new(move |cc| Ok(Box::new(app::App::new(cc, opts)))))
}

fn window_icon() -> egui::IconData {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png")).expect("icon").into_rgba8();
    let (width, height) = img.dimensions();
    egui::IconData { rgba: img.into_raw(), width, height }
}
