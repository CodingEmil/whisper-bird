//! Text an der Cursorposition der aktiven App einfügen und markierten Text
//! auslesen – jeweils ohne die Zwischenablage des Nutzers zu verlieren.

use crate::settings::PasteMethod;
use crate::win::{clipboard, input};
use std::time::{Duration, Instant};

pub struct InsertOptions {
    pub method: PasteMethod,
    pub preserve_clipboard: bool,
    pub auto_enter: bool,
}

/// Zeit, die Ziel-Apps zum Lesen der Zwischenablage nach Strg+V bekommen,
/// bevor der alte Inhalt zurückkommt (Electron-Apps lesen asynchron).
const PASTE_SETTLE: Duration = Duration::from_millis(450);

pub fn insert_text(text: &str, opts: &InsertOptions) -> anyhow::Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    input::wait_modifiers_released(Duration::from_millis(1200));
    match opts.method {
        PasteMethod::Type => input::type_text(text),
        PasteMethod::Clipboard => {
            let snapshot = opts.preserve_clipboard.then(clipboard::snapshot);
            clipboard::set_text(text, opts.preserve_clipboard)?;
            std::thread::sleep(Duration::from_millis(25));
            input::paste();
            if let Some(snapshot) = snapshot {
                std::thread::sleep(PASTE_SETTLE);
                if snapshot.is_empty() {
                    clipboard::clear();
                } else {
                    clipboard::restore(&snapshot);
                }
            }
        }
    }
    if opts.auto_enter {
        std::thread::sleep(Duration::from_millis(if opts.preserve_clipboard { 30 } else { 120 }));
        input::enter();
    }
    Ok(())
}

/// Kopiert die aktuelle Markierung der aktiven App (per Strg+C) und stellt die
/// Zwischenablage danach wieder her. `None`, wenn nichts markiert ist.
pub fn capture_selection() -> Option<String> {
    input::wait_modifiers_released(Duration::from_millis(1200));
    let snapshot = clipboard::snapshot();
    clipboard::clear();
    let before = clipboard::sequence();
    input::copy();
    let start = Instant::now();
    let mut text = None;
    while start.elapsed() < Duration::from_millis(600) {
        std::thread::sleep(Duration::from_millis(15));
        if clipboard::sequence() != before {
            // Manche Apps setzen mehrere Formate nacheinander.
            std::thread::sleep(Duration::from_millis(40));
            text = clipboard::get_text();
            break;
        }
    }
    if snapshot.is_empty() {
        clipboard::clear();
    } else {
        clipboard::restore(&snapshot);
    }
    text.filter(|t| !t.trim().is_empty())
}
