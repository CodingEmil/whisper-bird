use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activation {
    /// Einmal drücken startet, nochmal drücken stoppt.
    Toggle,
    /// Gedrückt halten zum Sprechen, loslassen stoppt.
    PushToTalk,
    /// Kurz tippen = Toggle, lange halten = Push-to-talk.
    Hybrid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PasteMethod {
    /// Über die Zwischenablage und Strg+V.
    Clipboard,
    /// Zeichen für Zeichen tippen (langsamer, aber ohne Zwischenablage).
    Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnhanceMode {
    /// Nie verbessern.
    Off,
    /// Jedes Diktat wird verbessert.
    Always,
    /// Nur über den zweiten Shortcut „Diktieren + Enhance“.
    SecondShortcut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnhanceProvider {
    /// Gemma 4 lokal über llama-server.
    Local,
    /// Lokale CLI (Claude Code, Codex, Gemini CLI, eigener Befehl).
    Cli,
    /// OpenAI-kompatible API (Ollama, LM Studio, OpenAI …).
    Api,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayPosition {
    TopCenter,
    BottomCenter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetingSource {
    Microphone,
    System,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accelerator {
    Auto,
    Cpu,
    Gpu,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    pub trigger: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditAction {
    pub id: String,
    pub label: String,
    pub instruction: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ui_language: String,
    pub onboarding_done: bool,

    // Diktat
    pub asr_model: String,
    pub language: String,
    pub activation: Activation,
    pub shortcut_dictate: String,
    pub shortcut_dictate_enhance: String,
    pub shortcut_edit: String,
    pub shortcut_meeting: String,
    pub microphone: Option<String>,
    pub accelerator: Accelerator,
    pub unload_after_min: u32,

    // Overlay & Feedback
    pub show_floating_icon: bool,
    pub floating_pos: Option<(f64, f64)>,
    pub overlay_position: OverlayPosition,
    pub sounds: bool,
    pub sound_volume: f32,
    pub audio_ducking: bool,
    pub ducking_level: f32,

    // Einfügen
    pub paste_method: PasteMethod,
    /// Diktat bleibt nach dem Einfügen in der Zwischenablage; sonst kommt der alte Inhalt zurück.
    pub copy_to_clipboard: bool,
    /// Frühere Einstellung „Zwischenablage erhalten“ (umgekehrt), nur noch beim Laden übernommen.
    #[serde(skip_serializing)]
    preserve_clipboard: Option<bool>,
    pub trailing_space: bool,
    pub auto_enter_apps: Vec<String>,

    // Wörter & Snippets
    pub custom_words: Vec<String>,
    pub snippets: Vec<Snippet>,
    pub remove_fillers: bool,

    // Enhance
    pub enhance_mode: EnhanceMode,
    pub enhance_provider: EnhanceProvider,
    pub enhance_local_model: String,
    /// Lokales Enhance-Modell nach so vielen Minuten Leerlauf entladen; 0 = nie.
    pub enhance_unload_after_min: u32,
    /// Frühere Einstellung „Modell geladen halten“, nur noch beim Laden übernommen.
    #[serde(skip_serializing)]
    enhance_keep_loaded: Option<bool>,
    pub enhance_cli_preset: String,
    pub enhance_cli_command: String,
    pub enhance_api_base_url: String,
    pub enhance_api_key: String,
    pub enhance_api_model: String,
    pub enhance_system_prompt: String,
    pub enhance_timeout_sec: u32,

    // Kontextmenü „Text bearbeiten“
    pub edit_actions: Vec<EditAction>,
    pub translate_target: String,

    // Verlauf
    pub history_limit: u32,

    // Meetings & Dateien
    pub meeting_source: MeetingSource,
    pub meeting_auto_notes: bool,
    pub file_output_dir: Option<String>,

    // System
    pub autostart: bool,
    pub start_hidden: bool,
    /// Beim Start und täglich auf GitHub nach einer neuen Version suchen.
    pub check_updates: bool,
    /// Version, deren Update-Hinweis zuletzt gezeigt wurde; daran erkennt die App ein Update.
    pub last_version: String,
    pub debug_logging: bool,
}

pub const DEFAULT_ASR_MODEL: &str = "parakeet-ultra";
pub const DEFAULT_LLM_MODEL: &str = "gemma-4-e4b";

pub fn default_edit_actions(lang: &str) -> Vec<EditAction> {
    let de = lang == "de";
    let a = |id: &str, de_label: &str, en_label: &str, instruction: &str| EditAction {
        id: id.into(),
        label: if de { de_label } else { en_label }.into(),
        instruction: instruction.into(),
    };
    vec![
        a(
            "improve",
            "Schreibstil verbessern",
            "Improve writing",
            "Improve the writing: clearer, more fluent and natural, keep the meaning, tone and language.",
        ),
        a(
            "grammar",
            "Rechtschreibung & Grammatik",
            "Spelling & grammar",
            "Fix spelling, grammar and punctuation only. Change nothing else.",
        ),
        a(
            "shorten",
            "Kürzen",
            "Shorten",
            "Make the text noticeably shorter while keeping all key information.",
        ),
        a(
            "expand",
            "Erweitern",
            "Expand",
            "Expand the text with more detail and smoother transitions, without inventing facts.",
        ),
        a(
            "professional",
            "Professioneller",
            "Make professional",
            "Rewrite the text in a professional, polite business tone.",
        ),
        a(
            "casual",
            "Lockerer",
            "Make casual",
            "Rewrite the text in a relaxed, friendly, casual tone.",
        ),
        a(
            "bullets",
            "Stichpunkte",
            "Bullet list",
            "Convert the text into a concise bullet list using \"- \" bullets.",
        ),
        a(
            "todo",
            "To-do-Liste",
            "To-do list",
            "Convert the text into an actionable to-do list using \"- [ ] \" items.",
        ),
        a(
            "translate",
            "Übersetzen",
            "Translate",
            "Translate the text into {target}. Keep formatting.",
        ),
    ]
}

impl Default for Settings {
    fn default() -> Self {
        let ui_language = system_ui_language();
        Self {
            edit_actions: default_edit_actions(&ui_language),
            translate_target: if ui_language == "de" { "Englisch" } else { "German" }.into(),
            ui_language,
            onboarding_done: false,

            asr_model: DEFAULT_ASR_MODEL.into(),
            language: "auto".into(),
            activation: Activation::Toggle,
            shortcut_dictate: "Ctrl+Shift+Space".into(),
            shortcut_dictate_enhance: String::new(),
            shortcut_edit: "Ctrl+Shift+E".into(),
            shortcut_meeting: String::new(),
            microphone: None,
            accelerator: Accelerator::Auto,
            unload_after_min: 0,

            show_floating_icon: true,
            floating_pos: None,
            overlay_position: OverlayPosition::BottomCenter,
            sounds: true,
            sound_volume: 0.5,
            audio_ducking: false,
            ducking_level: 0.3,

            paste_method: PasteMethod::Clipboard,
            copy_to_clipboard: false,
            preserve_clipboard: None,
            trailing_space: true,
            auto_enter_apps: Vec::new(),

            custom_words: Vec::new(),
            snippets: Vec::new(),
            remove_fillers: true,

            enhance_mode: EnhanceMode::Off,
            enhance_provider: EnhanceProvider::Local,
            enhance_local_model: DEFAULT_LLM_MODEL.into(),
            enhance_unload_after_min: 5,
            enhance_keep_loaded: None,
            enhance_cli_preset: "claude".into(),
            enhance_cli_command: crate::enhance::cli::preset_command("claude").into(),
            enhance_api_base_url: "http://localhost:11434/v1".into(),
            enhance_api_key: String::new(),
            enhance_api_model: "gemma4:e4b".into(),
            enhance_system_prompt: String::new(),
            enhance_timeout_sec: 30,

            history_limit: 1000,

            meeting_source: MeetingSource::Both,
            meeting_auto_notes: true,
            file_output_dir: None,

            autostart: false,
            start_hidden: false,
            check_updates: true,
            last_version: String::new(),
            debug_logging: false,
        }
    }
}

fn system_ui_language() -> String {
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::GetUserDefaultUILanguage;
        // Primäre Sprach-ID 0x07 = Deutsch.
        let lang = unsafe { GetUserDefaultUILanguage() };
        if lang & 0x3ff == 0x07 {
            return "de".into();
        }
    }
    "en".into()
}

pub fn load(path: &Path) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(_) => Settings::default(),
    }
}

fn parse(text: &str) -> Settings {
    let mut settings: Settings = serde_json::from_str(text).unwrap_or_else(|e| {
        log::warn!("Einstellungen unlesbar ({e}), verwende Standardwerte");
        Settings::default()
    });
    // „Zwischenablage erhalten: aus“ hieß: Das Diktat bleibt in der Zwischenablage.
    if let Some(preserve) = settings.preserve_clipboard.take() {
        settings.copy_to_clipboard = !preserve;
    }
    // „Modell geladen halten: aus“ hieß: nach zwei Minuten entladen. „An“ war der
    // Standard und hielt Gemma dauerhaft im Grafikspeicher; dafür gilt jetzt der neue Standard.
    if settings.enhance_keep_loaded.take() == Some(false) {
        settings.enhance_unload_after_min = 2;
    }
    settings
}

pub fn save(path: &Path, settings: &Settings) -> anyhow::Result<()> {
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alte_zwischenablage_einstellung_wird_uebernommen() {
        assert!(parse(r#"{"preserve_clipboard": false}"#).copy_to_clipboard);
        assert!(!parse(r#"{"preserve_clipboard": true}"#).copy_to_clipboard);
        assert!(!parse("{}").copy_to_clipboard);
        let json = serde_json::to_string(&parse(r#"{"preserve_clipboard": false}"#)).unwrap();
        assert!(!json.contains("preserve_clipboard"));
        assert!(json.contains(r#""copy_to_clipboard":true"#));
    }

    #[test]
    fn enhance_modell_wird_nicht_mehr_dauerhaft_gehalten() {
        assert_eq!(parse(r#"{"enhance_keep_loaded": false}"#).enhance_unload_after_min, 2);
        assert_eq!(parse(r#"{"enhance_keep_loaded": true}"#).enhance_unload_after_min, 5);
        assert_eq!(parse("{}").enhance_unload_after_min, 5);
        assert_eq!(parse(r#"{"enhance_unload_after_min": 0}"#).enhance_unload_after_min, 0);
        let json = serde_json::to_string(&parse(r#"{"enhance_keep_loaded": false}"#)).unwrap();
        assert!(!json.contains("enhance_keep_loaded"));
    }
}
