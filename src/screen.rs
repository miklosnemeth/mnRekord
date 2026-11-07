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
use windows::Win32::UI::WindowsAndMessaging::*;

struct Output {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    output: IDXGIOutput1,
    dup: Option<IDXGIOutputDuplication>,
    staging: Option<ID3D11Texture2D>,
    rect: Rect,
}

pub struct ScreenGrabber {
    region: Rect,
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
    pub fn new(region: Rect) -> Result<Self, String> {
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
                if rect.intersect(&self.region).is_none() {
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

    /// Latest picture of the region with the cursor, as top-down BGRA (stride = width * 4).
    pub fn frame(&mut self) -> &[u8] {
        let (w, h) = (self.region.w, self.region.h);
        unsafe {
            if self.gdi_only {
                let screen = GetDC(None);
                let _ = BitBlt(self.dc, 0, 0, w, h, Some(screen), self.region.x, self.region.y, SRCCOPY);
                ReleaseDC(None, screen);
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
        let tex: Option<ID3D11Texture2D> = res.and_then(|r| r.cast().ok());
        if let (Some(tex), true) = (tex, info.LastPresentTime != 0 || out.staging.is_none()) {
            if out.staging.is_none() {
                let mut d = D3D11_TEXTURE2D_DESC::default();
                tex.GetDesc(&mut d);
                d.Usage = D3D11_USAGE_STAGING;
                d.BindFlags = 0;
                d.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                d.MiscFlags = 0;
                let mut st = None;
                let _ = out.device.CreateTexture2D(&d, None, Some(&mut st));
                out.staging = st;
            }
            if let Some(st) = &out.staging {
                out.ctx.CopyResource(st, &tex);
                let mut m = D3D11_MAPPED_SUBRESOURCE::default();
                if out.ctx.Map(st, 0, D3D11_MAP_READ, 0, Some(&mut m)).is_ok() {
                    if let Some(part) = out.rect.intersect(&region) {
                        let line = part.w as usize * 4;
                        for row in 0..part.h as usize {
                            let sy = (part.y - out.rect.y) as usize + row;
                            let sx = (part.x - out.rect.x) as usize * 4;
                            let s = (m.pData as *const u8).add(sy * m.RowPitch as usize + sx);
                            let dy = (part.y - region.y) as usize + row;
                            let dx = (part.x - region.x) as usize * 4;
                            let d = self.base.as_mut_ptr().add(dy * region.w as usize * 4 + dx);
                            std::ptr::copy_nonoverlapping(s, d, line);
                        }
                    }
                    out.ctx.Unmap(st, 0);
                }
            }
        }
        let _ = dup.ReleaseFrame();
    }

    unsafe fn draw_cursor(&self) {
        let mut ci = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
        if GetCursorInfo(&mut ci).is_err() || ci.flags.0 & CURSOR_SHOWING.0 == 0 || ci.hCursor.is_invalid() {
            return;
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
