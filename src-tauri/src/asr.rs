//! Spracherkennung über transcribe-cpp. Hält das gewählte Modell geladen,
//! zerlegt lange Aufnahmen per Silero-VAD und liefert Text + Sprache.

use crate::settings::Accelerator;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;
use transcribe_cpp::{
    Backend, CancelToken, Feature, Model, ModelOptions, RunOptions, Session, VadOptions,
    VadSession,
};

/// Modell, auf das ein langer Lauf (Meeting, Datei) festgelegt ist.
pub struct AsrModel {
    pub id: String,
    pub path: PathBuf,
    pub accel: Accelerator,
    /// Gesetzt = statt der Datei eine OpenAI-kompatible API nutzen.
    pub remote: Option<Remote>,
}

impl AsrModel {
    /// Name für den Verlauf: bei Remote das Modell des Dienstes.
    pub fn label(&self) -> String {
        self.remote.as_ref().map(|r| r.model.clone()).unwrap_or_else(|| self.id.clone())
    }
}

/// OpenAI-kompatibler Transkriptionsdienst (OpenAI, Groq, speaches, whisper.cpp-Server …).
#[derive(Clone, Debug)]
pub struct Remote {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

pub struct AsrOptions<'a> {
    /// ISO-Code oder None für automatische Erkennung.
    pub language: Option<String>,
    pub vocabulary: &'a [String],
    pub cancel: Option<&'a CancelToken>,
    /// Wird vor jedem Abschnitt nachgeladen, falls ein Diktat dazwischen das Modell
    /// gewechselt hat. None = das gerade geladene.
    pub model: Option<&'a AsrModel>,
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

enum Engine {
    Local { model: Model, session: Session },
    Remote(Remote),
}

struct Loaded {
    id: String,
    engine: Engine,
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
        Accelerator::Gpu => GPU_BACKEND,
    }
}

/// Die GPU-Schnittstelle dieses Builds (siehe Cargo.toml).
#[cfg(target_os = "macos")]
const GPU_BACKEND: Backend = Backend::Metal;
#[cfg(not(target_os = "macos"))]
const GPU_BACKEND: Backend = Backend::Vulkan;

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
    pub fn ensure_loaded(&self, m: &AsrModel) -> anyhow::Result<()> {
        self.load_into(&mut self.loaded.lock().unwrap(), m)
    }

    fn load_into(&self, guard: &mut Option<Loaded>, m: &AsrModel) -> anyhow::Result<()> {
        if let Some(remote) = &m.remote {
            // Nichts zu laden; die Adresse kann sich seit dem letzten Mal geändert haben.
            *guard = Some(Loaded {
                id: m.id.clone(),
                engine: Engine::Remote(remote.clone()),
                languages: Vec::new(),
                // Unter dem 25-MB-Limit von OpenAI (WAV: ~13 min); Längeres zerlegt die VAD.
                max_audio_ms: 600_000,
            });
            return Ok(());
        }
        let (id, path, accel) = (m.id.as_str(), m.path.as_path(), m.accel);
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
            engine: Engine::Local { model, session },
            languages: caps.languages,
            max_audio_ms: caps.max_audio_ms,
        });
        *self.last_used.lock().unwrap() = Instant::now();
        Ok(())
    }

    /// Das Modell für diesen Lauf, im selben Lock geladen wie benutzt.
    fn lock_model(&self, opts: &AsrOptions) -> anyhow::Result<MutexGuard<'_, Option<Loaded>>> {
        let mut guard = self.loaded.lock().unwrap();
        if let Some(m) = opts.model {
            self.load_into(&mut guard, m)?;
        }
        Ok(guard)
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
            .lock_model(opts)?
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
        let mut guard = self.lock_model(opts)?;
        let loaded = guard.as_mut().ok_or_else(|| anyhow::anyhow!("Kein Modell geladen"))?;
        let language = opts
            .language
            .clone()
            .filter(|lang| loaded.languages.is_empty() || loaded.languages.iter().any(|l| l == lang));
        let started = Instant::now();
        let (model, session) = match &mut loaded.engine {
            Engine::Local { model, session } => (model, session),
            Engine::Remote(remote) => {
                let result = transcribe_remote(remote, pcm, language.as_deref(), opts.vocabulary);
                *self.last_used.lock().unwrap() = Instant::now();
                log::info!("Remote-Transkription: {:.1}s Audio in {:?}", pcm.len() as f32 / 16_000.0, started.elapsed());
                return result;
            }
        };
        let mut run = RunOptions { language, ..Default::default() };
        if !opts.vocabulary.is_empty() {
            if model.supports(Feature::Vocabulary) {
                run.vocabulary = opts.vocabulary.to_vec();
            } else if model.supports(Feature::ContextPrompt) {
                run.prompt = Some(opts.vocabulary.join(", "));
            }
        }
        if let Some(token) = opts.cancel {
            session.set_cancel_token(token);
        }
        let result = session.run(pcm, &run);
        session.clear_cancel_token();
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
            .lock_model(opts)?
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

