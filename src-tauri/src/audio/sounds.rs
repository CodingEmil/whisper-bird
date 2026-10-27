//! Kurze Hinweistöne (Start, Fertig, Fehler), synthetisch erzeugt und über
//! PlaySound aus dem Speicher abgespielt – keine Audiodateien nötig.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sound {
    Start,
    Done,
    Error,
}

/// (Frequenz in Hz, Dauer in ms) je Ton.
fn notes(sound: Sound) -> &'static [(f32, u32)] {
    match sound {
        Sound::Start => &[(784.0, 70), (1046.5, 110)],
        Sound::Done => &[(1046.5, 70), (784.0, 120)],
        Sound::Error => &[(330.0, 120), (0.0, 40), (262.0, 180)],
    }
}

fn synth(sound: Sound, volume: f32) -> Vec<u8> {
    const RATE: u32 = 44_100;
    let mut pcm: Vec<i16> = Vec::new();
    for &(freq, ms) in notes(sound) {
        let n = (RATE * ms / 1000) as usize;
        for i in 0..n {
            let t = i as f32 / RATE as f32;
            // Weiche Hüllkurve gegen Knacken.
            let attack = (i as f32 / (RATE as f32 * 0.008)).min(1.0);
            let release = ((n - i) as f32 / (RATE as f32 * 0.04)).min(1.0);
            let env = attack * release;
            let tone = (2.0 * std::f32::consts::PI * freq * t).sin()
                + 0.25 * (4.0 * std::f32::consts::PI * freq * t).sin();
            let v = if freq > 0.0 { tone * env * 0.35 * volume } else { 0.0 };
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
