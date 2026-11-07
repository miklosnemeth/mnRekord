//! Writes H.264 and AAC into a fragmented MP4 with Media Foundation. Fragments mean a
//! recording cut off by a crash still plays up to its last fragment.
use std::path::{Path, PathBuf};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::thread::JoinHandle;
use windows::core::{Interface, Result as WinResult, GUID, HSTRING};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

pub const AUDIO_RATE: u32 = 48_000;
pub const AUDIO_CHANNELS: u32 = 2;

pub enum Packet {
    Video { nv12: Vec<u8>, time: i64, duration: i64 },
    Audio { pcm: Vec<u8>, time: i64, duration: i64 },
    Finish,
}

#[derive(Clone)]
pub struct VideoSpec {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bits_per_pixel: f64,
}

pub struct Writer {
    pub tx: SyncSender<Packet>,
    pub handle: JoinHandle<Result<(), String>>,
}

impl Writer {
    pub fn spawn(path: &Path, video: VideoSpec, audio: bool) -> Result<Writer, String> {
        let (tx, rx) = sync_channel::<Packet>(64);
        let (ready_tx, ready_rx) = sync_channel::<Result<(), String>>(1);
        let path: PathBuf = path.to_path_buf();
        let handle = std::thread::spawn(move || unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
            let sink = match open(&path, &video, audio) {
                Ok(s) => {
                    let _ = ready_tx.send(Ok(()));
                    s
                }
                Err(e) => {
                    let msg = format!("Video File Open Failed ({e})");
                    let _ = ready_tx.send(Err(msg.clone()));
                    return Err(msg);
                }
            };
            let res = pump(&sink, &rx);
            let done = sink.writer.Finalize().map_err(|e| format!("Video Write Failed ({e})"));
            res.and(done)
        });
        ready_rx.recv().map_err(|_| "Failed to Init Recording".to_string())??;
        Ok(Writer { tx, handle })
    }
}

struct Sink {
    writer: IMFSinkWriter,
    video: u32,
    audio: Option<u32>,
}

fn pump(sink: &Sink, rx: &Receiver<Packet>) -> Result<(), String> {
    for p in rx {
        let r = match p {
            Packet::Video { nv12, time, duration } => write(&sink.writer, sink.video, &nv12, time, duration),
            Packet::Audio { pcm, time, duration } => match sink.audio {
                Some(a) => write(&sink.writer, a, &pcm, time, duration),
                None => Ok(()),
            },
            Packet::Finish => return Ok(()),
        };
        r.map_err(|e| format!("Video Write Failed ({e})"))?;
    }
    Ok(())
}

fn write(w: &IMFSinkWriter, stream: u32, data: &[u8], time: i64, duration: i64) -> WinResult<()> {
    unsafe {
        let buf = MFCreateMemoryBuffer(data.len() as u32)?;
        let mut p = std::ptr::null_mut();
        buf.Lock(&mut p, None, None)?;
        std::ptr::copy_nonoverlapping(data.as_ptr(), p, data.len());
        buf.Unlock()?;
        buf.SetCurrentLength(data.len() as u32)?;
        let s = MFCreateSample()?;
        s.AddBuffer(&buf)?;
        s.SetSampleTime(time)?;
        s.SetSampleDuration(duration)?;
        w.WriteSample(stream, &s)
    }
}

fn pack(a: u32, b: u32) -> u64 {
    ((a as u64) << 32) | b as u64
}

unsafe fn colour_info(t: &IMFMediaType) -> WinResult<()> {
    t.SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)?;
    t.SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)?;
    t.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
    t.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)
}

fn variant_u32(v: u32) -> windows::Win32::System::Variant::VARIANT {
    use windows::Win32::System::Variant::*;
    let mut var = VARIANT::default();
    unsafe {
        let inner = &mut *var.Anonymous.Anonymous;
        inner.vt = VT_UI4;
        inner.Anonymous.ulVal = v;
    }
    var
}

/// Quality-based rate control with the original's default quality (the encoder gets a quality number, not a bitrate).
/// Best effort: if the encoder refuses, the average bitrate set on the output type still applies.
unsafe fn set_quality(writer: &IMFSinkWriter, stream: u32) {
    let mut raw = std::ptr::null_mut();
    if writer.GetServiceForStream(stream, &GUID::zeroed(), &ICodecAPI::IID, &mut raw).is_err() || raw.is_null() {
        return;
    }
    let api: ICodecAPI = windows::core::Interface::from_raw(raw);
    let _ = api.SetValue(&CODECAPI_AVEncCommonRateControlMode, &variant_u32(eAVEncCommonRateControlMode_Quality.0 as u32));
    let _ = api.SetValue(&CODECAPI_AVEncCommonQuality, &variant_u32(crate::config::H264_QUALITY as u32));
}

