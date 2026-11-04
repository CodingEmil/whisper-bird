//! Schwebendes Kontextmenü „Text bearbeiten“: markierten Text per Anweisung
//! (Chip, getippt oder gesprochen) umschreiben und zurück einfügen.

use crate::history::NewEntry;
use crate::insert::{self, InsertOptions};
use crate::platform::foreground::{self, AppContext};
use crate::settings::EditAction;
use crate::state::state;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const LABEL: &str = "editmenu";
const WIDTH: f64 = 600.0;
const HEIGHT: f64 = 470.0;

struct Session {
    target: AppContext,
    selection: Option<String>,
}

#[derive(Default)]
pub struct EditMenu {
    session: Mutex<Option<Session>>,
    busy: Mutex<bool>,
    /// Das Menü ist schon zu sehen, die Markierung wird noch kopiert.
    capturing: AtomicBool,
}

#[derive(Clone, Serialize)]
struct SelectionPayload {
    selection: Option<String>,
    app: String,
}

#[derive(Clone, Serialize)]
struct OpenPayload {
    selection: Option<String>,
    /// Die Markierung folgt per `editmenu-selection`.
    pending: bool,
    app: String,
    actions: Vec<EditAction>,
    translate_target: String,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("editmenu.html".into()))
        .title("Whisper Bird – Text bearbeiten")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .initialization_script(crate::platform_script())
        .build()?;
    #[cfg(target_os = "macos")]
    crate::platform::macos::panel::make_menu(&window)?;
    window.on_window_event(|event| {
        // Je nach Einstellung schließt ein Klick daneben das Menü. Bleibt es offen,
        // lässt sich in der App neuer Text markieren und übernehmen (siehe `open`).
        if let WindowEvent::Focused(false) = event {
            if state().settings().edit_menu_stays_open {
                return;
            }
            // tao meldet beim Öffnen mitunter „Fokus weg“, während der Fokus nur in die
            // WebView wandert. Deshalb kurz warten und nur schließen, wenn wirklich ein
            // anderes Fenster vorn ist – und keine Anfrage läuft.
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(150));
                let st = state();
                if is_open() && !is_foreground() && !*st.editmenu.busy.lock().unwrap() && !st.dictation.is_busy() {
                    close(false);
                }
            });
        }
    });
    Ok(())
}

pub fn is_open() -> bool {
    state()
        .app
        .get_webview_window(LABEL)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false)
}

/// Ist das Menü vorn (hat es den Fokus)?
fn is_foreground() -> bool {
    let Some(w) = state().app.get_webview_window(LABEL) else { return false };
    #[cfg(windows)]
    let focused = w.hwnd().is_ok_and(|h| h.0 as isize == foreground::active_app().handle);
    // macOS: Das Panel hat die Tastatur, auch wenn die Ziel-App aktiv bleibt.
    #[cfg(target_os = "macos")]
    let focused = w.is_focused().unwrap_or(false);
    #[cfg(not(any(windows, target_os = "macos")))]
    let focused = { let _ = w; false };
    focused
}

/// Vom Shortcut aufgerufen (Hotkey-Thread). Ist das Menü schon offen, schließt
/// das Kürzel es, wenn es vorn ist; sonst übernimmt es die Markierung der App
/// im Vordergrund.
pub fn open() {
    let st = state();
    if st.editmenu.capturing.load(Ordering::SeqCst) {
        return;
    }
    if is_open() {
        if is_foreground() {
            close(true);
        } else {
            recapture();
        }
        return;
    }
    if st.dictation.is_busy() {
        return;
    }
    let target = foreground::active_app();
    let s = st.settings();
    // Lokales Modell laden, während die Anweisung getippt oder gesprochen wird.
    if st.enhancer.ready(&s).is_ok() {
        let s = s.clone();
        tauri::async_runtime::spawn(async move { state().enhancer.warm(&s).await });
    }
    *st.editmenu.session.lock().unwrap() = Some(Session {
        target: target.clone(),
        selection: None,
    });
    st.editmenu.capturing.store(true, Ordering::SeqCst);
    let payload = OpenPayload {
        selection: None,
        pending: true,
        app: target.display_name(),
        actions: s.edit_actions.clone(),
        translate_target: s.translate_target.clone(),
    };
    let app = st.app.clone();
    // Sofort zeigen, aber ohne Fokus: Das Strg+C zum Kopieren muss noch die Ziel-App
    // erreichen, und das wartet aufs Loslassen der Tasten (ohne Markierung bis 0,6 s).
    let _ = st.app.run_on_main_thread(move || {
        let Some(w) = app.get_webview_window(LABEL) else { return };
        if let Some(pos) = menu_position(&app) {
            let _ = w.set_position(pos);
        }
        let _ = w.emit_to(LABEL, "editmenu-open", &payload);
        // macOS: Das Panel erscheint ohne Tastatur, bis force_foreground sie ihm gibt.
        #[cfg(target_os = "macos")]
        if crate::platform::macos::panel::show(&app, LABEL) {
            return;
        }
        let _ = w.set_focusable(false);
        let _ = w.show();
    });
    let selection = insert::capture_selection();
    st.editmenu.capturing.store(false, Ordering::SeqCst);
    // Inzwischen per X geschlossen? Dann nicht wieder nach vorn holen.
    let still_open = match st.editmenu.session.lock().unwrap().as_mut() {
        Some(session) => {
            session.selection = selection.clone();
            true
        }
        None => false,
    };
    let payload = SelectionPayload {
        selection,
        app: target.display_name(),
    };
    let app = st.app.clone();
    let _ = st.app.run_on_main_thread(move || {
        let Some(w) = app.get_webview_window(LABEL) else { return };
        #[cfg(not(target_os = "macos"))]
        let _ = w.set_focusable(true);
        if still_open {
            let _ = w.emit_to(LABEL, "editmenu-selection", &payload);
            force_foreground(&w);
        }
    });
}

