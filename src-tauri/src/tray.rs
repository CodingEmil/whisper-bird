//! Tray-Icon mit Menü (Infobereich der Taskleiste) und das Rechtsklick-Menü
//! des Floating Icons. Beide teilen sich IDs und Handler.

use crate::audio::mic;
use crate::dictation::Mode;
use crate::i18n::{t, MENU_LANGUAGES};
use crate::settings::{Activation, EnhanceMode};
use crate::state::state;
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

pub const TRAY_ID: &str = "main";

static LAST: Mutex<String> = Mutex::new(String::new());
static STATUS: Mutex<&'static str> = Mutex::new("idle");

fn icon(status: &str) -> Option<Image<'static>> {
    let bytes: &'static [u8] = match status {
        "recording" | "meeting" => include_bytes!("../icons/tray-recording.png"),
        "transcribing" | "enhancing" => include_bytes!("../icons/tray-processing.png"),
        _ => include_bytes!("../icons/tray-idle.png"),
    };
    Image::from_bytes(bytes).ok()
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    if let Some(last) = state().history.last() {
        *LAST.lock().unwrap() = last.text;
    }
    let menu = build_menu(app)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Whisper Island")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|_tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                crate::show_main_window(None);
            }
        });
    if let Some(img) = icon("idle") {
        builder = builder.icon(img);
    }
    builder.build(app)?;
    Ok(())
}

pub fn rebuild() {
    let app = state().app.clone();
    let _ = state().app.run_on_main_thread(move || {
        if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_menu(&app)) {
            let _ = tray.set_menu(Some(menu));
        }
    });
}

pub fn set_status(status: &'static str) {
    *STATUS.lock().unwrap() = status;
    let app = state().app.clone();
    let _ = state().app.run_on_main_thread(move || {
        if let Some(tray) = app.tray_by_id(TRAY_ID) {
            let _ = tray.set_icon(icon(status));
            let tip = match status {
                "recording" => "Whisper Island – Aufnahme läuft",
                "meeting" => "Whisper Island – Meeting läuft",
                "transcribing" | "enhancing" => "Whisper Island – verarbeite …",
                _ => "Whisper Island",
            };
            let _ = tray.set_tooltip(Some(tip));
        }
    });
    rebuild();
}

pub fn set_last(text: &str) {
    *LAST.lock().unwrap() = text.to_string();
    rebuild();
}

fn truncate(text: &str, max: usize) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        one_line.chars().take(max).collect::<String>() + " …"
    }
}

fn language_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    let s = state().settings();
    let lang = &s.ui_language;
    let mut items: Vec<CheckMenuItem<Wry>> = Vec::new();
    items.push(CheckMenuItem::with_id(app, "lang:auto", t(lang, "tray_auto"), true, s.language == "auto", None::<&str>)?);
    for (code, name) in MENU_LANGUAGES {
        items.push(CheckMenuItem::with_id(app, format!("lang:{code}"), *name, true, s.language == *code, None::<&str>)?);
    }
    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    Submenu::with_items(app, t(lang, "tray_language"), true, &refs)
}

fn mic_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
    let s = state().settings();
    let lang = &s.ui_language;
    let mut items: Vec<CheckMenuItem<Wry>> = Vec::new();
    items.push(CheckMenuItem::with_id(app, "mic:", t(lang, "tray_default_mic"), true, s.microphone.is_none(), None::<&str>)?);
    for d in mic::list_devices() {
        let checked = s.microphone.as_deref() == Some(d.id.as_str());
        items.push(CheckMenuItem::with_id(app, format!("mic:{}", d.id), d.name, true, checked, None::<&str>)?);
    }
    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    Submenu::with_items(app, t(lang, "tray_mic"), true, &refs)
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let st = state();
    let s = st.settings();
    let lang = &s.ui_language;
    let status = *STATUS.lock().unwrap();
    let last = LAST.lock().unwrap().clone();
    let last_item = if last.is_empty() {
        MenuItem::with_id(app, "last", t(lang, "tray_none"), false, None::<&str>)?
    } else {
        MenuItem::with_id(app, "last", format!("„{}“", truncate(&last, 60)), true, None::<&str>)?
    };
    let dictate_label = if status == "recording" { t(lang, "tray_stop") } else { t(lang, "tray_dictate") };
    let shortcut = s.shortcut_dictate.replace("Opt", "Alt").replace("Cmd", "Win");
    let dictate = MenuItem::with_id(app, "dictate", format!("{dictate_label}\t{shortcut}"), status != "meeting", None::<&str>)?;
    let meeting_label = if st.meeting.is_active() { t(lang, "tray_meeting_stop") } else { t(lang, "tray_meeting_start") };
    let meeting = MenuItem::with_id(app, "meeting", meeting_label, true, None::<&str>)?;
    let file = MenuItem::with_id(app, "file", t(lang, "tray_file"), true, None::<&str>)?;
    let ptt = CheckMenuItem::with_id(app, "ptt", t(lang, "tray_ptt"), true, s.activation == Activation::PushToTalk, None::<&str>)?;
    let enhance = CheckMenuItem::with_id(app, "enhance", t(lang, "tray_enhance"), true, s.enhance_mode == EnhanceMode::Always, None::<&str>)?;
    let floating = CheckMenuItem::with_id(app, "floating", t(lang, "tray_floating"), true, s.show_floating_icon, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", t(lang, "tray_settings"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", t(lang, "tray_quit"), true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    Menu::with_items(
        app,
        &[
            &last_item,
            &sep()?,
            &dictate,
            &meeting,
            &file,
            &sep()?,
            &language_menu(app)?,
            &mic_menu(app)?,
            &ptt,
            &enhance,
            &floating,
            &sep()?,
            &settings,
            &quit,
        ],
    )
}

