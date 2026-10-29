mod asr;
mod audio;
mod catalog;
mod commands;
mod dictation;
mod downloads;
mod editmenu;
mod enhance;
mod files;
mod history;
mod hotkeys;
mod i18n;
mod insert;
mod meeting;
mod overlay;
mod settings;
mod state;
mod text;
mod tray;
mod updates;
mod win;

use state::{state, Dirs};
use std::time::Duration;
use tauri::{Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

pub const MAIN_LABEL: &str = "main";

/// Öffnet (oder erzeugt) das Einstellungsfenster, optional auf einer Seite.
pub fn show_main_window(page: Option<&str>) {
    let app = state().app.clone();
    let page = page.map(str::to_string);
    let _ = state().app.run_on_main_thread(move || {
        let window = match app.get_webview_window(MAIN_LABEL) {
            Some(w) => w,
            None => match WebviewWindowBuilder::new(&app, MAIN_LABEL, WebviewUrl::App("index.html".into()))
                .title("Whisper Bird")
                .inner_size(1100.0, 760.0)
                .min_inner_size(900.0, 600.0)
                .center()
                .theme(Some(tauri::Theme::Dark))
                // Erst nach dem Einfärben der Titelleiste zeigen (show() unten).
                .visible(false)
                .build()
            {
                Ok(w) => {
                    #[cfg(windows)]
                    style_title_bar(&w);
                    w
                }
                Err(e) => {
                    log::error!("Hauptfenster konnte nicht erstellt werden: {e}");
                    return;
                }
            },
        };
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        if let Some(page) = page {
            let _ = window.emit_to(MAIN_LABEL, "navigate", page);
        }
    });
}

/// Titelleiste in den Farben der Seitenleiste (--color-chrome, Schrift --color-ink).
/// Windows 11 übernimmt die Farben, Windows 10 zeigt über Theme::Dark die dunkle Leiste.
#[cfg(windows)]
fn style_title_bar(window: &tauri::WebviewWindow) {
    use windows::Win32::Foundation::{COLORREF, HWND};
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR};
    let Ok(h) = window.hwnd() else { return };
    // COLORREF ist 0x00BBGGRR.
    let rgb = |r: u32, g: u32, b: u32| COLORREF(r | g << 8 | b << 16);
    for (attr, color) in [(DWMWA_CAPTION_COLOR, rgb(20, 34, 39)), (DWMWA_TEXT_COLOR, rgb(241, 236, 227))] {
        unsafe {
            let _ = DwmSetWindowAttribute(
                HWND(h.0 as *mut _),
                attr,
                &color as *const COLORREF as *const _,
                std::mem::size_of::<COLORREF>() as u32,
            );
        }
    }
}

