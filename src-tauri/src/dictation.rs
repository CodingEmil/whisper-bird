//! Diktat-Ablauf: Aufnahme → Transkription → Aufräumen → (Enhance) → Einfügen.

use crate::asr::AsrOptions;
use crate::audio::{self, ducking, mic::MicRecording, sounds::{self, Sound}};
use crate::enhance::DictationContext;
use crate::history::NewEntry;
use crate::i18n::t;
use crate::insert::{self, InsertOptions};
use crate::overlay::{self, Phase as OverlayPhase};
use crate::settings::{EnhanceMode, Settings};
use crate::state::state;
use crate::text::{self, SnippetMatch};
use crate::win::foreground::{self, AppContext};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::Emitter;
use transcribe_cpp::CancelToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Normales Diktat.
    Plain,
    /// Diktat + Enhance (zweiter Shortcut).
    Enhance,
    /// Sprachbefehl für das Kontextmenü „Text bearbeiten“ – wird nicht eingefügt.
    Instruction,
}

enum Phase {
    Idle,
    Starting { mode: Mode, stop_requested: bool, generation: u64 },
    Recording { rec: MicRecording, started: Instant, mode: Mode, app: AppContext },
    Processing { cancel: CancelToken },
}

pub struct Dictation {
    phase: Mutex<Phase>,
    generation: AtomicU64,
}

impl Default for Dictation {
    fn default() -> Self {
        Self {
            phase: Mutex::new(Phase::Idle),
            generation: AtomicU64::new(0),
        }
    }
}

#[derive(Clone, Serialize)]
struct StatusEvent {
    status: &'static str,
    mode: Option<Mode>,
}

fn emit_status(status: &'static str, mode: Option<Mode>) {
    let _ = state().app.emit("dictation-status", StatusEvent { status, mode });
    crate::tray::set_status(status);
}

fn wants_enhance(s: &Settings, mode: Mode) -> bool {
    match mode {
        Mode::Instruction => false,
        Mode::Enhance => true,
        Mode::Plain => s.enhance_mode == EnhanceMode::Always,
    }
}

impl Dictation {
    pub fn is_recording(&self) -> bool {
        matches!(*self.phase.lock().unwrap(), Phase::Recording { .. } | Phase::Starting { .. })
    }

    pub fn is_busy(&self) -> bool {
        !matches!(*self.phase.lock().unwrap(), Phase::Idle)
    }

    pub fn toggle(&'static self, mode: Mode) {
        let action = match &*self.phase.lock().unwrap() {
            Phase::Idle => 1,
            Phase::Starting { .. } | Phase::Recording { .. } => 2,
            Phase::Processing { .. } => 0,
        };
        match action {
            1 => self.start(mode),
            2 => self.stop(),
            _ => {}
        }
    }

    pub fn start(&'static self, mode: Mode) {
        let st = state();
        let s = st.settings();
        if st.meeting.is_active() {
            overlay::set(OverlayPhase::Warning, Some(t(&s.ui_language, "meeting_running")), false, None);
            schedule_idle(2500);
            return;
        }
        if st.usable_asr_model().is_none() {
            if s.sounds {
                sounds::play(Sound::Error, s.sound_volume);
            }
            overlay::set(OverlayPhase::Error, Some(t(&s.ui_language, "no_model")), false, None);
            schedule_idle(4000);
            crate::show_main_window(Some("models"));
            return;
        }
        let generation = {
            let mut phase = self.phase.lock().unwrap();
            if !matches!(*phase, Phase::Idle) {
                return;
            }
            let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            *phase = Phase::Starting { mode, stop_requested: false, generation };
            generation
        };
        let enhance = wants_enhance(&s, mode);
        if s.sounds {
            sounds::play(Sound::Start, s.sound_volume);
        }
        if mode != Mode::Instruction {
            overlay::set(OverlayPhase::Recording, None, enhance, None);
        }
        st.hotkeys.set_cancel_active(true);
        emit_status("recording", Some(mode));

        std::thread::spawn(move || {
            let app_ctx = foreground::active_app();
            let on_chunk = Box::new({
                let mut n = 0u32;
                move |chunk: &[f32]| {
                    n += 1;
                    let level = audio::level(chunk);
                    overlay::level(level);
                    if mode == Mode::Instruction || n % 2 == 0 {
                        let _ = state().app.emit_to(crate::editmenu::LABEL, "audio-level", level);
                    }
                }
            });
            let on_fail = move |msg: String| {
                log::error!("Mikrofon ausgefallen: {msg}");
                let st = state();
                let lang = st.settings().ui_language;
                overlay::set(OverlayPhase::Error, Some(format!("{}: {msg}", t(&lang, "mic_failed"))), false, None);
                st.dictation.stop();
            };
            let result = MicRecording::start(s.microphone.clone(), true, on_chunk, on_fail);
            let mut phase = self.phase.lock().unwrap();
            let still_ours = matches!(*phase, Phase::Starting { generation: g, .. } if g == generation);
            match result {
                Ok(rec) if still_ours => {
                    let stop_now = matches!(*phase, Phase::Starting { stop_requested: true, .. });
                    *phase = Phase::Recording {
                        rec,
                        started: Instant::now(),
                        mode,
                        app: app_ctx,
                    };
                    drop(phase);
                    if s.audio_ducking {
                        // Kurz warten, damit der Startton noch zu hören ist.
                        std::thread::sleep(Duration::from_millis(250));
                        if self.is_recording() {
                            ducking::duck(s.ducking_level);
                        }
                    }
                    if stop_now {
                        self.stop();
                    }
                }
                Ok(rec) => {
                    // Inzwischen abgebrochen.
                    drop(phase);
                    let _ = rec.stop();
                }
                Err(e) => {
                    *phase = Phase::Idle;
                    drop(phase);
                    log::error!("Mikrofon konnte nicht geöffnet werden: {e:#}");
                    finish_with_error(&s, format!("{}: {e}", t(&s.ui_language, "mic_failed")));
                    return;
                }
            }
            // Modell und ggf. Enhance schon während des Sprechens vorbereiten.
            let st = state();
            if let Err(e) = st.load_asr() {
                log::error!("Modell konnte nicht geladen werden: {e:#}");
            }
            if enhance && st.enhancer.ready(&s).is_ok() {
                tauri::async_runtime::block_on(st.enhancer.warm(&s));
            }
        });
    }

