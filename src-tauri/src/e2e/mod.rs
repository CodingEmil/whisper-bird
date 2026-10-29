//! End-to-End-Tests mit echten Modellen und echtem Audio: eine Rede, ein
//! aufgezeichnetes Meeting mit vier Personen und ein deutscher Dialog.
//!
//! Sie laufen nur auf Anfrage (`scripts/test.ps1` oder
//! `cargo test --lib e2e:: -- --ignored --test-threads=1`). Testaudio und
//! fehlende Modelle landen beim ersten Lauf in `target/test-fixtures`,
//! installierte Modelle der App werden nur gelesen.
//!
//! Umgebungsvariablen: `WHISPER_ISLAND_TEST_ASR` (Modell-ID, Standard
//! Parakeet Ultra), `WHISPER_ISLAND_MODELS` (weiterer Modellordner),
//! `WHISPER_ISLAND_FIXTURES` (anderer Cache für Testaudio und Modelle).

mod hardware;
mod meeting;
mod transkription;

use crate::asr::Asr;
use crate::audio::decode;
use crate::catalog;
use crate::settings::Accelerator;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};

pub const SR: usize = 16_000;

/// Testaudio mit fester Prüfsumme.
pub struct Fixture {
    pub file: &'static str,
    url: &'static str,
    sha256: &'static str,
}

/// John F. Kennedy, Antrittsrede 1961 (gemeinfrei), 11 s – das Beispiel von whisper.cpp.
pub const JFK: Fixture = Fixture {
    file: "jfk.wav",
    url: "https://raw.githubusercontent.com/ggml-org/whisper.cpp/b0a11594aec50892a02cd8d129eee2dfe93a8bb8/samples/jfk.wav",
    sha256: "59dfb9a4acb36fe2a2affc14bacbee2920ff435cb13cc314a08c13f66ba7860e",
};

/// AMI-Meeting ES2004a (CC BY 4.0): vier Personen, Mix der Headset-Mikrofone, 17 min.
pub const AMI_AUDIO: Fixture = Fixture {
    file: "ES2004a.Mix-Headset.wav",
    url: "https://groups.inf.ed.ac.uk/ami/AMICorpusMirror/amicorpus/ES2004a/audio/ES2004a.Mix-Headset.wav",
    sha256: "3e2560b19bee6952c7c7ce041b0f1ea8a7ea9468044c4eea79d2a2c67e24ab0f",
};

/// Wer wann spricht in ES2004a (pyannote/AMI-diarization-setup, nur Wortgrenzen).
pub const AMI_RTTM: Fixture = Fixture {
    file: "ES2004a.rttm",
    url: "https://raw.githubusercontent.com/pyannote/AMI-diarization-setup/67c2d539286e89f68952d5dcf83912bd9f01dfae/only_words/rttms/test/ES2004a.rttm",
    sha256: "9869c6146c2fd9595403edb36c2caeda65c12ffa2c0af4ce48d6814b673fd5a9",
};

/// Gesprochene Wikipedia „Bamberger Bierkrieg“, Sprecherin Valerie277 (CC BY-SA 4.0), Ogg Vorbis.
pub const DE_BIERKRIEG: Fixture = Fixture {
    file: "de-bamberger-bierkrieg.ogg",
    url: "https://upload.wikimedia.org/wikipedia/commons/7/77/De_Bamberger_Bierkrieg-article.ogg",
    sha256: "580687dc2c60aecbb3f35e4683c4a42bcc08c30a88a6adc9943e36f081ad520a",
};

/// Gesprochene Wikipedia „Magic 8 Ball“, Sprecher ChristianHeldt (CC BY-SA 3.0), Ogg Vorbis.
pub const DE_MAGIC8: Fixture = Fixture {
    file: "de-magic-8-ball.ogg",
    url: "https://upload.wikimedia.org/wikipedia/commons/0/0c/De-Magic_8_Ball-article.ogg",
    sha256: "2a5179b5f8ef1fc1329060a4b9da92e93ba28abf77c756173b1aa9b303dc9db8",
};

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