/// POST {base_url}/audio/transcriptions mit der Aufnahme als WAV.
pub fn transcribe_remote(r: &Remote, pcm: &[f32], language: Option<&str>, vocabulary: &[String]) -> anyhow::Result<AsrResult> {
    use reqwest::blocking::multipart::{Form, Part};
    let url = format!("{}/audio/transcriptions", r.base_url.trim_end_matches('/'));
    let wav = Part::bytes(crate::audio::wav_bytes(pcm)?).file_name("audio.wav").mime_str("audio/wav")?;
    let mut form = Form::new().part("file", wav).text("model", r.model.clone()).text("response_format", "json");
    if let Some(lang) = language {
        form = form.text("language", lang.to_string());
    }
    if !vocabulary.is_empty() {
        form = form.text("prompt", vocabulary.join(", "));
    }
    let client = reqwest::blocking::Client::builder().timeout(std::time::Duration::from_secs(120)).build()?;
    let mut request = client.post(&url).multipart(form);
    if !r.api_key.is_empty() {
        request = request.bearer_auth(&r.api_key);
    }
    let response = request.send().map_err(|e| {
        if e.is_timeout() {
            anyhow::anyhow!("Zeitüberschreitung")
        } else if e.is_connect() {
            anyhow::anyhow!("Keine Verbindung zu {}", r.base_url)
        } else {
            anyhow::anyhow!("{e}")
        }
    })?;
    let status = response.status();
    let value: serde_json::Value = response.json().unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        let msg = value["error"]["message"].as_str().unwrap_or("unbekannter Fehler");
        anyhow::bail!("API-Fehler {status}: {msg}");
    }
    let text = value["text"].as_str().ok_or_else(|| anyhow::anyhow!("Antwort ohne Text"))?;
    Ok(AsrResult { text: text.to_string(), language: None })
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
    use super::{merge_segments, transcribe_remote, Remote};
    use std::io::{Read, Write};

    #[test]
    fn remote_schickt_wav_und_liest_text() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut conn, _) = listener.accept().unwrap();
            let mut req = Vec::new();
            let mut buf = [0u8; 4096];
            // Bis zur schließenden Multipart-Grenze lesen.
            while !req.ends_with(b"--\r\n") {
                let n = conn.read(&mut buf).unwrap();
                req.extend_from_slice(&buf[..n]);
            }
            let body = r#"{"text":"Hallo Welt"}"#;
            write!(conn, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
            String::from_utf8_lossy(&req).to_string()
        });
        let remote = Remote { base_url, api_key: "geheim".into(), model: "whisper-1".into() };
        let result = transcribe_remote(&remote, &[0.0; 1600], Some("de"), &["Birdy".into()]).unwrap();
        assert_eq!(result.text, "Hallo Welt");
        let req = server.join().unwrap();
        assert!(req.starts_with("POST /v1/audio/transcriptions"));
        assert!(req.to_lowercase().contains("authorization: bearer geheim"));
        for part in ["whisper-1", "RIFF", "name=\"language\"\r\n\r\nde", "name=\"prompt\"\r\n\r\nBirdy"] {
            assert!(req.contains(part), "{part} fehlt");
        }
    }

    #[test]
    fn merges_close_segments_up_to_limit() {
        let segs = [(0, 100), (150, 300), (1000, 1100), (1150, 1900)];
        assert_eq!(merge_segments(&segs, 800, 100), vec![(0, 300), (1000, 1100), (1150, 1900)]);
    }
}
