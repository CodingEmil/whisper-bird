//! Mikrofonaufnahme über handy-recorder (WASAPI), geliefert als 16 kHz mono.

use handy_recorder::{AudioChunk, Recorder, RecorderConfig, Sink};
use serde::Serialize;

pub type ChunkFn = Box<dyn FnMut(&[f32]) + Send>;

pub struct MicSink {
    samples: Vec<f32>,
    keep: bool,
    on_chunk: ChunkFn,
}

impl Sink for MicSink {
    fn process_chunk(&mut self, chunk: AudioChunk<'_>) {
        let real = &chunk.samples[..chunk.valid_frames * chunk.channels as usize];
        if self.keep {
            self.samples.extend_from_slice(real);
        }
        (self.on_chunk)(real);
    }
}

pub struct MicRecording {
    recorder: Recorder<MicSink>,
    pub device_name: String,
}

impl MicRecording {
    /// Öffnet das Mikrofon und startet die Aufnahme.
    /// `keep`: Samples sammeln (Diktat) oder nur weiterreichen (Meeting).
    /// `on_chunk`: bekommt alle ~30 ms einen Block (für Pegel/Streaming).
    /// `on_fail`: wird aufgerufen, wenn das Gerät während der Aufnahme ausfällt.
    pub fn start(
        device: Option<String>,
        keep: bool,
        on_chunk: ChunkFn,
        on_fail: impl FnOnce(String) + Send + 'static,
    ) -> anyhow::Result<Self> {
        let config = RecorderConfig {
            device,
            ..RecorderConfig::speech()
        };
        let recorder = match Recorder::open_with_failure_handler(config.clone(), move |e| {
            on_fail(e.to_string())
        }) {
            Ok(r) => r,
            // Gespeichertes Gerät nicht mehr vorhanden: auf Standardgerät ausweichen.
            Err(e) if config.device.is_some() => {
                log::warn!("Mikrofon {:?} nicht verfügbar ({e}), nutze Standard", config.device);
                Recorder::open(RecorderConfig::speech())?
            }
            Err(e) => return Err(e.into()),
        };
        let device_name = recorder.info().device.name.clone();
        recorder
            .start(MicSink {
                samples: Vec::with_capacity(16_000 * 30),
                keep,
                on_chunk,
            })
            .map_err(|e| anyhow::anyhow!("{}", e.error))?;
        Ok(Self {
            recorder,
            device_name,
        })
    }

    /// Stoppt und liefert die gesammelten Samples.
    pub fn stop(self) -> anyhow::Result<Vec<f32>> {
        let stopped = self.recorder.stop()?;
        if !stopped.is_complete() {
            log::warn!(
                "Aufnahme unvollständig: {:?}, {} Frames verloren",
                stopped.end_reason,
                stopped.dropped_frames
            );
        }
        let samples = stopped.sink.samples;
        let _ = self.recorder.close();
        Ok(samples)
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MicDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

pub fn list_devices() -> Vec<MicDevice> {
    match handy_recorder::list_input_devices() {
        Ok(devices) => devices
            .into_iter()
            .filter(|d| !d.is_monitor)
            .map(|d| MicDevice {
                id: d.id,
                name: d.name,
                is_default: d.is_default,
            })
            .collect(),
        Err(e) => {
            log::warn!("Mikrofone konnten nicht aufgelistet werden: {e}");
            Vec::new()
        }
    }
}
