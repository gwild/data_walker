//! Audio Converter - Spectrogram analysis to base-12 and MIDI notes
//!
//! Converts audio files to:
//! - Base-12 sequences for 3D walk visualization
//! - MIDI note sequences for accurate audio synthesis

use rustfft::{FftPlanner, num_complex::Complex};
use std::path::Path;

/// Window size for FFT (samples per frame)
const FFT_SIZE: usize = 2048;

/// Hop size between frames (50% overlap)
const HOP_SIZE: usize = 1024;

/// Minimum frequency for pitch detection (below piano range)
const MIN_PITCH_HZ: f32 = 20.0;

/// Maximum frequency for pitch detection (above piano range)
const MAX_PITCH_HZ: f32 = 4200.0;

/// A MIDI note with timing information
#[derive(Clone, Copy, Debug)]
pub struct MidiNote {
    /// MIDI note number (0-127, where 69 = A4 = 440Hz)
    pub note: u8,
    /// Amplitude/velocity (0.0-1.0)
    pub velocity: f32,
}

impl MidiNote {
    /// Get note name (e.g., "C4", "A#5")
    pub fn name(&self) -> String {
        let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
        let octave = (self.note / 12) as i32 - 1;
        let note_idx = (self.note % 12) as usize;
        format!("{}{}", names[note_idx], octave)
    }
}

/// Convert frequency to MIDI note number
/// Formula: midi = 69 + 12 * log2(freq / 440)
fn freq_to_midi(freq: f32) -> u8 {
    if freq < MIN_PITCH_HZ {
        return 0;
    }
    let midi = 69.0 + 12.0 * (freq / 440.0).log2();
    midi.round().clamp(0.0, 127.0) as u8
}

/// Convert MIDI note to base-12 for visualization
/// Maps chromatic notes (0-11 within octave) directly
pub fn midi_to_base12(midi: u8) -> u8 {
    midi % 12
}

/// Extract MIDI notes from audio using parabolic interpolation for sub-bin accuracy
/// Returns a sequence of MIDI notes with velocity information
pub fn audio_to_midi_notes(samples: &[f32], sample_rate: u32) -> Vec<MidiNote> {
    if samples.len() < FFT_SIZE {
        return vec![MidiNote { note: 69, velocity: 0.5 }]; // A4 default
    }

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(FFT_SIZE);

    let mut notes: Vec<MidiNote> = Vec::new();
    let mut pos = 0;

    // Hann window for smoother spectrum
    let window: Vec<f32> = (0..FFT_SIZE)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / FFT_SIZE as f32).cos()))
        .collect();

    // Calculate bin range for musical pitch detection
    let min_bin = (MIN_PITCH_HZ * FFT_SIZE as f32 / sample_rate as f32).ceil() as usize;
    let max_bin = (MAX_PITCH_HZ * FFT_SIZE as f32 / sample_rate as f32).floor() as usize;
    let max_bin = max_bin.min(FFT_SIZE / 2 - 1);

    while pos + FFT_SIZE <= samples.len() {
        // Apply window and convert to complex
        let mut buffer: Vec<Complex<f32>> = samples[pos..pos + FFT_SIZE]
            .iter()
            .zip(window.iter())
            .map(|(&s, &w)| Complex::new(s * w, 0.0))
            .collect();

        // FFT
        fft.process(&mut buffer);

        // Find dominant frequency in musical range with parabolic interpolation
        let magnitudes: Vec<f32> = buffer[min_bin..=max_bin]
            .iter()
            .map(|c| c.norm())
            .collect();

        if let Some((peak_idx, &peak_mag)) = magnitudes
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        {
            let actual_bin = peak_idx + min_bin;

            // Parabolic interpolation for sub-bin accuracy
            let interpolated_bin = if peak_idx > 0 && peak_idx < magnitudes.len() - 1 {
                let alpha = magnitudes[peak_idx - 1];
                let beta = magnitudes[peak_idx];
                let gamma = magnitudes[peak_idx + 1];
                let p = 0.5 * (alpha - gamma) / (alpha - 2.0 * beta + gamma);
                actual_bin as f32 + p
            } else {
                actual_bin as f32
            };

            // Convert bin to frequency
            let freq = interpolated_bin * sample_rate as f32 / FFT_SIZE as f32;

            // Convert to MIDI note
            let midi_note = freq_to_midi(freq);

            // Calculate velocity from magnitude (normalized)
            let velocity = (peak_mag / (FFT_SIZE as f32 / 2.0)).min(1.0);

            notes.push(MidiNote {
                note: midi_note,
                velocity: velocity.max(0.1), // Minimum velocity for audibility
            });
        } else {
            // Silence or no clear pitch - use A4 as placeholder
            notes.push(MidiNote { note: 69, velocity: 0.0 });
        }

        pos += HOP_SIZE;
    }

    if notes.is_empty() {
        notes.push(MidiNote { note: 69, velocity: 0.5 });
    }

    notes
}

