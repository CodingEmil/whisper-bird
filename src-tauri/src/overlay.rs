//! Das schwebende Overlay: im Ruhezustand ein verschiebbares Icon (optional),
//! während der Aufnahme eine Pille mit Waveform, danach Status/Fehler.
//! Das Fenster nimmt nie den Fokus, damit der Text in der richtigen App landet.

use crate::settings::OverlayPosition;
use crate::state::state;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Monitor, PhysicalPosition,
    Position, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

pub const LABEL: &str = "overlay";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Recording,
    Transcribing,
    Enhancing,
    Done,
    Warning,
    Error,
    Files,
}

#[derive(Clone, Debug, Serialize)]
pub struct OverlayState {
    pub phase: Phase,
    pub message: Option<String>,
    pub enhance: bool,
    pub progress: Option<f32>,
    pub floating: bool,
}

static CURRENT: Mutex<Phase> = Mutex::new(Phase::Idle);

/// Zuletzt von uns gesetzte Fensterposition und die Mitte, aus der sie
/// berechnet wurde. Weicht die echte Position davon ab, hat der Nutzer gezogen.
static PLACED: Mutex<Option<((f64, f64), (f64, f64))>> = Mutex::new(None);

/// Vom Nutzer gezogene Mitte – gilt sofort für Icon und Pille, auch bevor sie
/// verzögert in den Einstellungen landet. Ohne Floating Icon nur bis zum Ende
/// der laufenden Aufnahme.
static DRAGGED: Mutex<Option<(f64, f64)>> = Mutex::new(None);

/// Alle Positionen hier (auch `floating_pos`) sind Bildschirmkoordinaten des
/// Systems, in denen auch `monitor_from_point` rechnet: unter Windows physische
/// Pixel, unter macOS Punkte. Physische Pixel wären dort nicht eindeutig (jeder
/// Monitor hat seinen eigenen Faktor), und auf einem Retina-Display läge jede
/// Position in der rechten oder unteren Hälfte scheinbar außerhalb des Bildschirms.
///
/// Liefert die Einheiten pro logischem Pixel bei gegebenem Monitor-Faktor.
fn units(scale: f64) -> f64 {
    if cfg!(target_os = "macos") {
        1.0
    } else {
        scale
    }
}

/// Umrechnung eines physischen Werts von tauri in Bildschirmkoordinaten.
fn from_physical(value: f64, scale: f64) -> f64 {
    if cfg!(target_os = "macos") {
        value / scale
    } else {
        value
    }
}

/// Arbeitsbereich des Monitors in Bildschirmkoordinaten: x, y, Breite, Höhe.
fn work_area(m: &Monitor) -> (f64, f64, f64, f64) {
    let s = m.scale_factor();
    let wa = m.work_area();
    (
        from_physical(wa.position.x as f64, s),
        from_physical(wa.position.y as f64, s),
        from_physical(wa.size.width as f64, s),
        from_physical(wa.size.height as f64, s),
    )
}

/// Fensterposition (links oben) aus Mitte und Größe; waagerecht bleibt das
/// Fenster im Arbeitsbereich, die Mitte selbst bleibt davon unberührt.
fn window_origin(center: (f64, f64), size: (f64, f64), area: Option<(f64, f64, f64, f64)>) -> (f64, f64) {
    let (w, h) = (size.0 as i32, size.1 as i32);
    let mut x = center.0 as i32 - w / 2;
    let y = center.1 as i32 - h / 2;
    if let Some((ax, _, aw, _)) = area {
        let min_x = ax as i32;
        x = x.clamp(min_x, (min_x + aw as i32 - w).max(min_x));
    }
    (x as f64, y as f64)
}

fn logical_size(phase: Phase) -> (f64, f64) {
    match phase {
        Phase::Idle => (64.0, 64.0),
        Phase::Warning | Phase::Error => (400.0, 64.0),
        _ => (280.0, 64.0),
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("overlay.html".into()))
        .title("Whisper Bird Overlay")
        .inner_size(64.0, 64.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .closable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .visible(false)
        // macOS: Der erste Klick startet das Diktat, statt nur das Fenster zu wecken.
        .accept_first_mouse(true)
        .initialization_script(crate::platform_script())
        .build()?;
    #[cfg(target_os = "macos")]
    crate::platform::macos::panel::make_overlay(&window)?;

    let w = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Moved(_) => on_moved(&w),
        WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
            let paths = paths.clone();
            std::thread::spawn(move || crate::files::enqueue(paths));
        }
        WindowEvent::DragDrop(tauri::DragDropEvent::Enter { .. }) => {
            let _ = w.emit_to(LABEL, "overlay-drop", true);
        }
        WindowEvent::DragDrop(tauri::DragDropEvent::Leave) => {
            let _ = w.emit_to(LABEL, "overlay-drop", false);
        }
        _ => {}
    });
    Ok(())
}

