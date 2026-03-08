//! Real-time audio synthesis from MIDI notes and base-12 walk data
//!
//! Supports note-accurate synthesis from detected MIDI notes,
//! as well as legacy base-12 synthesis for visualization-based audio.

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use rodio::{OutputStreamHandle, Sink, Source};
use tracing::debug;

use crate::converters::audio::MidiNote;

/// Synthesis methods for generating audio
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum SynthMethod {
    #[default]
    ChromaticNotes, // Accurate MIDI note playback (multi-octave)
    SineTones,      // Pure sine waves at detected frequencies
    Percussion,     // Drum-like sounds based on note patterns
}

/// Sample rate for synthesis
const SAMPLE_RATE: u32 = 44100;

/// Default samples per note (controls playback speed) - ~10 notes per second
const DEFAULT_SAMPLES_PER_NOTE: u32 = SAMPLE_RATE / 10;

/// A real-time audio source that synthesizes from MIDI notes
pub struct MidiSynthSource {
    notes: Vec<MidiNote>,
    method: SynthMethod,
    sample_index: usize,
    phase: f32,
    stop_flag: Arc<AtomicBool>,
    samples_per_note: u32,
}

impl MidiSynthSource {
    pub fn new(
        notes: Vec<MidiNote>,
        method: SynthMethod,
        stop_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            notes,
            method,
            sample_index: 0,
            phase: 0.0,
            stop_flag,
            samples_per_note: DEFAULT_SAMPLES_PER_NOTE,
        }
    }

    /// Create with a specific note rate (notes per second)
    pub fn with_note_rate(
        notes: Vec<MidiNote>,
        method: SynthMethod,
        stop_flag: Arc<AtomicBool>,
        notes_per_second: f32,
    ) -> Self {
        let samples_per_note = if notes_per_second > 0.0 {
            (SAMPLE_RATE as f32 / notes_per_second) as u32
        } else {
            DEFAULT_SAMPLES_PER_NOTE
        };
        Self {
            notes,
            method,
            sample_index: 0,
            phase: 0.0,
            stop_flag,
            samples_per_note,
        }
    }

    /// Get the current MIDI note based on sample position
    fn current_note(&self) -> MidiNote {
        if self.notes.is_empty() {
            return MidiNote { note: 69, velocity: 0.5 }; // Default A4
        }
        let note_index = self.sample_index / self.samples_per_note as usize;
        self.notes.get(note_index % self.notes.len())
            .copied()
            .unwrap_or(MidiNote { note: 69, velocity: 0.5 })
    }

    /// Position within current note (0.0 to 1.0)
    fn note_progress(&self) -> f32 {
        (self.sample_index % self.samples_per_note as usize) as f32 / self.samples_per_note as f32
    }

    /// Generate accurate chromatic note at correct octave
    fn generate_chromatic(&mut self) -> f32 {
        let note = self.current_note();

        // Calculate frequency from MIDI note: freq = 440 * 2^((note-69)/12)
        let freq = 440.0 * 2.0_f32.powf((note.note as f32 - 69.0) / 12.0);

        // Advance phase
        self.phase += 2.0 * std::f32::consts::PI * freq / SAMPLE_RATE as f32;
        if self.phase > 2.0 * std::f32::consts::PI {
            self.phase -= 2.0 * std::f32::consts::PI;
        }

        // ADSR-like envelope
        let progress = self.note_progress();
        let attack = (progress * 20.0).min(1.0); // Fast attack
        let release = 1.0 - ((progress - 0.8).max(0.0) / 0.2); // Release in last 20%
        let envelope = attack * release;

        // Add slight harmonics for richer sound (piano-like)
        let fundamental = self.phase.sin();
        let harmonic2 = (self.phase * 2.0).sin() * 0.3;
        let harmonic3 = (self.phase * 3.0).sin() * 0.1;

        (fundamental + harmonic2 + harmonic3) * envelope * note.velocity * 0.25
    }

    /// Generate pure sine tone at MIDI note frequency
    fn generate_sine(&mut self) -> f32 {
        let note = self.current_note();
        let freq = 440.0 * 2.0_f32.powf((note.note as f32 - 69.0) / 12.0);

        // Advance phase
        self.phase += 2.0 * std::f32::consts::PI * freq / SAMPLE_RATE as f32;
        if self.phase > 2.0 * std::f32::consts::PI {
            self.phase -= 2.0 * std::f32::consts::PI;
        }

        self.phase.sin() * note.velocity * 0.25
    }

    /// Generate percussion sound based on note
    fn generate_percussion(&mut self) -> f32 {
        let note = self.current_note();
        let progress = self.note_progress();

        // Only trigger sound at start of note
        if progress > 0.3 {
            return 0.0;
        }

        let t = progress / 0.3;
        let decay = (-t * 15.0).exp();

        // Map note to drum type (every 4 semitones = different drum)
        match (note.note / 4) % 3 {
            0 => {
                // Kick: low frequency with pitch bend
                let freq = 60.0 * (1.0 + t * 5.0).min(2.0);
                let phase = 2.0 * std::f32::consts::PI * freq * t;
                phase.sin() * decay * note.velocity * 0.5
            }
            1 => {
                // Snare: noise burst
                let noise = (self.sample_index as f32 * 12345.67).sin()
                    * (self.sample_index as f32 * 7654.32).cos();
                noise * decay * note.velocity * 0.3
            }
            _ => {
                // Hi-hat: high frequency noise
                let noise = (self.sample_index as f32 * 54321.0).sin();
                noise * (-t * 30.0).exp() * note.velocity * 0.2
            }
        }
    }
}

