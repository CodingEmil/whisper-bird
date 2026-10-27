//! Schwebendes Kontextmenü „Text bearbeiten“: markierten Text per Anweisung
//! (Chip, getippt oder gesprochen) umschreiben und zurück einfügen.

use crate::history::NewEntry;
use crate::insert::{self, InsertOptions};
use crate::settings::EditAction;
use crate::state::state;
use crate::win::foreground::{self, AppContext};
use serde::Serialize;
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
}

#[derive(Clone, Serialize)]
struct OpenPayload {
    selection: Option<String>,
    app: String,
    actions: Vec<EditAction>,
    translate_target: String,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("editmenu.html".into()))
        .title("Whisper Island – Text bearbeiten")
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
        .build()?;
    window.on_window_event(|event| {
        if let WindowEvent::Focused(false) = event {
            // Klick daneben schließt das Menü – außer während einer Anfrage.
            let st = state();
            if !*st.editmenu.busy.lock().unwrap() && !st.dictation.is_busy() {
                close(false);
            }
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

/// Vom Shortcut aufgerufen (Hotkey-Thread).
pub fn open() {
    let st = state();
    if is_open() {
        close(true);
        return;
    }
    if st.dictation.is_busy() {
        return;
    }
    let target = foreground::active_app();
    let selection = insert::capture_selection();
    let s = st.settings();
    *st.editmenu.session.lock().unwrap() = Some(Session {
        target: target.clone(),
        selection: selection.clone(),
    });
    let payload = OpenPayload {
        selection,
        app: target.display_name(),
        actions: s.edit_actions.clone(),
        translate_target: s.translate_target.clone(),
    };
    let app = st.app.clone();
    let _ = st.app.run_on_main_thread(move || {
        let Some(w) = app.get_webview_window(LABEL) else { return };
        if let Some(pos) = menu_position(&app) {
            let _ = w.set_position(pos);
        }
        let _ = w.emit_to(LABEL, "editmenu-open", &payload);
        let _ = w.show();
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
fn force_foreground(w: &tauri::WebviewWindow) {
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
    if restore_focus {
        if let Some(s) = session {
            foreground::focus(s.target.hwnd);
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
        foreground::focus(session.target.hwnd);
        std::thread::sleep(Duration::from_millis(180));
        if let Err(e) = insert::insert_text(
            &text,
            &InsertOptions {
                method: s.paste_method,
                preserve_clipboard: s.preserve_clipboard,
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