fn background_tasks() {
    // Spracherkennungsmodell vorladen, damit das erste Diktat schnell ist.
    std::thread::spawn(|| {
        let st = state();
        if st.usable_asr_model().is_some() {
            if let Err(e) = st.load_asr() {
                log::warn!("Vorladen fehlgeschlagen: {e:#}");
            }
        }
        if st.model_installed(catalog::VAD_MODEL) {
            let _ = st.load_vad();
        }
    });
    // Modelle nach Leerlauf entladen.
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(30));
        let st = state();
        let s = st.settings();
        if s.unload_after_min > 0
            && st.asr.loaded_id().is_some()
            && !st.dictation.is_busy()
            && !st.meeting.is_active()
            && st.asr.idle_secs() > s.unload_after_min as u64 * 60
        {
            st.asr.unload();
        }
        if s.enhance_unload_after_min > 0
            && st.enhancer.llama.is_running()
            && !st.dictation.is_busy()
            && st.enhancer.llama.idle_secs() > s.enhance_unload_after_min as u64 * 60
        {
            st.enhancer.llama.stop();
        }
    });
}

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {
            show_main_window(None);
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("whisper-bird".into()),
                    }),
                ])
                .level(log::LevelFilter::Debug)
                .level_for("tao", log::LevelFilter::Warn)
                .level_for("wry", log::LevelFilter::Warn)
                .level_for("reqwest", log::LevelFilter::Warn)
                .level_for("hyper_util", log::LevelFilter::Warn)
                .level_for("symphonia_core", log::LevelFilter::Warn)
                .level_for("symphonia_format_isomp4", log::LevelFilter::Warn)
                .max_file_size(5_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_settings,
            commands::default_system_prompt,
            commands::default_edit_actions,
            commands::app_info,
            commands::list_models,
            commands::download_model,
            commands::cancel_download,
            commands::delete_model,
            commands::llama_status,
            commands::install_llama,
            commands::list_microphones,
            commands::play_test_sound,
            commands::start_shortcut_capture,
            commands::stop_shortcut_capture,
            commands::window_key,
            commands::window_blur,
            commands::dictation_toggle,
            commands::dictation_cancel,
            commands::overlay_context_menu,
            commands::show_settings,
            commands::history_query,
            commands::history_delete,
            commands::history_clear,
            commands::stats,
            commands::reset_stats,
            commands::copy_text,
            commands::enhance_test,
            commands::enhance_stop_local,
            commands::editmenu_run,
            commands::editmenu_apply,
            commands::editmenu_copy,
            commands::editmenu_close,
            commands::editmenu_voice,
            commands::files_pick,
            commands::files_pick_folder,
            commands::files_add,
            commands::files_list,
            commands::files_cancel,
            commands::files_clear,
            commands::meeting_start,
            commands::meeting_stop,
            commands::meeting_active,
            commands::meeting_list,
            commands::meeting_get,
            commands::meeting_delete,
            commands::meeting_update,
            commands::meeting_notes,
            commands::meeting_transcript,
            commands::save_text_file,
            commands::finish_onboarding,
            commands::open_data_dir,
            commands::open_notices,
            commands::update_status,
            commands::update_check,
            commands::update_install,
            commands::update_open_page,
            commands::update_notice_seen,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let data = app.path().app_data_dir()?;
            let dirs = Dirs {
                models: data.join("models"),
                llama: data.join("llama"),
                meetings: data.join("meetings"),
                logs: app.path().app_log_dir()?,
                data,
            };
            let st = state::init(handle.clone(), dirs)?;
            let s = st.settings();
            log::set_max_level(if s.debug_logging { log::LevelFilter::Debug } else { log::LevelFilter::Info });
            log::info!("Whisper Bird {} startet", app.package_info().version);

            // Maßgeblich ist Windows: Installer und Task-Manager ändern den Autostart an der App vorbei.
            {
                use tauri_plugin_autostart::ManagerExt;
                match app.autolaunch().is_enabled() {
                    Ok(enabled) if enabled != s.autostart => {
                        st.update_settings(|s| s.autostart = enabled);
                    }
                    Ok(_) => {}
                    Err(e) => log::warn!("Autostart-Status nicht lesbar: {e}"),
                }
            }

            overlay::create(&handle)?;
            editmenu::create(&handle)?;
            tray::create(&handle)?;
            app.on_menu_event(|app, event| tray::handle_menu(app, event.id().as_ref()));

            st.hotkeys.init()?;
            for e in st.hotkeys.apply() {
                log::warn!("Shortcut {}: {}", e.field, e.message);
            }

            let started_hidden = std::env::args().any(|a| a == "--hidden");
            if !s.onboarding_done || !(s.start_hidden || started_hidden) {
                show_main_window(if s.onboarding_done { None } else { Some("onboarding") });
            }
            overlay::set(overlay::Phase::Idle, None, false, None);
            background_tasks();
            updates::init();
            Ok(())
        })
        .on_window_event(|window, event| {
            // Hauptfenster schließen = nur verstecken; die App läuft im Tray weiter.
            if window.label() == MAIN_LABEL {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        });

    let app = builder
        .build(tauri::generate_context!())
        .expect("Fehler beim Starten von Whisper Bird");
    app.run(|_app, event| match event {
        // Ohne expliziten Exit-Code (letztes Fenster zu) im Tray weiterlaufen.
        RunEvent::ExitRequested { api, code, .. } if code.is_none() => api.prevent_exit(),
        RunEvent::Exit => updates::start_replacement(),
        _ => {}
    });
}
