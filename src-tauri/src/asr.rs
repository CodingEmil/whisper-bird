//! Spracherkennung über transcribe-cpp. Hält das gewählte Modell geladen,
//! zerlegt lange Aufnahmen per Silero-VAD und liefert Text + Sprache.

use crate::settings::Accelerator;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;
use transcribe_cpp::{
    Backend, CancelToken, Feature, Model, ModelOptions, RunOptions, Session, VadOptions,
    VadSession,
};

pub struct AsrOptions<'a> {
    /// ISO-Code oder None für automatische Erkennung.
    pub language: Option<String>,
    pub vocabulary: &'a [String],
    pub cancel: Option<&'a CancelToken>,
}

#[derive(Clone, Debug, Default)]
pub struct AsrResult {
    pub text: String,
    pub language: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TimedText {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

struct Loaded {
    id: String,
    model: Model,
    session: Session,
    languages: Vec<String>,
    max_audio_ms: i64,
}

pub struct Asr {
    loaded: Mutex<Option<Loaded>>,
    vad: Mutex<Option<VadSession>>,
    last_used: Mutex<Instant>,
}

fn backend(accel: Accelerator) -> Backend {
    match accel {
        Accelerator::Auto => Backend::Auto,
        Accelerator::Cpu => Backend::Cpu,
        Accelerator::Gpu => Backend::Vulkan,
    }
}

impl Default for Asr {
    fn default() -> Self {
        Self {
            loaded: Mutex::new(None),
            vad: Mutex::new(None),
            last_used: Mutex::new(Instant::now()),
        }
    }
}

impl Asr {
    pub fn loaded_id(&self) -> Option<String> {
        self.loaded.lock().unwrap().as_ref().map(|l| l.id.clone())
    }

    pub fn idle_secs(&self) -> u64 {
        self.last_used.lock().unwrap().elapsed().as_secs()
    }

    pub fn unload(&self) {
        if self.loaded.lock().unwrap().take().is_some() {
            log::info!("Spracherkennungsmodell entladen");
        }
    }

    /// Lädt das Modell, falls noch nicht (oder ein anderes) geladen ist.
    pub fn ensure_loaded(&self, id: &str, path: &Path, accel: Accelerator) -> anyhow::Result<()> {
        let mut guard = self.loaded.lock().unwrap();
        if guard.as_ref().map(|l| l.id == id).unwrap_or(false) {
            return Ok(());
        }
        *guard = None;
        let started = Instant::now();
        let options = ModelOptions {
            backend: backend(accel),
            ..Default::default()
        };
        let model = match Model::load_with(path, &options) {
            Ok(m) => m,
            Err(e) if accel == Accelerator::Gpu => {
                log::warn!("GPU-Laden fehlgeschlagen ({e}), weiche auf Auto aus");
                Model::load(path)?
            }
            Err(e) => return Err(e.into()),
        };
        let caps = model.capabilities()?;
        let session = model.session()?;
        log::info!(
            "Modell {id} geladen in {:?} (Backend {}, {} Sprachen)",
            started.elapsed(),
            model.backend(),
            caps.languages.len()
        );
        *guard = Some(Loaded {
            id: id.to_string(),
            model,
            session,
            languages: caps.languages,
            max_audio_ms: caps.max_audio_ms,
        });
        *self.last_used.lock().unwrap() = Instant::now();
        Ok(())
    }

    /// Lädt das VAD-Modell (klein, bleibt geladen).
    pub fn ensure_vad(&self, path: &Path) -> anyhow::Result<()> {
        let mut vad = self.vad.lock().unwrap();
        if vad.is_none() {
            let model = Model::load_with(
                path,
                &ModelOptions {
                    backend: Backend::Cpu,
                    ..Default::default()
                },
            )?;
            *vad = Some(model.vad_session()?);
        }
        Ok(())
    }

    pub fn has_vad(&self) -> bool {
        self.vad.lock().unwrap().is_some()
    }

    /// Sprachsegmente (in Samples) einer Aufnahme.
    pub fn speech_segments(&self, pcm: &[f32], max_speech_ms: i32) -> anyhow::Result<Vec<(usize, usize)>> {
        let mut vad = self.vad.lock().unwrap();
        let vad = vad.as_mut().ok_or_else(|| anyhow::anyhow!("VAD nicht geladen"))?;
        let opts = VadOptions {
            min_silence_ms: 400,
            speech_pad_ms: 120,
            max_speech_ms,
            ..Default::default()
        };
        let result = vad.run(pcm, &opts)?;
        Ok(result
            .segments
            .iter()
            .map(|s| (s.start_sample.max(0) as usize, (s.end_sample.max(0) as usize).min(pcm.len())))
            .filter(|(a, b)| b > a)
            .collect())
    }

    /// Transkribiert einen Block (Diktat). Lange Blöcke werden zerlegt.
    pub fn transcribe(&self, pcm: &[f32], opts: &AsrOptions) -> anyhow::Result<AsrResult> {
        let max_ms = self
            .loaded
            .lock()
            .unwrap()
            .as_ref()
            .map(|l| l.max_audio_ms)
            .ok_or_else(|| anyhow::anyhow!("Kein Modell geladen"))?;
        let dur_ms = pcm.len() as i64 * 1000 / 16_000;
        if max_ms > 0 && dur_ms > max_ms && self.has_vad() {
            let parts = self.transcribe_long(pcm, opts, &mut |_| {})?;
            let text = parts.iter().map(|p| p.text.trim()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
            return Ok(AsrResult { text, language: None });
        }
        self.run(pcm, opts)
    }

    fn run(&self, pcm: &[f32], opts: &AsrOptions) -> anyhow::Result<AsrResult> {
        let mut guard = self.loaded.lock().unwrap();
        let loaded = guard.as_mut().ok_or_else(|| anyhow::anyhow!("Kein Modell geladen"))?;
        let mut run = RunOptions::default();
        if let Some(lang) = &opts.language {
            if loaded.languages.is_empty() || loaded.languages.iter().any(|l| l == lang) {
                run.language = Some(lang.clone());
            }
        }
        if !opts.vocabulary.is_empty() {
            if loaded.model.supports(Feature::Vocabulary) {
                run.vocabulary = opts.vocabulary.to_vec();
            } else if loaded.model.supports(Feature::ContextPrompt) {
                run.prompt = Some(opts.vocabulary.join(", "));
            }
        }
        if let Some(token) = opts.cancel {
            loaded.session.set_cancel_token(token);
        }
        let started = Instant::now();
        let result = loaded.session.run(pcm, &run);
        loaded.session.clear_cancel_token();
        *self.last_used.lock().unwrap() = Instant::now();
        let transcript = result?;
        log::info!(
            "Transkription: {:.1}s Audio in {:?}",
            pcm.len() as f32 / 16_000.0,
            started.elapsed()
        );
        Ok(AsrResult {
            text: transcript.text,
            language: transcript.language,
        })
    }

    /// Lange Aufnahme (Datei, Meeting) per VAD in Abschnitte zerlegen und
    /// nacheinander transkribieren. `progress` bekommt 0.0..1.0.
    pub fn transcribe_long(
        &self,
        pcm: &[f32],
        opts: &AsrOptions,
        progress: &mut dyn FnMut(f32),
    ) -> anyhow::Result<Vec<TimedText>> {
        let max_ms = self
            .loaded
            .lock()
            .unwrap()
            .as_ref()
            .map(|l| l.max_audio_ms)
            .unwrap_or(30_000);
        // Abschnitte deutlich unter dem Modelllimit halten.
        let limit_ms: i64 = if max_ms > 0 { (max_ms - 2_000).clamp(10_000, 60_000) } else { 60_000 };
        let segments = self.speech_segments(pcm, limit_ms as i32)?;
        let chunks = merge_segments(&segments, (limit_ms as usize) * 16, 16_000 * 2);
        let total: usize = chunks.iter().map(|(a, b)| b - a).sum::<usize>().max(1);
        let mut done = 0usize;
        let mut out = Vec::new();
        for (a, b) in chunks {
            if opts.cancel.map(|c| c.is_cancelled()).unwrap_or(false) {
                anyhow::bail!("abgebrochen");
            }
            let res = self.run(&pcm[a..b], opts)?;
            let text = res.text.trim().to_string();
            if !text.is_empty() {
                out.push(TimedText {
                    start_ms: a as i64 / 16,
                    end_ms: b as i64 / 16,
                    text,
                });
            }
            done += b - a;
            progress(done as f32 / total as f32);
        }
        Ok(out)
    }
}

/// Fasst nahe Sprachsegmente zu Blöcken bis `max_len` Samples zusammen.
fn merge_segments(segments: &[(usize, usize)], max_len: usize, max_gap: usize) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &(a, b) in segments {
        if let Some(last) = out.last_mut() {
            if a.saturating_sub(last.1) <= max_gap && b - last.0 <= max_len {
                last.1 = b;
                continue;
            }
        }
        out.push((a, b));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::merge_segments;

    #[test]
    fn merges_close_segments_up_to_limit() {
        let segs = [(0, 100), (150, 300), (1000, 1100), (1150, 1900)];
        assert_eq!(merge_segments(&segs, 800, 100), vec![(0, 300), (1000, 1100), (1150, 1900)]);
    }
}
