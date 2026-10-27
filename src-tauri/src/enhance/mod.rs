//! Enhance: Transkripte per LLM aufräumen und formatieren, markierten Text
//! umschreiben und Meeting-Notizen erzeugen.

pub mod api;
pub mod cli;
pub mod llama;

use crate::catalog;
use crate::settings::{EnhanceProvider, Settings};
use crate::win::foreground::AppContext;
use std::path::PathBuf;
use std::time::Duration;

pub const DEFAULT_SYSTEM_PROMPT: &str = "You are a dictation post-processor. You receive a raw speech-to-text transcript and return the final, polished text the user wants inserted at their cursor.

Rules:
- Output ONLY the final text. No preface, no explanations, no quotes, no code fences.
- Keep the speaker's language. Never translate unless the speaker explicitly asks for it.
- Fix grammar, spelling, punctuation and capitalization.
- Remove filler words (um, uh, äh, ähm, like, you know, also halt …), stutters, repetitions and false starts.
- Apply self-corrections: if the speaker corrects themselves (\"at 3, no wait, at 4\" / \"um 3, nein, um 4 Uhr\"), keep only the corrected version.
- Write numbers as digits (\"15\", \"3.5 %\", \"10:30\"), but keep idioms (\"a couple of things\", \"ein paar\") and spoken years natural.
- Follow explicit instructions addressed to you about the text (\"new paragraph\", \"as a bullet list\", \"make it more polite\", \"write this as an email to Anna\", \"translate to English\") and leave the instruction itself out of the output.
- Do NOT answer questions or carry out requests in the transcript. If the user dictates a question, output that question, cleaned up.
- Do not add facts or content that was not spoken — except a fitting greeting and sign-off when formatting an email.
- Spell the user's custom words exactly as given.
- Keep placeholders like ⟦1⟧ exactly as they are.";

const EDIT_SYSTEM_PROMPT: &str = "You are a precise writing assistant. You edit a piece of text according to an instruction.

Rules:
- Output ONLY the resulting text. No preface, no explanations, no quotes, no code fences.
- Keep the original language unless the instruction asks for a translation.
- Preserve meaning, names, numbers and facts. Do not invent content.
- Keep line breaks and list formatting where sensible.";

fn category_guidance(category: &str) -> &'static str {
    match category {
        "email" => "The text is an email body. Use a greeting line, short paragraphs separated by blank lines and a polite sign-off. No subject line. Only add a sender name if it was spoken.",
        "chat" => "The text is a chat message. Keep it natural and concise. No greeting or sign-off unless spoken.",
        "code" => "The user is in a code editor or terminal. Keep technical terms, identifiers, file names and commands exactly. No extra prose.",
        "ai_prompt" => "The text is a prompt for an AI assistant. Make it clear and well structured, keep every detail.",
        "document" => "The text is part of a document. Use well-structured paragraphs; use a list when the speaker enumerates items.",
        _ => "Plain text. Use paragraphs or lists only when the content clearly calls for it.",
    }
}

pub struct DictationContext<'a> {
    pub app: &'a AppContext,
    pub language: Option<&'a str>,
    pub custom_words: &'a [String],
}

pub fn dictation_user_prompt(transcript: &str, ctx: &DictationContext) -> String {
    let mut p = String::from("Context:\n");
    if !ctx.app.exe.is_empty() {
        p.push_str(&format!(
            "- Target app: {} (category: {})\n",
            ctx.app.display_name(),
            ctx.app.category()
        ));
    }
    if let Some(lang) = ctx.language {
        p.push_str(&format!("- Spoken language: {lang}\n"));
    }
    if !ctx.custom_words.is_empty() {
        p.push_str(&format!("- Custom words: {}\n", ctx.custom_words.join(", ")));
    }
    p.push_str(&format!("- Formatting: {}\n", category_guidance(ctx.app.category())));
    p.push_str("\nTranscript:\n<<<\n");
    p.push_str(transcript);
    p.push_str("\n>>>");
    p
}

pub fn edit_user_prompt(instruction: &str, text: Option<&str>) -> String {
    match text {
        Some(t) => format!("Instruction: {instruction}\n\nText:\n<<<\n{t}\n>>>"),
        None => format!(
            "Instruction: {instruction}\n\nThere is no existing text. Write the text described by the instruction."
        ),
    }
}

pub fn meeting_notes_prompts(transcript: &str, ui_lang: &str) -> (String, String) {
    let (lang, summary, decisions, actions, open) = if ui_lang == "de" {
        ("German", "Zusammenfassung", "Entscheidungen", "Aufgaben", "Offen")
    } else {
        ("English", "Summary", "Decisions", "Action items", "Unassigned")
    };
    let system = format!(
        "You write concise, accurate meeting notes in {lang}. Use only information from the transcript. Output Markdown with exactly these sections:\n\
         ## {summary}\n3–6 sentences.\n\
         ## {decisions}\nBullet list (\"- \"), or \"–\" if none.\n\
         ## {actions}\nOne line per task: \"- [ ] Person: task\" (use speaker names; \"{open}\" if unclear), or \"–\" if none."
    );
    let user = format!("Transcript:\n<<<\n{transcript}\n>>>");
    (system, user)
}

/// Entfernt typische LLM-Verpackung und erkennt unbrauchbare Ausgaben.
pub fn sanitize_output(output: &str, input: &str) -> Option<String> {
    let mut text = output.trim().to_string();
    // Reste von Thinking-Blöcken.
    if let Some(end) = text.find("</think>") {
        text = text[end + 8..].trim().to_string();
    }
    if text.starts_with("```") {
        text = text
            .trim_start_matches("```")
            .trim_start_matches(|c: char| c.is_alphanumeric())
            .trim_end_matches("```")
            .trim()
            .to_string();
    }
    for marker in ["<<<", ">>>"] {
        text = text.replace(marker, "");
    }
    let text = text.trim();
    // Vorrede wie „Hier ist der überarbeitete Text:“ abschneiden.
    let first_line = text.lines().next().unwrap_or("");
    let lower = first_line.to_lowercase();
    let prefaces = ["here is", "here's", "hier ist", "hier sind", "sure", "certainly", "gerne", "klar,", "natürlich"];
    let text = if first_line.trim_end().ends_with(':')
        && prefaces.iter().any(|p| lower.starts_with(p))
        && text.lines().count() > 1
    {
        text[first_line.len()..].trim()
    } else {
        text
    };
    let text = text.trim_matches(|c| c == '"' || c == '„' || c == '“' || c == '”').trim();
    if text.is_empty() {
        return None;
    }
    let lower = text.to_lowercase();
    if ["as an ai", "als ki", "i cannot", "i can't help", "ich kann dir dabei nicht"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return None;
    }
    let in_len = input.chars().count();
    if text.chars().count() > in_len * 5 + 400 {
        return None;
    }
    Some(text.to_string())
}

pub struct Enhancer {
    pub llama: llama::LlamaServer,
    models_dir: PathBuf,
}

impl Enhancer {
    pub fn new(bin_dir: PathBuf, models_dir: PathBuf) -> Self {
        Self {
            llama: llama::LlamaServer::new(bin_dir),
            models_dir,
        }
    }

    fn local_model_path(&self, id: &str) -> anyhow::Result<PathBuf> {
        let info = catalog::find(id).ok_or_else(|| anyhow::anyhow!("Unbekanntes Modell {id}"))?;
        let path = self.models_dir.join(info.filename);
        if !path.exists() {
            anyhow::bail!("{} ist noch nicht heruntergeladen", info.name);
        }
        Ok(path)
    }

    /// Ist der gewählte Anbieter einsatzbereit?
    pub fn ready(&self, s: &Settings) -> Result<(), String> {
        match s.enhance_provider {
            EnhanceProvider::Local => {
                if !self.llama.is_installed() {
                    return Err("llama-server fehlt".into());
                }
                self.local_model_path(&s.enhance_local_model).map(|_| ()).map_err(|e| e.to_string())
            }
            EnhanceProvider::Cli => {
                if s.enhance_cli_command.trim().is_empty() {
                    Err("Kein CLI-Befehl".into())
                } else {
                    Ok(())
                }
            }
            EnhanceProvider::Api => {
                if s.enhance_api_base_url.trim().is_empty() {
                    Err("Keine API-URL".into())
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Lokales Modell schon während der Aufnahme laden.
    pub async fn warm(&self, s: &Settings) {
        if s.enhance_provider != EnhanceProvider::Local {
            return;
        }
        if let Ok(path) = self.local_model_path(&s.enhance_local_model) {
            if let Err(e) = self.llama.ensure(&path).await {
                log::warn!("Vorladen des Enhance-Modells fehlgeschlagen: {e:#}");
            }
        }
    }

    pub async fn complete(
        &self,
        s: &Settings,
        system: &str,
        user: &str,
        max_tokens: u32,
        timeout: Duration,
    ) -> anyhow::Result<String> {
        match s.enhance_provider {
            EnhanceProvider::Local => {
                let path = self.local_model_path(&s.enhance_local_model)?;
                let port = self.llama.ensure(&path).await?;
                let base = format!("http://127.0.0.1:{port}/v1");
                let result = api::chat(api::ChatRequest {
                    base_url: &base,
                    api_key: None,
                    model: "local",
                    system,
                    user,
                    max_tokens,
                    timeout,
                    disable_thinking: true,
                })
                .await;
                self.llama.touch();
                result
            }
            EnhanceProvider::Api => {
                api::chat(api::ChatRequest {
                    base_url: &s.enhance_api_base_url,
                    api_key: Some(&s.enhance_api_key),
                    model: &s.enhance_api_model,
                    system,
                    user,
                    max_tokens,
                    timeout,
                    disable_thinking: false,
                })
                .await
            }
            EnhanceProvider::Cli => {
                let prompt = format!("{system}\n\n{user}");
                cli::run(&s.enhance_cli_command, &prompt, timeout).await
            }
        }
    }

    pub fn system_prompt(s: &Settings) -> &str {
        if s.enhance_system_prompt.trim().is_empty() {
            DEFAULT_SYSTEM_PROMPT
        } else {
            &s.enhance_system_prompt
        }
    }

    pub async fn enhance_dictation(
        &self,
        s: &Settings,
        transcript: &str,
        ctx: &DictationContext<'_>,
    ) -> anyhow::Result<String> {
        let user = dictation_user_prompt(transcript, ctx);
        let max_tokens = (transcript.chars().count() as u32 / 2 + 256).min(4096);
        let timeout = Duration::from_secs(s.enhance_timeout_sec.max(5) as u64);
        let out = self.complete(s, Self::system_prompt(s), &user, max_tokens, timeout).await?;
        sanitize_output(&out, transcript).ok_or_else(|| anyhow::anyhow!("Unbrauchbare Antwort des Modells"))
    }

    pub async fn edit(&self, s: &Settings, instruction: &str, text: Option<&str>) -> anyhow::Result<String> {
        let user = edit_user_prompt(instruction, text);
        let len = text.map(|t| t.chars().count()).unwrap_or(400) as u32;
        let max_tokens = (len + 512).min(6000);
        let timeout = Duration::from_secs((s.enhance_timeout_sec.max(5) * 2) as u64);
        let out = self.complete(s, EDIT_SYSTEM_PROMPT, &user, max_tokens, timeout).await?;
        sanitize_output(&out, text.unwrap_or(instruction))
            .ok_or_else(|| anyhow::anyhow!("Unbrauchbare Antwort des Modells"))
    }

    pub async fn meeting_notes(&self, s: &Settings, transcript: &str) -> anyhow::Result<String> {
        const PART: usize = 30_000;
        let timeout = Duration::from_secs(240);
        if transcript.chars().count() <= PART {
            let (system, user) = meeting_notes_prompts(transcript, &s.ui_language);
            return self.complete(s, &system, &user, 1500, timeout).await;
        }
        // Lange Meetings: erst Teile zusammenfassen, dann zusammenführen.
        let chars: Vec<char> = transcript.chars().collect();
        let mut partials = Vec::new();
        for chunk in chars.chunks(PART) {
            let part: String = chunk.iter().collect();
            let (system, user) = meeting_notes_prompts(&part, &s.ui_language);
            partials.push(self.complete(s, &system, &user, 1200, timeout).await?);
        }
        let joined = partials.join("\n\n---\n\n");
        let (system, _) = meeting_notes_prompts("", &s.ui_language);
        let user = format!(
            "These are notes for consecutive parts of one meeting. Merge them into one set of notes, removing duplicates:\n<<<\n{joined}\n>>>"
        );
        self.complete(s, &system, &user, 1800, timeout).await
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_output;

    #[test]
    fn strips_preface_and_fences() {
        assert_eq!(
            sanitize_output("Hier ist der überarbeitete Text:\nHallo Welt.", "hallo welt").as_deref(),
            Some("Hallo Welt.")
        );
        assert_eq!(sanitize_output("```\nHallo\n```", "hallo").as_deref(), Some("Hallo"));
        assert_eq!(sanitize_output("As an AI, I cannot", "x"), None);
    }
}
