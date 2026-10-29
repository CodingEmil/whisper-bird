//! Meeting-Modus mit echtem Audio: Das Audio läuft wie vom Mikrofon in
//! 100-ms-Blöcken ein (Live-Transkript), danach folgen Sprechererkennung und
//! Neutranskription wie in `meeting::run`.

use super::*;
use crate::asr::AsrOptions;
use crate::meeting::{
    assign_speakers, diarize_and_retranscribe, live_segment, mix, transcript_text, LiveCutter, Meeting, Segment,
};
use crate::settings::MeetingSource;
use std::cell::Cell;
use std::ops::Range;
use std::time::Instant;

struct Recorded {
    meeting: Meeting,
    live: Vec<Segment>,
    /// Rechenzeit fürs Live-Transkript und für die Nachbearbeitung, je Sekunde Audio.
    live_rtf: f64,
    post_rtf: f64,
}

fn record(pcm: &[f32], language: Option<&str>) -> Recorded {
    let asr = asr();
    let diarize_model = model(catalog::DIARIZE_MODEL);
    let opts = AsrOptions {
        language: language.map(str::to_string),
        vocabulary: &[],
        cancel: None,
    };
    let mut m = Meeting {
        id: "e2e".into(),
        title: "E2E".into(),
        created_at: 0,
        duration_ms: 0,
        source: MeetingSource::Microphone,
        status: "recording".into(),
        segments: Vec::new(),
        speakers: Vec::new(),
        notes: None,
        error: None,
        diarized: false,
    };
    let mut full = Vec::new();
    let mut live = LiveCutter::default();
    let blocks: Vec<&[f32]> = pcm.chunks(SR / 10).collect();
    let started = Instant::now();
    for (i, block) in blocks.iter().enumerate() {
        let stopping = i + 1 == blocks.len();
        full.extend(mix(block, &[], block.len()));
        if let Some(range) = live.next(&full, asr, stopping) {
            if let Some(seg) = live_segment(asr, &full, range, &opts, true).unwrap() {
                m.segments.push(seg);
            }
        }
    }
    let live_rtf = started.elapsed().as_secs_f64() / seconds(pcm);
    m.duration_ms = (full.len() / 16) as i64;
    m.status = "processing".into();
    let live_segments = m.segments.clone();

    let started = Instant::now();
    let progress = Cell::new(0.0f32);
    let diarized = diarize_and_retranscribe(asr, &diarize_model, &full, &opts, true, &|p| {
        assert!(p >= progress.get(), "Fortschritt läuft rückwärts");
        progress.set(p);
    });
    if let Err(e) = &diarized {
        panic!("Sprechererkennung fehlgeschlagen: {e:#}");
    }
    assign_speakers(&mut m, diarized, "de");
    m.status = "done".into();
    Recorded {
        meeting: m,
        live: live_segments,
        live_rtf,
        post_rtf: started.elapsed().as_secs_f64() / seconds(pcm),
    }
}

fn text_of<'a>(segments: impl IntoIterator<Item = &'a Segment>) -> String {
    segments.into_iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ")
}

fn turns(m: &Meeting) -> Vec<Turn> {
    m.segments.iter().map(|s| (s.start_ms, s.end_ms, m.speakers[s.speaker as usize].clone())).collect()
}

