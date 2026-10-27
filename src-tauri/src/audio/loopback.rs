//! System-Audio („was der PC abspielt“) per WASAPI-Loopback aufnehmen.
//! Liefert 16 kHz mono an den Callback.

use super::resample::Resampler;
use super::SAMPLE_RATE;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

pub struct Loopback {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Loopback {
    pub fn start(on_samples: impl FnMut(&[f32]) + Send + 'static) -> anyhow::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let stop2 = stop.clone();
        let thread = std::thread::Builder::new()
            .name("loopback".into())
            .spawn(move || {
                #[cfg(windows)]
                {
                    if let Err(e) = imp::run(&stop2, on_samples, &ready_tx) {
                        log::error!("Loopback-Aufnahme beendet: {e:#}");
                        let _ = ready_tx.send(Err(format!("{e:#}")));
                    }
                }
                #[cfg(not(windows))]
                {
                    let _ = (&stop2, on_samples);
                    let _ = ready_tx.send(Err("nur unter Windows verfügbar".into()));
                }
            })?;
        match ready_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Self {
                stop,
                thread: Some(thread),
            }),
            Ok(Err(e)) => anyhow::bail!("System-Audio nicht verfügbar: {e}"),
            Err(_) => anyhow::bail!("System-Audio antwortet nicht"),
        }
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Loopback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };

    pub fn run(
        stop: &AtomicBool,
        mut on_samples: impl FnMut(&[f32]),
        ready: &std::sync::mpsc::Sender<Result<(), String>>,
    ) -> anyhow::Result<()> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            let result = capture(stop, &mut on_samples, ready);
            CoUninitialize();
            result
        }
    }

    unsafe fn capture(
        stop: &AtomicBool,
        on_samples: &mut dyn FnMut(&[f32]),
        ready: &std::sync::mpsc::Sender<Result<(), String>>,
    ) -> anyhow::Result<()> {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
        let fmt_ptr: *mut WAVEFORMATEX = client.GetMixFormat()?;
        let fmt = *fmt_ptr;
        let channels = fmt.nChannels.max(1) as usize;
        let rate = fmt.nSamplesPerSec;
        let bits = fmt.wBitsPerSample;
        let init = client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            10_000_000,
            0,
            fmt_ptr,
            None,
        );
        CoTaskMemFree(Some(fmt_ptr as *const _));
        init?;
        if bits != 32 && bits != 16 {
            anyhow::bail!("nicht unterstütztes Format ({bits} Bit)");
        }
        let capture: IAudioCaptureClient = client.GetService()?;
        client.Start()?;
        let _ = ready.send(Ok(()));
        log::info!("Loopback gestartet: {rate} Hz, {channels} Kanäle, {bits} Bit");

        let mut resampler = Resampler::new(rate, SAMPLE_RATE);
        let mut mono = Vec::new();
        let mut out = Vec::new();
        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(10));
            loop {
                let packet = capture.GetNextPacketSize()?;
                if packet == 0 {
                    break;
                }
                let mut data: *mut u8 = std::ptr::null_mut();
                let mut frames: u32 = 0;
                let mut flags: u32 = 0;
                capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                mono.clear();
                let n = frames as usize;
                if flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data.is_null() {
                    mono.resize(n, 0.0);
                } else if bits == 32 {
                    let s = std::slice::from_raw_parts(data as *const f32, n * channels);
                    for frame in s.chunks(channels) {
                        mono.push(frame.iter().sum::<f32>() / channels as f32);
                    }
                } else {
                    let s = std::slice::from_raw_parts(data as *const i16, n * channels);
                    for frame in s.chunks(channels) {
                        mono.push(
                            frame.iter().map(|&v| v as f32 / 32768.0).sum::<f32>() / channels as f32,
                        );
                    }
                }
                capture.ReleaseBuffer(frames)?;
                out.clear();
                resampler.process(&mono, &mut out);
                if !out.is_empty() {
                    on_samples(&out);
                }
            }
        }
        let _ = client.Stop();
        Ok(())
    }
}