pub fn fixtures_dir() -> PathBuf {
    std::env::var_os("WHISPER_ISLAND_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir().join("target").join("test-fixtures"))
}

pub fn fixture(f: &Fixture) -> PathBuf {
    download(f.url, &fixtures_dir().join(f.file), f.sha256)
}

/// Modell aus dem Katalog: aus `WHISPER_ISLAND_MODELS`, aus der installierten App
/// oder – falls nirgends vorhanden – frisch in den Testcache geladen.
pub fn model(id: &str) -> PathBuf {
    let info = catalog::find(id).unwrap_or_else(|| panic!("unbekanntes Modell {id}"));
    let mut dirs: Vec<PathBuf> = std::env::var_os("WHISPER_ISLAND_MODELS").map(PathBuf::from).into_iter().collect();
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(Path::new(&appdata).join("dev.emil.whisper-island").join("models"));
    }
    if let Some(p) = dirs.iter().map(|d| d.join(info.filename)).find(|p| p.exists()) {
        return p;
    }
    download(&info.url(), &fixtures_dir().join("models").join(info.filename), info.sha256)
}

fn download(url: &str, path: &Path, sha256: &str) -> PathBuf {
    // Parallele Tests sollen dieselbe Datei nicht doppelt laden.
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if path.exists() {
        return path.to_path_buf();
    }
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    eprintln!("Lade {url} …");
    let part = path.with_extension("part");
    let hash = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            use futures_util::StreamExt;
            use tokio::io::AsyncWriteExt;
            let resp = reqwest::Client::builder()
                .user_agent("whisper-island-tests/0.1")
                .build()?
                .get(url)
                .send()
                .await?
                .error_for_status()?;
            let mut file = tokio::fs::File::create(&part).await?;
            let mut hasher = Sha256::new();
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                hasher.update(&chunk);
                file.write_all(&chunk).await?;
            }
            file.flush().await?;
            anyhow::Ok(hex::encode(hasher.finalize()))
        })
        .unwrap_or_else(|e| panic!("Download von {url} fehlgeschlagen: {e:#}"));
    if hash != sha256 {
        let _ = std::fs::remove_file(&part);
        panic!("Prüfsumme von {url} stimmt nicht ({hash})");
    }
    std::fs::rename(&part, path).unwrap();
    path.to_path_buf()
}

pub fn asr_id() -> String {
    std::env::var("WHISPER_ISLAND_TEST_ASR").unwrap_or_else(|_| "parakeet-ultra".into())
}

/// Spracherkennung samt VAD für alle Tests, einmal geladen.
pub fn asr() -> &'static Asr {
    static ASR: OnceLock<Asr> = OnceLock::new();
    ASR.get_or_init(|| {
        let id = asr_id();
        let asr = Asr::default();
        asr.ensure_loaded(&id, &model(&id), Accelerator::Auto).expect("Spracherkennung lädt nicht");
        asr.ensure_vad(&model(catalog::VAD_MODEL)).expect("VAD lädt nicht");
        asr
    })
}

/// Dekodiert eine Datei wie die App (16 kHz mono).
pub fn load_audio(path: &Path) -> Vec<f32> {
    decode::decode_file(path, &AtomicBool::new(false), |_| {})
        .unwrap_or_else(|e| panic!("{} nicht lesbar: {e:#}", path.display()))
}

pub fn seconds(pcm: &[f32]) -> f64 {
    pcm.len() as f64 / SR as f64
}