/// Rechtsklick-Menü des Floating Icons.
pub fn popup_context_menu() {
    let app = state().app.clone();
    let _ = state().app.run_on_main_thread(move || {
        let st = state();
        let s = st.settings();
        let lang = &s.ui_language;
        let build = || -> tauri::Result<Menu<Wry>> {
            let ptt = CheckMenuItem::with_id(&app, "ptt", t(lang, "tray_ptt"), true, s.activation == Activation::PushToTalk, None::<&str>)?;
            let enhance = CheckMenuItem::with_id(&app, "enhance", t(lang, "tray_enhance"), true, s.enhance_mode == EnhanceMode::Always, None::<&str>)?;
            let file = MenuItem::with_id(&app, "file", t(lang, "tray_file"), true, None::<&str>)?;
            let hide = MenuItem::with_id(&app, "hide_icon", t(lang, "tray_hide_icon"), true, None::<&str>)?;
            let settings = MenuItem::with_id(&app, "settings", t(lang, "tray_settings"), true, None::<&str>)?;
            let sep = || PredefinedMenuItem::separator(&app);
            Menu::with_items(
                &app,
                &[&language_menu(&app)?, &mic_menu(&app)?, &ptt, &enhance, &sep()?, &file, &hide, &sep()?, &settings],
            )
        };
        if let (Some(w), Ok(menu)) = (app.get_webview_window(crate::overlay::LABEL), build()) {
            let _ = w.popup_menu(&menu);
        }
    });
}

pub fn handle_menu(app: &AppHandle, id: &str) {
    let st = state();
    match id {
        "last" => {
            let text = LAST.lock().unwrap().clone();
            let _ = crate::win::clipboard::set_text(&text, false);
        }
        "dictate" => st.dictation.toggle(Mode::Plain),
        "meeting" => crate::meeting::toggle(),
        "file" => pick_files(app.clone()),
        "ptt" => {
            st.update_settings(|s| {
                s.activation = if s.activation == Activation::PushToTalk { Activation::Toggle } else { Activation::PushToTalk };
            });
            rebuild();
        }
        "enhance" => {
            st.update_settings(|s| {
                s.enhance_mode = if s.enhance_mode == EnhanceMode::Always {
                    if s.shortcut_dictate_enhance.trim().is_empty() { EnhanceMode::Off } else { EnhanceMode::SecondShortcut }
                } else {
                    EnhanceMode::Always
                };
            });
            rebuild();
        }
        "floating" | "hide_icon" => {
            st.update_settings(|s| s.show_floating_icon = if id == "hide_icon" { false } else { !s.show_floating_icon });
            crate::overlay::refresh();
            rebuild();
        }
        "settings" => crate::show_main_window(None),
        "quit" => {
            st.enhancer.llama.stop();
            app.exit(0);
        }
        other => {
            if let Some(code) = other.strip_prefix("lang:") {
                st.update_settings(|s| s.language = code.to_string());
                rebuild();
            } else if let Some(mic_id) = other.strip_prefix("mic:") {
                st.update_settings(|s| s.microphone = (!mic_id.is_empty()).then(|| mic_id.to_string()));
                rebuild();
            }
        }
    }
}

pub fn pick_files(app: AppHandle) {
    use tauri_plugin_dialog::DialogExt;
    std::thread::spawn(move || {
        let picked = app
            .dialog()
            .file()
            .add_filter("Audio/Video", crate::audio::decode::MEDIA_EXTENSIONS)
            .blocking_pick_files();
        if let Some(files) = picked {
            let paths = files.into_iter().filter_map(|f| f.into_path().ok()).collect();
            crate::files::enqueue(paths);
        }
    });
}
