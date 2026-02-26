//! Data Converters - Transform raw data to base-12 sequences
//!
//! IMPORTANT: All conversion happens ON-THE-FLY during plotting.
//! Raw data files are loaded and converted when needed, never stored as base12.
//!
//! Each converter module handles a specific data type:
//! - math: Mathematical constants, fractals, sequences (no downloads)
//! - audio: Spectrogram analysis of WAV/MP3 files
//! - dna: ACGT base-4 to base-12 from FASTA files
//! - finance: Price deltas from JSON price arrays
//! - cosmos: LIGO strain data from .txt.gz files

pub mod audio;
pub mod math;

use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConvertError {
    #[error("Invalid input data: {0}")]
    InvalidInput(String),
    #[error("Conversion failed: {0}")]
    ConversionFailed(String),
}

/// Normalize values to 0-11 range
pub fn normalize_to_base12(values: &[f64]) -> Vec<u8> {
    if values.is_empty() {
        return vec![0];
    }

    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;

    if range == 0.0 {
        return vec![6; values.len()]; // Middle value
    }

    values
        .iter()
        .map(|&v| {
            let normalized = (v - min) / range;
            (normalized * 11.99).floor() as u8
        })
        .collect()
}

/// DNA Converter: ACGT (base-4) to base-12 streaming
pub fn convert_dna(sequence: &str) -> Vec<u8> {
    let base4_map = |c: char| -> Option<u64> {
        match c {
            'A' | 'a' => Some(0),
            'C' | 'c' => Some(1),
            'G' | 'g' => Some(2),
            'T' | 't' => Some(3),
            _ => None,
        }
    };

    let mut base12 = Vec::new();
    let mut accumulator: u64 = 0;
    let mut power: u64 = 1;

    for ch in sequence.chars() {
        if let Some(digit) = base4_map(ch) {
            accumulator += digit * power;
            power *= 4;

            // Emit base-12 digits when accumulator is large enough
            while accumulator >= 12 {
                base12.push((accumulator % 12) as u8);
                accumulator /= 12;
            }
        }
    }

    // Emit remaining
    if accumulator > 0 || base12.is_empty() {
        base12.push((accumulator % 12) as u8);
    }

    base12
}

/// Finance Converter: Price deltas to base-12
pub fn convert_finance(prices: &[f64]) -> Vec<u8> {
    if prices.len() < 2 {
        return vec![0];
    }

    let deltas: Vec<f64> = prices
        .windows(2)
        .map(|w| (w[1] - w[0]) / w[0])
        .collect();

    normalize_to_base12(&deltas)
}

/// Cosmos Converter: Strain amplitude to base-12
pub fn convert_cosmos(strain: &[f64]) -> Vec<u8> {
    normalize_to_base12(strain)
}

// ============================================================================
// Raw file loaders - load file and convert to base12 on-the-fly
// ============================================================================

/// Load FASTA file and convert to base-12
pub fn load_dna_raw(path: &Path) -> anyhow::Result<Vec<u8>> {
    let content = std::fs::read_to_string(path)?;

    // Extract sequence (skip header lines starting with >)
    let sequence: String = content
        .lines()
        .filter(|line| !line.starts_with('>'))
        .collect::<Vec<_>>()
        .join("");

    if sequence.is_empty() {
        anyhow::bail!("No sequence data in FASTA file");
    }

    Ok(convert_dna(&sequence))
}

/// Load finance JSON (raw prices) and convert to base-12
pub fn load_finance_raw(path: &Path) -> anyhow::Result<Vec<u8>> {
    let content = std::fs::read_to_string(path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;

    let prices: Vec<f64> = json["prices"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("No prices array in JSON"))?
        .iter()
        .filter_map(|v| v.as_f64())
        .collect();

    if prices.len() < 2 {
        anyhow::bail!("Not enough price data");
    }

    Ok(convert_finance(&prices))
}

/// Load cosmos strain data (.txt.gz) and convert to base-12
pub fn load_cosmos_raw(path: &Path) -> anyhow::Result<Vec<u8>> {
    use std::io::{BufRead, BufReader};
    use flate2::read::GzDecoder;

    let file = std::fs::File::open(path)?;
    let decoder = GzDecoder::new(file);
    let reader = BufReader::new(decoder);

    let mut strain_values: Vec<f64> = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Parse strain value (scientific notation supported)
        if let Ok(value) = trimmed.parse::<f64>() {
            if value.is_finite() {
                strain_values.push(value);
            }
        }
    }

    if strain_values.is_empty() {
        anyhow::bail!("No valid strain data in file");
    }

    Ok(convert_cosmos(&strain_values))
}

/// Load audio file (WAV or MP3) and convert to base-12
pub fn load_audio_raw(path: &Path) -> anyhow::Result<Vec<u8>> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "wav" => audio::wav_to_base12(path),
        "mp3" => {
            // For MP3, use symphonia to decode
            load_mp3_raw(path)
        }
        _ => anyhow::bail!("Unsupported audio format: {}", ext),
    }
}

/// Load MP3 file and convert to base-12
fn load_mp3_raw(path: &Path) -> anyhow::Result<Vec<u8>> {
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    use symphonia::core::audio::SampleBuffer;

    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("mp3");

    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)?;

    let mut format = probed.format;
    let track = format.default_track()
        .ok_or_else(|| anyhow::anyhow!("No audio track found"))?;

    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let track_id = track.id;
    let decoder_opts = DecoderOptions::default();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &decoder_opts)?;

    let mut samples: Vec<f32> = Vec::new();
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        match format.next_packet() {
            Ok(packet) => {
                // Skip packets from other tracks
                if packet.track_id() != track_id {
                    continue;
                }

                match decoder.decode(&packet) {
                    Ok(decoded) => {
                        // Get spec and channels before consuming decoded
                        let spec = *decoded.spec();
                        let channels = spec.channels.count();
                        let duration = decoded.capacity() as u64;

                        // Create sample buffer if needed
                        if sample_buf.is_none() {
                            sample_buf = Some(SampleBuffer::<f32>::new(duration, spec));
                        }

                        // Copy samples to buffer
                        if let Some(ref mut buf) = sample_buf {
                            buf.copy_interleaved_ref(decoded);

                            // Get mono by taking every N samples (N = channels)
                            for chunk in buf.samples().chunks(channels) {
                                if let Some(&first) = chunk.first() {
                                    samples.push(first);
                                }
                            }
                        }

                        // Limit to ~30 seconds worth of samples
                        if samples.len() > sample_rate as usize * 30 {
                            break;
                        }
                    }
                    Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
                    Err(_) => break,
                }
            }
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => break,
        }
    }

    if samples.is_empty() {
        anyhow::bail!("No audio samples decoded from MP3");
    }

    Ok(audio::audio_to_base12(&samples, sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dna_conversion() {
        let dna = "ACGT";
        let base12 = convert_dna(dna);
        assert!(!base12.is_empty());
        assert!(base12.iter().all(|&d| d < 12));
    }

    #[test]
    fn test_normalize() {
        let values = vec![0.0, 0.5, 1.0];
        let base12 = normalize_to_base12(&values);
        assert_eq!(base12[0], 0);
        assert_eq!(base12[2], 11);
    }

    #[test]
    fn test_finance() {
        let prices = vec![100.0, 110.0, 105.0, 115.0];
        let base12 = convert_finance(&prices);
        assert_eq!(base12.len(), 3); // n-1 deltas
    }
}