    pub fn stop(&'static self) {
        let mut phase = self.phase.lock().unwrap();
        match std::mem::replace(&mut *phase, Phase::Idle) {
            Phase::Starting { mode, generation, .. } => {
                *phase = Phase::Starting { mode, stop_requested: true, generation };
            }
            Phase::Recording { rec, started, mode, app } => {
                let cancel = CancelToken::new();
                *phase = Phase::Processing { cancel: cancel.clone() };
                drop(phase);
                ducking::restore();
                std::thread::spawn(move || {
                    let result = process(rec, started, mode, app, &cancel);
                    *self.phase.lock().unwrap() = Phase::Idle;
                    state().hotkeys.set_cancel_active(false);
                    emit_status("idle", None);
                    if let Err(e) = result {
                        let s = state().settings();
                        log::error!("Diktat fehlgeschlagen: {e:#}");
                        finish_with_error(&s, format!("{e}"));
                    }
                });
            }
            other => *phase = other,
        }
    }

    /// Bricht Aufnahme oder Verarbeitung ab – es wird nichts eingefügt.
    pub fn cancel(&self) {
        let mut phase = self.phase.lock().unwrap();
        match std::mem::replace(&mut *phase, Phase::Idle) {
            Phase::Recording { rec, mode, .. } => {
                drop(phase);
                let _ = rec.stop();
                ducking::restore();
                if mode == Mode::Instruction {
                    let _ = state().app.emit_to(crate::editmenu::LABEL, "editmenu-voice", "");
                }
            }
            Phase::Processing { cancel } => {
                cancel.cancel();
                *phase = Phase::Processing { cancel };
                return;
            }
            Phase::Starting { .. } | Phase::Idle => {}
        }
        state().hotkeys.set_cancel_active(false);
        emit_status("idle", None);
        overlay::set(OverlayPhase::Idle, None, false, None);
    }
}

fn schedule_idle(ms: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        if matches!(overlay::current(), OverlayPhase::Warning | OverlayPhase::Error | OverlayPhase::Done) {
            overlay::set(OverlayPhase::Idle, None, false, None);
        }
    });
}

fn finish_with_error(s: &Settings, message: String) {
    ducking::restore();
    state().hotkeys.set_cancel_active(false);
    emit_status("idle", None);
    if s.sounds {
        sounds::play(Sound::Error, s.sound_volume);
    }
    overlay::set(OverlayPhase::Error, Some(message), false, None);
    schedule_idle(5000);
}

fn cancelled(cancel: &CancelToken) -> bool {
    cancel.is_cancelled()
}

/// Führt eine Future aus und bricht ab, sobald das Token ausgelöst wird.
fn block_on_cancellable<T>(
    fut: impl std::future::Future<Output = anyhow::Result<T>>,
    cancel: &CancelToken,
) -> anyhow::Result<T> {
    tauri::async_runtime::block_on(async {
        tokio::pin!(fut);
        loop {
            tokio::select! {
                res = &mut fut => return res,
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    if cancel.is_cancelled() {
                        anyhow::bail!("abgebrochen");
                    }
                }
            }
        }
    })
}