unsafe fn open(path: &Path, v: &VideoSpec, audio: bool) -> WinResult<Sink> {
    let mut attrs: Option<IMFAttributes> = None;
    MFCreateAttributes(&mut attrs, 3)?;
    let attrs = attrs.unwrap();
    attrs.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_FMPEG4)?;
    attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
    let writer = MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, &attrs)?;

    let size = pack(v.width, v.height);
    let rate = pack(v.fps, 1);
    let out = MFCreateMediaType()?;
    out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
    let bits = (v.width as f64 * v.height as f64 * v.fps as f64 * v.bits_per_pixel).clamp(1e6, 80e6);
    out.SetUINT32(&MF_MT_AVG_BITRATE, bits as u32)?;
    out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
    out.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
    out.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
    out.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
    out.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Main.0 as u32)?;
    colour_info(&out)?;
    let video = writer.AddStream(&out)?;

    let inp = MFCreateMediaType()?;
    inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    inp.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
    inp.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
    inp.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
    inp.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
    inp.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
    inp.SetUINT32(&MF_MT_DEFAULT_STRIDE, v.width)?;
    colour_info(&inp)?;
    writer.SetInputMediaType(video, &inp, None)?;
    set_quality(&writer, video);

    let audio = if audio {
        let out = MFCreateMediaType()?;
        out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        out.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
        out.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        out.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, AUDIO_RATE)?;
        out.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, AUDIO_CHANNELS)?;
        out.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, crate::config::AUDIO_BYTES_PER_SEC)?;
        let idx = writer.AddStream(&out)?;
        let inp = MFCreateMediaType()?;
        inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        inp.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
        inp.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        inp.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, AUDIO_RATE)?;
        inp.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, AUDIO_CHANNELS)?;
        inp.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, AUDIO_CHANNELS * 2)?;
        inp.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, AUDIO_RATE * AUDIO_CHANNELS * 2)?;
        writer.SetInputMediaType(idx, &inp, None)?;
        Some(idx)
    } else {
        None
    };
    writer.BeginWriting()?;
    Ok(Sink { writer, video, audio })
}

/// BGRA (top-down) to NV12 in BT.709 limited range. `stride` is in bytes.
pub fn bgra_to_nv12(src: &[u8], stride: usize, w: usize, h: usize, out: &mut Vec<u8>) {
    out.resize(w * h * 3 / 2, 0);
    let (ys, uvs) = out.split_at_mut(w * h);
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).clamp(1, 4);
    let rows_per = ((h / 2) / threads).max(1);
    std::thread::scope(|sc| {
        let mut y_rest = ys;
        let mut uv_rest = uvs;
        let mut row = 0;
        while row < h / 2 {
            let n = rows_per.min(h / 2 - row);
            let (y_part, yr) = y_rest.split_at_mut(n * 2 * w);
            let (uv_part, ur) = uv_rest.split_at_mut(n * w);
            y_rest = yr;
            uv_rest = ur;
            let top = row * 2;
            sc.spawn(move || {
                for i in 0..n {
                    let r0 = (top + i * 2) * stride;
                    let a = &src[r0..r0 + w * 4];
                    let b = &src[r0 + stride..r0 + stride + w * 4];
                    for (d, p) in y_part[i * 2 * w..(i * 2 + 1) * w].iter_mut().zip(a.chunks_exact(4)) {
                        *d = luma(p);
                    }
                    for (d, p) in y_part[(i * 2 + 1) * w..(i * 2 + 2) * w].iter_mut().zip(b.chunks_exact(4)) {
                        *d = luma(p);
                    }
                    for (x, d) in uv_part[i * w..(i + 1) * w].chunks_exact_mut(2).enumerate() {
                        let (o, q) = (x * 8, x * 8 + 4);
                        let bl = (a[o] as i32 + a[q] as i32 + b[o] as i32 + b[q] as i32) >> 2;
                        let gr = (a[o + 1] as i32 + a[q + 1] as i32 + b[o + 1] as i32 + b[q + 1] as i32) >> 2;
                        let rd = (a[o + 2] as i32 + a[q + 2] as i32 + b[o + 2] as i32 + b[q + 2] as i32) >> 2;
                        d[0] = (((-26 * rd - 87 * gr + 112 * bl + 128) >> 8) + 128) as u8;
                        d[1] = (((112 * rd - 102 * gr - 10 * bl + 128) >> 8) + 128) as u8;
                    }
                }
            });
            row += n;
        }
    });
}

#[inline]
fn luma(p: &[u8]) -> u8 {
    (((47 * p[2] as i32 + 157 * p[1] as i32 + 16 * p[0] as i32 + 128) >> 8) + 16) as u8
}
