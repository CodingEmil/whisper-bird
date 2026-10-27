pub mod decode;
pub mod ducking;
pub mod loopback;
pub mod mic;
pub mod resample;
pub mod sounds;

pub const SAMPLE_RATE: u32 = 16_000;

/// RMS-Pegel eines Blocks, grob auf 0..1 skaliert (für die Waveform-Anzeige).
pub fn level(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
    // Sprache liegt typischerweise bei 0.01–0.2 RMS; logarithmisch aufspreizen.
    let db = 20.0 * rms.max(1e-6).log10();
    ((db + 55.0) / 45.0).clamp(0.0, 1.0)
}

/// Schreibt 16-kHz-Mono-Audio als WAV (für Meetings und Debug).
pub fn write_wav(path: &std::path::Path, samples: &[f32]) -> anyhow::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for &s in samples {
        writer.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(())
}
