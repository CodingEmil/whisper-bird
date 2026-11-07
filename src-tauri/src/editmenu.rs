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
use tauri::{AppHandle, Emitter, Manager, Position, WebviewUrl, WebviewWindowBuilder, WindowEvent};

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
fn menu_position(app: &AppHandle) -> Option<Position> {
    let (cursor, monitor) = crate::overlay::cursor_on_monitor(app)?;
    let origin = menu_origin(
        cursor,
        crate::overlay::work_area(&monitor),
        crate::overlay::units(monitor.scale_factor()),
    );
    Some(crate::overlay::native_position(origin))
}

/// Links oben des Menüs: unter dem Mauszeiger, fehlt dort der Platz darüber, und
/// immer im Arbeitsbereich (x, y, Breite, Höhe). Alles in Bildschirmkoordinaten, `units`
/// sind deren Einheiten pro logischem Pixel (siehe `overlay::units`).
fn menu_origin(cursor: (f64, f64), area: (f64, f64, f64, f64), units: f64) -> (f64, f64) {
    let (ax, ay, aw, ah) = (area.0 as i32, area.1 as i32, area.2 as i32, area.3 as i32);
    let (cx, cy) = (cursor.0 as i32, cursor.1 as i32);
    let w = (WIDTH * units) as i32;
    let h = (HEIGHT * units) as i32;
    let gap = (16.0 * units) as i32;
    let mut y = cy + gap;
    if y + h > ay + ah {
        y = cy - h - gap;
    }
    // Ist der Arbeitsbereich kleiner als das Menü, gilt die Ecke links oben.
    let x = (cx - w / 2).clamp(ax, (ax + aw - w).max(ax));
    let y = y.clamp(ay, (ay + ah - h).max(ay));
    (x as f64, y as f64)
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

#[cfg(test)]
mod tests {
    use super::*;

    // Retina-Display 1470 x 956 Punkte, Arbeitsbereich ohne Menüleiste und Dock.
    const MAC: (f64, f64, f64, f64) = (0.0, 33.0, 1470.0, 866.0);

    #[test]
    fn menue_liegt_unter_dem_mauszeiger() {
        assert_eq!(menu_origin((800.0, 200.0), MAC, 1.0), (500.0, 216.0));
    }

    #[test]
    fn menue_klappt_unten_ueber_den_mauszeiger() {
        assert_eq!(menu_origin((800.0, 800.0), MAC, 1.0), (500.0, 314.0));
    }

    #[test]
    fn menue_bleibt_in_den_ecken_im_arbeitsbereich() {
        assert_eq!(menu_origin((1465.0, 950.0), MAC, 1.0), (870.0, 429.0));
        assert_eq!(menu_origin((5.0, 5.0), MAC, 1.0), (0.0, 33.0));
    }

    #[test]
    fn windows_rechnet_in_physischen_pixeln() {
        // 150 %: Menü 900 x 705, Abstand 24.
        let area = (0.0, 0.0, 2560.0, 1400.0);
        assert_eq!(menu_origin((1000.0, 300.0), area, 1.5), (550.0, 324.0));
        assert_eq!(menu_origin((1000.0, 1000.0), area, 1.5), (550.0, 271.0));
        assert_eq!(menu_origin((2500.0, 1390.0), area, 1.5), (1660.0, 661.0));
        // Zweiter Monitor links vom Hauptmonitor.
        let left = (-1920.0, 0.0, 1920.0, 1040.0);
        assert_eq!(menu_origin((-1900.0, 100.0), left, 1.0), (-1920.0, 116.0));
    }

    #[test]
    fn zu_kleiner_arbeitsbereich_gilt_ab_links_oben() {
        assert_eq!(menu_origin((100.0, 100.0), (10.0, 20.0, 400.0, 300.0), 1.0), (10.0, 20.0));
    }
}
