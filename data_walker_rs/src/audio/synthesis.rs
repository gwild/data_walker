//! Real-time audio synthesis from MIDI notes and base-12 walk data
//!
//! Supports note-accurate synthesis from detected MIDI notes,
//! as well as legacy base-12 synthesis for visualization-based audio.

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;
use rodio::{OutputStreamHandle, Sink, Source};
use rodio::source::SeekError;
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
    looping: bool,
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
            looping: true,
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
            looping: true,
        }
    }

    fn one_shot(mut self) -> Self {
        self.looping = false;
        self
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

    fn note_sample_index(&self) -> usize {
        self.sample_index % self.samples_per_note as usize
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
        let note_sample_index = self.note_sample_index();
        let t = note_sample_index as f32 / SAMPLE_RATE as f32;
        let accent = note.velocity.clamp(0.0, 1.0);
        let drum = note.note % 12;

        match drum {
            0 => self.generate_kick(t, accent),
            1 => self.generate_low_tom(t, accent),
            2 => self.generate_snare(note_sample_index, t, accent),
            3 => self.generate_rim(note_sample_index, t, accent),
            4 => self.generate_closed_hat(note_sample_index, t, accent),
            5 => self.generate_open_hat(note_sample_index, t, accent),
            6 => self.generate_clap(note_sample_index, t, accent),
            7 => self.generate_mid_tom(t, accent),
            8 => self.generate_high_tom(t, accent),
            9 => self.generate_shaker(note_sample_index, t, accent),
            10 => self.generate_cowbell(t, accent),
            _ => self.generate_crash(note_sample_index, t, accent),
        }
    }

    fn generate_kick(&self, t: f32, accent: f32) -> f32 {
        if t > 0.22 {
            return 0.0;
        }

        let amp_env = (-t * 18.0).exp();
        let pitch_env = (-t * 28.0).exp();
        let freq = 42.0 + 130.0 * pitch_env;
        let body = (2.0 * std::f32::consts::PI * freq * t).sin();
        let click = (2.0 * std::f32::consts::PI * 1800.0 * t).sin() * (-t * 220.0).exp();

        (body * 0.9 + click * 0.18) * amp_env * accent * 0.75
    }

    fn generate_tom(&self, t: f32, accent: f32, base_freq: f32, decay_rate: f32) -> f32 {
        if t > 0.24 {
            return 0.0;
        }

        let amp_env = (-t * decay_rate).exp();
        let pitch_env = (-t * 18.0).exp();
        let freq = base_freq + base_freq * 0.55 * pitch_env;
        let body = (2.0 * std::f32::consts::PI * freq * t).sin();
        let overtone = (2.0 * std::f32::consts::PI * freq * 1.92 * t).sin() * 0.22;
        let click = (2.0 * std::f32::consts::PI * 2400.0 * t).sin() * (-t * 260.0).exp() * 0.08;

        (body + overtone + click) * amp_env * accent * 0.62
    }

    fn generate_low_tom(&self, t: f32, accent: f32) -> f32 {
        self.generate_tom(t, accent, 92.0, 13.0)
    }

    fn generate_mid_tom(&self, t: f32, accent: f32) -> f32 {
        self.generate_tom(t, accent, 138.0, 15.0)
    }

    fn generate_high_tom(&self, t: f32, accent: f32) -> f32 {
        self.generate_tom(t, accent, 198.0, 17.0)
    }

    fn generate_snare(&self, note_sample_index: usize, t: f32, accent: f32) -> f32 {
        if t > 0.18 {
            return 0.0;
        }

        let noise = shaped_noise(note_sample_index, 0x51A3C7D2);
        let noise_env = (-t * 24.0).exp();
        let body_env = (-t * 16.0).exp();
        let body = (2.0 * std::f32::consts::PI * 190.0 * t).sin()
            + 0.35 * (2.0 * std::f32::consts::PI * 330.0 * t).sin();
        let snap = (2.0 * std::f32::consts::PI * 3200.0 * t).sin() * (-t * 260.0).exp();

        (noise * noise_env * 0.7 + body * body_env * 0.32 + snap * 0.08) * accent * 0.7
    }

    fn generate_rim(&self, note_sample_index: usize, t: f32, accent: f32) -> f32 {
        if t > 0.05 {
            return 0.0;
        }

        let wood = (
            (2.0 * std::f32::consts::PI * 1760.0 * t).sin()
                + (2.0 * std::f32::consts::PI * 2480.0 * t).sin() * 0.6
        ) * (-t * 85.0).exp();
        let click = shaped_noise(note_sample_index, 0x19C3A56D) * (-t * 180.0).exp() * 0.25;

        (wood + click) * accent * 0.52
    }

    fn generate_hat_voice(
        &self,
        note_sample_index: usize,
        t: f32,
        accent: f32,
        decay_rate: f32,
        gain: f32,
    ) -> f32 {
        if t > 0.28 {
            return 0.0;
        }

        let noise_a = shaped_noise(note_sample_index, 0xA341316C);
        let noise_b = shaped_noise(note_sample_index, 0xC8013EA4);
        let metallic = (
            (2.0 * std::f32::consts::PI * 4020.0 * t).sin()
                + (2.0 * std::f32::consts::PI * 5300.0 * t).sin() * 0.6
                + (2.0 * std::f32::consts::PI * 7180.0 * t).sin() * 0.35
        ) * 0.18;
        let noise = (noise_a - noise_b * 0.85).clamp(-1.0, 1.0);
        let env = (-t * decay_rate).exp();

        (noise * 0.82 + metallic) * env * accent * gain
    }

    fn generate_closed_hat(&self, note_sample_index: usize, t: f32, accent: f32) -> f32 {
        if t > 0.08 {
            return 0.0;
        }

        self.generate_hat_voice(note_sample_index, t, accent, 52.0, 0.42)
    }

    fn generate_open_hat(&self, note_sample_index: usize, t: f32, accent: f32) -> f32 {
        self.generate_hat_voice(note_sample_index, t, accent, 15.0, 0.32)
    }

    fn generate_clap(&self, note_sample_index: usize, t: f32, accent: f32) -> f32 {
        if t > 0.20 {
            return 0.0;
        }

        let pulse = if t < 0.012 || (t > 0.026 && t < 0.040) || (t > 0.055 && t < 0.080) {
            1.0
        } else {
            0.0
        };
        let tail = shaped_noise(note_sample_index, 0x7F4A7C15) * (-t * 16.0).exp() * 0.35;
        let burst = shaped_noise(note_sample_index, 0xB992DDFA) * pulse * 0.8;

        (burst + tail) * accent * 0.58
    }

    fn generate_shaker(&self, note_sample_index: usize, t: f32, accent: f32) -> f32 {
        if t > 0.12 {
            return 0.0;
        }

        let grains = shaped_noise(note_sample_index, 0xC47E3B71) * shaped_noise(note_sample_index, 0x91E10DA5);
        let env = (-t * 34.0).exp();
        let sparkle = (2.0 * std::f32::consts::PI * 8400.0 * t).sin() * (-t * 90.0).exp() * 0.08;

        (grains * 0.85 + sparkle) * env * accent * 0.34
    }

    fn generate_cowbell(&self, t: f32, accent: f32) -> f32 {
        if t > 0.18 {
            return 0.0;
        }

        let env = (-t * 13.0).exp();
        let tone_a = (2.0 * std::f32::consts::PI * 587.0 * t).sin();
        let tone_b = (2.0 * std::f32::consts::PI * 845.0 * t).sin() * 0.8;
        let tone_c = (2.0 * std::f32::consts::PI * 1180.0 * t).sin() * 0.28;

        (tone_a + tone_b + tone_c) * env * accent * 0.34
    }

    fn generate_crash(&self, note_sample_index: usize, t: f32, accent: f32) -> f32 {
        if t > 0.65 {
            return 0.0;
        }

        let wash = self.generate_hat_voice(note_sample_index, t, accent, 4.8, 0.26);
        let broadband = shaped_noise(note_sample_index, 0xDA2F7C39) * (-t * 6.0).exp() * 0.18;
        let clang = (
            (2.0 * std::f32::consts::PI * 3120.0 * t).sin()
                + (2.0 * std::f32::consts::PI * 4650.0 * t).sin() * 0.45
        ) * (-t * 10.0).exp() * 0.10;

        (wash + broadband + clang) * accent
    }
}

