//! Meeting-Modus: Mikrofon und/oder System-Audio aufnehmen, live transkribieren,
//! danach nach Sprechern aufteilen (Sortformer) und Notizen erzeugen.

use crate::asr::AsrOptions;
use crate::audio::{self, loopback::Loopback, mic::MicRecording, sounds};
use crate::catalog;
use crate::history::NewEntry;
use crate::i18n::t;
use crate::settings::MeetingSource;
use crate::state::state;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;

const SR: usize = 16_000;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    /// -1 = noch nicht zugeordnet
    pub speaker: i32,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub duration_ms: i64,
    pub source: MeetingSource,
    /// "recording" | "processing" | "done" | "error"
    pub status: String,
    pub segments: Vec<Segment>,
    pub speakers: Vec<String>,
    pub notes: Option<String>,
    pub error: Option<String>,
    pub diarized: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct MeetingSummary {
    pub id: String,
    pub title: String,
    pub created_at: i64,
    pub duration_ms: i64,
    pub status: String,
    pub speakers: usize,
    pub preview: String,
}

struct Active {
    id: String,
    stop: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct Meetings {
    active: Mutex<Option<Active>>,
}

impl Meetings {
    pub fn is_active(&self) -> bool {
        self.active.lock().unwrap().is_some()
    }