/// Neue Markierung bei offenem Menü: Ziel-App und Text wechseln, die Anweisung bleibt.
fn recapture() {
    let st = state();
    let app = st.app.clone();
    // Während einer Anfrage nur nach vorn holen: Das Ergebnis gehört zum alten Text.
    let payload = if *st.editmenu.busy.lock().unwrap() || st.dictation.is_busy() {
        None
    } else {
        let target = foreground::active_app();
        let selection = insert::capture_selection();
        *st.editmenu.session.lock().unwrap() = Some(Session {
            target: target.clone(),
            selection: selection.clone(),
        });
        Some(SelectionPayload {
            selection,
            app: target.display_name(),
        })
    };
    let _ = st.app.run_on_main_thread(move || {
        let Some(w) = app.get_webview_window(LABEL) else { return };
        if let Some(payload) = payload {
            let _ = w.emit_to(LABEL, "editmenu-selection", &payload);
        }
        force_foreground(&w);
    });
}

/// Nahe am Mauszeiger, aber vollständig auf dem Bildschirm.
fn menu_position(app: &AppHandle) -> Option<PhysicalPosition<i32>> {
    let cursor = app.cursor_position().ok()?;
    let monitor = app.monitor_from_point(cursor.x, cursor.y).ok().flatten()?;
    let scale = monitor.scale_factor();
    let wa = monitor.work_area();
    let w = (WIDTH * scale) as i32;
    let h = (HEIGHT * scale) as i32;
    let mut x = cursor.x as i32 - w / 2;
    let mut y = cursor.y as i32 + (16.0 * scale) as i32;
    if y + h > wa.position.y + wa.size.height as i32 {
        y = cursor.y as i32 - h - (16.0 * scale) as i32;
    }
    x = x.clamp(wa.position.x, wa.position.x + wa.size.width as i32 - w);
    y = y.clamp(wa.position.y, wa.position.y + wa.size.height as i32 - h);
    Some(PhysicalPosition::new(x, y))
}

/// Windows erlaubt Hintergrundprozessen das Fokussieren nur eingeschränkt;
/// über AttachThreadInput an den aktuellen Vordergrund-Thread klappt es.
/// macOS: Das Panel bekommt die Tastatur, ohne die App zu aktivieren.
fn force_foreground(w: &tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    if crate::platform::macos::panel::focus(w) {
        return;
    }
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
        use windows::Win32::UI::WindowsAndMessaging::{
            BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow,
        };
        if let Ok(h) = w.hwnd() {
            let hwnd = HWND(h.0 as *mut _);
            let fg = GetForegroundWindow();
            let fg_thread = GetWindowThreadProcessId(fg, None);
            let me = GetCurrentThreadId();
            let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            if attached {
                let _ = AttachThreadInput(me, fg_thread, false);
            }
        }
    }
    let _ = w.set_focus();
}

/// Schließt das Menü; mit `restore_focus` bekommt die Ziel-App den Fokus zurück.
pub fn close(restore_focus: bool) {
    let st = state();
    let session = st.editmenu.session.lock().unwrap().take();
    if st.dictation.is_recording() {
        st.dictation.cancel();
    }
    let app = st.app.clone();
    let _ = st.app.run_on_main_thread(move || {
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.hide();
        }
    });
    // Das Menü verschwindet, bevor es das Loslassen der Tasten sieht. Ohne Freigabe
    // gälte das Kürzel als weiter gedrückt und der nächste Druck käme nicht an.
    st.hotkeys.window_blur();
    if restore_focus {
        if let Some(s) = session {
            foreground::focus(s.target.handle);
        }
    }
}

pub async fn run(instruction: String) -> Result<String, String> {
    let st = state();
    let s = st.settings();
    st.enhancer.ready(&s)?;
    let selection = st
        .editmenu
        .session
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|x| x.selection.clone());
    let instruction = instruction.replace("{target}", &s.translate_target);
    *st.editmenu.busy.lock().unwrap() = true;
    let result = st.enhancer.edit(&s, &instruction, selection.as_deref()).await;
    *st.editmenu.busy.lock().unwrap() = false;
    result.map_err(|e| format!("{e:#}"))
}

/// Ergebnis übernehmen: Menü schließen, Fokus zurück, Markierung ersetzen.
pub fn apply(text: String) -> Result<(), String> {
    let st = state();
    let session = st.editmenu.session.lock().unwrap().take();
    let Some(session) = session else { return Err("Keine aktive Sitzung".into()) };
    let app = st.app.clone();
    let _ = st.app.run_on_main_thread(move || {
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.hide();
        }
    });
    std::thread::spawn(move || {
        let st = state();
        let s = st.settings();
        foreground::focus(session.target.handle);
        std::thread::sleep(Duration::from_millis(180));
        if let Err(e) = insert::insert_text(
            &text,
            &InsertOptions {
                method: s.paste_method,
                // Die Einstellung gilt fürs Diktat; hier gibt es den Kopieren-Knopf.
                keep_in_clipboard: false,
                auto_enter: false,
            },
        ) {
            log::error!("Einfügen fehlgeschlagen: {e:#}");
            return;
        }
        if let Ok(entry) = st.history.add(
            NewEntry {
                text,
                raw_text: session.selection.unwrap_or_default(),
                source: "edit",
                app: session.target.display_name(),
                language: None,
                duration_ms: 0,
                model: String::new(),
            },
            s.history_limit,
        ) {
            let _ = st.app.emit("history-added", &entry);
        }
    });
    Ok(())
}
