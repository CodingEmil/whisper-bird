//! Globaler App-Zustand.

use crate::asr::Asr;
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

    /// Gewähltes Spracherkennungsmodell – oder ein anderes installiertes als Ersatz.
    pub fn usable_asr_model(&self) -> Option<(String, PathBuf)> {
        let wanted = self.settings().asr_model;
        if let Some(p) = self.model_file(&wanted).filter(|p| p.exists()) {
            return Some((wanted, p));
        }
        catalog::MODELS
            .iter()
            .filter(|m| m.kind == ModelKind::Asr)
            .find_map(|m| {
                let p = self.dirs.models.join(m.filename);
                p.exists().then(|| (m.id.to_string(), p))
            })
    }

    /// Lädt das Spracherkennungsmodell (blockierend).
    pub fn load_asr(&self) -> anyhow::Result<String> {
        let (id, path) = self
            .usable_asr_model()
            .ok_or_else(|| anyhow::anyhow!(crate::i18n::t(&self.settings().ui_language, "no_model")))?;
        self.asr.ensure_loaded(&id, &path, self.settings().accelerator)?;
        Ok(id)
    }

    pub fn load_vad(&self) -> anyhow::Result<()> {
        let path = self
            .model_file(catalog::VAD_MODEL)
            .filter(|p| p.exists())
            .ok_or_else(|| anyhow::anyhow!("VAD-Modell fehlt"))?;
        self.asr.ensure_vad(&path)
    }
}
