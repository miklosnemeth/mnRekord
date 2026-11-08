//! Screen frames: DXGI desktop duplication per monitor, GDI as the fallback, cursor drawn on top.
use crate::config::Rect;
use crate::win;
use windows::core::Interface;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::*;

struct Output {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    output: IDXGIOutput1,
    dup: Option<IDXGIOutputDuplication>,
    /// CPU-readable copy of the recorded part of this monitor, with its width and height.
    staging: Option<(ID3D11Texture2D, i32, i32)>,
    rect: Rect,
}

/// What the recording shows of the mouse pointer.
#[derive(Clone, Copy, Debug)]
pub struct CursorStyle {
    pub show: bool,
    /// A soft halo around the pointer and a ring while a mouse button is down.
    pub highlight: bool,
}

pub struct ScreenGrabber {
    region: Rect,
    cursor: CursorStyle,
    /// The region moved since the last frame, so the desktop is read again in full.
    moved: bool,
    all_outputs: bool,
    outputs: Vec<Output>,
    base: Vec<u8>,
    dc: HDC,
    dib: HBITMAP,
    bits: *mut u8,
    old: HGDIOBJ,
    gdi_only: bool,
}

unsafe impl Send for ScreenGrabber {}

impl ScreenGrabber {
    /// `all_outputs`: also watch the monitors outside the region, for a region that moves between them.
    pub fn new(region: Rect, cursor: CursorStyle, all_outputs: bool) -> Result<Self, String> {
        unsafe {
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            ReleaseDC(None, screen);
            let bmi = win::dib_info(region.w, region.h);
            let mut bits = std::ptr::null_mut();
            let dib = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|e| format!("Failed to Set Recording Region ({e})"))?;
            let old = SelectObject(dc, dib.into());
            let mut g = Self {
                region,
                cursor,
                all_outputs,
                moved: false,
                outputs: Vec::new(),
                base: vec![0; (region.w * region.h * 4) as usize],
                dc,
                dib,
                bits: bits as *mut u8,
                old,
                gdi_only: false,
            };
            g.gdi_only = g.open_outputs().is_err();
            Ok(g)
        }
    }

    unsafe fn open_outputs(&mut self) -> windows::core::Result<()> {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut a = 0;
        while let Ok(adapter) = factory.EnumAdapters1(a) {
            a += 1;
            let mut o = 0;
            while let Ok(output) = adapter.EnumOutputs(o) {
                o += 1;
                let desc = output.GetDesc()?;
                let rect = win::rect_of(&desc.DesktopCoordinates);
                if !self.all_outputs && rect.intersect(&self.region).is_none() {
                    continue;
                }
                if desc.Rotation != DXGI_MODE_ROTATION_IDENTITY && desc.Rotation != DXGI_MODE_ROTATION_UNSPECIFIED {
                    return Err(E_NOTIMPL.into());
                }
                let mut device = None;
                let mut ctx = None;
                D3D11CreateDevice(
                    &adapter,
                    D3D_DRIVER_TYPE_UNKNOWN,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_FLAG(0),
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut ctx),
                )?;
                let device = device.unwrap();
                let output: IDXGIOutput1 = output.cast()?;
                let dup = output.DuplicateOutput(&device)?;
                self.outputs.push(Output { device, ctx: ctx.unwrap(), output, dup: Some(dup), staging: None, rect });
            }
        }
        if self.outputs.is_empty() {
            return Err(E_FAIL.into());
        }
        Ok(())
    }

    /// Moves the region (same size), for recordings that follow a window.
    pub fn move_to(&mut self, x: i32, y: i32) {
        if (x, y) != (self.region.x, self.region.y) {
            self.region.x = x;
            self.region.y = y;
            self.moved = true;
        }
    }

    /// Latest picture of the region with the cursor, as top-down BGRA (stride = width * 4).
    pub fn frame(&mut self) -> &[u8] {
        let (w, h) = (self.region.w, self.region.h);
        unsafe {
            if self.gdi_only || self.moved {
                // The duplication only reports changes, so after a move the region is read directly.
                let screen = GetDC(None);
                let _ = BitBlt(self.dc, 0, 0, w, h, Some(screen), self.region.x, self.region.y, SRCCOPY);
                ReleaseDC(None, screen);
                if !self.gdi_only {
                    let _ = GdiFlush();
                    std::ptr::copy_nonoverlapping(self.bits, self.base.as_mut_ptr(), self.base.len());
                }
                self.moved = false;
            } else {
                for i in 0..self.outputs.len() {
                    self.pull(i);
                }
                std::ptr::copy_nonoverlapping(self.base.as_ptr(), self.bits, self.base.len());
            }
            self.draw_cursor();
            let _ = GdiFlush();
            std::slice::from_raw_parts(self.bits, (w * h * 4) as usize)
        }
    }

    unsafe fn pull(&mut self, i: usize) {
        let region = self.region;
        let out = &mut self.outputs[i];
        if out.dup.is_none() {
            match out.output.DuplicateOutput(&out.device) {
                Ok(d) => out.dup = Some(d),
                Err(_) => return,
            }
        }
        let dup = out.dup.as_ref().unwrap();
        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut res = None;
        match dup.AcquireNextFrame(0, &mut info, &mut res) {
            Ok(()) => {}
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return,
            Err(_) => {
                out.dup = None; // lost after a mode change or secure desktop; retried next frame
                return;
            }
        }
        if out.rect.intersect(&region).is_none() {
            // Nothing of this monitor is in the picture: don't copy it.
            let _ = dup.ReleaseFrame();
            return;
        }
        let tex: Option<ID3D11Texture2D> = res.and_then(|r| r.cast().ok());
        // Only the part of the monitor inside the region is copied off the GPU, not the whole desktop.
        let part = out.rect.intersect(&region);
        if let (Some(tex), Some(part)) = (tex, part) {
            let fits = out.staging.as_ref().is_some_and(|(_, w, h)| (*w, *h) == (part.w, part.h));
            if !fits {
                let mut d = D3D11_TEXTURE2D_DESC::default();
                tex.GetDesc(&mut d);
                d.Width = part.w as u32;
                d.Height = part.h as u32;
                d.MipLevels = 1;
                d.ArraySize = 1;
                d.Usage = D3D11_USAGE_STAGING;
                d.BindFlags = 0;
                d.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                d.MiscFlags = 0;
                let mut st = None;
                let _ = out.device.CreateTexture2D(&d, None, Some(&mut st));
                out.staging = st.map(|t| (t, part.w, part.h));
            }
            if let (Some((st, _, _)), true) = (&out.staging, info.LastPresentTime != 0 || !fits) {
                let left = (part.x - out.rect.x) as u32;
                let top = (part.y - out.rect.y) as u32;
                let bx = D3D11_BOX { left, top, front: 0, right: left + part.w as u32, bottom: top + part.h as u32, back: 1 };
                out.ctx.CopySubresourceRegion(st, 0, 0, 0, 0, &tex, 0, Some(&bx));
                let mut m = D3D11_MAPPED_SUBRESOURCE::default();
                if out.ctx.Map(st, 0, D3D11_MAP_READ, 0, Some(&mut m)).is_ok() {
                    let line = part.w as usize * 4;
                    for row in 0..part.h as usize {
                        let s = (m.pData as *const u8).add(row * m.RowPitch as usize);
                        let dy = (part.y - region.y) as usize + row;
                        let dx = (part.x - region.x) as usize * 4;
                        let d = self.base.as_mut_ptr().add(dy * region.w as usize * 4 + dx);
                        std::ptr::copy_nonoverlapping(s, d, line);
                    }
                    out.ctx.Unmap(st, 0);
                }
            }
        }
        let _ = dup.ReleaseFrame();
    }

    /// Blends a filled disc (or a ring when `ring` is set) of `color` (B, G, R) into the picture.
    unsafe fn blend_disc(&self, cx: i32, cy: i32, radius: f32, ring: bool, color: (u8, u8, u8), alpha: f32) {
        let (w, h) = (self.region.w, self.region.h);
        let r = radius.ceil() as i32 + 1;
        for y in (cy - r).max(0)..(cy + r + 1).min(h) {
            for x in (cx - r).max(0)..(cx + r + 1).min(w) {
                let d = (((x - cx) * (x - cx) + (y - cy) * (y - cy)) as f32).sqrt();
                // Edge coverage over one pixel, so the shapes are smooth.
                let cover = if ring { (3.0 - (d - radius).abs()).clamp(0.0, 1.0) } else { (radius - d + 0.5).clamp(0.0, 1.0) };
                if cover <= 0.0 {
                    continue;
                }
                let a = alpha * cover;
                let p = self.bits.add((y * w + x) as usize * 4);
                for (k, c) in [color.0, color.1, color.2].into_iter().enumerate() {
                    let old = *p.add(k) as f32;
                    *p.add(k) = (old + (c as f32 - old) * a) as u8;
                }
            }
        }
    }

    unsafe fn draw_cursor(&self) {
        if !self.cursor.show {
            return;
        }
        let mut ci = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
        if GetCursorInfo(&mut ci).is_err() || ci.flags.0 & CURSOR_SHOWING.0 == 0 || ci.hCursor.is_invalid() {
            return;
        }
        if self.cursor.highlight {
            let (cx, cy) = (ci.ptScreenPos.x - self.region.x, ci.ptScreenPos.y - self.region.y);
            let _ = GdiFlush(); // the bits are written directly below
            self.blend_disc(cx, cy, 24.0, false, (0, 220, 255), 0.30); // yellow halo
            let down = GetAsyncKeyState(0x01) as u16 & 0x8000 != 0 || GetAsyncKeyState(0x02) as u16 & 0x8000 != 0;
            if down {
                self.blend_disc(cx, cy, 20.0, true, (61, 83, 229), 0.95); // coral ring
            }
        }
        let icon: HICON = HICON(ci.hCursor.0);
        let mut ii = ICONINFO::default();
        if GetIconInfo(icon, &mut ii).is_err() {
            return;
        }
        let x = ci.ptScreenPos.x - ii.xHotspot as i32 - self.region.x;
        let y = ci.ptScreenPos.y - ii.yHotspot as i32 - self.region.y;
        let _ = DrawIconEx(self.dc, x, y, icon, 0, 0, 0, None, DI_NORMAL);
        if !ii.hbmColor.is_invalid() {
            let _ = DeleteObject(ii.hbmColor.into());
        }
        if !ii.hbmMask.is_invalid() {
            let _ = DeleteObject(ii.hbmMask.into());
        }
    }
}

impl Drop for ScreenGrabber {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.dib.into());
            let _ = DeleteDC(self.dc);
        }
    }
}