/// Convert audio samples to base-12 using MIDI note detection
/// This preserves musical pitch relationships (chromatic scale)
pub fn audio_to_base12(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    // Use MIDI note detection and convert to base-12 (chromatic mapping)
    audio_to_midi_notes(samples, sample_rate)
        .iter()
        .map(|n| midi_to_base12(n.note))
        .collect()
}

/// Convert audio samples to base-4 using MIDI note detection
/// Maps notes to 4 values (useful for simpler visualization)
pub fn audio_to_base4(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    // Use MIDI detection and map to base-4 (3 notes per value)
    audio_to_midi_notes(samples, sample_rate)
        .iter()
        .map(|n| (n.note % 12) / 3) // 0-2=0, 3-5=1, 6-8=2, 9-11=3
        .collect()
}

/// Load audio samples from a WAV file
fn load_wav_samples(path: &Path) -> anyhow::Result<(Vec<f32>, u32)> {
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

    Ok((mono, sample_rate))
}

/// Load WAV file and extract MIDI notes
pub fn wav_to_midi_notes(path: &Path) -> anyhow::Result<Vec<MidiNote>> {
    let (samples, sample_rate) = load_wav_samples(path)?;
    Ok(audio_to_midi_notes(&samples, sample_rate))
}

/// Load WAV file and convert to base digits
pub fn wav_to_base(path: &Path, base: u32) -> anyhow::Result<Vec<u8>> {
    let (samples, sample_rate) = load_wav_samples(path)?;

    Ok(match base {
        4 => audio_to_base4(&samples, sample_rate),
        6 => audio_to_base12(&samples, sample_rate).iter().map(|&d| d % 6).collect(),
        _ => audio_to_base12(&samples, sample_rate),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_sine(freq: f32, duration: f32, sample_rate: u32) -> Vec<f32> {
        (0..(sample_rate as f32 * duration) as usize)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate as f32).sin())
            .collect()
    }

    #[test]
    fn test_audio_to_base12() {
        let sample_rate = 44100;
        let samples = generate_sine(440.0, 1.0, sample_rate); // A4

        let base12 = audio_to_base12(&samples, sample_rate);
        assert!(!base12.is_empty());
        assert!(base12.iter().all(|&d| d < 12));

        // A4 should map to base12 value 9 (A is the 10th note, 0-indexed = 9)
        assert!(base12.iter().all(|&d| d == 9), "A4 should map to 9, got {:?}", base12[0]);
    }

    #[test]
    fn test_midi_note_accuracy() {
        let sample_rate = 44100;

        // Test various notes
        let test_cases = [
            (261.63, 60, "C4"),   // Middle C
            (440.0, 69, "A4"),    // A440
            (523.25, 72, "C5"),   // C5
            (329.63, 64, "E4"),   // E4
            (130.81, 48, "C3"),   // C3
        ];

        for (freq, expected_midi, name) in test_cases {
            let samples = generate_sine(freq, 0.5, sample_rate);
            let notes = audio_to_midi_notes(&samples, sample_rate);

            assert!(!notes.is_empty(), "Should detect notes for {}", name);

            // Check that most detected notes match expected (allow ±1 for FFT precision)
            let matching = notes.iter().filter(|n| {
                (n.note as i32 - expected_midi as i32).abs() <= 1
            }).count();

            let match_ratio = matching as f32 / notes.len() as f32;
            assert!(
                match_ratio > 0.9,
                "{} ({:.1}Hz) should be MIDI {}, got {:?} (match ratio: {:.1}%)",
                name, freq, expected_midi, notes[0].note, match_ratio * 100.0
            );
        }
    }

    #[test]
    fn test_midi_note_name() {
        let note = MidiNote { note: 60, velocity: 1.0 };
        assert_eq!(note.name(), "C4");

        let note = MidiNote { note: 69, velocity: 1.0 };
        assert_eq!(note.name(), "A4");

        let note = MidiNote { note: 72, velocity: 1.0 };
        assert_eq!(note.name(), "C5");
    }

    #[test]
    fn test_freq_to_midi() {
        assert_eq!(freq_to_midi(440.0), 69);  // A4
        assert_eq!(freq_to_midi(261.63), 60); // C4
        assert_eq!(freq_to_midi(523.25), 72); // C5
        assert_eq!(freq_to_midi(880.0), 81);  // A5
    }
}
