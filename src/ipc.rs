//! Shortcuts registered with Windows, and the named pipe that lets a second launch
//! hand its command-line arguments to the running copy.
use crate::config::Shortcut;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use windows::core::HSTRING;
use windows::Win32::Foundation::*;
use windows::Win32::Storage::FileSystem::*;
use windows::Win32::System::Pipes::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Toggle,
    Record,
    Pause,
    Resume,
    Stop,
    WriteToFile,
    Marker,
    SetFps(u32),
    SetOutput(String),
    SetRegion(i32, i32, i32, i32),
    SystemAudio(bool),
    Screen(bool),
    SetH264,
    DrawRegion,
}

/// Same names as the original's command line (with `/` or `-`).
pub fn parse_args(args: &[String]) -> Vec<Cmd> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let name = args[i].trim_start_matches(['/', '-']).to_ascii_lowercase();
        let val = args.get(i + 1).map(|s| s.trim_matches('"').to_string());
        let num = val.as_ref().and_then(|s| s.parse::<i64>().ok());
        match name.as_str() {
            "record" => out.push(Cmd::Record),
            "pause" => out.push(Cmd::Pause),
            "resume" => out.push(Cmd::Resume),
            "stop" => out.push(Cmd::Stop),
            "writetofile" => out.push(Cmd::WriteToFile),
            "setrecordtoh264" => out.push(Cmd::SetH264),
            "drawregion" => out.push(Cmd::DrawRegion),
            "unmutesystemaudio" => out.push(Cmd::SystemAudio(true)),
            "mutesystemaudio" => out.push(Cmd::SystemAudio(false)),
            "enablescreenrecording" => out.push(Cmd::Screen(true)),
            "disablescreenrecording" => out.push(Cmd::Screen(false)),
            "setscreenframerate" => {
                if let Some(n) = num {
                    out.push(Cmd::SetFps(n as u32));
                }
                i += 1;
            }
            "setoutputpath" => {
                if let Some(s) = val {
                    out.push(Cmd::SetOutput(s));
                }
                i += 1;
            }
            "setregion" => {
                let n: Vec<i32> = (1..=4).filter_map(|k| args.get(i + k).and_then(|s| s.parse().ok())).collect();
                if n.len() == 4 {
                    out.push(Cmd::SetRegion(n[0], n[1], n[2], n[3]));
                }
                i += 4;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

const PIPE: &str = r"\\.\pipe\mnRekord";

/// True when this is the only running copy.
pub fn claim_single_instance() -> bool {
    unsafe {
        let h = CreateMutexW(None, true, &HSTRING::from("mnRekordSingleInstance"));
        h.is_ok() && GetLastError() != ERROR_ALREADY_EXISTS
    }
}

pub fn send_to_first(args: &[String]) -> bool {
    unsafe {
        let file = CreateFileW(&HSTRING::from(PIPE), GENERIC_WRITE.0, FILE_SHARE_NONE, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None);
        let Ok(file) = file else { return false };
        let text = args.join("\n");
        let ok = WriteFile(file, Some(text.as_bytes()), None, None).is_ok();
        let _ = CloseHandle(file);
        ok
    }
}

pub fn serve_pipe(tx: Sender<Cmd>, on_cmd: impl Fn() + Send + 'static) {
    std::thread::spawn(move || unsafe {
        loop {
            let pipe = CreateNamedPipeW(&HSTRING::from(PIPE), PIPE_ACCESS_INBOUND, PIPE_TYPE_BYTE | PIPE_WAIT, 1, 4096, 4096, 0, None);
            if pipe.is_invalid() {
                return;
            }
            if ConnectNamedPipe(pipe, None).is_ok() || GetLastError() == ERROR_PIPE_CONNECTED {
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let mut n = 0u32;
                    if ReadFile(pipe, Some(&mut buf), Some(&mut n), None).is_err() || n == 0 {
                        break;
                    }
                    data.extend_from_slice(&buf[..n as usize]);
                }
                let args: Vec<String> = String::from_utf8_lossy(&data).split('\n').map(str::to_string).collect();
                for c in parse_args(&args) {
                    let _ = tx.send(c);
                }
                on_cmd();
            }
            let _ = DisconnectNamedPipe(pipe);
            let _ = CloseHandle(pipe);
        }
    });
}

pub struct Hotkeys {
    thread: u32,
    pub failed: Arc<Mutex<Vec<String>>>,
    pending: Arc<Mutex<Vec<(Shortcut, Cmd)>>>,
}

const WM_RELOAD: u32 = WM_USER + 1;

impl Hotkeys {
    pub fn start(tx: Sender<Cmd>, on_key: impl Fn(&Cmd) + Send + 'static) -> Hotkeys {
        let failed: Arc<Mutex<Vec<String>>> = Default::default();
        let pending: Arc<Mutex<Vec<(Shortcut, Cmd)>>> = Default::default();
        let (f, p) = (failed.clone(), pending.clone());
        let (id_tx, id_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || unsafe {
            let mut msg = MSG::default();
            let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE); // makes the queue
            let _ = id_tx.send(GetCurrentThreadId());
            let mut active: Vec<Cmd> = Vec::new();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_RELOAD {
                    for i in 0..active.len() {
                        let _ = UnregisterHotKey(None, i as i32 + 1);
                    }
                    active.clear();
                    let mut bad = Vec::new();
                    for (sc, cmd) in p.lock().unwrap().iter() {
                        let mut m = MOD_NOREPEAT;
                        if sc.ctrl {
                            m |= MOD_CONTROL;
                        }
                        if sc.alt {
                            m |= MOD_ALT;
                        }
                        if sc.shift {
                            m |= MOD_SHIFT;
                        }
                        active.push(cmd.clone());
                        let ok = sc.vk().is_some_and(|vk| RegisterHotKey(None, active.len() as i32, m, vk).is_ok());
                        if !ok {
                            bad.push(sc.label());
                        }
                    }
                    *f.lock().unwrap() = bad;
                } else if msg.message == WM_HOTKEY {
                    if let Some(c) = active.get(msg.wParam.0.wrapping_sub(1)) {
                        on_key(c);
                        let _ = tx.send(c.clone());
                    }
                }
            }
        });
        let thread = id_rx.recv().unwrap_or(0);
        Hotkeys { thread, failed, pending }
    }

    pub fn register(&self, list: Vec<(Shortcut, Cmd)>) {
        *self.pending.lock().unwrap() = list;
        unsafe {
            let _ = PostThreadMessageW(self.thread, WM_RELOAD, WPARAM(0), LPARAM(0));
        }
    }
}
