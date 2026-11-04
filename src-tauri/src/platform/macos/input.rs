//! Synthetische Tastatureingaben über CGEvent: ⌘V, ⌘C, Enter und Unicode-Tippen.
//! Ohne Bedienungshilfen-Recht verwirft macOS die Ereignisse stillschweigend.

use objc2_core_foundation::CFRetained;
use objc2_core_graphics::{
    CGEvent, CGEventField, CGEventFlags, CGEventSource, CGEventSourceStateID, CGEventTapLocation,
    CGKeyCode,
};
use std::time::{Duration, Instant};

/// Markierung, damit eigene Eingaben erkennbar sind (wie unter Windows).
const USER_DATA: i64 = 0x5749_4953; // "WIIS"

// Virtuelle Tastencodes (Carbon kVK_*). Sie bezeichnen die Position auf einer
// ANSI-Tastatur; bei QWERTZ und QWERTY liegt V dort, wo ⌘V es erwartet.
const KEY_C: CGKeyCode = 8;
const KEY_V: CGKeyCode = 9;
const KEY_RETURN: CGKeyCode = 36;

const MODIFIERS: CGEventFlags = CGEventFlags::MaskShift
    .union(CGEventFlags::MaskControl)
    .union(CGEventFlags::MaskAlternate)
    .union(CGEventFlags::MaskCommand);

fn source() -> Option<CFRetained<CGEventSource>> {
    CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
}

/// Drückt und löst eine Taste mit genau diesen Modifiern; gerade gehaltene
/// Tasten des Nutzers fließen nicht ein.
fn tap(code: CGKeyCode, flags: CGEventFlags) {
    let src = source();
    for down in [true, false] {
        if let Some(event) = CGEvent::new_keyboard_event(src.as_deref(), code, down) {
            CGEvent::set_flags(Some(&event), flags);
            CGEvent::set_integer_value_field(Some(&event), CGEventField::EventSourceUserData, USER_DATA);
            CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&event));
        }
    }
}

pub fn paste() {
    tap(KEY_V, CGEventFlags::MaskCommand);
}

pub fn copy() {
    tap(KEY_C, CGEventFlags::MaskCommand);
}

pub fn enter() {
    tap(KEY_RETURN, CGEventFlags::empty());
}

/// Tippt Text Zeichen für Zeichen als Unicode-Ereignisse.
pub fn type_text(text: &str) {
    let src = source();
    for (i, ch) in text.chars().enumerate() {
        if ch == '\r' {
            continue;
        }
        if ch == '\n' {
            // Umschalt+Enter: Zeilenumbruch, ohne in Chats abzuschicken.
            tap(KEY_RETURN, CGEventFlags::MaskShift);
            continue;
        }
        let mut buf = [0u16; 2];
        let units = ch.encode_utf16(&mut buf);
        for down in [true, false] {
            // Tastencode 0 ist nur ein Träger; maßgeblich ist der Unicode-Text.
            if let Some(event) = CGEvent::new_keyboard_event(src.as_deref(), 0, down) {
                CGEvent::set_flags(Some(&event), CGEventFlags::empty());
                CGEvent::set_integer_value_field(Some(&event), CGEventField::EventSourceUserData, USER_DATA);
                unsafe { CGEvent::keyboard_set_unicode_string(Some(&event), units.len() as _, units.as_ptr()) };
                CGEvent::post(CGEventTapLocation::SessionEventTap, Some(&event));
            }
        }
        // Kurze Pausen, damit langsame Apps keine Zeichen verlieren.
        if i % 16 == 15 {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

fn modifiers_down() -> bool {
    CGEventSource::flags_state(CGEventSourceStateID::HIDSystemState).intersects(MODIFIERS)
}

/// Wartet, bis der Nutzer alle Modifier losgelassen hat (sonst würde z. B.
/// aus ⌘V ein ⌘⌥V). Anders als unter Windows lassen sich gehaltene Tasten
/// hier nicht lösen; nach der Wartezeit geht es trotzdem weiter.
pub fn wait_modifiers_released(timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !modifiers_down() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    log::debug!("Modifier nach {timeout:?} noch gedrückt, sende trotzdem");
}
