//! Enhance über eine lokal installierte CLI (Claude Code, Codex, Gemini CLI
//! oder ein eigener Befehl). Der komplette Prompt geht über stdin hinein.

use std::time::Duration;
use tokio::io::AsyncWriteExt;

pub fn preset_command(preset: &str) -> &'static str {
    match preset {
        "claude" => "claude -p --model haiku",
        "codex" => "codex exec --skip-git-repo-check -",
        "gemini" => "gemini -m gemini-2.5-flash -p \" \"",
        _ => "",
    }
}

pub async fn run(command: &str, prompt: &str, timeout: Duration) -> anyhow::Result<String> {
    let command = command.trim();
    if command.is_empty() {
        anyhow::bail!("Kein CLI-Befehl konfiguriert");
    }
    // Über cmd.exe starten, damit .cmd/.ps1-Shims (npm-Installationen) gefunden werden.
    let mut cmd = tokio::process::Command::new("cmd.exe");
    #[cfg(windows)]
    {
        cmd.raw_arg(format!("/D /S /C \"{command}\""));
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    {
        cmd = tokio::process::Command::new("sh");
        cmd.args(["-c", command]);
    }
    // Neutrales Arbeitsverzeichnis, damit keine Projektdateien (CLAUDE.md …) greifen.
    cmd.current_dir(std::env::temp_dir())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = cmd.spawn().map_err(|e| anyhow::anyhow!("CLI konnte nicht gestartet werden: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes()).await?;
        stdin.shutdown().await?;
    }
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| anyhow::anyhow!("CLI hat nicht rechtzeitig geantwortet"))??;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
        if stderr.contains("is not recognized") || stderr.contains("wird nicht als interner") {
            anyhow::bail!("Befehl nicht gefunden. Ist die CLI installiert und im PATH?");
        }
        anyhow::bail!("CLI-Fehler ({}): {detail}", output.status);
    }
    if stdout.is_empty() {
        anyhow::bail!("CLI hat keine Ausgabe geliefert");
    }
    Ok(stdout)
}
