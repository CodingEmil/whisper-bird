//! Audio- und Videodateien zu 16 kHz mono dekodieren. Erst symphonia
//! (WAV, MP3, FLAC, OGG, M4A/MP4/MOV, MKV/WebM, AIFF …), sonst ffmpeg, falls installiert.

use super::resample::Resampler;
use super::SAMPLE_RATE;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

pub const MEDIA_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "m4a", "aac", "flac", "ogg", "oga", "opus", "wma", "aif", "aiff", "caf", "mp4",
    "m4v", "mov", "mkv", "webm", "avi", "wmv", "mpeg", "mpg", "3gp", "amr",
];

pub fn is_media_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| MEDIA_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Dekodiert eine Datei. `progress` bekommt Werte von 0.0 bis 1.0.
pub fn decode_file(
    path: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f32),
) -> anyhow::Result<Vec<f32>> {
    match decode_symphonia(path, cancel, &mut progress) {
        Ok(samples) if !samples.is_empty() => Ok(samples),
        Ok(_) => decode_ffmpeg(path).map_err(|e| anyhow::anyhow!("Keine Audiospur gefunden ({e})")),
        Err(e) => {
            if cancel.load(Ordering::SeqCst) {
                return Err(e);
            }
            log::info!("symphonia konnte {} nicht lesen ({e:#}), versuche ffmpeg", path.display());
            decode_ffmpeg(path).map_err(|fe| {
                anyhow::anyhow!("Format nicht unterstützt ({e:#}). Mit installiertem ffmpeg klappt es meist: {fe:#}")
            })
        }
    }
}

fn decode_symphonia(
    path: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> anyhow::Result<Vec<f32>> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL && t.codec_params.sample_rate.is_some())
        .ok_or_else(|| anyhow::anyhow!("keine Audiospur"))?;
    let track_id = track.id;
    let rate = track.codec_params.sample_rate.unwrap_or(SAMPLE_RATE);
    let total_frames = track.codec_params.n_frames.unwrap_or(0);
    let mut decoder =
        symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;

    let mut resampler = Resampler::new(rate, SAMPLE_RATE);
    let mut out = Vec::new();
    let mut mono = Vec::new();
    let mut decoded_frames: u64 = 0;
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        if cancel.load(Ordering::SeqCst) {
            anyhow::bail!("abgebrochen");
        }
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(e)) => {
                log::debug!("Dekodierfehler übersprungen: {e}");
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        let spec = *decoded.spec();
        let channels = spec.channels.count().max(1);
        let needed = decoded.capacity() as u64;
        if sample_buf.as_ref().map(|b| b.capacity() < needed as usize * channels).unwrap_or(true) {
            sample_buf = Some(SampleBuffer::<f32>::new(needed, spec));
        }
        let buf = sample_buf.as_mut().unwrap();
        buf.copy_interleaved_ref(decoded);
        let samples = buf.samples();
        mono.clear();
        for frame in samples.chunks(channels) {
            mono.push(frame.iter().sum::<f32>() / channels as f32);
        }
        decoded_frames += (samples.len() / channels) as u64;
        resampler.process(&mono, &mut out);
        if total_frames > 0 {
            progress((decoded_frames as f32 / total_frames as f32).min(1.0));
        }
    }
    resampler.flush(&mut out);
    progress(1.0);
    Ok(out)
}

fn decode_ffmpeg(path: &Path) -> anyhow::Result<Vec<f32>> {
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.args(["-nostdin", "-v", "error", "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "f32le", "-"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = cmd.output().map_err(|e| anyhow::anyhow!("ffmpeg nicht gefunden: {e}"))?;
    if !output.status.success() {
        anyhow::bail!("ffmpeg: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(output
        .stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}
