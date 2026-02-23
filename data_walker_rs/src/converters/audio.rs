//! Audio Converter - Spectrogram analysis to base-12
//!
//! Converts audio files to base-12 sequences using FFT spectrogram analysis.
//! The dominant frequency in each time window is normalized to 0-11.

use rustfft::{FftPlanner, num_complex::Complex};
use std::path::Path;

/// Window size for FFT (samples per frame)
const FFT_SIZE: usize = 2048;

/// Hop size between frames (50% overlap)
const HOP_SIZE: usize = 1024;

/// Convert audio samples to base-12 using spectrogram analysis
pub fn audio_to_base12(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    if samples.len() < FFT_SIZE {
        return vec![6]; // Not enough data, return middle value
    }

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);

    let mut base12 = Vec::new();
    let mut pos = 0;

    // Hann window for smoother spectrum
    let window: Vec<f32> = (0..FFT_SIZE)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos()))
        .collect();

    while pos + FFT_SIZE <= samples.len() {
        // Apply window and convert to complex
        let mut buffer: Vec<Complex<f32>> = samples[pos..pos + FFT_SIZE]
            .iter()
            .zip(window.iter())
            .map(|(&s, &w)| Complex::new(s * w, 0.0))
            .collect();

        // FFT
        fft.process(&mut buffer);

        // Find dominant frequency (only positive frequencies)
        let half = FFT_SIZE / 2;
        let (max_bin, _max_magnitude) = buffer[1..half]
            .iter()
            .enumerate()
            .map(|(i, c)| (i + 1, c.norm()))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap_or((1, 0.0));

        // Convert bin to frequency
        let freq = max_bin as f32 * sample_rate as f32 / FFT_SIZE as f32;

        // Map frequency to base-12 using log scale
        // Human hearing range: ~20Hz to ~20kHz
        // Log scale mapping to 0-11
        let log_freq = (freq.max(20.0) / 20.0).ln();
        let log_max = (20000.0_f32 / 20.0).ln(); // ln(1000)
        let normalized = (log_freq / log_max).clamp(0.0, 1.0);
        let digit = (normalized * 11.99).floor() as u8;

        base12.push(digit);
        pos += HOP_SIZE;
    }

    if base12.is_empty() {
        base12.push(6);
    }

    base12
}

/// Load WAV file and convert to base-12
pub fn wav_to_base12(path: &Path) -> anyhow::Result<Vec<u8>> {
    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let sample_rate = spec.sample_rate;

    // Read samples and convert to f32
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.into_samples::<f32>().filter_map(|s| s.ok()).collect()
        }
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let max_val = (1i32 << (bits - 1)) as f32;
            reader
                .into_samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect()
        }
    };

    // If stereo, convert to mono by averaging channels
    let mono: Vec<f32> = if spec.channels == 2 {
        samples.chunks(2).map(|c| (c[0] + c.get(1).unwrap_or(&0.0)) / 2.0).collect()
    } else {
        samples
    };

    Ok(audio_to_base12(&mono, sample_rate))
}

/// Convert raw audio bytes (WAV format) to base-12
pub fn wav_bytes_to_base12(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    let cursor = std::io::Cursor::new(data);
    let reader = hound::WavReader::new(cursor)?;
    let spec = reader.spec();
    let sample_rate = spec.sample_rate;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.into_samples::<f32>().filter_map(|s| s.ok()).collect()
        }
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let max_val = (1i32 << (bits - 1)) as f32;
            reader
                .into_samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect()
        }
    };

    let mono: Vec<f32> = if spec.channels == 2 {
        samples.chunks(2).map(|c| (c[0] + c.get(1).unwrap_or(&0.0)) / 2.0).collect()
    } else {
        samples
    };

    Ok(audio_to_base12(&mono, sample_rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_to_base12() {
        // Generate a simple sine wave
        let sample_rate = 44100;
        let duration = 1.0; // 1 second
        let freq = 440.0; // A4

        let samples: Vec<f32> = (0..(sample_rate as f32 * duration) as usize)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate as f32).sin())
            .collect();

        let base12 = audio_to_base12(&samples, sample_rate);
        assert!(!base12.is_empty());
        assert!(base12.iter().all(|&d| d < 12));
    }
}