/// Kennzahlen eines Tests: auf die Konsole und nach `target/test-reports/e2e/<name>.json`,
/// damit `scripts/test.ps1` sie neben den letzten Lauf stellen kann.
pub fn report(name: &str, metrics: &[(&str, f64)]) {
    let line = metrics.iter().map(|(k, v)| format!("{k}={v:.3}")).collect::<Vec<_>>().join("  ");
    println!("[{name}] {line}");
    let dir = manifest_dir().join("target").join("test-reports").join("e2e");
    let _ = std::fs::create_dir_all(&dir);
    let metrics: serde_json::Map<String, serde_json::Value> =
        metrics.iter().map(|(k, v)| (k.to_string(), serde_json::json!(v))).collect();
    let json = serde_json::json!({ "test": name, "asr": asr_id(), "metrics": metrics });
    let _ = std::fs::write(dir.join(format!("{name}.json")), serde_json::to_vec_pretty(&json).unwrap());
}

/// Wörter in Kleinbuchstaben, ohne Satzzeichen.
pub fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(|w| w.trim_matches('\''))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Wortfehlerrate: Ersetzungen, Auslassungen und Einfügungen je Referenzwort.
pub fn wer(reference: &str, hypothesis: &str) -> f64 {
    let (r, h) = (words(reference), words(hypothesis));
    if r.is_empty() {
        return if h.is_empty() { 0.0 } else { 1.0 };
    }
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for (i, rw) in r.iter().enumerate() {
        let mut cur = vec![i + 1; h.len() + 1];
        for (j, hw) in h.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(rw != hw)).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[h.len()] as f64 / r.len() as f64
}

/// Sprecherabschnitt: Start und Ende in ms, Sprecher.
pub type Turn = (i64, i64, String);

/// RTTM-Zeilen im Fenster `from_s..to_s`, auf den Fensteranfang verschoben.
pub fn parse_rttm(text: &str, from_s: f64, to_s: f64) -> Vec<Turn> {
    let (from, to) = ((from_s * 1000.0) as i64, (to_s * 1000.0) as i64);
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.first() != Some(&"SPEAKER") || f.len() < 8 {
                return None;
            }
            let start = (f[3].parse::<f64>().ok()? * 1000.0) as i64;
            let end = start + (f[4].parse::<f64>().ok()? * 1000.0) as i64;
            let (a, b) = (start.max(from), end.min(to));
            (b > a).then(|| (a - from, b - from, f[7].to_string()))
        })
        .collect()
}

pub struct Diarization {
    pub ref_speakers: usize,
    pub hyp_speakers: usize,
    /// Anteil der Zeit mit genau einem Referenzsprecher, in der der zugeordnete
    /// erkannte Sprecher stimmt (nur Zeit, in der überhaupt jemand erkannt wurde).
    pub accuracy: f64,
    /// Diarization Error Rate ohne Toleranz an den Grenzen.
    pub der: f64,
    /// Erkannter Sprecher → Referenzsprecher.
    pub mapping: Vec<(String, String)>,
}

fn labels(turns: &[Turn]) -> Vec<String> {
    let mut v: Vec<String> = turns.iter().map(|t| t.2.clone()).collect();
    v.sort();
    v.dedup();
    v
}

/// Vergleicht erkannte Sprecherabschnitte in 10-ms-Schritten mit der Referenz.
/// Erkannte Sprecher werden den Referenzsprechern mit der größten Überlappung zugeordnet.
pub fn score_diarization(reference: &[Turn], hypothesis: &[Turn]) -> Diarization {
    let (rl, hl) = (labels(reference), labels(hypothesis));
    let end = reference.iter().chain(hypothesis).map(|t| t.1).max().unwrap_or(0);
    let frames = (end / 10) as usize + 1;
    let active = |turns: &[Turn], ls: &[String]| {
        let mut f = vec![Vec::<usize>::new(); frames];
        for (a, b, s) in turns {
            let i = ls.iter().position(|l| l == s).unwrap();
            for frame in &mut f[(a / 10) as usize..(b / 10) as usize] {
                if !frame.contains(&i) {
                    frame.push(i);
                }
            }
        }
        f
    };
    let (rf, hf) = (active(reference, &rl), active(hypothesis, &hl));
    let mut overlap = vec![vec![0usize; rl.len()]; hl.len()];
    for (r, h) in rf.iter().zip(&hf) {
        for &hi in h {
            for &ri in r {
                overlap[hi][ri] += 1;
            }
        }
    }
    let map = best_mapping(&overlap, rl.len());
    let (mut total, mut errors, mut single, mut single_ok) = (0usize, 0usize, 0usize, 0usize);
    for (r, h) in rf.iter().zip(&hf) {
        let correct = h.iter().filter(|&&hi| map[hi].is_some_and(|ri| r.contains(&ri))).count();
        total += r.len();
        // Verpasst + Fehlalarm + Verwechslung
        errors += r.len().max(h.len()) - correct;
        if r.len() == 1 && !h.is_empty() {
            single += 1;
            single_ok += usize::from(correct > 0);
        }
    }
    Diarization {
        ref_speakers: rl.len(),
        hyp_speakers: hl.len(),
        accuracy: single_ok as f64 / single.max(1) as f64,
        der: errors as f64 / total.max(1) as f64,
        mapping: map
            .iter()
            .enumerate()
            .filter_map(|(hi, ri)| ri.map(|ri| (hl[hi].clone(), rl[ri].clone())))
            .collect(),
    }
}