/// Was in der App sichtbar wird, muss in sich stimmig sein. `max_drift`: erlaubte
/// Wortfehlerrate zwischen Live-Transkript und Endergebnis – zerschnittene Wörter
/// im Live-Transkript fallen hier auf.
fn check_consistency(r: &Recorded, max_drift: f64) {
    let m = &r.meeting;
    assert!(!r.live.is_empty(), "kein Live-Transkript");
    for w in r.live.windows(2) {
        assert!(w[0].end_ms <= w[1].start_ms, "Live-Abschnitte überlappen: {:?}", (&w[0], &w[1]));
    }
    assert!(r.live.iter().all(|s| s.end_ms <= m.duration_ms && !s.text.is_empty() && s.speaker == -1));

    assert!(m.diarized, "keine Sprecher erkannt");
    assert!(m.segments.windows(2).all(|w| w[0].start_ms <= w[1].start_ms), "Abschnitte nicht sortiert");
    assert!(m.segments.iter().all(|s| s.start_ms < s.end_ms && s.end_ms <= m.duration_ms + 100 && !s.text.is_empty()));
    assert!(m.segments.iter().all(|s| (0..m.speakers.len() as i32).contains(&s.speaker)));
    assert!(transcript_text(m).starts_with("Sprecher 1: "), "{}", transcript_text(m));

    // Speichern und Laden (meeting.json) verliert nichts.
    let back: Meeting = serde_json::from_slice(&serde_json::to_vec_pretty(m).unwrap()).unwrap();
    assert_eq!(transcript_text(&back), transcript_text(m));
    let wav = std::env::temp_dir().join(format!("whisper-island-e2e-{}.wav", std::process::id()));
    let pcm: Vec<f32> = (0..SR).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
    crate::audio::write_wav(&wav, &pcm).unwrap();
    assert_eq!(load_audio(&wav).len(), pcm.len());
    let _ = std::fs::remove_file(&wav);

    // Live-Transkript und neu transkribiertes Ergebnis sagen dasselbe.
    let drift = wer(&text_of(&r.live), &text_of(&m.segments));
    assert!(drift <= max_drift, "Live und Endergebnis weichen zu stark ab (WER {drift:.2})");
}

#[test]
#[ignore = "braucht Modelle und Testaudio – scripts/test.ps1"]
fn meeting_mit_vier_personen() {
    // Minute 4 bis 9: alle vier sprechen jeweils mindestens 36 s.
    const FROM: f64 = 240.0;
    const TO: f64 = 540.0;
    let audio = load_audio(&fixture(&AMI_AUDIO));
    let pcm = &audio[FROM as usize * SR..TO as usize * SR];
    let reference = parse_rttm(&std::fs::read_to_string(fixture(&AMI_RTTM)).unwrap(), FROM, TO);
    let r = record(pcm, None);
    let d = score_diarization(&reference, &turns(&r.meeting));
    println!("{}", transcript_text(&r.meeting));
    let drift = wer(&text_of(&r.live), &text_of(&r.meeting.segments));
    report(
        "meeting_vier_personen",
        &[
            ("sprecher", d.hyp_speakers as f64),
            ("sprecher_referenz", d.ref_speakers as f64),
            ("zuordnung", d.accuracy),
            ("der", d.der),
            ("live_abschnitte", r.live.len() as f64),
            ("live_vs_ende_wer", drift),
            ("live_echtzeitfaktor", r.live_rtf),
            ("nachbearbeitung_echtzeitfaktor", r.post_rtf),
        ],
    );
    // Normal rund 0,19: Durcheinanderreden und Füllwörter.
    check_consistency(&r, 0.3);
    assert_eq!(d.hyp_speakers, 4, "{} statt 4 Sprecher erkannt", d.hyp_speakers);
    assert!(d.accuracy >= 0.9, "Sprecher-Zuordnung nur {:.0} %", d.accuracy * 100.0);
    // Die DER enthält Pausen, die die App dem laufenden Sprecher zuschlägt (Parakeet Ultra: 0,36).
    assert!(d.der <= 0.45, "DER {:.2}", d.der);
}

