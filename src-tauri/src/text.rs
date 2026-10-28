//! Nachbearbeitung von Transkripten: Aufräumen, Custom Words, Snippets.

use crate::settings::Snippet;
use regex::Regex;
use std::sync::OnceLock;

/// Typische Whisper-Halluzinationen bei Stille oder Musik.
const HALLUCINATIONS: &[&str] = &[
    "untertitel der amara.org-community",
    "untertitel im auftrag des zdf",
    "untertitelung des zdf",
    "vielen dank fürs zuschauen",
    "vielen dank für's zuschauen",
    "danke fürs zuschauen",
    "thank you for watching",
    "thanks for watching",
    "please subscribe",
    "subtitles by the amara.org community",
    "[musik]",
    "[music]",
    "(musik)",
    "(music)",
    "[applaus]",
    "[blank_audio]",
    "[silence]",
];

fn filler_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Kein „um“: im Deutschen ein echtes Wort („um 5 Uhr“).
        Regex::new(r"(?i)(^|\s)(?:äh+m*|öh+m*|hm+|mhm|uh+m*|uhm+|erm+)[,.]?(\s|$)").unwrap()
    })
}

fn spaces_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[ \t]{2,}").unwrap())
}

fn space_before_punct_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+([,.;:!?])").unwrap())
}

/// Bereinigt ein Roh-Transkript.
pub fn clean(text: &str, remove_fillers: bool) -> String {
    let mut t = text.replace(['\r', '\n'], " ").trim().to_string();
    let lower = t.to_lowercase();
    let stripped = lower.trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());
    if HALLUCINATIONS.iter().any(|h| stripped == h.trim_matches(|c: char| c.is_ascii_punctuation())) {
        return String::new();
    }
    for h in HALLUCINATIONS {
        if let Some(pos) = t.to_lowercase().find(h) {
            // Nur am Ende entfernen – mitten im Text kann es echt sein.
            if pos > 0 && pos + h.len() >= t.len().saturating_sub(2) && t.is_char_boundary(pos) {
                t.truncate(pos);
            }
        }
    }
    if remove_fillers {
        // Mehrfach anwenden, weil aufeinanderfolgende Treffer sich das Leerzeichen teilen.
        for _ in 0..3 {
            t = filler_re().replace_all(&t, "$1").to_string();
        }
        t = t.replace(" ,", ",").replace(",,", ",");
        t = t.trim_start_matches([',', ' ']).to_string();
    }
    t = spaces_re().replace_all(&t, " ").to_string();
    t = space_before_punct_re().replace_all(&t, "$1").to_string();
    let t = t.trim().to_string();
    capitalize_first(&t)
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_lowercase() => c.to_uppercase().collect::<String>() + chars.as_str(),
        _ => s.to_string(),
    }
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Ersetzt ähnlich klingende Schreibweisen durch die Custom Words
/// („Kubernetis“ → „Kubernetes“, „whisper bird“ → „Whisper Bird“).
pub fn apply_custom_words(text: &str, words: &[String]) -> String {
    if words.is_empty() || text.is_empty() {
        return text.to_string();
    }
    let mut tokens: Vec<String> = text.split(' ').map(|s| s.to_string()).collect();
    for word in words {
        let target_tokens: Vec<&str> = word.split_whitespace().collect();
        let n = target_tokens.len();
        if n == 0 || n > tokens.len() {
            continue;
        }
        let target_norm = normalize(word);
        if target_norm.chars().count() < 3 {
            continue;
        }
        let mut i = 0;
        while i + n <= tokens.len() {
            let window = tokens[i..i + n].join(" ");
            // Satzzeichen am Rand erhalten.
            let lead: String = window.chars().take_while(|c| !c.is_alphanumeric()).collect();
            let trail: String = window
                .chars()
                .rev()
                .take_while(|c| !c.is_alphanumeric())
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let cand_norm = normalize(&window);
            if cand_norm.is_empty() {
                i += 1;
                continue;
            }
            let sim = strsim::jaro_winkler(&cand_norm, &target_norm);
            let target_len = target_norm.chars().count();
            let len_ratio = cand_norm.chars().count() as f64 / target_len as f64;
            // Kurze Wörter brauchen fast exakte Treffer („Email“ ≠ „Emil“).
            let threshold = match target_len {
                0..=5 => 0.97,
                6..=8 => 0.94,
                _ => 0.90,
            };
            let core = window.trim_matches(|c: char| !c.is_alphanumeric());
            if sim >= threshold && (0.8..=1.25).contains(&len_ratio) && core != word {
                let replacement = format!("{lead}{word}{trail}");
                tokens.splice(i..i + n, std::iter::once(replacement));
                i += 1;
                continue;
            }
            i += 1;
        }
    }
    tokens.join(" ")
}