/// Zuordnung mit der größten Gesamtüberlappung – erschöpfend, es sind nur wenige Sprecher.
fn best_mapping(overlap: &[Vec<usize>], n_ref: usize) -> Vec<Option<usize>> {
    fn search(
        h: usize,
        overlap: &[Vec<usize>],
        used: &mut [bool],
        cur: &mut Vec<Option<usize>>,
        score: usize,
        best: &mut (usize, Vec<Option<usize>>),
    ) {
        if h == overlap.len() {
            if score > best.0 {
                *best = (score, cur.clone());
            }
            return;
        }
        cur.push(None);
        search(h + 1, overlap, used, cur, score, best);
        cur.pop();
        for r in 0..used.len() {
            if !used[r] {
                used[r] = true;
                cur.push(Some(r));
                search(h + 1, overlap, used, cur, score + overlap[h][r], best);
                cur.pop();
                used[r] = false;
            }
        }
    }
    let mut best = (0, vec![None; overlap.len()]);
    search(0, overlap, &mut vec![false; n_ref], &mut Vec::new(), 0, &mut best);
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(a: i64, b: i64, s: &str) -> Turn {
        (a, b, s.to_string())
    }

    #[test]
    fn wortfehlerrate_ignoriert_schreibweise() {
        assert_eq!(wer("Ask not, what your country", "ask NOT what your country."), 0.0);
        assert!((wer("eins zwei drei", "eins vier drei") - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(wer("eins zwei", "eins zwei drei vier"), 1.0);
    }

    #[test]
    fn sprecherbewertung_findet_die_zuordnung() {
        let reference = [t(0, 10_000, "anna"), t(10_000, 20_000, "ben")];
        let same = [t(0, 10_000, "Sprecher 2"), t(10_000, 20_000, "Sprecher 1")];
        let d = score_diarization(&reference, &same);
        assert_eq!((d.ref_speakers, d.hyp_speakers), (2, 2));
        assert_eq!((d.accuracy, d.der), (1.0, 0.0));
        assert!(d.mapping.contains(&("Sprecher 2".into(), "anna".into())));

        // Ein Sprecher für alles: die Hälfte stimmt.
        let one = [t(0, 20_000, "Sprecher 1")];
        let d = score_diarization(&reference, &one);
        assert!((d.accuracy - 0.5).abs() < 0.01 && (d.der - 0.5).abs() < 0.01);
    }

    #[test]
    fn rttm_wird_auf_das_fenster_zugeschnitten() {
        let rttm = "SPEAKER x 1 1.5 2.0 <NA> <NA> A <NA> <NA>\nSPEAKER x 1 9.0 3.0 <NA> <NA> B <NA> <NA>\n";
        assert_eq!(parse_rttm(rttm, 2.0, 10.0), vec![t(0, 1500, "A"), t(7000, 8000, "B")]);
    }
}