/// Baut aus Einzelaufnahmen einen Dialog: Die Sprecher wechseln sich ab, jeder
/// Beitrag endet an einer Sprechpause nahe der Wunschlänge, dazwischen 0,4 s Stille.
/// Liefert das Audio und die Referenz, wer wann spricht.
fn dialog(sources: &[(&[f32], &str)], turn_secs: &[f64]) -> (Vec<f32>, Vec<Turn>) {
    let speech: Vec<Vec<(usize, usize)>> = sources.iter().map(|(pcm, _)| asr().speech_segments(pcm, 0).unwrap()).collect();
    let mut pos: Vec<usize> = speech.iter().map(|s| s[0].0).collect();
    let mut out = Vec::new();
    let mut reference = Vec::new();
    for (k, &secs) in turn_secs.iter().enumerate() {
        let i = k % sources.len();
        let (pcm, name) = sources[i];
        let target = pos[i] + (secs * SR as f64) as usize;
        let end = speech[i]
            .iter()
            .map(|s| s.1)
            .filter(|&e| e > pos[i] + 3 * SR)
            .min_by_key(|&e| e.abs_diff(target))
            .expect("Aufnahme zu kurz für den Dialog");
        if !out.is_empty() {
            out.extend(vec![0.0; SR * 2 / 5]);
        }
        let start_ms = (out.len() / 16) as i64;
        out.extend_from_slice(&pcm[pos[i]..end]);
        reference.push((start_ms, (out.len() / 16) as i64, name.to_string()));
        pos[i] = speech[i].iter().map(|s| s.0).find(|&a| a >= end).unwrap_or(pcm.len());
    }
    (out, reference)
}

fn range_ms(t: &Turn) -> Range<usize> {
    t.0 as usize * 16..t.1 as usize * 16
}

#[test]
#[ignore = "braucht Modelle und Testaudio – scripts/test.ps1"]
fn deutscher_dialog_mit_zwei_sprechern() {
    let bierkrieg = load_audio(&fixture(&DE_BIERKRIEG));
    let magic8 = load_audio(&fixture(&DE_MAGIC8));
    let (pcm, reference) = dialog(&[(&bierkrieg, "bierkrieg"), (&magic8, "magic8")], &[16.0, 12.0, 20.0, 9.0, 14.0, 18.0, 11.0, 15.0]);
    // Was die Erkennung zu jedem Beitrag allein sagt – die Messlatte für die Zuordnung der Texte.
    let mut expected: Vec<(String, String)> = Vec::new();
    for t in &reference {
        let parts = asr()
            .transcribe_long(&pcm[range_ms(t)], &AsrOptions { language: None, vocabulary: &[], cancel: None }, &mut |_| {})
            .unwrap();
        expected.push((t.2.clone(), parts.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join(" ")));
    }

    let r = record(&pcm, None);
    let d = score_diarization(&reference, &turns(&r.meeting));
    println!("{}", transcript_text(&r.meeting));
    let mut text_wer = Vec::new();
    for (hyp, refname) in &d.mapping {
        let said = text_of(r.meeting.segments.iter().filter(|s| &r.meeting.speakers[s.speaker as usize] == hyp));
        let want = expected.iter().filter(|e| &e.0 == refname).map(|e| e.1.as_str()).collect::<Vec<_>>().join(" ");
        text_wer.push(wer(&want, &said));
    }
    let worst_text_wer = text_wer.iter().copied().fold(0.0, f64::max);
    let drift = wer(&text_of(&r.live), &text_of(&r.meeting.segments));
    report(
        "meeting_deutscher_dialog",
        &[
            ("sprecher", d.hyp_speakers as f64),
            ("zuordnung", d.accuracy),
            ("der", d.der),
            ("text_je_sprecher_wer", worst_text_wer),
            ("live_vs_ende_wer", drift),
            ("live_echtzeitfaktor", r.live_rtf),
            ("nachbearbeitung_echtzeitfaktor", r.post_rtf),
        ],
    );
    check_consistency(&r, 0.15);
    assert_eq!(d.hyp_speakers, 2, "{} statt 2 Sprecher erkannt", d.hyp_speakers);
    assert!(d.accuracy >= 0.95, "Sprecher-Zuordnung nur {:.0} %", d.accuracy * 100.0);
    assert!(d.der <= 0.1, "DER {:.2}", d.der);
    assert_eq!(text_wer.len(), 2);
    assert!(worst_text_wer <= 0.15, "Texte landen beim falschen Sprecher (WER {text_wer:?})");
}
