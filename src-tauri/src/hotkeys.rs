//! Globale Shortcuts über handy-keys (Low-Level-Hook, registrierte Kombis werden
//! für andere Apps blockiert). Unterstützt Toggle, Push-to-talk und Hybrid.

use crate::dictation::Mode;
use crate::settings::Activation;
use crate::state::state;
use handy_keys::{Hotkey, HotkeyId, HotkeyManager, HotkeyState, Key, Modifiers};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
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
    ids: Mutex<HashMap<HotkeyId, (Action, Hotkey)>>,
    cancel_id: Mutex<Option<HotkeyId>>,
    cancel_active: AtomicBool,
    pressed_at: Mutex<HashMap<HotkeyId, Instant>>,
    suspended: AtomicBool,
    capture_session: Mutex<Option<String>>,
    /// Im eigenen Fenster gedrückte Shortcuts (siehe `window_key`).
    window_pressed: Mutex<HashSet<HotkeyId>>,
}

/// Welche Shortcuts ein Tastenereignis drückt oder loslässt, nach denselben
/// Regeln wie der Hotkey-Manager von handy-keys.
fn transitions<I: Copy + Eq + Hash>(
    hotkeys: impl IntoIterator<Item = (I, Hotkey)>,
    pressed: &mut HashSet<I>,
    modifiers: Modifiers,
    key: Option<Key>,
    down: bool,
) -> Vec<(I, HotkeyState)> {
    hotkeys
        .into_iter()
        .filter_map(|(id, hk)| {
            if down {
                let hit = hk.modifiers.matches(modifiers) && hk.key == key;
                (hit && pressed.insert(id)).then_some((id, HotkeyState::Pressed))
            } else {
                let gone = match key {
                    Some(_) => hk.key == key,
                    None => !hk.modifiers.matches(modifiers),
                };
                (gone && pressed.remove(&id)).then_some((id, HotkeyState::Released))
            }
        })
        .collect()
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
        for (id, (action, _)) in ids.iter() {
            if *action != Action::Cancel {
                let _ = manager.unregister(*id);
            }
        }
        // Die separate Escape-Registrierung einer laufenden Aufnahme behalten.
        ids.retain(|_, (action, _)| *action == Action::Cancel);
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
                manager.register(hk).map(|id| (id, hk)).map_err(|e| e.to_string())
            }) {
                Ok((id, hk)) => {
                    ids.insert(id, (action, hk));
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
                        ids.insert(id, (Action::Cancel, hk));
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
                        self.ids.lock().unwrap().insert(id, (Action::Cancel, hk));
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
        let Some(action) = self.ids.lock().unwrap().get(&id).map(|(action, _)| *action) else { return };
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

    /// Tasten aus dem eigenen Fenster: Hat dessen WebView den Fokus, ruft
    /// Windows den Low-Level-Hook dieses Prozesses nicht auf. Die Oberfläche
    /// meldet ihre Tasten deshalb selbst. Dort, wo der Hook greift, blockiert er
    /// die Kombi, bevor die WebView sie sieht, also löst nichts doppelt aus.
    pub fn window_key(&self, modifiers: Modifiers, key: Option<Key>, down: bool) {
        if self.suspended.load(Ordering::SeqCst) {
            return;
        }
        let events = {
            let ids = self.ids.lock().unwrap();
            let hotkeys = ids.iter().map(|(id, (_, hk))| (*id, *hk));
            transitions(hotkeys, &mut self.window_pressed.lock().unwrap(), modifiers, key, down)
        };
        for (id, hk_state) in events {
            self.dispatch(id, hk_state);
        }
    }

    /// Das Fenster hat den Fokus verloren, bevor die Tasten losgelassen wurden.
    pub fn window_blur(&self) {
        let held: Vec<HotkeyId> = self.window_pressed.lock().unwrap().drain().collect();
        for id in held {
            self.dispatch(id, HotkeyState::Released);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_keys_match_like_the_hook() {
        use HotkeyState::{Pressed, Released};
        let hotkeys: Vec<(u32, Hotkey)> = [(1, "Ctrl+Shift+Space"), (2, "CtrlRight"), (3, "CtrlLeft+CmdLeft")]
            .into_iter()
            .map(|(id, text)| (id, text.parse().unwrap()))
            .collect();
        let mut pressed = HashSet::new();
        let mut step = |mods: &str, key: Option<&str>, down: bool| {
            let key = key.map(|k| k.parse().unwrap());
            transitions(hotkeys.iter().copied(), &mut pressed, mods.parse().unwrap(), key, down)
        };

        // Strg+Umschalt+Leertaste, Leertaste zuerst losgelassen.
        assert_eq!(step("CtrlLeft", None, true), vec![]);
        assert_eq!(step("CtrlLeft+ShiftLeft", None, true), vec![]);
        assert_eq!(step("CtrlLeft+ShiftLeft", Some("Space"), true), vec![(1, Pressed)]);
        assert_eq!(step("CtrlLeft+ShiftLeft", Some("Space"), true), vec![], "Wiederholung");
        assert_eq!(step("CtrlLeft+ShiftLeft", Some("Space"), false), vec![(1, Released)]);
        assert_eq!(step("CtrlLeft", None, false), vec![]);
        assert_eq!(step("", None, false), vec![]);

        // Ohne Umschalt passt die Kombi nicht.
        assert_eq!(step("CtrlLeft", Some("Space"), true), vec![]);
        assert_eq!(step("CtrlLeft", Some("Space"), false), vec![]);
        assert_eq!(step("", None, false), vec![]);

        // Nur Modifier, seitengenau; losgelassen, sobald die Modifier nicht mehr passen.
        assert_eq!(step("CtrlLeft", None, true), vec![]);
        assert_eq!(step("CtrlLeft+CmdLeft", None, true), vec![(3, Pressed)]);
        assert_eq!(step("CmdLeft", None, false), vec![(3, Released)]);
        assert_eq!(step("", None, false), vec![]);
        assert_eq!(step("CtrlRight", None, true), vec![(2, Pressed)]);
        assert_eq!(step("", None, false), vec![(2, Released)]);
    }
}