/// Ergebnis der Snippet-Expansion.
pub enum SnippetMatch {
    /// Das ganze Diktat war ein Trigger: Snippet-Text unverändert einfügen.
    Full(String),
    /// Trigger kamen im Text vor und wurden ersetzt (oder keine Treffer).
    Inline(String),
}

pub fn expand_snippets(text: &str, snippets: &[Snippet]) -> SnippetMatch {
    let norm = normalize(text);
    for s in snippets {
        if !s.trigger.trim().is_empty() && normalize(&s.trigger) == norm {
            return SnippetMatch::Full(s.text.clone());
        }
    }
    let mut out = text.to_string();
    for s in snippets {
        let trigger = s.trigger.trim();
        if trigger.split_whitespace().count() < 2 {
            // Einzelwörter nur als komplettes Diktat, sonst zu viele Fehltreffer.
            continue;
        }
        let pattern = trigger
            .split_whitespace()
            .map(regex::escape)
            .collect::<Vec<_>>()
            .join(r"[\s,]+");
        if let Ok(re) = Regex::new(&format!(r"(?i)\b{pattern}\b[.!?]?")) {
            out = re.replace_all(&out, regex::NoExpand(&s.text)).to_string();
        }
    }
    SnippetMatch::Inline(out)
}

/// Ersetzt Snippet-Trigger durch Platzhalter ⟦1⟧, ⟦2⟧ …, damit Enhance sie
/// nicht umformuliert. Gibt den Text und die Snippet-Inhalte zurück.
pub fn protect_snippets(text: &str, snippets: &[Snippet]) -> (String, Vec<String>) {
    let mut out = text.to_string();
    let mut parts = Vec::new();
    for s in snippets {
        let trigger = s.trigger.trim();
        if trigger.split_whitespace().count() < 2 {
            continue;
        }
        let pattern = trigger
            .split_whitespace()
            .map(regex::escape)
            .collect::<Vec<_>>()
            .join(r"[\s,]+");
        if let Ok(re) = Regex::new(&format!(r"(?i)\b{pattern}\b[.!?]?")) {
            if re.is_match(&out) {
                parts.push(s.text.clone());
                let placeholder = format!("⟦{}⟧", parts.len());
                out = re.replace_all(&out, regex::NoExpand(&placeholder)).to_string();
            }
        }
    }
    (out, parts)
}

pub fn restore_placeholders(text: &str, parts: &[String]) -> String {
    let mut out = text.to_string();
    for (i, part) in parts.iter().enumerate() {
        out = out.replace(&format!("⟦{}⟧", i + 1), part);
    }
    out
}

pub fn word_count(text: &str) -> u32 {
    text.split_whitespace()
        .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_fillers_and_hallucinations() {
        assert_eq!(clean("äh, ich wollte ähm sagen", true), "Ich wollte sagen");
        assert_eq!(clean("Untertitel der Amara.org-Community", true), "");
        assert_eq!(clean("hallo welt .", false), "Hallo welt.");
    }

    #[test]
    fn custom_words_fix_spelling() {
        let words = vec!["Kubernetes".to_string(), "Whisper Bird".to_string()];
        assert_eq!(
            apply_custom_words("Wir nutzen Kubernetis und whisper bird.", &words),
            "Wir nutzen Kubernetes und Whisper Bird."
        );
        assert_eq!(apply_custom_words("Das ist gut.", &words), "Das ist gut.");
    }

    #[test]
    fn snippets_expand() {
        let snippets = vec![Snippet { trigger: "meine adresse".into(), text: "Musterstr. 1\n12345 Stadt".into() }];
        match expand_snippets("Meine Adresse.", &snippets) {
            SnippetMatch::Full(t) => assert!(t.contains("12345")),
            _ => panic!(),
        }
        match expand_snippets("Bitte an meine Adresse schicken", &snippets) {
            SnippetMatch::Inline(t) => assert!(t.contains("Musterstr. 1\n12345 Stadt schicken")),
            _ => panic!(),
        }
    }
}