fn window() -> Option<WebviewWindow> {
    state().app.get_webview_window(LABEL)
}

static PENDING_CENTER: Mutex<Option<((f64, f64), std::time::Instant)>> = Mutex::new(None);

/// Erkennt Verschieben durch den Nutzer (Icon oder Pille) und verschiebt die
/// gemerkte Mitte um denselben Betrag. Die echte Position wird hier gelesen
/// statt aus dem Event, damit sie exakt mit `set_position` vergleichbar ist.
fn on_moved(w: &WebviewWindow) {
    let Ok(pos) = w.outer_position() else { return };
    let Ok(scale) = w.scale_factor() else { return };
    let pos = (from_physical(pos.x as f64, scale), from_physical(pos.y as f64, scale));
    let mut placed = PLACED.lock().unwrap();
    let Some((at, center)) = *placed else { return };
    if (pos.0 - at.0).abs() < 0.5 && (pos.1 - at.1).abs() < 0.5 {
        return; // eigenes set_position
    }
    let center = (center.0 + pos.0 - at.0, center.1 + pos.1 - at.1);
    *placed = Some((pos, center));
    drop(placed);
    *DRAGGED.lock().unwrap() = Some(center);
    if state().settings().show_floating_icon {
        save_center(center);
    }
}

/// Speichert die Mitte erst, wenn das Ziehen 400 ms ruht, damit nicht jedes
/// Move-Event die Einstellungen schreibt.
fn save_center(center: (f64, f64)) {
    let mut pending = PENDING_CENTER.lock().unwrap();
    let schedule = pending.is_none();
    *pending = Some((center, std::time::Instant::now()));
    if !schedule {
        return;
    }
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let mut pending = PENDING_CENTER.lock().unwrap();
        let Some((center, at)) = *pending else { return };
        if at.elapsed() < std::time::Duration::from_millis(400) {
            continue;
        }
        *pending = None;
        drop(pending);
        let st = state();
        let moved = st
            .settings()
            .floating_pos
            .map(|(x, y)| (x - center.0).abs() > 2.0 || (y - center.1).abs() > 2.0)
            .unwrap_or(true);
        if moved {
            st.update_settings(|s| s.floating_pos = Some(center));
        }
        // Ab jetzt gilt wieder die gespeicherte Einstellung.
        let mut dragged = DRAGGED.lock().unwrap();
        if *dragged == Some(center) {
            *dragged = None;
        }
        return;
    });
}

/// Setzt Phase und Inhalt; positioniert, vergrößert und zeigt/versteckt das Fenster.
pub fn set(phase: Phase, message: Option<String>, enhance: bool, progress: Option<f32>) {
    let st = state();
    let s = st.settings();
    *CURRENT.lock().unwrap() = phase;
    let Some(w) = window() else { return };
    let payload = OverlayState {
        phase,
        message,
        enhance,
        progress,
        floating: s.show_floating_icon,
    };
    let _ = w.emit_to(LABEL, "overlay-state", &payload);

    let app = st.app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        let Some(w) = app.get_webview_window(LABEL) else { return };
        if phase == Phase::Idle && !s.show_floating_icon {
            // Nächste Aufnahme startet wieder an der eingestellten Position.
            *DRAGGED.lock().unwrap() = None;
            let _ = w.hide();
            return;
        }
        place(&app, &w, phase, s.show_floating_icon, s.floating_pos, s.overlay_position);
        show_no_activate(&w);
    });
}

pub fn current() -> Phase {
    *CURRENT.lock().unwrap()
}

/// Nach Einstellungsänderungen (Floating Icon an/aus) neu anordnen.
pub fn refresh() {
    let phase = current();
    if phase == Phase::Idle {
        set(Phase::Idle, None, false, None);
    }
}

pub fn level(value: f32) {
    if let Some(w) = window() {
        let _ = w.emit_to(LABEL, "audio-level", value);
    }
}