fn process(
    rec: MicRecording,
    started: Instant,
    mode: Mode,
    app_ctx: AppContext,
    cancel: &CancelToken,
) -> anyhow::Result<()> {
    let st = state();
    let s = st.settings();
    let samples = rec.stop()?;
    let duration_ms = started.elapsed().as_millis() as i64;
    let peak = samples.iter().fold(0.0f32, |m, v| m.max(v.abs()));

    // Kurze oder stille Aufnahmen verschwinden leise.
    if samples.len() < 16_000 * 3 / 10 || peak < 0.003 {
        log::info!("Aufnahme verworfen (Länge {} Samples, Spitze {peak:.4})", samples.len());
        if mode == Mode::Instruction {
            let _ = st.app.emit_to(crate::editmenu::LABEL, "editmenu-voice", "");
        }
        overlay::set(OverlayPhase::Idle, None, false, None);
        return Ok(());
    }

    let enhance = wants_enhance(&s, mode);
    if mode != Mode::Instruction {
        overlay::set(OverlayPhase::Transcribing, None, enhance, None);
    }
    emit_status("transcribing", Some(mode));

    let model_id = st.load_asr()?;
    let language = (s.language != "auto").then(|| s.language.clone());
    let result = st.asr.transcribe(
        &samples,
        &AsrOptions {
            language: language.clone(),
            vocabulary: &s.custom_words,
            cancel: Some(cancel),
        },
    );
    if cancelled(cancel) {
        overlay::set(OverlayPhase::Idle, None, false, None);
        return Ok(());
    }
    let result = result.map_err(|e| anyhow::anyhow!("{}: {e}", t(&s.ui_language, "transcribe_failed")))?;
    let raw = result.text.trim().to_string();
    let detected_lang = result.language.clone().or(language);
    let cleaned = text::apply_custom_words(&text::clean(&raw, s.remove_fillers), &s.custom_words);
    if cleaned.is_empty() {
        if mode == Mode::Instruction {
            let _ = st.app.emit_to(crate::editmenu::LABEL, "editmenu-voice", "");
        }
        overlay::set(OverlayPhase::Idle, None, false, None);
        return Ok(());
    }

    if mode == Mode::Instruction {
        let _ = st.app.emit_to(crate::editmenu::LABEL, "editmenu-voice", cleaned.clone());
        overlay::set(OverlayPhase::Idle, None, false, None);
        return Ok(());
    }

    let mut warning: Option<String> = None;
    let mut source = "dictation";
    let final_text = match text::expand_snippets(&cleaned, &s.snippets) {
        SnippetMatch::Full(snippet) => snippet,
        SnippetMatch::Inline(expanded) if enhance => match st.enhancer.ready(&s) {
            Ok(()) => {
                overlay::set(OverlayPhase::Enhancing, None, true, None);
                emit_status("enhancing", Some(mode));
                let (protected, parts) = text::protect_snippets(&cleaned, &s.snippets);
                let ctx = DictationContext {
                    app: &app_ctx,
                    language: detected_lang.as_deref(),
                    custom_words: &s.custom_words,
                };
                match block_on_cancellable(st.enhancer.enhance_dictation(&s, &protected, &ctx), cancel) {
                    Ok(out) => {
                        source = "enhanced";
                        text::restore_placeholders(&out, &parts)
                    }
                    Err(_) if cancelled(cancel) => {
                        overlay::set(OverlayPhase::Idle, None, false, None);
                        return Ok(());
                    }
                    Err(e) => {
                        log::warn!("Enhance fehlgeschlagen: {e:#}");
                        warning = Some(format!("{} ({e})", t(&s.ui_language, "enhance_failed")));
                        expanded
                    }
                }
            }
            Err(reason) => {
                log::warn!("Enhance nicht bereit: {reason}");
                warning = Some(t(&s.ui_language, "enhance_not_ready"));
                expanded
            }
        },
        SnippetMatch::Inline(expanded) => expanded,
    };
    if cancelled(cancel) {
        overlay::set(OverlayPhase::Idle, None, false, None);
        return Ok(());
    }

    let mut to_insert = final_text.clone();
    if s.trailing_space && !to_insert.ends_with(['\n', ' ', '\t']) {
        to_insert.push(' ');
    }
    // Ziel ist die App, die jetzt den Fokus hat (das Overlay nimmt ihn nie).
    let target = foreground::active_app();
    let auto_enter = s.auto_enter_apps.iter().any(|a| a.eq_ignore_ascii_case(&target.exe));
    insert::insert_text(
        &to_insert,
        &InsertOptions {
            method: s.paste_method,
            preserve_clipboard: s.preserve_clipboard,
            auto_enter,
        },
    )
    .map_err(|e| anyhow::anyhow!("{}: {e}", t(&s.ui_language, "insert_failed")))?;

    match st.history.add(
        NewEntry {
            text: final_text,
            raw_text: raw,
            source,
            app: if target.exe.is_empty() { app_ctx.display_name() } else { target.display_name() },
            language: detected_lang,
            duration_ms,
            model: model_id,
        },
        s.history_limit,
    ) {
        Ok(entry) => {
            let _ = st.app.emit("history-added", &entry);
            crate::tray::set_last(&entry.text);
        }
        Err(e) => log::error!("Verlauf konnte nicht gespeichert werden: {e:#}"),
    }

    if let Some(w) = warning {
        if s.sounds {
            sounds::play(Sound::Error, s.sound_volume);
        }
        overlay::set(OverlayPhase::Warning, Some(w), false, None);
        schedule_idle(3500);
    } else {
        if s.sounds {
            sounds::play(Sound::Done, s.sound_volume);
        }
        overlay::set(OverlayPhase::Done, None, enhance, None);
        schedule_idle(700);
    }
    Ok(())
}
