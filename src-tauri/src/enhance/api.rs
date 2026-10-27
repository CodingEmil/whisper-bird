//! Minimaler Client für OpenAI-kompatible Chat-APIs (llama-server, Ollama,
//! LM Studio, OpenAI …).

use serde_json::{json, Value};
use std::time::Duration;

pub struct ChatRequest<'a> {
    pub base_url: &'a str,
    pub api_key: Option<&'a str>,
    pub model: &'a str,
    pub system: &'a str,
    pub user: &'a str,
    pub max_tokens: u32,
    pub timeout: Duration,
    /// Thinking über das Chat-Template abschalten (llama-server).
    pub disable_thinking: bool,
}

pub async fn chat(req: ChatRequest<'_>) -> anyhow::Result<String> {
    let url = format!("{}/chat/completions", req.base_url.trim_end_matches('/'));
    let mut body = json!({
        "model": req.model,
        "messages": [
            { "role": "system", "content": req.system },
            { "role": "user", "content": req.user },
        ],
        "temperature": 0.2,
        "max_tokens": req.max_tokens,
        "stream": false,
    });
    if req.disable_thinking {
        body["chat_template_kwargs"] = json!({ "enable_thinking": false });
    }
    let client = reqwest::Client::builder().timeout(req.timeout).build()?;
    let mut request = client.post(&url).json(&body);
    if let Some(key) = req.api_key.filter(|k| !k.is_empty()) {
        request = request.bearer_auth(key);
    }
    let response = request.send().await.map_err(|e| {
        if e.is_timeout() {
            anyhow::anyhow!("Zeitüberschreitung")
        } else if e.is_connect() {
            anyhow::anyhow!("Keine Verbindung zu {}", req.base_url)
        } else {
            anyhow::anyhow!("{e}")
        }
    })?;
    let status = response.status();
    let value: Value = response.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let msg = value["error"]["message"].as_str().unwrap_or("unbekannter Fehler");
        anyhow::bail!("API-Fehler {status}: {msg}");
    }
    let content = value["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Antwort ohne Text"))?;
    Ok(content.to_string())
}
