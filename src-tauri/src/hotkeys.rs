//! Globale Shortcuts über handy-keys (Low-Level-Hook, registrierte Kombis werden
//! für andere Apps blockiert). Unterstützt Toggle, Push-to-talk und Hybrid.

use crate::dictation::Mode;
use crate::settings::Activation;
use crate::state::state;
use handy_keys::{Hotkey, HotkeyId, HotkeyManager, HotkeyState, Key, Modifiers};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

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
    cancel_active: AtomicBool,
    pressed_at: Mutex<HashMap<HotkeyId, Instant>>,
    suspended: AtomicBool,
    capture_session: Mutex<Option<String>>,
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
        for (id, action) in ids.iter() {
            if *action != Action::Cancel {
                let _ = manager.unregister(*id);
            }
        }
        // Die separate Escape-Registrierung einer laufenden Aufnahme behalten.
        ids.retain(|_, action| *action == Action::Cancel);
        // Während der Auswahl dürfen registrierte Kombis keine Eingabe blockieren.
        if self.suspended.load(Ordering::SeqCst) {
            if let Some(id) = self.cancel_id.lock().unwrap().take() {
                let _ = manager.unregister(id);
                ids.remove(&id);
            }
            return errors;
        }
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
        // Falls während einer laufenden Aufnahme ausgewählt wurde, Escape
        // nach dem Ende der Auswahl wiederherstellen.
        if self.cancel_active.load(Ordering::SeqCst) {
            let mut cancel = self.cancel_id.lock().unwrap();
            if cancel.is_none() {
                if let Ok(hk) = Hotkey::new(Modifiers::empty(), Key::Escape) {
                    if let Ok(id) = manager.register(hk) {
                        *cancel = Some(id);
                        ids.insert(id, Action::Cancel);
                    }
                }
            }
        }
        errors
    }

    /// Escape zum Abbrechen nur registrieren, solange etwas läuft – sonst
    /// würde Escape in allen Apps blockiert.
    pub fn set_cancel_active(&self, active: bool) {
        self.cancel_active.store(active, Ordering::SeqCst);
        let active = active && !self.suspended.load(Ordering::SeqCst);
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

    /// Die Oberfläche nimmt ihre eigenen Tastaturereignisse auf. Hier nur die
    /// globalen Aktionen pausieren, damit sie die Tasten nicht verschlucken.
    /// Kein zweiter, kurzlebiger Windows-Hook und keine Event-Listener-Rennen.
    pub fn start_capture(&'static self, session: String) {
        {
            let mut active = self.capture_session.lock().unwrap();
            *active = Some(session.clone());
            self.suspended.store(true, Ordering::SeqCst);
            self.pressed_at.lock().unwrap().clear();
            self.apply();
        }
        // Failsafe, falls die WebView während der Aufnahme beendet/neu geladen
        // wird und ihr Cleanup nicht mehr beim Backend ankommt.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(30));
            self.stop_capture(&session);
        });
    }

    pub fn stop_capture(&self, session: &str) {
        let mut active = self.capture_session.lock().unwrap();
        // Ein verspätetes Cleanup des vorigen Feldes darf das neue nicht beenden.
        if active.as_deref() == Some(session) {
            *active = None;
            self.suspended.store(false, Ordering::SeqCst);
            self.apply();
        }
    }
}
