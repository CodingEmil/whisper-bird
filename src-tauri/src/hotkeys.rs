//! Globale Shortcuts über handy-keys (Low-Level-Hook, registrierte Kombis werden
//! für andere Apps blockiert). Unterstützt Toggle, Push-to-talk und Hybrid.

use crate::dictation::Mode;
use crate::settings::Activation;
use crate::state::state;
use handy_keys::{Hotkey, HotkeyId, HotkeyManager, HotkeyState, Key, KeyboardListener, Modifiers};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Dictate,
    DictateEnhance,
    Edit,
    Meeting,
    Cancel,
}

/// Ab dieser Haltedauer gilt ein Druck im Hybrid-Modus als Push-to-talk.
const HOLD_THRESHOLD: Duration = Duration::from_millis(350);

#[derive(Default)]
pub struct Hotkeys {
    manager: Mutex<Option<HotkeyManager>>,
    ids: Mutex<HashMap<HotkeyId, Action>>,
    cancel_id: Mutex<Option<HotkeyId>>,
    pressed_at: Mutex<HashMap<HotkeyId, Instant>>,
    suspended: AtomicBool,
    capture_stop: Mutex<Option<Arc<AtomicBool>>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ShortcutError {
    pub field: String,
    pub message: String,
}

impl Hotkeys {
    pub fn init(&'static self) -> anyhow::Result<()> {
        let manager = HotkeyManager::new_with_blocking().or_else(|e| {
            log::warn!("Blockierende Hotkeys nicht verfügbar ({e}), nutze nicht-blockierend");
            HotkeyManager::new()
        })?;
        *self.manager.lock().unwrap() = Some(manager);
        std::thread::Builder::new()
            .name("hotkeys".into())
            .spawn(move || loop {
                let event = {
                    let guard = self.manager.lock().unwrap();
                    guard.as_ref().and_then(|m| m.try_recv())
                };
                match event {
                    Some(ev) => self.dispatch(ev.id, ev.state),
                    None => std::thread::sleep(Duration::from_millis(8)),
                }
            })?;
        Ok(())
    }

    /// Registriert alle Shortcuts aus den Einstellungen neu.
    pub fn apply(&self) -> Vec<ShortcutError> {
        let s = state().settings();
        let mut errors = Vec::new();
        let guard = self.manager.lock().unwrap();
        let Some(manager) = guard.as_ref() else {
            return errors;
        };
        let mut ids = self.ids.lock().unwrap();
        for id in ids.keys() {
            let _ = manager.unregister(*id);
        }
        ids.clear();
        let entries = [
            ("shortcut_dictate", &s.shortcut_dictate, Action::Dictate),
            ("shortcut_dictate_enhance", &s.shortcut_dictate_enhance, Action::DictateEnhance),
            ("shortcut_edit", &s.shortcut_edit, Action::Edit),
            ("shortcut_meeting", &s.shortcut_meeting, Action::Meeting),
        ];
        for (field, text, action) in entries {
            if text.trim().is_empty() {
                continue;
            }
            match text.parse::<Hotkey>().map_err(|e| e.to_string()).and_then(|hk| {
                manager.register(hk).map_err(|e| e.to_string())
            }) {
                Ok(id) => {
                    ids.insert(id, action);
                }
                Err(message) => {
                    log::warn!("Shortcut {field} = {text:?} ungültig: {message}");
                    errors.push(ShortcutError {
                        field: field.into(),
                        message,
                    });
                }
            }
        }
        errors
    }

    /// Escape zum Abbrechen nur registrieren, solange etwas läuft – sonst
    /// würde Escape in allen Apps blockiert.
    pub fn set_cancel_active(&self, active: bool) {
        let guard = self.manager.lock().unwrap();
        let Some(manager) = guard.as_ref() else { return };
        let mut cancel = self.cancel_id.lock().unwrap();
        match (active, *cancel) {
            (true, None) => {
                if let Ok(hk) = Hotkey::new(Modifiers::empty(), Key::Escape) {
                    if let Ok(id) = manager.register(hk) {
                        *cancel = Some(id);
                        self.ids.lock().unwrap().insert(id, Action::Cancel);
                    }
                }
            }
            (false, Some(id)) => {
                let _ = manager.unregister(id);
                self.ids.lock().unwrap().remove(&id);
                *cancel = None;
            }
            _ => {}
        }
    }