    pub fn active_id(&self) -> Option<String> {
        self.active.lock().unwrap().as_ref().map(|a| a.id.clone())
    }
}

fn dir(id: &str) -> PathBuf {
    state().dirs.meetings.join(id)
}

pub fn save(m: &Meeting) -> anyhow::Result<()> {
    let d = dir(&m.id);
    std::fs::create_dir_all(&d)?;
    let tmp = d.join("meeting.json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(m)?)?;
    std::fs::rename(tmp, d.join("meeting.json"))?;
    Ok(())
}

pub fn load(id: &str) -> anyhow::Result<Meeting> {
    if id.contains(['/', '\\', '.']) {
        anyhow::bail!("ungültige ID");
    }
    Ok(serde_json::from_slice(&std::fs::read(dir(id).join("meeting.json"))?)?)
}

fn emit_update(m: &Meeting) {
    let _ = state().app.emit("meeting-update", m);
}

pub fn list() -> Vec<MeetingSummary> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&state().dirs.meetings) {
        for e in entries.flatten() {
            let id = e.file_name().to_string_lossy().to_string();
            if let Ok(m) = load(&id) {
                let preview: String = m
                    .segments
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(160)
                    .collect();
                out.push(MeetingSummary {
                    id: m.id,
                    title: m.title,
                    created_at: m.created_at,
                    duration_ms: m.duration_ms,
                    status: m.status,
                    speakers: m.speakers.len(),
                    preview,
                });
            }
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

pub fn delete(id: &str) -> anyhow::Result<()> {
    if state().meeting.active_id().as_deref() == Some(id) {
        anyhow::bail!("Meeting läuft noch");
    }
    load(id)?; // validiert die ID
    std::fs::remove_dir_all(dir(id))?;
    Ok(())
}

pub fn toggle() {
    if state().meeting.is_active() {
        stop();
    } else if let Err(e) = start() {
        log::error!("Meeting konnte nicht starten: {e}");
        let _ = state().app.emit("meeting-error", e);
    }
}

pub fn start() -> Result<String, String> {
    let st = state();
    let s = st.settings();
    if st.meeting.is_active() {
        return Err("Es läuft bereits ein Meeting".into());
    }
    if st.dictation.is_busy() {
        return Err("Diktat läuft gerade".into());
    }
    if st.usable_asr_model().is_none() {
        return Err(t(&s.ui_language, "no_model"));
    }
    if !st.model_installed(catalog::VAD_MODEL) {
        return Err("VAD-Modell fehlt – bitte unter „Modelle“ laden.".into());
    }
    let now = chrono::Local::now();
    let id = now.format("%Y-%m-%d_%H-%M-%S").to_string();
    let title = format!("{} {}", t(&s.ui_language, "meeting_title"), now.format("%d.%m.%Y %H:%M"));
    let meeting = Meeting {
        id: id.clone(),
        title,
        created_at: now.timestamp_millis(),
        duration_ms: 0,
        source: s.meeting_source,
        status: "recording".into(),
        segments: Vec::new(),
        speakers: Vec::new(),
        notes: None,
        error: None,
        diarized: false,
    };
    save(&meeting).map_err(|e| e.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    *st.meeting.active.lock().unwrap() = Some(Active {
        id: id.clone(),
        stop: stop.clone(),
    });
    if s.sounds {
        sounds::play(sounds::Sound::Start, s.sound_volume);
    }
    crate::tray::set_status("meeting");
    emit_update(&meeting);
    std::thread::Builder::new()
        .name("meeting".into())
        .spawn(move || {
            let result = run(meeting, &stop);
            *state().meeting.active.lock().unwrap() = None;
            crate::tray::set_status("idle");
            if let Err((mut m, e)) = result {
                log::error!("Meeting fehlgeschlagen: {e:#}");
                m.status = "error".into();
                m.error = Some(format!("{e:#}"));
                let _ = save(&m);
                emit_update(&m);
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(id)
}

pub fn stop() {
    let st = state();
    if let Some(a) = st.meeting.active.lock().unwrap().as_ref() {
        a.stop.store(true, Ordering::SeqCst);
    }
    let s = st.settings();
    if s.sounds {
        sounds::play(sounds::Sound::Stop, s.sound_volume);
    }
}

#[derive(Clone, Serialize)]
struct Levels {
    id: String,
    mic: f32,
    system: f32,
    duration_ms: i64,
}

type Failure = (Meeting, anyhow::Error);

fn run(mut m: Meeting, stop: &AtomicBool) -> Result<(), Failure> {
    let st = state();
    let s = st.settings();
    macro_rules! tryf {
        ($e:expr) => {
            match $e {
                Ok(v) => v,
                Err(e) => return Err((m.clone(), anyhow::anyhow!("{e}"))),
            }
        };
    }
    tryf!(st.load_asr());
    tryf!(st.load_vad());

    let use_mic = matches!(m.source, MeetingSource::Microphone | MeetingSource::Both);
    let use_sys = matches!(m.source, MeetingSource::System | MeetingSource::Both);
    let mic_buf: Arc<Mutex<Vec<f32>>> = Arc::default();
    let sys_buf: Arc<Mutex<Vec<f32>>> = Arc::default();

    let mic = if use_mic {
        let buf = mic_buf.clone();
        Some(tryf!(MicRecording::start(
            s.microphone.clone(),
            false,
            Box::new(move |chunk: &[f32]| buf.lock().unwrap().extend_from_slice(chunk)),
            |e| log::error!("Meeting-Mikrofon ausgefallen: {e}"),
        )))
    } else {
        None
    };
    let lb = if use_sys {
        let buf = sys_buf.clone();
        match Loopback::start(move |samples| buf.lock().unwrap().extend_from_slice(samples)) {
            Ok(l) => Some(l),
            Err(e) if use_mic => {
                log::warn!("System-Audio nicht verfügbar, nur Mikrofon: {e:#}");
                None
            }
            Err(e) => return Err((m.clone(), e)),
        }
    } else {
        None
    };
    let have_sys = lb.is_some();

    let started = Instant::now();
    let mut full: Vec<f32> = Vec::with_capacity(SR * 60 * 30);
    let mut committed = 0usize;
    let mut last_check = Instant::now();
    let mut last_save = Instant::now();
    let language = (s.language != "auto").then(|| s.language.clone());

    loop {
        let stopping = stop.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(100));

        // --- Mischen: Mikrofon gibt den Takt vor, sonst die Uhr. ---
        let mic_chunk: Vec<f32> = if use_mic && mic.is_some() {
            std::mem::take(&mut *mic_buf.lock().unwrap())
        } else {
            Vec::new()
        };
        let n = if use_mic && mic.is_some() {
            mic_chunk.len()
        } else {
            let target = started.elapsed().as_millis() as usize * SR / 1000;
            let available = sys_buf.lock().unwrap().len();
            let needed = target.saturating_sub(full.len());
            // Etwas Puffer lassen, damit verspätete Loopback-Daten nicht als Stille enden.
            needed.saturating_sub(SR * 3 / 10).max(available.min(needed))
        };
        let sys_chunk: Vec<f32> = if have_sys {
            let mut b = sys_buf.lock().unwrap();
            if b.len() > n + SR {
                let excess = b.len() - n - SR / 4;
                b.drain(..excess);
            }
            let take = b.len().min(n);
            b.drain(..take).collect()
        } else {
            Vec::new()
        };
        let mut mixed = vec![0.0f32; n];
        for (i, v) in mixed.iter_mut().enumerate() {
            let a = mic_chunk.get(i).copied().unwrap_or(0.0);
            let b = sys_chunk.get(i).copied().unwrap_or(0.0);
            *v = (a + b).clamp(-1.0, 1.0);
        }
        full.extend_from_slice(&mixed);
        m.duration_ms = (full.len() / 16) as i64;
        let _ = st.app.emit(
            "meeting-level",
            Levels {
                id: m.id.clone(),
                mic: audio::level(&mic_chunk),
                system: audio::level(&sys_chunk),
                duration_ms: m.duration_ms,
            },
        );

        // --- Live-Transkription an Sprechpausen ---
        let pending = full.len() - committed;
        if stopping || (pending >= SR * 2 && last_check.elapsed() >= Duration::from_millis(1500)) {
            last_check = Instant::now();
            let slice = &full[committed..];
            let segments = st.asr.speech_segments(slice, 0).unwrap_or_default();
            let quiet_tail = SR * 7 / 10;
            let cut = if stopping {
                slice.len()
            } else if segments.is_empty() {
                // Nur Stille: bis auf die letzte Sekunde überspringen.
                if slice.len() > SR * 10 {
                    committed = full.len() - SR;
                }
                0
            } else if let Some(last_end) = segments
                .iter()
                .map(|s| s.1)
                .filter(|&e| e + quiet_tail <= slice.len())
                .last()
            {
                if slice.len() > SR * 25 || last_end >= SR * 3 {
                    last_end
                } else {
                    0
                }
            } else if slice.len() > SR * 25 {
                slice.len() - SR / 2
            } else {
                0
            };
            if cut > 0 {
                let chunk = &full[committed..committed + cut];
                let has_speech = !st.asr.speech_segments(chunk, 0).unwrap_or_default().is_empty();
                if has_speech {
                    match st.asr.transcribe(
                        chunk,
                        &AsrOptions {
                            language: language.clone(),
                            vocabulary: &s.custom_words,
                            cancel: None,
                        },
                    ) {
                        Ok(r) => {
                            let text = crate::text::apply_custom_words(
                                &crate::text::clean(&r.text, s.remove_fillers),
                                &s.custom_words,
                            );
                            if !text.is_empty() {
                                m.segments.push(Segment {
                                    start_ms: (committed / 16) as i64,
                                    end_ms: ((committed + cut) / 16) as i64,
                                    speaker: -1,
                                    text,
                                });
                                emit_update(&m);
                            }
                        }
                        Err(e) => log::warn!("Live-Transkription fehlgeschlagen: {e:#}"),
                    }
                }
                committed += cut;
            }
        }
        if last_save.elapsed() > Duration::from_secs(15) {
            last_save = Instant::now();
            let _ = save(&m);
        }
        if stopping {
            break;
        }
    }

    if let Some(mic) = mic {
        let _ = mic.stop();
    }
    if let Some(lb) = lb {
        lb.stop();
    }

    // --- Nachbearbeitung ---
    m.status = "processing".into();
    let _ = save(&m);
    emit_update(&m);
    if let Err(e) = audio::write_wav(&dir(&m.id).join("audio.wav"), &full) {
        log::warn!("Meeting-Audio nicht gespeichert: {e:#}");
    }
    let meeting_id = m.id.clone();
    let progress = move |p: f32| {
        let _ = state().app.emit("meeting-progress", (meeting_id.clone(), p));
    };
    match diarize_and_retranscribe(&full, language.clone(), &progress) {
        Ok(Some(segments)) if !segments.is_empty() => {
            let speakers = segments.iter().map(|s| s.speaker).max().unwrap_or(0) + 1;
            m.speakers = (1..=speakers).map(|i| format!("{} {i}", t(&s.ui_language, "speaker"))).collect();
            m.segments = segments;
            m.diarized = true;
        }
        Ok(_) => {
            for seg in &mut m.segments {
                seg.speaker = 0;
            }
            m.speakers = vec![format!("{} 1", t(&s.ui_language, "speaker"))];
        }
        Err(e) => {
            log::warn!("Sprechererkennung fehlgeschlagen: {e:#}");
            for seg in &mut m.segments {
                seg.speaker = 0;
            }
            m.speakers = vec![format!("{} 1", t(&s.ui_language, "speaker"))];
        }
    }
    m.status = "done".into();
    let _ = save(&m);
    emit_update(&m);

    let transcript = transcript_text(&m);
    if !transcript.is_empty() {
        if let Ok(entry) = st.history.add(
            NewEntry {
                text: transcript.clone(),
                raw_text: String::new(),
                source: "meeting",
                app: m.title.clone(),
                language: language.clone(),
                duration_ms: m.duration_ms,
                model: st.asr.loaded_id().unwrap_or_default(),
            },
            s.history_limit,
        ) {
            let _ = st.app.emit("history-added", &entry);
        }
    }
    if s.meeting_auto_notes && !transcript.is_empty() && st.enhancer.ready(&s).is_ok() {
        match tauri::async_runtime::block_on(st.enhancer.meeting_notes(&s, &transcript)) {
            Ok(notes) => {
                m.notes = Some(notes.trim().to_string());
                let _ = save(&m);
                emit_update(&m);
            }
            Err(e) => log::warn!("Meeting-Notizen fehlgeschlagen: {e:#}"),
        }
    }
    Ok(())
}

/// Sprecherwechsel erkennen und jede Sprecherpassage neu transkribieren.
/// `Ok(None)`, wenn kein Diarisierungsmodell installiert ist.
fn diarize_and_retranscribe(
    pcm: &[f32],
    language: Option<String>,
    progress: &dyn Fn(f32),
) -> anyhow::Result<Option<Vec<Segment>>> {
    let st = state();
    let s = st.settings();
    let Some(path) = st.model_file(catalog::DIARIZE_MODEL).filter(|p| p.exists()) else {
        return Ok(None);
    };
    if pcm.len() < SR * 2 {
        return Ok(None);
    }
    progress(0.0);
    let model = transcribe_cpp::Model::load(&path)?;
    let mut session = model.diarize_session()?;
    let turns = session.run(pcm, &transcribe_cpp::DiarizeOptions::default())?;
    drop(session);
    drop(model);
    let turns = clean_turns(
        turns.into_iter().map(|t| (t.t0_ms, t.t1_ms, t.speaker_id)).collect(),
        pcm.len() as i64 / 16,
    );
    if turns.is_empty() {
        return Ok(None);
    }
    progress(0.1);
    let total: i64 = turns.iter().map(|t| t.1 - t.0).sum::<i64>().max(1);
    let mut done = 0i64;
    let mut out = Vec::new();
    for (t0, t1, speaker) in turns {
        let a = (t0 as usize * 16).min(pcm.len());
        let b = (t1 as usize * 16).min(pcm.len());
        if b <= a + SR / 4 {
            continue;
        }
        let parts = st.asr.transcribe_long(
            &pcm[a..b],
            &AsrOptions {
                language: language.clone(),
                vocabulary: &s.custom_words,
                cancel: None,
            },
            &mut |_| {},
        )?;
        let text = parts.iter().map(|p| p.text.trim()).collect::<Vec<_>>().join(" ");
        let text = crate::text::apply_custom_words(&crate::text::clean(&text, s.remove_fillers), &s.custom_words);
        if !text.is_empty() {
            out.push(Segment {
                start_ms: t0,
                end_ms: t1,
                speaker,
                text,
            });
        }
        done += t1 - t0;
        progress(0.1 + 0.9 * done as f32 / total as f32);
    }
    Ok(Some(out))
}

/// Glättet Sprecher-Turns: winzige „Sprecher“ (Lachen, Musik) dem Nachbarn
/// zuordnen, gleiche Sprecher zusammenfassen und IDs nach Auftreten nummerieren.
fn clean_turns(mut turns: Vec<(i64, i64, i32)>, total_ms: i64) -> Vec<(i64, i64, i32)> {
    turns.retain(|t| t.1 > t.0);
    turns.sort_by_key(|t| t.0);
    let mut totals = std::collections::HashMap::<i32, i64>::new();
    for t in &turns {
        *totals.entry(t.2).or_default() += t.1 - t.0;
    }
    let min_ms = (total_ms * 3 / 100).max(5_000);
    let minor: Vec<i32> = totals.iter().filter(|(_, &d)| d < min_ms).map(|(&k, _)| k).collect();
    if minor.len() < totals.len() {
        let mut prev_major: Option<i32> = None;
        for i in 0..turns.len() {
            if minor.contains(&turns[i].2) {
                let next_major = turns[i + 1..].iter().find(|t| !minor.contains(&t.2)).map(|t| t.2);
                if let Some(sp) = prev_major.or(next_major) {
                    turns[i].2 = sp;
                }
            } else {
                prev_major = Some(turns[i].2);
            }
        }
    }
    let mut merged: Vec<(i64, i64, i32)> = Vec::new();
    for t in turns {
        if let Some(last) = merged.last_mut() {
            if last.2 == t.2 && t.0 - last.1 < 1_500 {
                last.1 = last.1.max(t.1);
                continue;
            }
        }
        merged.push(t);
    }
    let mut order: Vec<i32> = Vec::new();
    for t in &merged {
        if !order.contains(&t.2) {
            order.push(t.2);
        }
    }
    merged
        .into_iter()
        .map(|(a, b, sp)| (a, b, order.iter().position(|&o| o == sp).unwrap_or(0) as i32))
        .collect()
}

/// „Name: Text“-Zeilen, aufeinanderfolgende Beiträge desselben Sprechers zusammengefasst.
pub fn transcript_text(m: &Meeting) -> String {
    let mut lines: Vec<(i32, String)> = Vec::new();
    for seg in &m.segments {
        match lines.last_mut() {
            Some((sp, text)) if *sp == seg.speaker => {
                text.push(' ');
                text.push_str(&seg.text);
            }
            _ => lines.push((seg.speaker, seg.text.clone())),
        }
    }
    lines
        .into_iter()
        .map(|(sp, text)| {
            let name = m.speakers.get(sp.max(0) as usize).cloned().unwrap_or_default();
            if name.is_empty() {
                text
            } else {
                format!("{name}: {text}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub async fn generate_notes(id: String) -> Result<Meeting, String> {
    let st = state();
    let s = st.settings();
    st.enhancer.ready(&s)?;
    let mut m = load(&id).map_err(|e| e.to_string())?;
    let transcript = transcript_text(&m);
    let notes = st
        .enhancer
        .meeting_notes(&s, &transcript)
        .await
        .map_err(|e| format!("{e:#}"))?;
    m.notes = Some(notes.trim().to_string());
    save(&m).map_err(|e| e.to_string())?;
    emit_update(&m);
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::clean_turns;

    #[test]
    fn merges_and_renumbers_turns() {
        let turns = vec![
            (0, 10_000, 3),
            (10_200, 20_000, 3),
            (20_000, 21_000, 7), // Lachen: winziger Sprecher
            (21_000, 40_000, 1),
        ];
        let out = clean_turns(turns, 40_000);
        assert_eq!(out, vec![(0, 21_000, 0), (21_000, 40_000, 1)]);
    }
}