fn shaped_noise(sample_index: usize, seed: u32) -> f32 {
    let mut x = seed ^ sample_index as u32;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

impl Iterator for MidiSynthSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        // Check stop flag
        if self.stop_flag.load(Ordering::Relaxed) {
            return None;
        }

        let total_samples = self.notes.len() * self.samples_per_note as usize;
        if self.sample_index >= total_samples {
            if !self.looping {
                return None;
            }
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

    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        if self.notes.is_empty() {
            self.sample_index = 0;
            self.phase = 0.0;
            return Ok(());
        }

        let total_samples = self.notes.len() * self.samples_per_note as usize;
        if total_samples == 0 {
            self.sample_index = 0;
            self.phase = 0.0;
            return Ok(());
        }

        let target_sample = (pos.as_secs_f32() * SAMPLE_RATE as f32) as usize;
        self.sample_index = target_sample.min(total_samples.saturating_sub(1));
        self.phase = 0.0;
        Ok(())
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

/// Create a one-shot sink for a single generated note/hit.
pub fn create_one_shot_midi_sink(
    stream_handle: &OutputStreamHandle,
    note: MidiNote,
    method: SynthMethod,
    stop_flag: Arc<AtomicBool>,
) -> anyhow::Result<Sink> {
    let duration_secs = match method {
        SynthMethod::Percussion => percussion_hit_duration_secs(note),
        SynthMethod::ChromaticNotes | SynthMethod::SineTones => 0.12,
    }
    .max(0.01);

    let notes_per_second = 1.0 / duration_secs;
    let source = MidiSynthSource::with_note_rate(vec![note], method, stop_flag, notes_per_second).one_shot();

    let sink = Sink::try_new(stream_handle)?;
    sink.append(source);
    sink.pause();

    debug!(
        "Created one-shot MIDI sink ({:?}) for note {} lasting {:.3}s",
        method,
        note.note,
        duration_secs
    );
    Ok(sink)
}

fn percussion_hit_duration_secs(note: MidiNote) -> f32 {
    match note.note % 12 {
        0 => 0.22,
        1 => 0.24,
        2 => 0.18,
        3 => 0.05,
        4 => 0.08,
        5 => 0.28,
        6 => 0.20,
        7 => 0.24,
        8 => 0.24,
        9 => 0.12,
        10 => 0.18,
        _ => 0.65,
    }
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
