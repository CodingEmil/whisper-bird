//! Datei- und Ordner-Transkription im Hintergrund: eine Textdatei pro Aufnahme.

use crate::asr::AsrOptions;
use crate::audio::decode;
use crate::history::NewEntry;
use crate::overlay::{self, Phase as OverlayPhase};
use crate::state::state;
use serde::Serialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::Emitter;
use transcribe_cpp::CancelToken;

#[derive(Clone, Debug, Serialize)]
pub struct FileJob {
    pub id: u64,
    pub path: String,
    pub name: String,
    /// "queued" | "decoding" | "transcribing" | "done" | "error" | "cancelled"
    pub status: &'static str,
    pub progress: f32,
    pub output: Option<String>,
    pub error: Option<String>,
    pub preview: Option<String>,
}

#[derive(Default)]
pub struct FileJobs {
    jobs: Mutex<Vec<FileJob>>,
    queue: Mutex<VecDeque<u64>>,
    next_id: AtomicU64,
    running: AtomicBool,
    current: Mutex<Option<(u64, CancelToken, Arc<AtomicBool>)>>,
}

impl FileJobs {
    pub fn list(&self) -> Vec<FileJob> {
        self.jobs.lock().unwrap().clone()
    }

    fn update(&self, id: u64, f: impl FnOnce(&mut FileJob)) {
        let snapshot = {
            let mut jobs = self.jobs.lock().unwrap();
            let Some(job) = jobs.iter_mut().find(|j| j.id == id) else { return };
            f(job);
            job.clone()
        };
        let _ = state().app.emit("file-job", &snapshot);
    }

    pub fn cancel(&self, id: u64) {
        self.queue.lock().unwrap().retain(|&q| q != id);
        if let Some((cur, token, flag)) = &*self.current.lock().unwrap() {
            if *cur == id {
                token.cancel();
                flag.store(true, Ordering::SeqCst);
            }
        }
        self.update(id, |j| {
            if j.status != "done" && j.status != "error" {
                j.status = "cancelled";
            }
        });
    }

    pub fn clear_finished(&self) {
        self.jobs
            .lock()
            .unwrap()
            .retain(|j| matches!(j.status, "queued" | "decoding" | "transcribing"));
        let _ = state().app.emit("file-jobs-cleared", ());
    }
}

fn collect_media(path: &Path, out: &mut Vec<PathBuf>, depth: u32) {
    if path.is_dir() {
        if depth > 4 {
            return;
        }
        if let Ok(entries) = std::fs::read_dir(path) {
            let mut children: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
            children.sort();
            for child in children {
                collect_media(&child, out, depth + 1);
            }
        }
    } else if decode::is_media_file(path) {
        out.push(path.to_path_buf());
    }
}

/// Dateien oder Ordner einreihen (Drag & Drop, Dateiauswahl).
pub fn enqueue(paths: Vec<PathBuf>) {
    let st = state();
    let mut files = Vec::new();
    for p in &paths {
        collect_media(p, &mut files, 0);
    }
    if files.is_empty() {
        let lang = st.settings().ui_language;
        overlay::set(
            OverlayPhase::Warning,
            Some(if lang == "de" { "Keine Audio- oder Videodateien gefunden".into() } else { "No audio or video files found".into() }),
            false,
            None,
        );
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(2500));
            if overlay::current() == OverlayPhase::Warning {
                overlay::set(OverlayPhase::Idle, None, false, None);
            }
        });
        return;
    }
    let jobs = &st.files;
    for f in files {
        let id = jobs.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let job = FileJob {
            id,
            path: f.display().to_string(),
            name: f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            status: "queued",
            progress: 0.0,
            output: None,
            error: None,
            preview: None,
        };
        let _ = st.app.emit("file-job", &job);
        jobs.jobs.lock().unwrap().push(job);
        jobs.queue.lock().unwrap().push_back(id);
    }
    if !jobs.running.swap(true, Ordering::SeqCst) {
        std::thread::Builder::new()
            .name("file-jobs".into())
            .spawn(worker)
            .ok();
    }
}

