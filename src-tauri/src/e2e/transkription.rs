//! Spracherkennung, VAD und Dekodierung mit echtem Audio – die Bausteine von
//! Diktat und Datei-Transkription.

use super::*;
use crate::asr::AsrOptions;
use std::time::Instant;

pub const JFK_TEXT: &str =
    "And so, my fellow Americans, ask not what your country can do for you, ask what you can do for your country.";

/// Häufige deutsche Funktionswörter – in deutschem Text gut ein Viertel aller Wörter.
const GERMAN_COMMON: &[&str] = &[
    "der", "die", "das", "und", "in", "im", "von", "zu", "zum", "zur", "den", "dem", "des", "ist", "mit", "sich",
    "auf", "für", "nicht", "ein", "eine", "einen", "einer", "als", "auch", "es", "an", "am", "wurde", "er", "sie",
    "aus", "bei", "nach", "wird", "werden", "oder", "dass", "wie", "um", "so", "war",
];

fn plain(language: Option<&str>) -> AsrOptions<'static> {
    AsrOptions {
        language: language.map(str::to_string),
        vocabulary: &[],
        cancel: None,
    }
}

#[test]
#[ignore = "braucht Modelle und Testaudio – scripts/test.ps1"]
fn diktat_erkennt_rede_wortgenau() {
    let pcm = load_audio(&fixture(&JFK));
    let asr = asr();
    let started = Instant::now();
    let result = asr.transcribe(&pcm, &plain(None)).unwrap();
    let rtf = started.elapsed().as_secs_f64() / seconds(&pcm);
    let text = crate::text::clean(&result.text, true);
    let wer = wer(JFK_TEXT, &text);
    println!("{text}");
    report("diktat_rede", &[("wer", wer), ("echtzeitfaktor", rtf)]);
    assert!(wer <= 0.1, "Wortfehlerrate {wer:.2}: {text}");
}

#[test]
#[ignore = "braucht Modelle und Testaudio – scripts/test.ps1"]
fn vad_trennt_sprache_von_stille() {
    let speech = load_audio(&fixture(&JFK));
    let mut pcm = speech.clone();
    pcm.extend(vec![0.0; SR * 4]);
    pcm.extend(&speech);
    let segments = asr().speech_segments(&pcm, 0).unwrap();
    // Die inneren 3 s der Pause müssen frei bleiben.
    let gap = speech.len() + SR / 2..speech.len() + SR * 7 / 2;
    let speech_s = segments.iter().map(|(a, b)| b - a).sum::<usize>() as f64 / SR as f64;
    report("vad", &[("abschnitte", segments.len() as f64), ("sprache_s", speech_s)]);
    assert!(segments.len() >= 2, "{segments:?}");
    assert!(
        segments.iter().all(|&(a, b)| b <= gap.start || a >= gap.end),
        "Sprache in der Pause erkannt: {segments:?}"
    );
    assert!((12.0..=22.0).contains(&speech_s), "{speech_s:.1} s Sprache erkannt (2 × 11 s Rede)");

    let hiss: Vec<f32> = (0..SR * 5).map(|i| ((i * 7919 % 1000) as f32 / 1000.0 - 0.5) * 0.002).collect();
    assert!(asr().speech_segments(&hiss, 0).unwrap().is_empty(), "Rauschen als Sprache erkannt");
}

#[test]
#[ignore = "braucht Testaudio – scripts/test.ps1"]
fn dateien_werden_auf_16_khz_dekodiert() {
    // WAV in 16 kHz und Ogg Vorbis in 48 bzw. 44,1 kHz (geht durch den Resampler).
    // Sollwerte aus WAV-Header bzw. Ogg-Granule-Position.
    for (f, expected_s) in [(&JFK, 11.0), (&AMI_AUDIO, 1049.355), (&DE_BIERKRIEG, 154.208), (&DE_MAGIC8, 164.061)] {
        let pcm = load_audio(&fixture(f));
        let got = seconds(&pcm);
        let peak = pcm.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!((got - expected_s).abs() < 0.1, "{}: {got:.3} s statt {expected_s} s", f.file);
        assert!(peak > 0.05 && peak <= 1.0, "{}: Spitze {peak}", f.file);
    }
}

#[test]
#[ignore = "braucht Modelle und Testaudio – scripts/test.ps1"]
fn lange_datei_wird_in_abschnitten_transkribiert() {
    let pcm = load_audio(&fixture(&DE_BIERKRIEG));
    let asr = asr();
    let mut progress = Vec::new();
    let started = Instant::now();
    // Sprache automatisch, wie in der Standardeinstellung.
    let parts = asr.transcribe_long(&pcm, &plain(None), &mut |p| progress.push(p)).unwrap();
    let rtf = started.elapsed().as_secs_f64() / seconds(&pcm);
    let text = parts.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join(" ");
    let all = words(&text);
    let german = all.iter().filter(|w| GERMAN_COMMON.contains(&w.as_str())).count() as f64 / all.len().max(1) as f64;
    println!("{text}");
    report(
        "datei_deutsch",
        &[("abschnitte", parts.len() as f64), ("woerter", all.len() as f64), ("anteil_deutsch", german), ("echtzeitfaktor", rtf)],
    );
    assert!(parts.len() >= 3, "nur {} Abschnitte", parts.len());
    assert!(parts.windows(2).all(|w| w[0].end_ms <= w[1].start_ms), "Abschnitte überlappen");
    assert!(parts.last().unwrap().end_ms <= (seconds(&pcm) * 1000.0) as i64 + 10);
    assert!(progress.windows(2).all(|w| w[0] <= w[1]) && progress.last() == Some(&1.0), "{progress:?}");
    // 2:34 min Vorlesetempo: rund 300 Wörter.
    assert!((200..=450).contains(&all.len()), "{} Wörter", all.len());
    assert!(german >= 0.2, "kaum deutsche Wörter ({german:.2}): {text}");
    assert!(text.contains("Bamberg"), "Titel fehlt: {text}");
}
