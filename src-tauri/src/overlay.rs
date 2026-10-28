//! Das schwebende Overlay: im Ruhezustand ein verschiebbares Icon (optional),
//! während der Aufnahme eine Pille mit Waveform, danach Status/Fehler.
//! Das Fenster nimmt nie den Fokus, damit der Text in der richtigen App landet.

use crate::settings::OverlayPosition;
use crate::state::state;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent,
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
static PLACED: Mutex<Option<(PhysicalPosition<i32>, (f64, f64))>> = Mutex::new(None);

/// Vom Nutzer gezogene Mitte – gilt sofort für Icon und Pille, auch bevor sie
/// verzögert in den Einstellungen landet. Ohne Floating Icon nur bis zum Ende
/// der laufenden Aufnahme.
static DRAGGED: Mutex<Option<(f64, f64)>> = Mutex::new(None);

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
        .build()?;

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
    let mut placed = PLACED.lock().unwrap();
    let Some((at, center)) = *placed else { return };
    if pos == at {
        return; // eigenes set_position
    }
    let center = (
        center.0 + (pos.x - at.x) as f64,
        center.1 + (pos.y - at.y) as f64,
    );
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
    let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
    let size = PhysicalSize::new((lw * scale) as i32, (lh * scale) as i32);
    let mut x = center.0 as i32 - size.width / 2;
    let y = center.1 as i32 - size.height / 2;
    if let Some(m) = monitor {
        let wa = m.work_area();
        let max_x = wa.position.x + wa.size.width as i32 - size.width;
        x = x.clamp(wa.position.x, max_x.max(wa.position.x));
    }
    let pos = PhysicalPosition::new(x, y);
    let _ = w.set_size(LogicalSize::new(lw, lh));
    // Erst nach set_size merken (ein Move-Event davon trägt noch die alte
    // Position) und vor set_position (dessen Event muss als eigenes gelten).
    *PLACED.lock().unwrap() = Some((pos, center));
    let _ = w.set_position(pos);
}

/// Mitte der Pille oben bzw. unten mittig auf dem Monitor mit dem Mauszeiger.
fn anchor_center(app: &AppHandle, anchor: OverlayPosition, lh: f64) -> Option<(f64, f64)> {
    let m = app
        .cursor_position()
        .ok()
        .and_then(|c| app.monitor_from_point(c.x, c.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let scale = m.scale_factor();
    let wa = m.work_area();
    let offset = 14.0 * scale + lh * scale / 2.0;
    let x = wa.position.x as f64 + wa.size.width as f64 / 2.0;
    let y = match anchor {
        OverlayPosition::TopCenter => wa.position.y as f64 + offset,
        OverlayPosition::BottomCenter => wa.position.y as f64 + wa.size.height as f64 - offset,
    };
    Some((x, y))
}

fn default_center(app: &AppHandle) -> (f64, f64) {
    match app.primary_monitor().ok().flatten() {
        Some(m) => {
            let wa = m.work_area();
            let s = m.scale_factor();
            (
                wa.position.x as f64 + wa.size.width as f64 / 2.0,
                wa.position.y as f64 + wa.size.height as f64 - 140.0 * s,
            )
        }
        None => (1200.0, 700.0),
    }
}

/// Zeigt das Fenster ganz oben, ohne es zu aktivieren.
fn show_no_activate(w: &WebviewWindow) {
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
