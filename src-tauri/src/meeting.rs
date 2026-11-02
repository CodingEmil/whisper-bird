//! Meeting-Modus: Mikrofon und/oder System-Audio aufnehmen, live transkribieren,
//! danach nach Sprechern aufteilen (Sortformer) und Notizen erzeugen.
//!
//! Laufen Mikrofon und PC-Audio zusammen, bleiben beide Spuren auch getrennt
//! erhalten: Das Mikrofon ist „Ich“ (ohne Echo der Lautsprecher), nur die
//! Gegenseite im PC-Audio wird nach Sprechern aufgeteilt.

use crate::asr::{Asr, AsrOptions};
use crate::audio::{self, loopback::Loopback, mic::MicRecording, sounds};
use crate::catalog;
use crate::history::NewEntry;
use crate::i18n::t;
use crate::settings::MeetingSource;
use crate::state::state;
use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::path::{Path, PathBuf};
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
    // Beide Quellen da: Spuren zusätzlich getrennt behalten (16 Bit, halber Speicher).
    let separate = mic.is_some() && have_sys;
    let mut mic_track: Vec<i16> = Vec::new();
    let mut sys_track: Vec<i16> = Vec::new();

    let started = Instant::now();
    let mut full: Vec<f32> = Vec::with_capacity(SR * 60 * 30);
    let mut live = LiveCutter::default();
    let mut last_save = Instant::now();
    let language = (s.language != "auto").then(|| s.language.clone());
    let opts = AsrOptions {
        language: language.clone(),
        vocabulary: &s.custom_words,
        cancel: None,
    };

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
        full.extend(mix(&mic_chunk, &sys_chunk, n));
        if separate {
            push_track(&mut mic_track, &mic_chunk, n);
            push_track(&mut sys_track, &sys_chunk, n);
        }
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
        if let Some(range) = live.next(&full, &st.asr, stopping) {
            match live_segment(&st.asr, &full, range, &opts, s.remove_fillers) {
                Ok(Some(seg)) => {
                    m.segments.push(seg);
                    emit_update(&m);
                }
                Ok(None) => {}
                Err(e) => log::warn!("Live-Transkription fehlgeschlagen: {e:#}"),
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
    let diarize_model = st.model_file(catalog::DIARIZE_MODEL).filter(|p| p.exists());
    let diarized = if separate {
        // Speicher sparen: Die Mischung ist gespeichert, die Spuren werden zu f32.
        drop(full);
        let mut mic_pcm = track_f32(&std::mem::take(&mut mic_track));
        let sys_pcm = track_f32(&std::mem::take(&mut sys_track));
        diarize_tracks(&st.asr, diarize_model.as_deref(), &mut mic_pcm, &sys_pcm, &opts, s.remove_fillers, &progress)
    } else {
        match diarize_model {
            Some(model) => diarize_and_retranscribe(&st.asr, &model, &full, &opts, s.remove_fillers, &progress),
            None => Ok(None),
        }
    };
    assign_speakers(&mut m, diarized, &s.ui_language);
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

/// Mischt Mikrofon und System-Audio zu einem Block von `n` Samples.
pub fn mix(mic: &[f32], sys: &[f32], n: usize) -> Vec<f32> {
    (0..n)
        .map(|i| {
            let a = mic.get(i).copied().unwrap_or(0.0);
            let b = sys.get(i).copied().unwrap_or(0.0);
            (a + b).clamp(-1.0, 1.0)
        })
        .collect()
}

/// Hängt einen Block an eine Einzelspur an, auf `n` Samples aufgefüllt – so bleiben
/// beide Spuren auf die Sample genau neben der Mischung.
pub fn push_track(track: &mut Vec<i16>, chunk: &[f32], n: usize) {
    track.extend(
        (0..n).map(|i| (chunk.get(i).copied().unwrap_or(0.0).clamp(-1.0, 1.0) * i16::MAX as f32) as i16),
    );
}

pub fn track_f32(track: &[i16]) -> Vec<f32> {
    track.iter().map(|&v| v as f32 / i16::MAX as f32).collect()
}

/// Zerlegt das laufende Meeting-Audio an Sprechpausen in Abschnitte fürs Live-Transkript.
#[derive(Default)]
pub struct LiveCutter {
    /// Bis hierhin ist das Audio transkribiert oder als Stille übersprungen.
    committed: usize,
    /// Audiolänge bei der letzten Prüfung.
    checked: usize,
}

impl LiveCutter {
    /// Nächster abgeschlossener Abschnitt von `full` als Samplebereich. Geprüft wird
    /// höchstens alle 1,5 s Audio; beim Stoppen kommt der ganze Rest.
    pub fn next(&mut self, full: &[f32], asr: &Asr, stopping: bool) -> Option<Range<usize>> {
        let pending = full.len() - self.committed;
        if !stopping && (pending < SR * 2 || full.len() - self.checked < SR * 3 / 2) {
            return None;
        }
        self.checked = full.len();
        let slice = &full[self.committed..];
        let segments = asr.speech_segments(slice, 0).unwrap_or_default();
        let quiet_tail = SR * 7 / 10;
        let cut = if stopping {
            slice.len()
        } else if segments.is_empty() {
            // Nur Stille: bis auf die letzte Sekunde überspringen.
            if slice.len() > SR * 10 {
                self.committed = full.len() - SR;
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
        if cut == 0 {
            return None;
        }
        let range = self.committed..self.committed + cut;
        self.committed += cut;
        Some(range)
    }
}

/// Transkribiert einen Live-Abschnitt. `None`, wenn darin niemand spricht.
pub fn live_segment(
    asr: &Asr,
    full: &[f32],
    range: Range<usize>,
    opts: &AsrOptions,
    remove_fillers: bool,
) -> anyhow::Result<Option<Segment>> {
    let chunk = &full[range.clone()];
    if asr.speech_segments(chunk, 0).unwrap_or_default().is_empty() {
        return Ok(None);
    }
    let text = tidy(&asr.transcribe(chunk, opts)?.text, opts, remove_fillers);
    Ok((!text.is_empty()).then(|| Segment {
        start_ms: (range.start / 16) as i64,
        end_ms: (range.end / 16) as i64,
        speaker: -1,
        text,
    }))
}

fn tidy(text: &str, opts: &AsrOptions, remove_fillers: bool) -> String {
    crate::text::apply_custom_words(&crate::text::clean(text, remove_fillers), opts.vocabulary)
}

/// Ergebnis der Sprechererkennung.
#[derive(Debug)]
pub struct Diarized {
    pub segments: Vec<Segment>,
    /// Sprecher 0 ist das eigene Mikrofon („Ich“).
    pub me: bool,
}

/// Übernimmt das Ergebnis der Sprechererkennung – oder ordnet alles einem Sprecher zu.
pub fn assign_speakers(m: &mut Meeting, diarized: anyhow::Result<Option<Diarized>>, ui_language: &str) {
    let name = |i: i32| format!("{} {i}", t(ui_language, "speaker"));
    match diarized {
        Ok(Some(d)) if !d.segments.is_empty() => {
            let speakers = d.segments.iter().map(|s| s.speaker).max().unwrap_or(0) + 1;
            m.speakers = if d.me {
                std::iter::once(t(ui_language, "me")).chain((1..speakers).map(name)).collect()
            } else {
                (1..=speakers).map(name).collect()
            };
            m.segments = d.segments;
            m.diarized = true;
            return;
        }
        Ok(_) => {}
        Err(e) => log::warn!("Sprechererkennung fehlgeschlagen: {e:#}"),
    }
    for seg in &mut m.segments {
        seg.speaker = 0;
    }
    m.speakers = vec![name(1)];
}

/// Sprecherwechsel mit dem Diarisierungsmodell `model` erkennen und jede
/// Sprecherpassage neu transkribieren. `Ok(None)`, wenn das Audio zu kurz ist.
pub fn diarize_and_retranscribe(
    asr: &Asr,
    model: &Path,
    pcm: &[f32],
    opts: &AsrOptions,
    remove_fillers: bool,
    progress: &dyn Fn(f32),
) -> anyhow::Result<Option<Diarized>> {
    if pcm.len() < SR * 2 {
        return Ok(None);
    }
    progress(0.0);
    let turns = sortformer_turns(model, pcm)?;
    if turns.is_empty() {
        return Ok(None);
    }
    progress(0.1);
    let turns: Vec<Turn> = turns.into_iter().map(|(t0, t1, speaker)| (t0, t1, speaker, pcm)).collect();
    let segments = transcribe_turns(asr, &turns, opts, remove_fillers, &|p| progress(0.1 + 0.9 * p))?;
    Ok(Some(Diarized { segments, me: false }))
}

/// Getrennte Spuren: Das Mikrofon ist „Ich“. Zuerst wird das Lautsprecher-Echo
/// aus `mic` abgezogen (an Ort und Stelle), danach fallen Passagen weg, in denen
/// es nur noch die Lautsprecher mithört. Das PC-Audio wird mit `model` nach
/// Sprechern aufgeteilt, ohne Modell gilt es als ein Sprecher. Jede Passage wird
/// aus ihrer eigenen Spur transkribiert, so stört die andere Seite nicht.
pub fn diarize_tracks(
    asr: &Asr,
    model: Option<&Path>,
    mic: &mut [f32],
    sys: &[f32],
    opts: &AsrOptions,
    remove_fillers: bool,
    progress: &dyn Fn(f32),
) -> anyhow::Result<Option<Diarized>> {
    if mic.len().max(sys.len()) < SR * 2 {
        return Ok(None);
    }
    progress(0.0);
    if let Err(e) = audio::echo::cancel(mic, sys) {
        log::warn!("Echounterdrückung fehlgeschlagen, nur Echo-Erkennung: {e:#}");
    }
    let mic: &[f32] = mic;
    let to_ms = |r: &(usize, usize)| ((r.0 / 16) as i64, (r.1 / 16) as i64);
    let own = audio::echo::own_speech(mic, sys, &asr.speech_segments(mic, 0)?);
    let mic_turns = merge_turns(own.iter().map(|r| { let (a, b) = to_ms(r); (a, b, 0) }).collect());

    let sys_turns = match model.filter(|_| sys.len() >= SR * 2) {
        Some(model) => match sortformer_turns(model, sys) {
            Ok(turns) => Some(turns),
            Err(e) => {
                log::warn!("Sprechererkennung im PC-Audio fehlgeschlagen, ein Sprecher: {e:#}");
                None
            }
        },
        None => None,
    };
    let sys_turns = match sys_turns {
        Some(turns) if !turns.is_empty() => turns,
        _ => merge_turns(asr.speech_segments(sys, 0)?.iter().map(|r| { let (a, b) = to_ms(r); (a, b, 0) }).collect()),
    };
    progress(0.1);

    let me = !mic_turns.is_empty();
    let offset = me as i32;
    let mut turns: Vec<Turn> = mic_turns.into_iter().map(|(a, b, sp)| (a, b, sp, mic)).collect();
    turns.extend(sys_turns.into_iter().map(|(a, b, sp)| (a, b, sp + offset, sys)));
    turns.sort_by_key(|t| (t.0, t.2));
    if turns.is_empty() {
        return Ok(None);
    }
    let mut segments = transcribe_turns(asr, &turns, opts, remove_fillers, &|p| progress(0.1 + 0.9 * p))?;
    let me = renumber(&mut segments, me);
    Ok(Some(Diarized { segments, me }))
}

/// Eine Sprecherpassage: Beginn und Ende in ms, Sprecher, Spur.
type Turn<'a> = (i64, i64, i32, &'a [f32]);

fn sortformer_turns(model: &Path, pcm: &[f32]) -> anyhow::Result<Vec<(i64, i64, i32)>> {
    let model = transcribe_cpp::Model::load(model)?;
    let mut session = model.diarize_session()?;
    let turns = session.run(pcm, &transcribe_cpp::DiarizeOptions::default())?;
    Ok(clean_turns(
        turns.into_iter().map(|t| (t.t0_ms, t.t1_ms, t.speaker_id)).collect(),
        pcm.len() as i64 / 16,
    ))
}

/// Transkribiert jede Passage aus ihrer Spur; `progress` läuft von 0 bis 1.
fn transcribe_turns(
    asr: &Asr,
    turns: &[Turn],
    opts: &AsrOptions,
    remove_fillers: bool,
    progress: &dyn Fn(f32),
) -> anyhow::Result<Vec<Segment>> {
    let total: i64 = turns.iter().map(|t| t.1 - t.0).sum::<i64>().max(1);
    let mut done = 0i64;
    let mut out = Vec::new();
    for &(t0, t1, speaker, pcm) in turns {
        let a = (t0 as usize * 16).min(pcm.len());
        let b = (t1 as usize * 16).min(pcm.len());
        done += t1 - t0;
        if b <= a + SR / 4 {
            continue;
        }
        let parts = asr.transcribe_long(&pcm[a..b], opts, &mut |_| {})?;
        let text = parts.iter().map(|p| p.text.trim()).collect::<Vec<_>>().join(" ");
        let text = tidy(&text, opts, remove_fillers);
        if !text.is_empty() {
            out.push(Segment {
                start_ms: t0,
                end_ms: t1,
                speaker,
                text,
            });
        }
        progress(done as f32 / total as f32);
    }
    Ok(out)
}

/// Fasst Passagen desselben Sprechers mit weniger als 1,5 s Pause zusammen.
fn merge_turns(turns: Vec<(i64, i64, i32)>) -> Vec<(i64, i64, i32)> {
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
    merged
}

/// Lückenlose Sprechernummern, „Ich“ bleibt vorn. Liefert, ob „Ich“ noch vorkommt.
fn renumber(segments: &mut [Segment], me: bool) -> bool {
    let mut used: Vec<i32> = segments.iter().map(|s| s.speaker).collect();
    used.sort_unstable();
    used.dedup();
    for seg in segments.iter_mut() {
        seg.speaker = used.iter().position(|&u| u == seg.speaker).unwrap_or(0) as i32;
    }
    me && used.first() == Some(&0)
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
    let merged = merge_turns(turns);
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
    use super::*;

    fn meeting(segments: Vec<Segment>) -> Meeting {
        Meeting {
            id: "test".into(),
            title: "Test".into(),
            created_at: 0,
            duration_ms: 0,
            source: MeetingSource::Microphone,
            status: "processing".into(),
            segments,
            speakers: Vec::new(),
            notes: None,
            error: None,
            diarized: false,
        }
    }

    fn seg(start_ms: i64, speaker: i32, text: &str) -> Segment {
        Segment { start_ms, end_ms: start_ms + 1000, speaker, text: text.into() }
    }

    #[test]
    fn mischen_fuellt_auf_und_begrenzt() {
        assert_eq!(mix(&[0.5, 0.9], &[0.25, 0.5, -2.0], 4), vec![0.75, 1.0, -1.0, 0.0]);
        assert_eq!(mix(&[0.1, 0.2, 0.3], &[], 2), vec![0.1, 0.2]);
    }

    #[test]
    fn ohne_sprechererkennung_spricht_einer() {
        let mut m = meeting(vec![seg(0, -1, "Hallo."), seg(2000, -1, "Weiter.")]);
        assign_speakers(&mut m, Ok(None), "de");
        assert!(!m.diarized);
        assert_eq!(m.speakers, vec!["Sprecher 1"]);
        assert!(m.segments.iter().all(|s| s.speaker == 0));

        let mut m = meeting(vec![seg(0, -1, "Hallo.")]);
        assign_speakers(&mut m, Err(anyhow::anyhow!("kaputt")), "en");
        assert_eq!(m.speakers, vec!["Speaker 1"]);
    }

    #[test]
    fn transkript_fasst_sprecher_zusammen() {
        let mut m = meeting(Vec::new());
        let diarized = vec![seg(0, 0, "Guten Tag."), seg(1000, 0, "Los geht's."), seg(2000, 1, "Hallo.")];
        assign_speakers(&mut m, Ok(Some(Diarized { segments: diarized, me: false })), "de");
        assert!(m.diarized);
        assert_eq!(m.speakers, vec!["Sprecher 1", "Sprecher 2"]);
        assert_eq!(transcript_text(&m), "Sprecher 1: Guten Tag. Los geht's.\n\nSprecher 2: Hallo.");
    }

    #[test]
    fn eigenes_mikrofon_heisst_ich() {
        let mut m = meeting(Vec::new());
        let diarized = vec![seg(0, 0, "Wie war's?"), seg(1000, 1, "Gut."), seg(2000, 2, "Sehr gut.")];
        assign_speakers(&mut m, Ok(Some(Diarized { segments: diarized, me: true })), "de");
        assert_eq!(m.speakers, vec!["Ich", "Sprecher 1", "Sprecher 2"]);
        assert_eq!(transcript_text(&m), "Ich: Wie war's?\n\nSprecher 1: Gut.\n\nSprecher 2: Sehr gut.");
    }

    #[test]
    fn nummern_ohne_luecken() {
        // Sprecher 2 hat nichts Verständliches gesagt.
        let mut segs = vec![seg(0, 0, "a"), seg(1000, 3, "b"), seg(2000, 1, "c")];
        assert!(renumber(&mut segs, true));
        assert_eq!(segs.iter().map(|s| s.speaker).collect::<Vec<_>>(), vec![0, 2, 1]);
        // Vom Mikrofon kam nichts: „Ich“ entfällt.
        let mut segs = vec![seg(0, 1, "a"), seg(1000, 2, "b")];
        assert!(!renumber(&mut segs, true));
        assert_eq!(segs.iter().map(|s| s.speaker).collect::<Vec<_>>(), vec![0, 1]);
    }

    #[test]
    fn spuren_bleiben_deckungsgleich() {
        let mut mic = Vec::new();
        let mut sys = Vec::new();
        push_track(&mut mic, &[0.5, -1.5, 0.25], 3);
        push_track(&mut sys, &[0.5], 3);
        assert_eq!((mic.len(), sys.len()), (3, 3));
        assert_eq!(sys[1..], [0, 0]);
        let back = track_f32(&mic);
        assert!((back[0] - 0.5).abs() < 1e-3 && back[1] == -1.0 && (back[2] - 0.25).abs() < 1e-3);
    }

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
