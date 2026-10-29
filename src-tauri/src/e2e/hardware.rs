//! Aufnahme über echte Geräte – nur mit `scripts/test.ps1 -Hardware`. Der
//! Loopback-Test spielt die Kennedy-Rede (11 s) über den Standard-Lautsprecher ab.

use super::*;
use super::transkription::JFK_TEXT;
use crate::asr::AsrOptions;
use crate::audio::{self, loopback::Loopback, mic};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[test]
#[ignore = "Hardware: Mikrofon – scripts/test.ps1 -Hardware"]
fn mikrofon_nimmt_mit_16_khz_auf() {
    if mic::list_devices().is_empty() {
        println!("Kein Mikrofon angeschlossen – übersprungen");
        return;
    }
    let streamed = Arc::new(AtomicUsize::new(0));
    let failure = Arc::new(Mutex::new(None::<String>));
    let rec = mic::MicRecording::start(
        None,
        true,
        Box::new({
            let streamed = streamed.clone();
            move |chunk: &[f32]| {
                streamed.fetch_add(chunk.len(), Ordering::SeqCst);
            }
        }),
        {
            let failure = failure.clone();
            move |e| *failure.lock().unwrap() = Some(e)
        },
    )
    .expect("Mikrofon lässt sich nicht öffnen");
    println!("Gerät: {}", rec.device_name);
    std::thread::sleep(Duration::from_secs(2));
    let samples = rec.stop().unwrap();
    let secs = seconds(&samples);
    report("hardware_mikrofon", &[("sekunden", secs), ("pegel", audio::level(&samples) as f64)]);
    assert_eq!(*failure.lock().unwrap(), None);
    assert!((1.6..=2.6).contains(&secs), "{secs:.2} s statt 2 s aufgenommen");
    assert_eq!(streamed.load(Ordering::SeqCst), samples.len(), "Live-Blöcke und Aufnahme unterscheiden sich");
}

#[cfg(windows)]
fn play_blocking(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Media::Audio::{PlaySoundW, SND_FILENAME, SND_NODEFAULT, SND_SYNC};
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let ok = unsafe { PlaySoundW(PCWSTR(wide.as_ptr()), None, SND_FILENAME | SND_SYNC | SND_NODEFAULT) };
    assert!(ok.as_bool(), "Wiedergabe fehlgeschlagen");
}

#[cfg(windows)]
#[test]
#[ignore = "Hardware: Lautsprecher – scripts/test.ps1 -Hardware"]
fn systemaudio_wird_mitgeschnitten_und_erkannt() {
    let path = fixture(&JFK);
    let asr = asr();
    let captured = Arc::new(Mutex::new(Vec::<f32>::new()));
    let lb = Loopback::start({
        let captured = captured.clone();
        move |s| captured.lock().unwrap().extend_from_slice(s)
    })
    .expect("System-Audio lässt sich nicht mitschneiden");
    play_blocking(&path);
    std::thread::sleep(Duration::from_millis(300));
    lb.stop();
    let pcm = std::mem::take(&mut *captured.lock().unwrap());
    let text = asr
        .transcribe(&pcm, &AsrOptions { language: None, vocabulary: &[], cancel: None })
        .map(|r| crate::text::clean(&r.text, true))
        .unwrap_or_default();
    let wer = wer(JFK_TEXT, &text);
    println!("{text}");
    report("hardware_systemaudio", &[("sekunden", seconds(&pcm)), ("pegel", audio::level(&pcm) as f64), ("wer", wer)]);
    assert!(seconds(&pcm) >= 9.0, "nur {:.1} s mitgeschnitten", seconds(&pcm));
    assert!(wer <= 0.3, "Wortfehlerrate {wer:.2}: {text}");
}
