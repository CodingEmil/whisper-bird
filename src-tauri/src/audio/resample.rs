//! Streaming-Resampler (gefensterter Sinc, Polyphasen-Tabelle). Klein, ohne
//! externe Abhängigkeit und für Sprache mehr als ausreichend.

use std::f64::consts::PI;

const PHASES: usize = 256;
const ZERO_CROSSINGS: f64 = 10.0;

pub struct Resampler {
    step: f64,
    half: usize,
    table: Vec<f32>,
    buf: Vec<f32>,
    pos: f64,
    passthrough: bool,
}

impl Resampler {
    pub fn new(from: u32, to: u32) -> Self {
        let step = from as f64 / to as f64;
        // Tiefpass knapp unter der Nyquist-Frequenz des Ziels (bzw. der Quelle).
        let fc = (1.0 / step).min(1.0) * 0.92;
        let half = (ZERO_CROSSINGS / fc).ceil() as usize;
        let n = 2 * half * PHASES + 1;
        let mut table = Vec::with_capacity(n);
        for m in 0..n {
            let d = m as f64 / PHASES as f64 - half as f64;
            let x = d / half as f64;
            let window = if x.abs() >= 1.0 {
                0.0
            } else {
                0.42 + 0.5 * (PI * x).cos() + 0.08 * (2.0 * PI * x).cos()
            };
            let arg = PI * fc * d;
            let sinc = if arg.abs() < 1e-9 { 1.0 } else { arg.sin() / arg };
            table.push((fc * sinc * window) as f32);
        }
        Self {
            step,
            half,
            table,
            // Vorlauf aus Nullen, damit die erste Ausgabe bei Sample 0 zentriert ist.
            buf: vec![0.0; half],
            pos: half as f64,
            passthrough: from == to,
        }
    }

    /// Nimmt weitere Eingabe-Samples und hängt die fertigen Ausgabe-Samples an `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.passthrough {
            out.extend_from_slice(input);
            return;
        }
        self.buf.extend_from_slice(input);
        self.drain(out);
    }

    /// Restliche Samples ausgeben (am Ende eines Streams).
    pub fn flush(&mut self, out: &mut Vec<f32>) {
        if self.passthrough {
            return;
        }
        let pad = vec![0.0; self.half + 1];
        self.buf.extend_from_slice(&pad);
        self.drain(out);
    }

    fn drain(&mut self, out: &mut Vec<f32>) {
        let half = self.half as isize;
        loop {
            let i0 = self.pos.floor() as isize;
            if i0 + half >= self.buf.len() as isize {
                break;
            }
            let frac = self.pos - i0 as f64;
            let mut acc = 0.0f32;
            for j in (-half + 1)..=half {
                let d = j as f64 - frac;
                let m = ((d + half as f64) * PHASES as f64).round() as usize;
                acc += self.buf[(i0 + j) as usize] * self.table[m];
            }
            out.push(acc);
            self.pos += self.step;
        }
        // Verbrauchte Eingabe verwerfen, aber genug Historie für den Kernel behalten.
        let keep_from = (self.pos.floor() as isize - half).max(0) as usize;
        if keep_from > 0 {
            self.buf.drain(..keep_from);
            self.pos -= keep_from as f64;
        }
    }
}

/// Einmal-Resampling eines kompletten Puffers.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to {
        return input.to_vec();
    }
    let mut r = Resampler::new(from, to);
    let mut out = Vec::with_capacity((input.len() as f64 * to as f64 / from as f64) as usize + 64);
    r.process(input, &mut out);
    r.flush(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_and_tone_survive() {
        let from = 48_000;
        let input: Vec<f32> = (0..from)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / from as f32).sin() * 0.5)
            .collect();
        let out = resample(&input, from, 16_000);
        assert!((out.len() as i64 - 16_000).abs() < 40, "len {}", out.len());
        let rms = (out[1000..15000].iter().map(|s| s * s).sum::<f32>() / 14000.0).sqrt();
        assert!((rms - 0.3535).abs() < 0.02, "rms {rms}");
    }
}
