//! System sound (loopback) through WASAPI, always 48 kHz stereo float.
use crate::encode::{AUDIO_CHANNELS, AUDIO_RATE};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;

pub struct AudioSource {
    pub level: Arc<AtomicU32>,
    pub queue: Arc<Mutex<VecDeque<f32>>>,
    pub capture_on: Arc<AtomicBool>,
    pub error: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
}

impl AudioSource {
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
    /// Captures the sound the computer plays.
    pub fn start() -> Arc<AudioSource> {
        let s = Arc::new(AudioSource {
            level: Default::default(),
            queue: Default::default(),
            capture_on: Default::default(),
            error: Default::default(),
            stop: Default::default(),
        });
        let t = s.clone();
        std::thread::spawn(move || {
            if let Err(e) = unsafe { run(&t) } {
                *t.error.lock().unwrap() = Some(e.message().to_string());
            }
        });
        s
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn device_enum() -> windows::core::Result<IMMDeviceEnumerator> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
    }
}

pub fn has_playback_device() -> bool {
    unsafe { device_enum().and_then(|e| e.GetDefaultAudioEndpoint(eRender, eConsole)).is_ok() }
}

unsafe fn run(s: &AudioSource) -> windows::core::Result<()> {
    let en = device_enum()?;
    let device = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
    let fmt = WAVEFORMATEX {
        wFormatTag: 3, // float
        nChannels: AUDIO_CHANNELS as u16,
        nSamplesPerSec: AUDIO_RATE,
        nAvgBytesPerSec: AUDIO_RATE * AUDIO_CHANNELS * 4,
        nBlockAlign: (AUDIO_CHANNELS * 4) as u16,
        wBitsPerSample: 32,
        cbSize: 0,
    };
    let flags = AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY | AUDCLNT_STREAMFLAGS_LOOPBACK;
    client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 10_000_000, 0, &fmt, None)?;
    let cap: IAudioCaptureClient = client.GetService()?;
    client.Start()?;
    let max_queue = (AUDIO_RATE * AUDIO_CHANNELS * 2) as usize; // two seconds
    let mut peak = 0f32;
    let mut quiet = 0u32;
    while !s.stop.load(Ordering::Relaxed) {
        let mut got = false;
        while cap.GetNextPacketSize()? > 0 {
            got = true;
            let mut data = std::ptr::null_mut();
            let (mut frames, mut fl) = (0u32, 0u32);
            cap.GetBuffer(&mut data, &mut frames, &mut fl, None, None)?;
            let n = (frames * AUDIO_CHANNELS) as usize;
            let silent = fl & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
            let samples: &[f32] = if silent { &[] } else { std::slice::from_raw_parts(data as *const f32, n) };
            for x in samples {
                peak = peak.max(x.abs());
            }
            if s.capture_on.load(Ordering::Relaxed) {
                let mut q = s.queue.lock().unwrap();
                if silent {
                    q.extend(std::iter::repeat_n(0.0, n));
                } else {
                    q.extend(samples.iter().copied());
                }
                let extra = q.len().saturating_sub(max_queue);
                q.drain(..extra);
            }
            cap.ReleaseBuffer(frames)?;
        }
        if got {
            quiet = 0;
        } else {
            quiet += 1;
        }
        let shown = if quiet > 10 { 0.0 } else { peak };
        let old = f32::from_bits(s.level.load(Ordering::Relaxed));
        s.level.store(shown.max(old * 0.88).to_bits(), Ordering::Relaxed);
        peak = 0.0;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let _ = client.Stop();
    Ok(())
}
