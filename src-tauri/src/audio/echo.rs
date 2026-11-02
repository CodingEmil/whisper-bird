//! Echo im Mikrofon erkennen: Ohne Headset nimmt das Mikrofon auf, was die
//! Lautsprecher abspielen. Diese Passagen gehören nicht zur eigenen Stimme.
//!
//! Erkannt wird das am Lautstärkeverlauf: Ein Echo folgt dem PC-Audio mit kurzer
//! Verzögerung (Raum, Treiber). Sprechen beide gleichzeitig, passt der Verlauf
//! nicht mehr, die Passage bleibt also erhalten.

/// 20 ms bei 16 kHz.
pub const FRAME: usize = 320;
/// Prüffenster: 1 s.
const WINDOW_FRAMES: usize = 50;
/// Längste angenommene Echo-Verzögerung: 300 ms.
const MAX_LAG_FRAMES: usize = 15;
/// Ab dieser Korrelation der Verläufe kommt ein Fenster als Echo infrage.
const ECHO_CORRELATION: f32 = 0.7;
/// Ein Mikrofon-Frame gilt als laut, wenn er 10 dB über dem Grundrauschen liegt.
const ACTIVE_ABOVE_FLOOR: f32 = 1.0;
/// Erlaubte Streuung des Pegelabstands Mikrofon/PC-Audio bei einem Echo (log10, 0,4 = 4 dB).
const MAX_GAIN_SPREAD: f32 = 0.4;
/// Unter diesem Pegel (log10 der mittleren Energie, entspricht RMS ≈ 0,003) ist das PC-Audio still.
const SILENT: f32 = -5.0;
/// Kürzere eigene Passagen fallen weg (Klicks, Atmen).
const MIN_OWN: usize = 16_000 * 2 / 5;

/// Lautstärkeverlauf: log10 der mittleren Energie je 20-ms-Frame.
pub fn envelope(x: &[f32]) -> Vec<f32> {
    x.chunks(FRAME)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32 + 1e-7).log10())
        .collect()
}

fn pearson(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n < 2 {
        return 0.0;
    }
    let (a, b) = (&a[..n], &b[..n]);
    let ma = a.iter().sum::<f32>() / n as f32;
    let mb = b.iter().sum::<f32>() / n as f32;
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        cov += (x - ma) * (y - mb);
        va += (x - ma) * (x - ma);
        vb += (y - mb) * (y - mb);
    }
    if va < 1e-6 || vb < 1e-6 {
        return 0.0;
    }
    cov / (va * vb).sqrt()
}

/// Ist das Mikrofon in den Frames `from..to` nur ein Echo des PC-Audios?
/// `floor` ist das Grundrauschen des Mikrofons (log10 der Energie).
///
/// Echo heißt: Der Verlauf folgt dem PC-Audio bei einer Verzögerung bis 300 ms,
/// der Pegelabstand bleibt fast gleich, und wo das PC-Audio schweigt, ist das
/// Mikrofon kaum lauter als sein Rauschen – ein Echo braucht eine Quelle.
fn is_echo(mic: &[f32], sys: &[f32], from: usize, to: usize, floor: f32) -> bool {
    let to = to.min(mic.len());
    if to <= from + 5 {
        return false;
    }
    let lo = from.saturating_sub(MAX_LAG_FRAMES);
    let sys_active = sys.get(lo..to.min(sys.len())).is_some_and(|s| s.iter().any(|&e| e > SILENT));
    if !sys_active {
        return false;
    }
    let loud = |m: &[f32]| m.iter().filter(|&&e| e > floor + ACTIVE_ABOVE_FLOOR).count();
    // Kaum über dem Rauschen, während das PC-Audio läuft: höchstens ein leises Echo.
    if loud(&mic[from..to]) < 5 {
        return true;
    }
    // Am Aufnahmebeginn fehlt das PC-Audio davor: dann ein etwas kürzerer Vergleich.
    (0..=MAX_LAG_FRAMES).filter(|&lag| to > from.max(lag) + 5).any(|lag| {
        let start = from.max(lag);
        let Some(s) = sys.get(start - lag..to - lag) else {
            return false;
        };
        let m = &mic[start..to];
        let unexplained = m
            .iter()
            .zip(s)
            .filter(|(&me, &se)| me > floor + ACTIVE_ABOVE_FLOOR && se <= SILENT)
            .count();
        unexplained * 10 <= loud(m) && pearson(m, s) > ECHO_CORRELATION && steady_gain(m, s, floor)
    })
}

