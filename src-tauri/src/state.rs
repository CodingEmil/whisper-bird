//! Globaler App-Zustand.

use crate::asr::{Asr, AsrModel, Remote};
use crate::catalog::{self, ModelKind};
use crate::downloads::Downloads;
use crate::enhance::Enhancer;
use crate::history::History;
use crate::settings::{self, Settings};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};
use tauri::{AppHandle, Emitter};

pub struct Dirs {
    pub data: PathBuf,
    pub models: PathBuf,
    pub llama: PathBuf,
    pub meetings: PathBuf,
    pub logs: PathBuf,
}

pub struct AppState {
    pub app: AppHandle,
    pub dirs: Dirs,
    settings: RwLock<Settings>,
    pub asr: Asr,
    pub history: History,
    pub downloads: Arc<Downloads>,
    pub enhancer: Enhancer,
    pub dictation: crate::dictation::Dictation,
    pub hotkeys: crate::hotkeys::Hotkeys,
    pub meeting: crate::meeting::Meetings,
    pub files: crate::files::FileJobs,
    pub editmenu: crate::editmenu::EditMenu,
    pub updates: crate::updates::Updates,
}

static STATE: OnceLock<Arc<AppState>> = OnceLock::new();

pub fn state() -> &'static Arc<AppState> {
    STATE.get().expect("AppState nicht initialisiert")
}

pub fn init(app: AppHandle, dirs: Dirs) -> anyhow::Result<&'static Arc<AppState>> {
    for d in [&dirs.data, &dirs.models, &dirs.llama, &dirs.meetings, &dirs.logs] {
        std::fs::create_dir_all(d)?;
    }
    let settings = settings::load(&dirs.data.join("settings.json"));
    let history = History::open(&dirs.data.join("history.db"))?;
    let enhancer = Enhancer::new(dirs.llama.clone(), dirs.models.clone());
    let updates = crate::updates::Updates::new(app.package_info().version.clone());
    let state = Arc::new(AppState {
        app,
        settings: RwLock::new(settings),
        asr: Asr::default(),
        history,
        downloads: Arc::new(Downloads::default()),
        enhancer,
        dictation: Default::default(),
        hotkeys: Default::default(),
        meeting: Default::default(),
        files: Default::default(),
        editmenu: Default::default(),
        updates,
        dirs,
    });
    let _ = STATE.set(state);
    Ok(STATE.get().unwrap())
}

impl AppState {
    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    /// Ändert Einstellungen, speichert sie und informiert alle Fenster.
    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> Settings {
        let snapshot = {
            let mut s = self.settings.write().unwrap();
            f(&mut s);
            s.clone()
        };
        if let Err(e) = settings::save(&self.dirs.data.join("settings.json"), &snapshot) {
            log::error!("Einstellungen konnten nicht gespeichert werden: {e:#}");
        }
        let _ = self.app.emit("settings-changed", &snapshot);
        snapshot
    }

    pub fn model_file(&self, id: &str) -> Option<PathBuf> {
        catalog::find(id).map(|m| self.dirs.models.join(m.filename))
    }

    pub fn model_installed(&self, id: &str) -> bool {
        self.model_file(id).map(|p| p.exists()).unwrap_or(false)
    }

    /// Spracherkennung mit dieser ID, falls installiert bzw. (remote) eingerichtet.
    fn asr_model(&self, s: &Settings, id: &str) -> Option<AsrModel> {
        if id == settings::REMOTE_ASR_MODEL {
            return (!s.asr_remote_url.is_empty()).then(|| AsrModel {
                id: id.into(),
                path: PathBuf::new(),
                accel: s.accelerator,
                remote: Some(Remote {
                    base_url: s.asr_remote_url.clone(),
                    api_key: s.asr_remote_key.clone(),
                    model: s.asr_remote_model.clone(),
                }),
            });
        }
        let path = self.model_file(id).filter(|p| p.exists())?;
        Some(AsrModel { id: id.into(), path, accel: s.accelerator, remote: None })
    }

    /// Gewähltes Spracherkennungsmodell – oder ein anderes installiertes als Ersatz.
    pub fn usable_asr_model(&self) -> Option<AsrModel> {
        let s = self.settings();
        self.asr_model(&s, &s.asr_model).or_else(|| {
            catalog::MODELS
                .iter()
                .filter(|m| m.kind == ModelKind::Asr)
                .find_map(|m| self.asr_model(&s, m.id))
        })
    }

    /// Lädt das Spracherkennungsmodell (blockierend). Liefert den Namen für den Verlauf.
    pub fn load_asr(&self) -> anyhow::Result<String> {
        let m = self
            .usable_asr_model()
            .ok_or_else(|| anyhow::anyhow!(crate::i18n::t(&self.settings().ui_language, "no_model")))?;
        self.asr.ensure_loaded(&m)?;
        Ok(m.label())
    }

    /// Modell für Meetings und Dateien: das eigens gewählte, sonst wie beim Diktat.
    pub fn long_asr_model(&self) -> anyhow::Result<AsrModel> {
        let s = self.settings();
        self.asr_model(&s, &s.meeting_asr_model)
            .or_else(|| self.usable_asr_model())
            .ok_or_else(|| anyhow::anyhow!(crate::i18n::t(&s.ui_language, "no_model")))
    }

    /// Nach Meeting oder Datei: Das Diktatmodell zurückholen, damit das nächste Diktat schnell startet.
    pub fn restore_dictation_model(&self) {
        if self.meeting.is_active() || self.files.is_running() {
            return;
        }
        if let Err(e) = self.load_asr() {
            log::warn!("Modell konnte nicht geladen werden: {e:#}");
        }
    }

    pub fn load_vad(&self) -> anyhow::Result<()> {
        let path = self
            .model_file(catalog::VAD_MODEL)
            .filter(|p| p.exists())
            .ok_or_else(|| anyhow::anyhow!("VAD-Modell fehlt"))?;
        self.asr.ensure_vad(&path)
    }
}
