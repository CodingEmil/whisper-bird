//! Tauri-Befehle für die Oberfläche.

use crate::catalog::{self, ModelInfo, ModelKind};
use crate::dictation::Mode;
use crate::downloads::DownloadRequest;
use crate::enhance::llama;
use crate::history::{Entry, Query, Stats};
use crate::hotkeys::ShortcutError;
use crate::meeting::{Meeting, MeetingSummary};
use crate::settings::{EditAction, Settings};
use crate::state::state;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

type CmdResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ---------- Einstellungen ----------

#[tauri::command]
pub fn get_settings() -> Settings {
    state().settings()
}

#[derive(Serialize)]
pub struct SettingsResult {
    settings: Settings,
    shortcut_errors: Vec<ShortcutError>,
}

/// Übernimmt die übergebenen Felder (JSON-Patch) und wendet Nebenwirkungen an.
#[tauri::command]
pub fn set_settings(app: AppHandle, patch: serde_json::Value) -> CmdResult<SettingsResult> {
    let st = state();
    let old = st.settings();
    let mut value = serde_json::to_value(&old).map_err(err)?;
    if let (Some(target), Some(patch)) = (value.as_object_mut(), patch.as_object()) {
        for (k, v) in patch {
            target.insert(k.clone(), v.clone());
        }
    }
    let new: Settings = serde_json::from_value(value).map_err(err)?;
    let new2 = new.clone();
    let settings = st.update_settings(move |s| *s = new2);

    let mut shortcut_errors = Vec::new();
    if old.shortcut_dictate != new.shortcut_dictate
        || old.shortcut_dictate_enhance != new.shortcut_dictate_enhance
        || old.shortcut_edit != new.shortcut_edit
        || old.shortcut_meeting != new.shortcut_meeting
    {
        shortcut_errors = st.hotkeys.apply();
    }
    if old.asr_model != new.asr_model || old.accelerator != new.accelerator {
        st.asr.unload();
        std::thread::spawn(|| {
            if let Err(e) = state().load_asr() {
                log::warn!("Modell konnte nicht geladen werden: {e:#}");
            }
        });
    }
    if old.enhance_local_model != new.enhance_local_model || old.enhance_provider != new.enhance_provider {
        st.enhancer.llama.stop();
    }
    if old.autostart != new.autostart {
        use tauri_plugin_autostart::ManagerExt;
        let mgr = app.autolaunch();
        let r = if new.autostart { mgr.enable() } else { mgr.disable() };
        if let Err(e) = r {
            log::warn!("Autostart konnte nicht geändert werden: {e}");
        }
    }
    if old.debug_logging != new.debug_logging {
        log::set_max_level(if new.debug_logging { log::LevelFilter::Debug } else { log::LevelFilter::Info });
    }
    if old.show_floating_icon != new.show_floating_icon || old.overlay_position != new.overlay_position {
        crate::overlay::refresh();
    }
    crate::tray::rebuild();
    Ok(SettingsResult {
        settings,
        shortcut_errors,
    })
}

#[tauri::command]
pub fn default_system_prompt() -> String {
    crate::enhance::DEFAULT_SYSTEM_PROMPT.to_string()
}

#[tauri::command]
pub fn default_edit_actions() -> Vec<EditAction> {
    crate::settings::default_edit_actions(&state().settings().ui_language)
}

#[derive(Serialize)]
pub struct AppInfo {
    version: String,
    data_dir: String,
    models_dir: String,
    log_dir: String,
    devices: Vec<String>,
    asr_loaded: Option<String>,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    let st = state();
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        data_dir: st.dirs.data.display().to_string(),
        models_dir: st.dirs.models.display().to_string(),
        log_dir: st.dirs.logs.display().to_string(),
        devices: transcribe_cpp::devices()
            .into_iter()
            .map(|d| format!("{} ({})", d.description, d.kind))
            .collect(),
        asr_loaded: st.asr.loaded_id(),
    }
}

// ---------- Modelle ----------

#[derive(Serialize)]
pub struct ModelStatus {
    #[serde(flatten)]
    info: ModelInfo,
    installed: bool,
    downloading: bool,
}

#[tauri::command]
pub fn list_models() -> Vec<ModelStatus> {
    let st = state();
    catalog::MODELS
        .iter()
        .map(|m| ModelStatus {
            info: m.clone(),
            installed: st.model_installed(m.id),
            downloading: st.downloads.is_active(m.id),
        })
        .collect()
}

fn start_model_download(id: &str) -> CmdResult<()> {
    let st = state();
    let info = catalog::find(id).ok_or("Unbekanntes Modell")?;
    let dest = st.dirs.models.join(info.filename);
    let kind = info.kind;
    st.downloads.start(
        st.app.clone(),
        DownloadRequest {
            id: info.id.to_string(),
            url: info.url(),
            dest,
            size: info.size,
            sha256: Some(info.sha256.to_string()),
        },
        move |_app| {
            let st = state();
            if kind == ModelKind::Asr {
                // VAD gehört dazu (1 MB) – für Dateien und Meetings.
                if !st.model_installed(catalog::VAD_MODEL) {
                    let _ = start_model_download(catalog::VAD_MODEL);
                }
                std::thread::spawn(|| {
                    if let Err(e) = state().load_asr() {
                        log::warn!("Modell konnte nicht geladen werden: {e:#}");
                    }
                });
            }
        },
    );
    Ok(())
}

