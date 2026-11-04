//! Aus dem Finder oder per Autostart gestartete Mac-Apps bekommen nur einen
//! minimalen PATH (/usr/bin:/bin:/usr/sbin:/sbin). CLIs wie `claude` oder
//! `codex` (npm, Homebrew) und `ffmpeg` lägen damit außer Reichweite. Deshalb
//! einmal beim Start den PATH der Login-Shell des Nutzers übernehmen.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MARKER: &str = "__WHISPER_BIRD_PATH__";

/// Übliche Orte, falls die Shell nichts liefert.
fn fallback_dirs() -> Vec<String> {
    let mut dirs = vec!["/opt/homebrew/bin".to_string(), "/usr/local/bin".to_string()];
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy();
        dirs.push(format!("{home}/.local/bin"));
        dirs.push(format!("{home}/.npm-global/bin"));
        dirs.push(format!("{home}/.bun/bin"));
    }
    dirs
}

/// PATH aus `$SHELL -ilc`; interaktiv, weil nvm & Co. ihn oft in .zshrc setzen.
fn login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/zsh".into());
    let mut child = Command::new(&shell)
        .args(["-ilc", &format!("printf '{MARKER}%s{MARKER}' \"$PATH\"")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // Eine hängende Shell-Konfiguration darf den Start nicht aufhalten.
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                log::warn!("Login-Shell {shell} antwortet nicht, PATH bleibt unverändert");
                return None;
            }
        }
    }
    let output = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let start = text.find(MARKER)? + MARKER.len();
    let end = start + text[start..].find(MARKER)?;
    Some(text[start..end].to_string())
}

/// Führt PATH der Login-Shell, bisherigen PATH und übliche Orte zusammen.
pub fn adopt_login_path() {
    let current = std::env::var("PATH").unwrap_or_default();
    let mut dirs: Vec<String> = Vec::new();
    let login = login_shell_path().unwrap_or_default();
    for dir in login.split(':').chain(current.split(':')).map(str::to_string).chain(fallback_dirs()) {
        if !dir.is_empty() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    std::env::set_var("PATH", dirs.join(":"));
}
