//! Fortsetzbare Downloads mit Fortschritts-Events und SHA-256-Prüfung.

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

#[derive(Clone, Debug, Serialize)]
pub struct DownloadProgress {
    pub id: String,
    pub downloaded: u64,
    pub total: u64,
    /// "downloading" | "verifying" | "done" | "error" | "cancelled"
    pub state: &'static str,
    pub error: Option<String>,
}

pub struct DownloadRequest {
    pub id: String,
    pub url: String,
    pub dest: PathBuf,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Default)]
pub struct Downloads {
    active: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl Downloads {
    pub fn is_active(&self, id: &str) -> bool {
        self.active.lock().unwrap().contains_key(id)
    }

    pub fn active_ids(&self) -> Vec<String> {
        self.active.lock().unwrap().keys().cloned().collect()
    }

    pub fn cancel(&self, id: &str) {
        if let Some(flag) = self.active.lock().unwrap().get(id) {
            flag.store(true, Ordering::SeqCst);
        }
    }

    /// Startet den Download im Hintergrund. Gibt false zurück, wenn er schon läuft.
    /// `on_done` wird nach erfolgreichem Abschluss aufgerufen.
    pub fn start<F>(self: &Arc<Self>, app: AppHandle, req: DownloadRequest, on_done: F) -> bool
    where
        F: FnOnce(&AppHandle) + Send + 'static,
    {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut active = self.active.lock().unwrap();
            if active.contains_key(&req.id) {
                return false;
            }
            active.insert(req.id.clone(), cancel.clone());
        }
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            let id = req.id.clone();
            let result = run(&app, &req, &cancel).await;
            this.active.lock().unwrap().remove(&id);
            let (state, error) = match result {
                Ok(()) => ("done", None),
                Err(_) if cancel.load(Ordering::SeqCst) => ("cancelled", None),
                Err(e) => {
                    log::error!("Download {id} fehlgeschlagen: {e:#}");
                    ("error", Some(format!("{e:#}")))
                }
            };
            let _ = app.emit(
                "download-progress",
                DownloadProgress {
                    id: id.clone(),
                    downloaded: if state == "done" { req.size } else { 0 },
                    total: req.size,
                    state,
                    error,
                },
            );
            if state == "done" {
                on_done(&app);
            }
        });
        true
    }
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    dest.with_file_name(name)
}

async fn run(app: &AppHandle, req: &DownloadRequest, cancel: &AtomicBool) -> anyhow::Result<()> {
    if let Some(parent) = req.dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let part = part_path(&req.dest);
    let mut have = tokio::fs::metadata(&part).await.map(|m| m.len()).unwrap_or(0);

    let client = reqwest::Client::builder()
        .user_agent(concat!("WhisperBird/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .build()?;

    let mut request = client.get(&req.url);
    if have > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let response = request.send().await?;
    let status = response.status();
    if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE && have > 0 {
        // Datei ist bereits vollständig; direkt prüfen.
    } else if !status.is_success() {
        anyhow::bail!("HTTP {status}");
    }

    let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
    if !resumed && status != reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        have = 0;
    }
    let total = if req.size > 0 {
        req.size
    } else {
        response.content_length().unwrap_or(0) + have
    };

    if status != reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&part)
            .await?;
        let mut stream = response.bytes_stream();
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::SeqCst) {
                anyhow::bail!("abgebrochen");
            }
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            have += chunk.len() as u64;
            if last_emit.elapsed() >= Duration::from_millis(150) {
                last_emit = Instant::now();
                let _ = app.emit(
                    "download-progress",
                    DownloadProgress {
                        id: req.id.clone(),
                        downloaded: have,
                        total,
                        state: "downloading",
                        error: None,
                    },
                );
            }
        }
        file.flush().await?;
    }

    if let Some(expected) = &req.sha256 {
        let _ = app.emit(
            "download-progress",
            DownloadProgress {
                id: req.id.clone(),
                downloaded: have,
                total,
                state: "verifying",
                error: None,
            },
        );
        let path = part.clone();
        let actual = tokio::task::spawn_blocking(move || sha256_file(&path)).await??;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = tokio::fs::remove_file(&part).await;
            anyhow::bail!("Prüfsumme stimmt nicht (Datei beschädigt). Bitte erneut laden.");
        }
    }

    tokio::fs::rename(&part, &req.dest).await?;
    Ok(())
}

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Löscht eine angefangene Teildatei (z. B. nach „Abbrechen“).
pub fn remove_partial(dest: &Path) {
    let _ = std::fs::remove_file(part_path(dest));
}