fn place(
    app: &AppHandle,
    w: &WebviewWindow,
    phase: Phase,
    floating: bool,
    floating_pos: Option<(f64, f64)>,
    anchor: OverlayPosition,
) {
    let (lw, lh) = logical_size(phase);
    let on_screen = |(x, y): &(f64, f64)| app.monitor_from_point(*x, *y).ok().flatten().is_some();
    let dragged = DRAGGED.lock().unwrap().filter(on_screen);
    let center = match dragged {
        Some(c) => c,
        None if floating => floating_pos.filter(on_screen).unwrap_or_else(|| default_center(app)),
        None => match anchor_center(app, anchor, lh) {
            Some(c) => c,
            None => return,
        },
    };
    let monitor = app
        .monitor_from_point(center.0, center.1)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let scale = units(monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0));
    let pos = window_origin(center, (lw * scale, lh * scale), monitor.as_ref().map(work_area));
    let _ = w.set_size(LogicalSize::new(lw, lh));
    // Erst nach set_size merken (ein Move-Event davon trägt noch die alte
    // Position) und vor set_position (dessen Event muss als eigenes gelten).
    *PLACED.lock().unwrap() = Some((pos, center));
    let native: Position = if cfg!(target_os = "macos") {
        LogicalPosition::new(pos.0, pos.1).into()
    } else {
        PhysicalPosition::new(pos.0 as i32, pos.1 as i32).into()
    };
    let _ = w.set_position(native);
}

/// Mitte der Pille oben bzw. unten mittig auf dem Monitor mit dem Mauszeiger.
fn anchor_center(app: &AppHandle, anchor: OverlayPosition, lh: f64) -> Option<(f64, f64)> {
    let primary = app.primary_monitor().ok().flatten();
    // Den Mauszeiger meldet tauri mit dem Faktor des Hauptmonitors umgerechnet.
    let primary_scale = primary.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
    let m = app
        .cursor_position()
        .ok()
        .and_then(|c| {
            let (x, y) = (from_physical(c.x, primary_scale), from_physical(c.y, primary_scale));
            app.monitor_from_point(x, y).ok().flatten()
        })
        .or(primary)?;
    let scale = units(m.scale_factor());
    let (ax, ay, aw, ah) = work_area(&m);
    let offset = 14.0 * scale + lh * scale / 2.0;
    let x = ax + aw / 2.0;
    let y = match anchor {
        OverlayPosition::TopCenter => ay + offset,
        OverlayPosition::BottomCenter => ay + ah - offset,
    };
    Some((x, y))
}

fn default_center(app: &AppHandle) -> (f64, f64) {
    match app.primary_monitor().ok().flatten() {
        Some(m) => {
            let (ax, ay, aw, ah) = work_area(&m);
            (ax + aw / 2.0, ay + ah - 140.0 * units(m.scale_factor()))
        }
        None => (1200.0, 700.0),
    }
}

/// Zeigt das Fenster ganz oben, ohne es zu aktivieren.
fn show_no_activate(w: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    if crate::platform::macos::panel::show(w.app_handle(), LABEL) {
        return;
    }
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
        };
        if let Ok(h) = w.hwnd() {
            unsafe {
                let _ = SetWindowPos(
                    HWND(h.0 as *mut _),
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
            return;
        }
    }
    let _ = w.show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn koordinaten_passen_zur_plattform() {
        // Retina (Faktor 2): macOS rechnet in Punkten, Windows in physischen Pixeln.
        let expected = if cfg!(target_os = "macos") { 829.0 } else { 1658.0 };
        assert_eq!(from_physical(1658.0, 2.0), expected);
        assert_eq!(units(2.0), if cfg!(target_os = "macos") { 1.0 } else { 2.0 });
        // Windows bleibt auch bei krummem Faktor exakt.
        if !cfg!(target_os = "macos") {
            assert_eq!(from_physical(1234.0, 1.1), 1234.0);
        }
    }

    #[test]
    fn fenster_liegt_um_die_mitte() {
        let area = Some((0.0, 25.0, 1280.0, 760.0));
        assert_eq!(window_origin((900.0, 600.0), (64.0, 64.0), area), (868.0, 568.0));
        assert_eq!(window_origin((900.0, 600.0), (280.0, 64.0), area), (760.0, 568.0));
    }

    #[test]
    fn pille_bleibt_waagerecht_im_arbeitsbereich() {
        let area = Some((0.0, 25.0, 1280.0, 760.0));
        assert_eq!(window_origin((1250.0, 600.0), (280.0, 64.0), area).0, 1000.0);
        assert_eq!(window_origin((30.0, 600.0), (280.0, 64.0), area).0, 0.0);
        // Zweiter Monitor links vom Hauptmonitor.
        let left = Some((-1920.0, 0.0, 1920.0, 1080.0));
        assert_eq!(window_origin((-1900.0, 600.0), (280.0, 64.0), left).0, -1920.0);
        // Ohne Monitor keine Begrenzung.
        assert_eq!(window_origin((5000.0, 600.0), (280.0, 64.0), None).0, 4860.0);
    }
}
