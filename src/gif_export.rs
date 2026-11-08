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
    // The decoder's own NV12 output is used: asking for RGB would convert every decoded picture,
    // while only about one in three becomes a GIF frame.
    let reader = MFCreateSourceReaderFromURL(&windows::core::HSTRING::from(src.as_os_str()), None)?;
    let video = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
    reader.SetStreamSelection(video, true)?;

    let want = MFCreateMediaType()?;
    want.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    want.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
    reader.SetCurrentMediaType(video, None, &want)?;
    let got = reader.GetCurrentMediaType(video)?;
    let size = got.GetUINT64(&MF_MT_FRAME_SIZE)?;
    // The decoded picture may be taller than the video (1088 rows for 1080): the visible part is the aperture.
    let (fw, fh) = ((size >> 32) as usize, (size & 0xFFFF_FFFF) as usize);
    let stride = got.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|s| (s as i32).unsigned_abs() as usize).unwrap_or(fw).max(fw);
    let mut area = MFVideoArea::default();
    let has_area = got
        .GetBlob(&MF_MT_MINIMUM_DISPLAY_APERTURE, std::slice::from_raw_parts_mut(&mut area as *mut _ as *mut u8, std::mem::size_of::<MFVideoArea>()), None)
        .is_ok();
    let (x0, y0, w, h) = if has_area && area.Area.cx > 0 && area.Area.cy > 0 {
        (area.OffsetX.value.max(0) as usize, area.OffsetY.value.max(0) as usize, area.Area.cx as usize, area.Area.cy as usize)
    } else {
        (0, 0, fw, fh)
    };
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
    // Pictures are decoded in order, then their palettes are worked out on all cores, a batch at a time.
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 16);
    let mut batch: Vec<Vec<u8>> = Vec::with_capacity(threads * 2);
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
        // Rows of the Y plane, from the buffer itself: the decoder may pad the picture (1088 rows for
        // 1080) even when the media type says otherwise, and the colour plane starts after the padding.
        let rows = (pixels.len() * 2 / 3 / stride).max(fh.min(pixels.len() / stride));
        let rgba = nv12_to_rgba(pixels, stride, rows, (x0, y0, w, h), ow, oh);
        let _ = buf.Unlock();
        batch.push(rgba);
        frames += 1;
        if batch.len() == batch.capacity() {
            write_batch(&mut enc, &mut batch, ow, oh, threads)?;
        }
    }
    write_batch(&mut enc, &mut batch, ow, oh, threads)?;
    if frames == 0 {
        return Err(Fail::Msg("No frames to convert".into()));
    }
    Ok(())
}

/// Scales the visible part `(x0, y0, w, h)` of an NV12 picture to `ow` x `oh` RGBA (nearest pixel),
/// BT.709 limited range as the recorder writes it. `rows` is the height of the Y plane.
fn nv12_to_rgba(p: &[u8], stride: usize, rows: usize, (x0, y0, w, h): (usize, usize, usize, usize), ow: usize, oh: usize) -> Vec<u8> {
    let mut out = vec![255u8; ow * oh * 4];
    let uv = stride * rows;
    for y in 0..oh {
        let sy = y0 + y * h / oh;
        for x in 0..ow {
            let sx = x0 + x * w / ow;
            let (yi, ci) = (sy * stride + sx, uv + (sy / 2) * stride + (sx & !1));
            if ci + 1 >= p.len() {
                continue;
            }
            let c = (p[yi] as i32 - 16) * 298;
            let (u, v) = (p[ci] as i32 - 128, p[ci + 1] as i32 - 128);
            let d = (y * ow + x) * 4;
            out[d] = ((c + 459 * v + 128) >> 8).clamp(0, 255) as u8;
            out[d + 1] = ((c - 55 * u - 136 * v + 128) >> 8).clamp(0, 255) as u8;
            out[d + 2] = ((c + 541 * u + 128) >> 8).clamp(0, 255) as u8;
        }
    }
    out
}

/// Quantizes the pictures of `batch` in parallel and writes them in order.
fn write_batch<W: std::io::Write>(enc: &mut gif::Encoder<W>, batch: &mut Vec<Vec<u8>>, ow: usize, oh: usize, threads: usize) -> Result<(), Fail> {
    if batch.is_empty() {
        return Ok(());
    }
    let per = batch.len().div_ceil(threads);
    let frames: Option<Vec<gif::Frame<'static>>> = std::thread::scope(|sc| {
        let jobs: Vec<_> = batch
            .chunks_mut(per)
            .map(|part| {
                sc.spawn(move || {
                    part.iter_mut()
                        .map(|rgba| {
                            let mut f = gif::Frame::from_rgba_speed(ow as u16, oh as u16, rgba, 10);
                            f.delay = (100 / FPS) as u16;
                            f
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut all = Vec::new();
        for j in jobs {
            all.extend(j.join().ok()?);
        }
        Some(all)
    });
    batch.clear();
    let frames = frames.ok_or_else(|| Fail::Msg("The GIF palette could not be made".into()))?;
    for f in &frames {
        enc.write_frame(f).map_err(|e| Fail::Msg(e.to_string()))?;
    }
    Ok(())
}
