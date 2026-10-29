//! Texte, die das Backend selbst anzeigt (Tray, Overlay-Meldungen).

pub fn t(lang: &str, key: &str) -> String {
    let de = lang == "de";
    let s = match key {
        "no_model" => if de { "Kein Spracherkennungsmodell installiert – bitte unter „Modelle“ laden." } else { "No speech model installed – download one under “Models”." },
        "enhance_failed" => if de { "Enhance fehlgeschlagen – Rohtext eingefügt" } else { "Enhance failed – inserted plain transcription" },
        "enhance_not_ready" => if de { "Enhance ist nicht eingerichtet – Rohtext eingefügt" } else { "Enhance is not set up – inserted plain transcription" },
        "mic_failed" => if de { "Mikrofon-Fehler" } else { "Microphone error" },
        "meeting_running" => if de { "Während eines Meetings ist kein Diktat möglich" } else { "Dictation is unavailable during a meeting" },
        "insert_failed" => if de { "Einfügen fehlgeschlagen" } else { "Could not insert text" },
        "transcribe_failed" => if de { "Transkription fehlgeschlagen" } else { "Transcription failed" },
        "tray_dictate" => if de { "Diktat starten" } else { "Start dictation" },
        "tray_stop" => if de { "Diktat beenden" } else { "Stop dictation" },
        "tray_meeting_start" => if de { "Meeting aufnehmen" } else { "Record meeting" },
        "tray_meeting_stop" => if de { "Meeting beenden" } else { "Stop meeting" },
        "tray_file" => if de { "Datei transkribieren …" } else { "Transcribe file …" },
        "tray_language" => if de { "Sprache" } else { "Language" },
        "tray_auto" => if de { "Automatisch" } else { "Automatic" },
        "tray_mic" => if de { "Mikrofon" } else { "Microphone" },
        "tray_default_mic" => if de { "Systemstandard" } else { "System default" },
        "tray_ptt" => "Push-to-talk",
        "tray_floating" => if de { "Floating Icon anzeigen" } else { "Show floating icon" },
        "tray_enhance" => if de { "Enhance immer an" } else { "Always enhance" },
        "tray_settings" => if de { "Einstellungen …" } else { "Settings …" },
        "tray_update" => if de { "Update verfügbar" } else { "Update available" },
        "update_busy" => if de { "Bitte erst Diktat oder Meeting beenden." } else { "Please finish the dictation or meeting first." },
        "tray_quit" => if de { "Whisper Bird beenden" } else { "Quit Whisper Bird" },
        "tray_none" => if de { "Noch keine Transkription" } else { "No transcription yet" },
        "tray_copy_hint" => if de { "Klicken zum Kopieren" } else { "Click to copy" },
        "tray_hide_icon" => if de { "Floating Icon ausblenden" } else { "Hide floating icon" },
        "files_done" => if de { "Fertig" } else { "Done" },
        "files_failed" => if de { "Datei konnte nicht transkribiert werden" } else { "Could not transcribe file" },
        "speaker" => if de { "Sprecher" } else { "Speaker" },
        "meeting_title" => if de { "Meeting" } else { "Meeting" },
        _ => key,
    };
    s.to_string()
}

/// Sprachen für Tray- und Kontextmenü (Code, Anzeigename).
pub const MENU_LANGUAGES: &[(&str, &str)] = &[
    ("de", "Deutsch"),
    ("en", "English"),
    ("fr", "Français"),
    ("es", "Español"),
    ("it", "Italiano"),
    ("nl", "Nederlands"),
    ("pl", "Polski"),
    ("pt", "Português"),
    ("tr", "Türkçe"),
    ("uk", "Українська"),
    ("ru", "Русский"),
    ("zh", "中文"),
    ("ja", "日本語"),
    ("ko", "한국어"),
];
