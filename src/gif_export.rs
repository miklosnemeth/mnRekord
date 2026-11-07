//! Turns a finished MP4 into an animated GIF: Media Foundation decodes, the gif crate encodes.
use std::path::Path;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

/// Widest picture and fastest frame rate of the GIF, and the longest part of the recording used.
pub const MAX_WIDTH: usize = 640;
pub const FPS: u32 = 12;
pub const MAX_SECONDS: f64 = 30.0;

pub fn export(src: &Path, dest: &Path) -> Result<(), String> {
    unsafe { run(src, dest) }.map_err(|e| e.to_string())
}

#[derive(Debug)]
enum Fail {
    Win(windows::core::Error),
    Msg(String),
}
impl From<windows::core::Error> for Fail {
    fn from(e: windows::core::Error) -> Self {
        Fail::Win(e)
    }
}
impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fail::Win(e) => write!(f, "{}", e.message()),
            Fail::Msg(m) => write!(f, "{m}"),
        }
    }
}

unsafe fn run(src: &Path, dest: &Path) -> Result<(), Fail> {
    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
    let result = convert(src, dest);
    let _ = MFShutdown();
    result
}

unsafe fn convert(src: &Path, dest: &Path) -> Result<(), Fail> {
    let mut attrs: Option<IMFAttributes> = None;
    MFCreateAttributes(&mut attrs, 1)?;
    let attrs = attrs.unwrap();
    attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
    let reader = MFCreateSourceReaderFromURL(&windows::core::HSTRING::from(src.as_os_str()), &attrs)?;
    let video = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
    reader.SetStreamSelection(video, true)?;

    let want = MFCreateMediaType()?;
    want.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    want.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
    reader.SetCurrentMediaType(video, None, &want)?;
    let got = reader.GetCurrentMediaType(video)?;
    let size = got.GetUINT64(&MF_MT_FRAME_SIZE)?;
    let (w, h) = ((size >> 32) as usize, (size & 0xFFFF_FFFF) as usize);
    // A negative stride means the rows are stored bottom-up.
    let stride = got.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|s| s as i32).unwrap_or(w as i32 * 4);
    let bottom_up = stride < 0;
    let stride = stride.unsigned_abs() as usize;
    if w == 0 || h == 0 {
        return Err(Fail::Msg("The video has no picture size".into()));
    }

    let (ow, oh) = if w > MAX_WIDTH { (MAX_WIDTH, (h * MAX_WIDTH / w).max(1)) } else { (w, h) };
    let (ow, oh) = (ow & !1, oh & !1);
    let file = std::fs::File::create(dest).map_err(|e| Fail::Msg(e.to_string()))?;
    let mut enc = gif::Encoder::new(file, ow as u16, oh as u16, &[]).map_err(|e| Fail::Msg(e.to_string()))?;
    enc.set_repeat(gif::Repeat::Infinite).map_err(|e| Fail::Msg(e.to_string()))?;

    let step = 10_000_000 / FPS as i64; // 100 ns units
    let limit = (MAX_SECONDS * 1e7) as i64;
    let mut next = 0i64;
    let mut frames = 0;
    loop {
        let mut flags = 0u32;
        let mut stamp = 0i64;
        let mut sample: Option<IMFSample> = None;
        reader.ReadSample(video, 0, None, Some(&mut flags), Some(&mut stamp), Some(&mut sample))?;
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 || stamp > limit {
            break;
        }
        let Some(sample) = sample else { continue };
        if stamp < next {
            continue;
        }
        next = stamp - stamp % step + step;
        let buf = sample.ConvertToContiguousBuffer()?;
        let mut data = std::ptr::null_mut();
        let mut len = 0u32;
        buf.Lock(&mut data, None, Some(&mut len))?;
        let pixels = std::slice::from_raw_parts(data, len as usize);
        let mut rgba = vec![255u8; ow * oh * 4];
        for y in 0..oh {
            let sy = y * h / oh;
            let row = if bottom_up { h - 1 - sy } else { sy };
            for x in 0..ow {
                let o = row * stride + (x * w / ow) * 4;
                if o + 2 < pixels.len() {
                    let d = (y * ow + x) * 4;
                    rgba[d] = pixels[o + 2];
                    rgba[d + 1] = pixels[o + 1];
                    rgba[d + 2] = pixels[o];
                }
            }
        }
        let _ = buf.Unlock();
        let mut frame = gif::Frame::from_rgba_speed(ow as u16, oh as u16, &mut rgba, 10);
        frame.delay = (100 / FPS) as u16;
        enc.write_frame(&frame).map_err(|e| Fail::Msg(e.to_string()))?;
        frames += 1;
    }
    if frames == 0 {
        return Err(Fail::Msg("No frames to convert".into()));
    }
    Ok(())
}