fn worker() {
    let st = state();
    loop {
        let next = st.files.queue.lock().unwrap().pop_front();
        let Some(id) = next else { break };
        let path = match st.files.jobs.lock().unwrap().iter().find(|j| j.id == id) {
            Some(j) => PathBuf::from(&j.path),
            None => continue,
        };
        let token = CancelToken::new();
        let flag = Arc::new(AtomicBool::new(false));
        *st.files.current.lock().unwrap() = Some((id, token.clone(), flag.clone()));
        let result = transcribe_one(id, &path, &token, &flag);
        *st.files.current.lock().unwrap() = None;
        match result {
            Ok((output, preview)) => st.files.update(id, |j| {
                j.status = "done";
                j.progress = 1.0;
                j.output = Some(output.display().to_string());
                j.preview = Some(preview);
            }),
            Err(_) if flag.load(Ordering::SeqCst) => st.files.update(id, |j| j.status = "cancelled"),
            Err(e) => {
                log::error!("Datei {} fehlgeschlagen: {e:#}", path.display());
                st.files.update(id, |j| {
                    j.status = "error";
                    j.error = Some(format!("{e:#}"));
                });
            }
        }
    }
    st.files.running.store(false, Ordering::SeqCst);
    if overlay::current() == OverlayPhase::Files {
        overlay::set(OverlayPhase::Done, None, false, None);
        std::thread::sleep(std::time::Duration::from_millis(900));
        if overlay::current() == OverlayPhase::Done {
            overlay::set(OverlayPhase::Idle, None, false, None);
        }
    }
    // Neue Jobs während des Abschlusses?
    if !st.files.queue.lock().unwrap().is_empty() && !st.files.running.swap(true, Ordering::SeqCst) {
        std::thread::spawn(worker);
    }
}

fn show_progress(name: &str, progress: f32) {
    let st = state();
    if !st.dictation.is_busy() && !st.meeting.is_active() {
        overlay::set(OverlayPhase::Files, Some(name.to_string()), false, Some(progress));
    }
}

fn transcribe_one(id: u64, path: &Path, token: &CancelToken, flag: &AtomicBool) -> anyhow::Result<(PathBuf, String)> {
    let st = state();
    let s = st.settings();
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    st.files.update(id, |j| j.status = "decoding");
    show_progress(&name, 0.0);
    let mut last = 0.0f32;
    let pcm = decode::decode_file(path, flag, |p| {
        if p - last >= 0.02 {
            last = p;
            st.files.update(id, |j| j.progress = p * 0.2);
            show_progress(&name, p * 0.2);
        }
    })?;
    let duration_ms = pcm.len() as i64 / 16;

    st.files.update(id, |j| j.status = "transcribing");
    let model_id = st.load_asr()?;
    st.load_vad()?;
    let language = (s.language != "auto").then(|| s.language.clone());
    let mut last = 0.0f32;
    let parts = st.asr.transcribe_long(
        &pcm,
        &AsrOptions {
            language,
            vocabulary: &s.custom_words,
            cancel: Some(token),
        },
        &mut |p| {
            if p - last >= 0.01 {
                last = p;
                let total = 0.2 + p * 0.8;
                st.files.update(id, |j| j.progress = total);
                show_progress(&name, total);
            }
        },
    )?;
    let paragraphs: Vec<String> = parts
        .iter()
        .map(|p| crate::text::apply_custom_words(&crate::text::clean(&p.text, s.remove_fillers), &s.custom_words))
        .filter(|t| !t.is_empty())
        .collect();
    let text = paragraphs.join("\n\n");

    let out_dir = s
        .file_output_dir
        .as_ref()
        .map(PathBuf::from)
        .filter(|d| d.is_dir())
        .or_else(|| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| st.dirs.data.clone());
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "transkript".into());
    let mut output = out_dir.join(format!("{stem}.txt"));
    let mut n = 2;
    while output.exists() {
        output = out_dir.join(format!("{stem} ({n}).txt"));
        n += 1;
    }
    std::fs::write(&output, &text)?;

    let preview: String = text.chars().take(280).collect();
    if let Ok(entry) = st.history.add(
        NewEntry {
            text: text.clone(),
            raw_text: String::new(),
            source: "file",
            app: name,
            language: s.language.ne("auto").then(|| s.language.clone()),
            duration_ms,
            model: model_id,
        },
        s.history_limit,
    ) {
        let _ = st.app.emit("history-added", &entry);
    }
    Ok((output, preview))
}