impl Iterator for MidiSynthSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        // Check stop flag
        if self.stop_flag.load(Ordering::Relaxed) {
            return None;
        }

        // Check if we've reached the end
        let total_samples = self.notes.len() * self.samples_per_note as usize;
        if self.sample_index >= total_samples {
            // Loop back to start
            self.sample_index = 0;
            self.phase = 0.0;
        }

        let sample = match self.method {
            SynthMethod::ChromaticNotes => self.generate_chromatic(),
            SynthMethod::SineTones => self.generate_sine(),
            SynthMethod::Percussion => self.generate_percussion(),
        };

        self.sample_index += 1;
        Some(sample)
    }
}

impl Source for MidiSynthSource {
    fn current_frame_len(&self) -> Option<usize> {
        None // Continuous source
    }

    fn channels(&self) -> u16 {
        1 // Mono
    }

    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    fn total_duration(&self) -> Option<Duration> {
        let total_samples = self.notes.len() * self.samples_per_note as usize;
        Some(Duration::from_secs_f32(total_samples as f32 / SAMPLE_RATE as f32))
    }
}

/// Create a sink with synthesized audio from MIDI notes (note-accurate)
pub fn create_midi_synth_sink(
    stream_handle: &OutputStreamHandle,
    notes: Vec<MidiNote>,
    method: SynthMethod,
    stop_flag: Arc<AtomicBool>,
) -> anyhow::Result<Sink> {
    let source = MidiSynthSource::new(notes.clone(), method, stop_flag);
    let duration = source.total_duration().map(|d| d.as_secs_f32()).unwrap_or(0.0);

    let sink = Sink::try_new(stream_handle)?;
    sink.append(source);
    sink.pause(); // Start paused

    debug!("Created MIDI synth sink ({:?}) for {} notes (duration: {:.1}s)",
        method, notes.len(), duration);
    Ok(sink)
}

/// Create a sink with synthesized audio at a specific note rate (for flight sync)
pub fn create_midi_synth_sink_with_rate(
    stream_handle: &OutputStreamHandle,
    notes: Vec<MidiNote>,
    method: SynthMethod,
    stop_flag: Arc<AtomicBool>,
    notes_per_second: f32,
) -> anyhow::Result<Sink> {
    let source = MidiSynthSource::with_note_rate(notes.clone(), method, stop_flag, notes_per_second);
    let duration = source.total_duration().map(|d| d.as_secs_f32()).unwrap_or(0.0);

    let sink = Sink::try_new(stream_handle)?;
    sink.append(source);
    sink.pause(); // Start paused

    debug!("Created synced MIDI synth sink ({:?}) for {} notes at {:.1} notes/sec (duration: {:.1}s)",
        method, notes.len(), notes_per_second, duration);
    Ok(sink)
}

/// Create a sink with synthesized audio from base-12 data (legacy, for visualization)
/// Converts base-12 to MIDI notes assuming C4 base octave
pub fn create_synth_sink(
    stream_handle: &OutputStreamHandle,
    base_digits: Vec<u8>,
    method: SynthMethod,
    stop_flag: Arc<AtomicBool>,
) -> anyhow::Result<Sink> {
    // Convert base-12 to MIDI notes (C4 = 60 + digit)
    let notes: Vec<MidiNote> = base_digits.iter()
        .map(|&d| MidiNote {
            note: 60 + (d % 12), // C4 + chromatic offset
            velocity: 0.8,
        })
        .collect();

    create_midi_synth_sink(stream_handle, notes, method, stop_flag)
}

/// Create a sink with synthesized audio from base-12 data at a specific note rate
pub fn create_synth_sink_with_rate(
    stream_handle: &OutputStreamHandle,
    base_digits: Vec<u8>,
    method: SynthMethod,
    stop_flag: Arc<AtomicBool>,
    notes_per_second: f32,
) -> anyhow::Result<Sink> {
    // Convert base-12 to MIDI notes (C4 = 60 + digit)
    let notes: Vec<MidiNote> = base_digits.iter()
        .map(|&d| MidiNote {
            note: 60 + (d % 12), // C4 + chromatic offset
            velocity: 0.8,
        })
        .collect();

    create_midi_synth_sink_with_rate(stream_handle, notes, method, stop_flag, notes_per_second)
}