/// Ein Echo ist das PC-Audio, nur leiser: Wo beide laut sind, bleibt der
/// Pegelabstand fast gleich (Streuung unter 4 dB).
fn steady_gain(mic: &[f32], sys: &[f32], floor: f32) -> bool {
    let diffs: Vec<f32> = mic
        .iter()
        .zip(sys)
        .filter(|(&me, &se)| me > floor + ACTIVE_ABOVE_FLOOR && se > SILENT)
        .map(|(me, se)| me - se)
        .collect();
    if diffs.len() < 5 {
        return true;
    }
    let mean = diffs.iter().sum::<f32>() / diffs.len() as f32;
    let var = diffs.iter().map(|d| (d - mean) * (d - mean)).sum::<f32>() / diffs.len() as f32;
    var.sqrt() < MAX_GAIN_SPREAD
}

/// Grundrauschen: der leiseste Zehntel der Frames.
fn noise_floor(env: &[f32]) -> f32 {
    if env.is_empty() {
        return -7.0;
    }
    let mut v = env.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 10]
}

/// Die Teile der Mikrofon-Sprachabschnitte `segments` (Samplebereiche), in denen
/// wirklich am Mikrofon gesprochen wird – ohne Echo des PC-Audios `sys`.
pub fn own_speech(mic: &[f32], sys: &[f32], segments: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let me = envelope(mic);
    let se = envelope(sys);
    let floor = noise_floor(&me);
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &(a, b) in segments {
        let (fa, fb) = (a / FRAME, b.div_ceil(FRAME).min(me.len()));
        let mut windows: Vec<(usize, usize, bool)> = Vec::new();
        let mut f = fa;
        while f < fb {
            // Ein kurzer Rest hängt am letzten Fenster.
            let end = if fb - f < WINDOW_FRAMES * 3 / 2 { fb } else { f + WINDOW_FRAMES };
            windows.push((f, end, is_echo(&me, &se, f, end, floor)));
            f = end;
        }
        // Ein einzelnes „Echo“-Fenster mitten in eigener Rede ist eher ein Zufallstreffer.
        let flags: Vec<bool> = windows.iter().map(|w| w.2).collect();
        for i in 1..windows.len().saturating_sub(1) {
            if flags[i] && !flags[i - 1] && !flags[i + 1] {
                windows[i].2 = false;
            }
        }
        for (f, end, echo) in windows {
            if echo {
                continue;
            }
            let (sa, sb) = ((f * FRAME).max(a), (end * FRAME).min(b));
            match out.last_mut() {
                Some(last) if last.1 >= sa => last.1 = last.1.max(sb),
                _ => out.push((sa, sb)),
            }
        }
    }
    out.retain(|r| r.1 - r.0 >= MIN_OWN);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: usize = 16_000;

    /// Sprachähnliches Signal: Rauschen in Silben von 120–300 ms, dazwischen Pausen.
    fn speech(seconds: usize, seed: u64) -> Vec<f32> {
        let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let mut rnd = move || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((state >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0
        };
        let mut out = Vec::with_capacity(seconds * SR);
        while out.len() < seconds * SR {
            let syllable = (0.12 + 0.09 * (rnd() + 1.0)) * SR as f32;
            let gain = 0.05 + 0.1 * (rnd() + 1.0);
            for i in 0..syllable as usize {
                let shape = (std::f32::consts::PI * i as f32 / syllable).sin();
                out.push(rnd() * gain * shape);
            }
            let pause = if rnd() > 0.6 { 0.3 } else { 0.05 };
            out.extend(std::iter::repeat(0.0).take((pause * SR as f32) as usize));
        }
        out.truncate(seconds * SR);
        out
    }

    /// Was ein Mikrofon von `sys` aus den Lautsprechern mitbekommt, mit Grundrauschen.
    fn echo_of(sys: &[f32], delay_ms: usize, gain: f32) -> Vec<f32> {
        let d = delay_ms * 16;
        let hiss = noise(sys.len(), delay_ms as u64);
        (0..sys.len()).map(|i| if i >= d { sys[i - d] * gain } else { 0.0 } + hiss[i]).collect()
    }

    /// Gleichmäßiges Rauschen eines Mikrofons (RMS ≈ 0,002).
    fn noise(len: usize, seed: u64) -> Vec<f32> {
        let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (((state >> 33) as f32 / (1u64 << 31) as f32) * 2.0 - 1.0) * 0.0035
            })
            .collect()
    }

    /// Eigene Stimme am Mikrofon, mit Grundrauschen.
    fn voice(seconds: usize, seed: u64) -> Vec<f32> {
        speech(seconds, seed).iter().zip(noise(seconds * SR, seed + 77)).map(|(a, b)| a + b).collect()
    }

    fn total(r: &[(usize, usize)]) -> usize {
        r.iter().map(|r| r.1 - r.0).sum()
    }

    #[test]
    fn echo_faellt_weg() {
        let sys = speech(10, 1);
        for delay in [0, 40, 120, 250] {
            let mic = echo_of(&sys, delay, 0.08);
            let own = own_speech(&mic, &sys, &[(0, mic.len())]);
            assert!(own.is_empty(), "Verzögerung {delay} ms: {own:?}");
        }
    }

    #[test]
    fn eigene_stimme_bleibt() {
        let sys = speech(10, 1);
        let mic = voice(10, 2);
        assert_eq!(own_speech(&mic, &sys, &[(0, mic.len())]), vec![(0, mic.len())]);
        // PC-Audio still: alles gehört dem Mikrofon.
        let silent = vec![0.0; mic.len()];
        assert_eq!(own_speech(&mic, &silent, &[(SR, 5 * SR)]), vec![(SR, 5 * SR)]);
    }

    #[test]
    fn gleichzeitig_sprechen_bleibt() {
        let sys = speech(10, 3);
        let own = speech(10, 4);
        let mic: Vec<f32> = own.iter().zip(echo_of(&sys, 60, 0.05)).map(|(a, b)| a + b).collect();
        let own = own_speech(&mic, &sys, &[(0, mic.len())]);
        assert!(total(&own) >= 9 * SR, "{own:?}");
    }

    #[test]
    fn wechsel_wird_getrennt() {
        // 0–6 s: ich spreche, das PC-Audio schweigt. 6–12 s: Gegenseite, im Mikrofon nur als Echo.
        let mut sys = vec![0.0; 6 * SR];
        sys.extend(speech(6, 5));
        let mut mic = voice(6, 6);
        mic.extend(echo_of(&sys, 80, 0.1)[6 * SR..].iter());
        let own = own_speech(&mic, &sys, &[(0, mic.len())]);
        assert_eq!(own.len(), 1, "{own:?}");
        assert_eq!(own[0].0, 0);
        assert!(own[0].1.abs_diff(6 * SR) <= SR, "{own:?}");
    }

    #[test]
    fn kurze_reste_fallen_weg() {
        let mic = voice(5, 7);
        let silent = vec![0.0; mic.len()];
        assert!(own_speech(&mic, &silent, &[(0, SR / 10)]).is_empty());
    }
}