#[tauri::command]
pub fn download_model(id: String) -> CmdResult<()> {
    start_model_download(&id)
}

#[tauri::command]
pub fn cancel_download(id: String) {
    let st = state();
    st.downloads.cancel(&id);
    if let Some(info) = catalog::find(&id) {
        let dest = st.dirs.models.join(info.filename);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(500));
            crate::downloads::remove_partial(&dest);
        });
    }
}

#[tauri::command]
pub fn delete_model(id: String) -> CmdResult<()> {
    let st = state();
    let info = catalog::find(&id).ok_or("Unbekanntes Modell")?;
    if st.asr.loaded_id().as_deref() == Some(info.id) {
        st.asr.unload();
    }
    if info.kind == ModelKind::Llm {
        st.enhancer.llama.stop();
    }
    std::fs::remove_file(st.dirs.models.join(info.filename)).map_err(err)
}

#[derive(Serialize)]
pub struct LlamaStatus {
    installed: bool,
    running: bool,
    downloading: bool,
    size: u64,
}

#[tauri::command]
pub fn llama_status() -> LlamaStatus {
    let st = state();
    LlamaStatus {
        installed: st.enhancer.llama.is_installed(),
        running: st.enhancer.llama.is_running(),
        downloading: st.downloads.is_active("llama-server"),
        size: llama::LLAMA_ZIP_SIZE,
    }
}

#[tauri::command]
pub fn install_llama() -> CmdResult<()> {
    let st = state();
    let zip = st.enhancer.llama.bin_dir().join("llama-server.zip");
    st.downloads.start(
        st.app.clone(),
        DownloadRequest {
            id: "llama-server".into(),
            url: llama::zip_url(),
            dest: zip.clone(),
            size: llama::LLAMA_ZIP_SIZE,
            sha256: Some(llama::LLAMA_ZIP_SHA256.into()),
        },
        move |app| {
            let result = state().enhancer.llama.install_from_zip(&zip);
            if let Err(e) = &result {
                log::error!("llama-server konnte nicht entpackt werden: {e:#}");
            }
            let _ = app.emit("llama-installed", result.map_err(|e| e.to_string()));
        },
    );
    Ok(())
}

// ---------- Audio ----------

#[tauri::command]
pub fn list_microphones() -> Vec<crate::audio::mic::MicDevice> {
    crate::audio::mic::list_devices()
}

#[tauri::command]
pub fn play_test_sound() {
    let s = state().settings();
    crate::audio::sounds::play(crate::audio::sounds::Sound::Start, s.sound_volume);
}

// ---------- Shortcuts ----------

#[tauri::command]
pub fn start_shortcut_capture(session: String) {
    state().hotkeys.start_capture(session);
}

#[tauri::command]
pub fn stop_shortcut_capture(session: String) {
    state().hotkeys.stop_capture(&session);
}

// ---------- Diktat & Overlay ----------

#[tauri::command]
pub fn dictation_toggle(enhance: bool) {
    let st = state();
    st.dictation.toggle(if enhance { Mode::Enhance } else { Mode::Plain });
}

#[tauri::command]
pub fn dictation_cancel() {
    state().dictation.cancel();
}

#[tauri::command]
pub fn overlay_context_menu() {
    crate::tray::popup_context_menu();
}

#[tauri::command]
pub fn show_settings(page: Option<String>) {
    crate::show_main_window(page.as_deref());
}

// ---------- Verlauf & Statistik ----------

#[tauri::command]
pub fn history_query(query: Query) -> CmdResult<Vec<Entry>> {
    state().history.query(&query).map_err(err)
}

#[tauri::command]
pub fn history_delete(id: i64) -> CmdResult<()> {
    state().history.delete(id).map_err(err)
}

#[tauri::command]
pub fn history_clear() -> CmdResult<()> {
    state().history.clear().map_err(err)
}

#[tauri::command]
pub fn stats() -> CmdResult<Stats> {
    state().history.stats().map_err(err)
}

#[tauri::command]
pub fn reset_stats() -> CmdResult<()> {
    state().history.reset_stats().map_err(err)
}

#[tauri::command]
pub fn copy_text(text: String) -> CmdResult<()> {
    crate::win::clipboard::set_text(&text, false).map_err(err)
}

// ---------- Enhance ----------

