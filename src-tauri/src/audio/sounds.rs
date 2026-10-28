//! Kurze Hinweistöne (Start- und Stopp-Zwitschern, Fertig-Triller, Fehler),
//! synthetisch erzeugt und über PlaySound aus dem Speicher abgespielt – keine
//! Audiodateien nötig.

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sound {
    Start,
    Stop,
    Done,
    Error,
}

/// Ein Abschnitt eines Hinweistons: Die Frequenz gleitet in `ms`
/// exponentiell von `from` nach `to` Hz; 0 Hz ist eine Pause.
#[derive(Clone, Copy)]
struct Segment {
    from: f32,
    to: f32,
    ms: u32,
    pure: bool,
}

/// Fester Ton mit leichtem Oberton.
const fn tone(freq: f32, ms: u32) -> Segment {
    Segment { from: freq, to: freq, ms, pure: false }
}

/// Vogelpfiff: reiner Sinus und etwas leiser – hohe Töne wirken mit Oberton
/// und voller Lautstärke schnell schrill.
const fn chirp(from: f32, to: f32, ms: u32) -> Segment {
    Segment { from, to, ms, pure: true }
}

/// Ganz kurzes Zwitschern: ein steigender und ein fallender Pfiff.
const START: &[Segment] = &[chirp(3400.0, 5400.0, 40), tone(0.0, 30), chirp(5800.0, 3800.0, 55)];
/// Gegenstück zum Start: zwei fallende Pfiffe, der zweite tiefer – das Mikro ist zu.
const STOP: &[Segment] = &[chirp(5000.0, 3400.0, 45), tone(0.0, 30), chirp(4200.0, 2800.0, 70)];
/// Aufsteigender Triller: Text ist da.
const DONE: &[Segment] = &[
    chirp(3200.0, 3800.0, 28),
    tone(0.0, 18),
    chirp(3600.0, 4300.0, 28),
    tone(0.0, 18),
    chirp(4000.0, 5000.0, 50),
];
const ERROR: &[Segment] = &[tone(330.0, 120), tone(0.0, 40), tone(262.0, 180)];

fn segments(sound: Sound) -> &'static [Segment] {
    match sound {
        Sound::Start => START,
        Sound::Stop => STOP,
        Sound::Done => DONE,
        Sound::Error => ERROR,
    }
}

fn synth(sound: Sound, volume: f32) -> Vec<u8> {
    const RATE: u32 = 44_100;
    let mut pcm: Vec<i16> = Vec::new();
    for seg in segments(sound) {
        let n = (RATE * seg.ms / 1000) as usize;
        // Weiche Hüllkurve gegen Knacken; bei kurzen Pfiffen anteilig kürzer.
        let attack = (n as f32 * 0.15).min(RATE as f32 * 0.008);
        let release = (n as f32 * 0.6).min(RATE as f32 * 0.04);
        let (gain, overtone) = if seg.pure { (0.22, 0.0) } else { (0.35, 0.25) };
        let mut phase = 0.0f32;
        for i in 0..n {
            let v = if seg.from > 0.0 {
                let freq = seg.from * (seg.to / seg.from).powf(i as f32 / n as f32);
                // Phase aufsummieren, damit der Gleitton keine Sprünge macht.
                phase = (phase + TAU * freq / RATE as f32) % TAU;
                let env = (i as f32 / attack).min(1.0) * ((n - i) as f32 / release).min(1.0);
                (phase.sin() + overtone * (2.0 * phase).sin()) * env * gain * volume
            } else {
                0.0
            };
            pcm.push((v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16);
        }
    }
    let data_len = (pcm.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    wav
}

// PlaySound mit SND_ASYNC liest asynchron aus dem Speicher – die Puffer
// müssen also dauerhaft leben. Pro Ton und Lautstärke-Stufe einmal erzeugen.
static CACHE: Mutex<Option<HashMap<(Sound, u8), &'static [u8]>>> = Mutex::new(None);

pub fn play(sound: Sound, volume: f32) {
    let step = (volume.clamp(0.0, 1.0) * 20.0).round() as u8;
    if step == 0 {
        return;
    }
    let data: &'static [u8] = {
        let mut cache = CACHE.lock().unwrap();
        let map = cache.get_or_insert_with(HashMap::new);
        *map.entry((sound, step))
            .or_insert_with(|| Box::leak(synth(sound, step as f32 / 20.0).into_boxed_slice()))
    };
    #[cfg(windows)]
    unsafe {
        use windows::core::PCWSTR;
        use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
        let _ = PlaySoundW(
            PCWSTR(data.as_ptr() as *const u16),
            None,
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
    #[cfg(not(windows))]
    let _ = data;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zwitschern_ist_kurz_und_uebersteuert_nicht() {
        for sound in [Sound::Start, Sound::Stop, Sound::Done] {
            let wav = synth(sound, 1.0);
            assert_eq!(&wav[..4], b"RIFF");
            let samples: Vec<i16> = wav[44..].chunks(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
            assert!(samples.len() <= 44_100 * 150 / 1000, "{sound:?} länger als 150 ms");
            let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
            assert!(peak > 1000 && peak < i16::MAX as u16, "{sound:?} Spitze {peak}");
        }
    }
}
