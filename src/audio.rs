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
    /// Captures the sound the computer plays, or only the sound of the process `pid` and its children.
    pub fn start(pid: Option<u32>) -> Arc<AudioSource> {
        let s = Arc::new(AudioSource {
            level: Default::default(),
            queue: Default::default(),
            capture_on: Default::default(),
            error: Default::default(),
            stop: Default::default(),
        });
        let t = s.clone();
        std::thread::spawn(move || {
            if let Err(e) = unsafe { run(&t, pid) } {
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

/// Receives the result of `ActivateAudioInterfaceAsync`.
#[windows::core::implement(IActivateAudioInterfaceCompletionHandler)]
struct Done(std::sync::mpsc::Sender<()>);

impl IActivateAudioInterfaceCompletionHandler_Impl for Done_Impl {
    fn ActivateCompleted(&self, _op: windows::core::Ref<'_, IActivateAudioInterfaceAsyncOperation>) -> windows::core::Result<()> {
        let _ = self.0.send(());
        Ok(())
    }
}

/// An audio client that hears only the process `pid` and its children (Windows 10 2004 or newer).
unsafe fn process_client(pid: u32) -> windows::core::Result<IAudioClient> {
    use windows::core::{Interface, HSTRING, PCWSTR};
    use windows::Win32::System::Com::StructuredStorage::{PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0};
    use windows::Win32::System::Variant::VT_BLOB;

    let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    let blob = windows::Win32::System::Com::BLOB {
        cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
        pBlobData: &mut params as *mut _ as *mut u8,
    };
    // The blob points into this stack frame: the PROPVARIANT must never try to free it.
    let prop = std::mem::ManuallyDrop::new(PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 { blob },
            }),
        },
    });
    let (tx, rx) = std::sync::mpsc::channel();
    let handler: IActivateAudioInterfaceCompletionHandler = Done(tx).into();
    let path = HSTRING::from("VAD\\Process_Loopback");
    let op = ActivateAudioInterfaceAsync(PCWSTR(path.as_ptr()), &IAudioClient::IID, Some(&*prop as *const PROPVARIANT), &handler)?;
    rx.recv_timeout(std::time::Duration::from_secs(5)).map_err(|_| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;
    let mut hr = windows::core::HRESULT(0);
    let mut unk: Option<windows::core::IUnknown> = None;
    op.GetActivateResult(&mut hr, &mut unk)?;
    hr.ok()?;
    unk.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?.cast()
}

unsafe fn run(s: &AudioSource, pid: Option<u32>) -> windows::core::Result<()> {
    let client: IAudioClient = match pid {
        Some(pid) => process_client(pid)?,
        None => {
            let en = device_enum()?;
            let device = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
            device.Activate(CLSCTX_ALL, None)?
        }
    };
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
    // A process loopback client is only offered with event driven buffers.
    let event = if pid.is_some() { Some(windows::Win32::System::Threading::CreateEventW(None, false, false, None)?) } else { None };
    let flags = if event.is_some() { flags | AUDCLNT_STREAMFLAGS_EVENTCALLBACK } else { flags };
    client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 10_000_000, 0, &fmt, None)?;
    if let Some(e) = event {
        client.SetEventHandle(e)?;
    }
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
    if let Some(e) = event {
        let _ = windows::Win32::Foundation::CloseHandle(e);
    }
    Ok(())
}