#[tauri::command]
pub async fn enhance_test() -> CmdResult<String> {
    let st = state();
    let s = st.settings();
    st.enhancer.ready(&s)?;
    let sample = if s.ui_language == "de" {
        "äh also ich wollte fragen ob wir uns morgen um drei nein um vier treffen können und ähm ob du die unterlagen mitbringst"
    } else {
        "uh so i wanted to ask if we can meet tomorrow at three no at four and um if you could bring the documents"
    };
    let app = crate::win::foreground::AppContext {
        exe: "outlook.exe".into(),
        title: "Outlook".into(),
        hwnd: 0,
    };
    let ctx = crate::enhance::DictationContext {
        app: &app,
        language: Some(if s.ui_language == "de" { "de" } else { "en" }),
        custom_words: &s.custom_words,
    };
    let started = std::time::Instant::now();
    let out = st
        .enhancer
        .enhance_dictation(&s, sample, &ctx)
        .await
        .map_err(|e| format!("{e:#}"))?;
    Ok(format!("{out}\n\n({:.1} s)", started.elapsed().as_secs_f32()))
}

#[tauri::command]
pub fn enhance_stop_local() {
    state().enhancer.llama.stop();
}

// ---------- Kontextmenü „Text bearbeiten“ ----------

#[tauri::command]
pub async fn editmenu_run(instruction: String) -> CmdResult<String> {
    crate::editmenu::run(instruction).await
}

#[tauri::command]
pub fn editmenu_apply(text: String) -> CmdResult<()> {
    crate::editmenu::apply(text)
}

#[tauri::command]
pub fn editmenu_copy(text: String) -> CmdResult<()> {
    crate::win::clipboard::set_text(&text, false).map_err(err)?;
    crate::editmenu::close(true);
    Ok(())
}

#[tauri::command]
pub fn editmenu_close() {
    crate::editmenu::close(true);
}

#[tauri::command]
pub fn editmenu_voice(start: bool) {
    let st = state();
    if start {
        st.dictation.start(Mode::Instruction);
    } else {
        st.dictation.stop();
    }
}

// ---------- Dateien ----------

#[tauri::command]
pub fn files_pick(app: AppHandle) {
    crate::tray::pick_files(app);
}

#[tauri::command]
pub fn files_pick_folder(app: AppHandle) {
    use tauri_plugin_dialog::DialogExt;
    std::thread::spawn(move || {
        if let Some(folder) = app.dialog().file().blocking_pick_folder() {
            if let Ok(path) = folder.into_path() {
                crate::files::enqueue(vec![path]);
            }
        }
    });
}

#[tauri::command]
pub fn files_add(paths: Vec<String>) {
    let paths = paths.into_iter().map(PathBuf::from).collect();
    std::thread::spawn(move || crate::files::enqueue(paths));
}

#[tauri::command]
pub fn files_list() -> Vec<crate::files::FileJob> {
    state().files.list()
}

#[tauri::command]
pub fn files_cancel(id: u64) {
    state().files.cancel(id);
}

#[tauri::command]
pub fn files_clear() {
    state().files.clear_finished();
}

// ---------- Meetings ----------

#[tauri::command]
pub fn meeting_start() -> CmdResult<String> {
    crate::meeting::start()
}

#[tauri::command]
pub fn meeting_stop() {
    crate::meeting::stop();
}

#[tauri::command]
pub fn meeting_active() -> Option<String> {
    state().meeting.active_id()
}

#[tauri::command]
pub fn meeting_list() -> Vec<MeetingSummary> {
    crate::meeting::list()
}

#[tauri::command]
pub fn meeting_get(id: String) -> CmdResult<Meeting> {
    crate::meeting::load(&id).map_err(err)
}

#[tauri::command]
pub fn meeting_delete(id: String) -> CmdResult<()> {
    crate::meeting::delete(&id).map_err(err)
}

#[tauri::command]
pub fn meeting_update(id: String, title: Option<String>, speakers: Option<Vec<String>>) -> CmdResult<Meeting> {
    let mut m = crate::meeting::load(&id).map_err(err)?;
    if let Some(t) = title {
        m.title = t;
    }
    if let Some(sp) = speakers {
        m.speakers = sp;
    }
    crate::meeting::save(&m).map_err(err)?;
    Ok(m)
}

#[tauri::command]
pub async fn meeting_notes(id: String) -> CmdResult<Meeting> {
    crate::meeting::generate_notes(id).await
}

#[tauri::command]
pub fn meeting_transcript(id: String) -> CmdResult<String> {
    let m = crate::meeting::load(&id).map_err(err)?;
    Ok(crate::meeting::transcript_text(&m))
}

#[tauri::command]
pub fn save_text_file(path: String, content: String) -> CmdResult<()> {
    std::fs::write(path, content).map_err(err)
}

// ---------- System ----------

#[tauri::command]
pub fn finish_onboarding() {
    state().update_settings(|s| s.onboarding_done = true);
}

#[tauri::command]
pub fn open_data_dir(app: AppHandle, which: String) -> CmdResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let st = state();
    let dir = match which.as_str() {
        "logs" => &st.dirs.logs,
        "models" => &st.dirs.models,
        "meetings" => &st.dirs.meetings,
        _ => &st.dirs.data,
    };
    app.opener().open_path(dir.display().to_string(), None::<&str>).map_err(err)
}
