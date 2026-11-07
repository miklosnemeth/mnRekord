//! Small Windows helpers: monitors, windows, cursor, shell, and the region frame.
use crate::config::Rect;
use std::path::Path;
use windows::core::{BOOL, HSTRING, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;

pub fn rect_of(r: &RECT) -> Rect {
    Rect::new(r.left, r.top, r.right - r.left, r.bottom - r.top)
}

pub fn virtual_screen() -> Rect {
    unsafe {
        Rect::new(
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

/// Monitors with the primary one first, as the original's default region uses the first.
pub fn monitors() -> Vec<Rect> {
    unsafe extern "system" fn cb(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let list = &mut *(data.0 as *mut Vec<(bool, Rect)>);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(m, &mut info).as_bool() {
            list.push((info.dwFlags & MONITORINFOF_PRIMARY != 0, rect_of(&info.rcMonitor)));
        }
        true.into()
    }
    let mut list: Vec<(bool, Rect)> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut list as *mut _ as isize));
    }
    list.sort_by_key(|(primary, r)| (!*primary, r.x, r.y));
    list.into_iter().map(|(_, r)| r).collect()
}

/// Visible top-level windows, front to back, for click-to-pick.
pub fn visible_windows(skip: HWND) -> Vec<Rect> {
    unsafe extern "system" fn cb(h: HWND, data: LPARAM) -> BOOL {
        let (list, skip) = &mut *(data.0 as *mut (Vec<Rect>, HWND));
        if h != *skip && IsWindowVisible(h).as_bool() && !IsIconic(h).as_bool() {
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4);
            let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
            if cloaked == 0 && ex & WS_EX_TOOLWINDOW.0 == 0 && GetWindowTextLengthW(h) > 0 {
                let mut r = RECT::default();
                if DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut _ as *mut _, 16).is_ok() {
                    let rect = rect_of(&r);
                    if rect.w > 40 && rect.h > 40 {
                        list.push(rect);
                    }
                }
            }
        }
        true.into()
    }
    let mut data = (Vec::new(), skip);
    unsafe {
        let _ = EnumWindows(Some(cb), LPARAM(&mut data as *mut _ as isize));
    }
    data.0
}

/// Hide or show every window of this thread in screen capture.
pub fn mask_thread_windows(masked: bool) {
    unsafe extern "system" fn cb(h: HWND, data: LPARAM) -> BOOL {
        let affinity = if data.0 != 0 { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
        let _ = SetWindowDisplayAffinity(h, affinity);
        true.into()
    }
    unsafe {
        let _ = EnumThreadWindows(GetCurrentThreadId(), Some(cb), LPARAM(masked as isize));
    }
}

pub fn cursor_pos() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

pub fn set_cursor_pos(x: i32, y: i32) {
    unsafe {
        let _ = SetCursorPos(x, y);
    }
}

pub fn window_rect(h: isize) -> Rect {
    let mut r = RECT::default();
    unsafe {
        let _ = GetWindowRect(HWND(h as *mut _), &mut r);
    }
    rect_of(&r)
}

pub fn place_window(h: isize, r: Rect, topmost: bool) {
    unsafe {
        let after = if topmost { HWND_TOPMOST } else { HWND_NOTOPMOST };
        let _ = SetWindowPos(HWND(h as *mut _), Some(after), r.x, r.y, r.w, r.h, SWP_NOACTIVATE);
    }
}

pub fn restore_window(h: isize) {
    unsafe {
        let _ = ShowWindowAsync(HWND(h as *mut _), SW_RESTORE);
    }
}

pub fn free_disk(path: &Path) -> Option<u64> {
    let mut free = 0u64;
    unsafe {
        GetDiskFreeSpaceExW(&HSTRING::from(path.as_os_str()), Some(&mut free), None, None).ok()?;
    }
    Some(free)
}

pub fn open_path(path: &Path) {
    unsafe {
        ShellExecuteW(None, &HSTRING::from("open"), &HSTRING::from(path.as_os_str()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

pub fn show_in_folder(path: &Path) {
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
}

/// Copy part of the desktop, scaled to `out_w` x `out_h`, as top-down RGBA.
pub fn grab_rgba(src: Rect, out_w: i32, out_h: i32) -> Option<Vec<u8>> {
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmi = dib_info(out_w, out_h);
        let mut bits = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(mem), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let old = SelectObject(mem, dib.into());
        let _ = SetStretchBltMode(mem, COLORONCOLOR); // fast; the preview refreshes many times a second
        let ok = StretchBlt(mem, 0, 0, out_w, out_h, Some(screen), src.x, src.y, src.w, src.h, SRCCOPY).as_bool();
        let mut out = None;
        if ok {
            let n = (out_w * out_h * 4) as usize;
            let mut px = std::slice::from_raw_parts(bits as *const u8, n).to_vec();
            for p in px.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
                p[3] = 255;
            }
            out = Some(px);
        }
        SelectObject(mem, old);
        let _ = DeleteObject(dib.into());
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        out
    }
}

pub fn dib_info(w: i32, h: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// The thin frame drawn around the region on screen. It lets clicks through.
pub struct RegionBorder {
    hwnd: HWND,
    shown: Option<Rect>,
}

impl RegionBorder {
    pub fn new() -> Option<Self> {
        unsafe {
            let class = HSTRING::from("mnRekordBorder");
            let inst = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).ok()?;
            let brush = CreateSolidBrush(COLORREF(0x003D53E5)); // coral, as 0x00BBGGRR
            let wc = WNDCLASSW {
                lpfnWndProc: Some(border_proc),
                hInstance: inst.into(),
                lpszClassName: PCWSTR(class.as_ptr()),
                hbrBackground: brush,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                &class,
                &HSTRING::from(""),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                Some(inst.into()),
                None,
            )
            .ok()?;
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
            let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
            Some(Self { hwnd, shown: None })
        }
    }

    pub fn show(&mut self, r: Rect) {
        if self.shown == Some(r) {
            return;
        }
        self.shown = Some(r);
        let t = border_px();
        let (w, h) = (r.w + 2 * t, r.h + 2 * t);
        unsafe {
            let outer = CreateRectRgn(0, 0, w, h);
            let inner = CreateRectRgn(t, t, w - t, h - t);
            let _ = CombineRgn(Some(outer), Some(outer), Some(inner), RGN_DIFF);
            let _ = DeleteObject(inner.into());
            SetWindowRgn(self.hwnd, Some(outer), true);
            let _ = SetWindowPos(self.hwnd, Some(HWND_TOPMOST), r.x - t, r.y - t, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
        }
    }

    pub fn hide(&mut self) {
        if self.shown.take().is_some() {
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        }
    }
}

unsafe extern "system" fn border_proc(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    DefWindowProcW(h, msg, w, l)
}

/// Border thickness: 3 px times the display scale, rounded.
fn border_px() -> i32 {
    let dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForSystem() } as f64;
    ((crate::config::BORDER_PX as f64) * dpi / 96.0).round() as i32
}