    fn dispatch(&self, id: HotkeyId, hk_state: HotkeyState) {
        if self.suspended.load(Ordering::SeqCst) {
            return;
        }
        let Some(action) = self.ids.lock().unwrap().get(&id).copied() else { return };
        let st = state();
        let pressed = matches!(hk_state, HotkeyState::Pressed);
        match action {
            Action::Cancel => {
                if pressed {
                    st.dictation.cancel();
                }
            }
            // Eigene Threads: Markierung auslesen bzw. Meeting starten dauert,
            // und der Hotkey-Thread soll sofort wieder Tasten annehmen.
            Action::Edit => {
                if pressed {
                    std::thread::spawn(crate::editmenu::open);
                }
            }
            Action::Meeting => {
                if pressed {
                    std::thread::spawn(crate::meeting::toggle);
                }
            }
            Action::Dictate | Action::DictateEnhance => {
                let mode = if action == Action::DictateEnhance { Mode::Enhance } else { Mode::Plain };
                let activation = st.settings().activation;
                let recording = st.dictation.is_recording();
                match (activation, pressed) {
                    (Activation::Toggle, true) => st.dictation.toggle(mode),
                    (Activation::PushToTalk, true) => {
                        if !recording {
                            st.dictation.start(mode);
                        }
                    }
                    (Activation::PushToTalk, false) => st.dictation.stop(),
                    (Activation::Hybrid, true) => {
                        self.pressed_at.lock().unwrap().insert(id, Instant::now());
                        st.dictation.toggle(mode);
                    }
                    (Activation::Hybrid, false) => {
                        let held = self
                            .pressed_at
                            .lock()
                            .unwrap()
                            .remove(&id)
                            .map(|t| t.elapsed())
                            .unwrap_or_default();
                        if held >= HOLD_THRESHOLD && st.dictation.is_recording() {
                            st.dictation.stop();
                        }
                    }
                    (Activation::Toggle, false) => {}
                }
            }
        }
    }

    /// Nimmt eine neue Tastenkombination auf (für die Einstellungen).
    /// Sendet "shortcut-capture" mit { display, done, cancelled }.
    pub fn start_capture(&'static self) {
        self.stop_capture();
        self.suspended.store(true, Ordering::SeqCst);
        let stop = Arc::new(AtomicBool::new(false));
        *self.capture_stop.lock().unwrap() = Some(stop.clone());
        std::thread::spawn(move || {
            let result = capture_loop(&stop);
            self.suspended.store(false, Ordering::SeqCst);
            let app = &state().app;
            match result {
                Ok(Some(hk)) => {
                    let _ = app.emit("shortcut-capture", CaptureEvent::done(hk.to_string()));
                }
                Ok(None) => {
                    let _ = app.emit("shortcut-capture", CaptureEvent::cancelled());
                }
                Err(e) => {
                    log::error!("Shortcut-Aufnahme fehlgeschlagen: {e}");
                    let _ = app.emit("shortcut-capture", CaptureEvent::cancelled());
                }
            }
        });
    }

    pub fn stop_capture(&self) {
        if let Some(stop) = self.capture_stop.lock().unwrap().take() {
            stop.store(true, Ordering::SeqCst);
        }
    }
}

#[derive(Clone, Serialize)]
struct CaptureEvent {
    display: String,
    done: bool,
    cancelled: bool,
}

impl CaptureEvent {
    fn live(display: String) -> Self {
        Self { display, done: false, cancelled: false }
    }
    fn done(display: String) -> Self {
        Self { display, done: true, cancelled: false }
    }
    fn cancelled() -> Self {
        Self { display: String::new(), done: false, cancelled: true }
    }
}

/// Liest Tastendrücke, bis eine Kombination steht:
/// - Modifier + Taste → sofort fertig
/// - nur Modifier (z. B. Strg rechts) → fertig beim Loslassen
/// - Escape allein → abbrechen
fn capture_loop(stop: &AtomicBool) -> handy_keys::Result<Option<Hotkey>> {
    let listener = KeyboardListener::new()?;
    let app = &state().app;
    let started = Instant::now();
    let mut peak = Modifiers::empty();
    while !stop.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(20) {
        let Some(ev) = listener.recv_timeout(Duration::from_millis(50)).ok() else {
            continue;
        };
        if ev.is_key_down {
            if ev.key == Some(Key::Escape) && ev.modifiers.is_empty() {
                return Ok(None);
            }
            if let Some(key) = ev.key {
                return Ok(Some(Hotkey::new(ev.modifiers, key)?));
            }
            peak |= ev.modifiers;
            if let Ok(hk) = Hotkey::new(peak, None) {
                let _ = app.emit("shortcut-capture", CaptureEvent::live(hk.to_string()));
            }
        } else if ev.modifiers.is_empty() && !peak.is_empty() {
            return Ok(Some(Hotkey::new(peak, None)?));
        }
    }
    Ok(None)
}
