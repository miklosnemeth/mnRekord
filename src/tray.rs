//! A tray icon with a small menu: show the window, start or stop the recording, quit.
use crate::i18n::tr;
use crate::ipc::Cmd;
use eframe::egui;
use std::sync::mpsc::Sender;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct Tray {
    _icon: TrayIcon,
    show: MenuItem,
    toggle: MenuItem,
    quit: MenuItem,
    recording: bool,
}

impl Tray {
    pub fn new(tx: Sender<Cmd>, ctx: egui::Context) -> Option<Tray> {
        let img = image::load_from_memory(include_bytes!("../assets/icon.png")).ok()?.into_rgba8();
        let (w, h) = img.dimensions();
        let icon = Icon::from_rgba(img.into_raw(), w, h).ok()?;

        let show = MenuItem::new(tr("Show mnRekord"), true, None);
        let toggle = MenuItem::new(tr("Start recording"), true, None);
        let quit = MenuItem::new(tr("Quit"), true, None);
        let menu = Menu::new();
        menu.append_items(&[&show, &toggle, &PredefinedMenuItem::separator(), &quit]).ok()?;

        let (show_id, toggle_id, quit_id) = (show.id().clone(), toggle.id().clone(), quit.id().clone());
        let (t, c) = (tx.clone(), ctx.clone());
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let cmd = if e.id == show_id {
                Cmd::Show
            } else if e.id == toggle_id {
                Cmd::ToggleRecording
            } else if e.id == quit_id {
                Cmd::Quit
            } else {
                return;
            };
            let _ = t.send(cmd);
            c.request_repaint();
        }));
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                let _ = tx.send(Cmd::Show);
                ctx.request_repaint();
            }
        }));

        let tray = TrayIconBuilder::new().with_menu(Box::new(menu)).with_tooltip("mnRekord").with_icon(icon).build().ok()?;
        Some(Tray { _icon: tray, show, toggle, quit, recording: false })
    }

    pub fn set_recording(&mut self, recording: bool) {
        if recording != self.recording {
            self.recording = recording;
            self.toggle.set_text(if recording { tr("Stop recording") } else { tr("Start recording") });
        }
    }

    /// After the interface language changed.
    pub fn refresh_language(&mut self) {
        self.show.set_text(tr("Show mnRekord"));
        self.quit.set_text(tr("Quit"));
        self.toggle.set_text(if self.recording { tr("Stop recording") } else { tr("Start recording") });
    }
}
