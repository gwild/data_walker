//! Audio playback engine for Data Walker flight mode
//!
//! Supports both file-based audio (WAV/MP3) and real-time synthesis
//! from base-12 walk data. Includes time-stretching for synced playback.

mod playback;
mod synthesis;
mod timestretch;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use rodio::{OutputStream, OutputStreamHandle, Sink};
use tracing::{info, warn, debug};

pub use synthesis::{SynthMethod, create_midi_synth_sink};
pub use timestretch::{load_and_stretch, StretchedSource};

use crate::converters::audio::MidiNote;

/// Mixing modes for multiple audio sources
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum MixingMode {
    #[default]
    Simultaneous,   // All sources play at equal volume
    CameraFocus,    // Loudest = source closest to camera target
    DistanceBased,  // Volume inversely proportional to distance from camera
}

/// Audio settings (stored in GUI state)
#[derive(Clone, Debug)]
pub struct AudioSettings {
    pub enabled: bool,
    pub master_volume: f32,
    pub synthesis_method: SynthMethod,
    pub mixing_mode: MixingMode,
    pub sync_to_flight: bool,  // Lock playback duration to the current flight duration
    pub force_synthesis: bool, // Prefer generated notes/drums over source audio when available
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            master_volume: 0.7,
            synthesis_method: SynthMethod::default(),
            mixing_mode: MixingMode::default(),
            sync_to_flight: false,  // Default OFF - playback runs at its natural speed
            force_synthesis: true,  // Default ON - generated audio follows walk points directly
        }
    }
}

/// Source type for audio playback
#[derive(Clone, Debug)]
pub enum SourceType {
    /// Audio file (WAV or MP3)
    AudioFile { path: PathBuf },
    /// Synthesized from base-12 data (legacy, single octave)
    Synthesized { base_digits: Vec<u8> },
    /// Synthesized from MIDI notes (note-accurate, multi-octave)
    MidiNotes { notes: Vec<MidiNote> },
}

/// Per-source audio state
struct AudioSource {
    source_id: String,
    source_type: SourceType,
    sink: Option<Sink>,
    duration_secs: f32,
    volume: f32,
    last_seek_progress: f32,  // Track last seek position to avoid constant seeking
    synth_rate: Option<f32>,  // Notes per second for synced synthesis (None = default 10/sec)
}

/// Main audio playback engine
pub struct AudioEngine {
    // rodio output stream (must stay alive)
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    // Active audio sources
    sources: HashMap<String, AudioSource>,
    // Shared stop flag for synthesis threads
    stop_flag: Arc<AtomicBool>,
}

impl AudioEngine {
    /// Create a new audio engine
    pub fn new() -> anyhow::Result<Self> {
        let (stream, stream_handle) = OutputStream::try_default()?;
        info!("Audio engine initialized");
        Ok(Self {
            _stream: stream,
            stream_handle,
            sources: HashMap::new(),
            stop_flag: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Prepare an audio source for playback
    pub fn prepare_source(&mut self, source_id: &str, source_type: SourceType) -> anyhow::Result<()> {
        // Remove existing source if any
        self.remove_source(source_id);

        let (sink, duration) = match &source_type {
            SourceType::AudioFile { path } => {
                playback::create_file_sink(&self.stream_handle, path)?
            }
            SourceType::Synthesized { base_digits } => {
                // For synthesized sources, we'll create the sink on play
                // Duration is based on digit count at ~10 digits/second
                let duration = base_digits.len() as f32 / 10.0;
                (None, duration)
            }
            SourceType::MidiNotes { notes } => {
                // For MIDI note sources, we'll create the sink on play
                // Duration is based on note count at ~10 notes/second
                let duration = notes.len() as f32 / 10.0;
                (None, duration)
            }
        };

        self.sources.insert(source_id.to_string(), AudioSource {
            source_id: source_id.to_string(),
            source_type,
            sink,
            duration_secs: duration,
            volume: 1.0,
            last_seek_progress: 0.0,
            synth_rate: None,
        });

        debug!("Prepared audio source: {} (duration: {:.1}s)", source_id, duration);
        Ok(())
    }

    /// Prepare an audio source with time-stretching to match flight duration
    pub fn prepare_source_stretched(
        &mut self,
        source_id: &str,
        source_type: SourceType,
        target_duration_secs: f32,
    ) -> anyhow::Result<()> {
        // Remove existing source if any
        self.remove_source(source_id);

        let (sink, duration, synth_rate) = match &source_type {
            SourceType::AudioFile { path } => {
                // Load and time-stretch the audio
                info!("Loading and time-stretching {:?} to {:.1}s", path, target_duration_secs);
                let (samples, sample_rate, channels) = load_and_stretch(path, target_duration_secs)?;

                // Create a sink with the stretched audio
                let sink = Sink::try_new(&self.stream_handle)?;
                let source = StretchedSource::new(samples, sample_rate, channels);
                let duration = source.duration().as_secs_f32();
                sink.append(source);
                sink.pause(); // Start paused

                (Some(sink), duration, None)
            }
            SourceType::Synthesized { base_digits } => {
                // For synthesized, calculate playback rate based on target duration
                let synth_rate = base_digits.len() as f32 / target_duration_secs;
                info!("Synth rate for flight sync: {:.1} notes/sec (target {:.1}s for {} digits)",
                    synth_rate, target_duration_secs, base_digits.len());
                (None, target_duration_secs, Some(synth_rate))
            }
            SourceType::MidiNotes { notes } => {
                // For MIDI notes, calculate playback rate based on target duration
                let synth_rate = notes.len() as f32 / target_duration_secs;
                info!("MIDI synth rate for flight sync: {:.1} notes/sec (target {:.1}s for {} notes)",
                    synth_rate, target_duration_secs, notes.len());
                (None, target_duration_secs, Some(synth_rate))
            }
        };

        self.sources.insert(source_id.to_string(), AudioSource {
            source_id: source_id.to_string(),
            source_type,
            sink,
            duration_secs: duration,
            volume: 1.0,
            last_seek_progress: 0.0,
            synth_rate,
        });

        info!("Prepared time-stretched audio source: {} (duration: {:.1}s)", source_id, duration);
        Ok(())
    }

    /// Prepare an audio file source from already-stretched samples.
    pub fn prepare_pre_stretched_file_source(
        &mut self,
        source_id: &str,
        path: PathBuf,
        samples: Vec<f32>,
        sample_rate: u32,
        channels: u16,
    ) -> anyhow::Result<()> {
        self.remove_source(source_id);

        let sink = Sink::try_new(&self.stream_handle)?;
        let source = StretchedSource::new(samples, sample_rate, channels);
        let duration = source.duration().as_secs_f32();
        sink.append(source);
        sink.pause();

        self.sources.insert(source_id.to_string(), AudioSource {
            source_id: source_id.to_string(),
            source_type: SourceType::AudioFile { path },
            sink: Some(sink),
            duration_secs: duration,
            volume: 1.0,
            last_seek_progress: 0.0,
            synth_rate: None,
        });

        info!(
            "Prepared pre-stretched audio source: {} (duration: {:.1}s)",
            source_id,
            duration
        );
        Ok(())
    }

    /// Remove an audio source
    pub fn remove_source(&mut self, source_id: &str) {
        if let Some(source) = self.sources.remove(source_id) {
            if let Some(sink) = source.sink {
                sink.stop();
            }
            debug!("Removed audio source: {}", source_id);
        }
    }

    /// Start or resume playback of all prepared sources
    pub fn play(&mut self, settings: &AudioSettings) {
        info!("AudioEngine::play() called with {} sources", self.sources.len());
        for source in self.sources.values_mut() {
            if let Some(ref sink) = source.sink {
                info!("Playing audio source: {} (volume: {:.2})", source.source_id, settings.master_volume * source.volume);
                sink.set_volume(settings.master_volume * source.volume);
                sink.play();
            } else {
                // Create synthesis sink based on source type and synth_rate
                let sink_result = match (&source.source_type, source.synth_rate) {
                    (SourceType::Synthesized { base_digits }, Some(rate)) => {
                        // Synced base-12 synthesis at specified rate
                        info!("Creating synced synth for {} at {:.1} notes/sec", source.source_id, rate);
                        synthesis::create_synth_sink_with_rate(
                            &self.stream_handle,
                            base_digits.clone(),
                            settings.synthesis_method,
                            self.stop_flag.clone(),
                            rate,
                        )
                    }
                    (SourceType::Synthesized { base_digits }, None) => {
                        // Default base-12 synthesis at 10 notes/sec
                        synthesis::create_synth_sink(
                            &self.stream_handle,
                            base_digits.clone(),
                            settings.synthesis_method,
                            self.stop_flag.clone(),
                        )
                    }
                    (SourceType::MidiNotes { notes }, Some(rate)) => {
                        // Synced MIDI synthesis at specified rate
                        info!("Creating synced MIDI synth for {} at {:.1} notes/sec", source.source_id, rate);
                        synthesis::create_midi_synth_sink_with_rate(
                            &self.stream_handle,
                            notes.clone(),
                            settings.synthesis_method,
                            self.stop_flag.clone(),
                            rate,
                        )
                    }
                    (SourceType::MidiNotes { notes }, None) => {
                        // Default MIDI synthesis at 10 notes/sec
                        synthesis::create_midi_synth_sink(
                            &self.stream_handle,
                            notes.clone(),
                            settings.synthesis_method,
                            self.stop_flag.clone(),
                        )
                    }
                    (SourceType::AudioFile { .. }, _) => continue, // Already handled above
                };

                if let Ok(sink) = sink_result {
                    sink.set_volume(settings.master_volume * source.volume);
                    sink.play();
                    source.sink = Some(sink);
                }
            }
        }
    }

    /// Pause all playback
    pub fn pause(&self) {
        info!("AudioEngine::pause() called");
        for source in self.sources.values() {
            if let Some(ref sink) = source.sink {
                info!("Pausing audio source: {}", source.source_id);
                sink.pause();
            }
        }
    }

    /// Stop all playback
    pub fn stop_all(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        for source in self.sources.values_mut() {
            if let Some(sink) = source.sink.take() {
                sink.stop();
            }
        }
        self.stop_flag.store(false, Ordering::SeqCst);
    }

    /// Sync playback position to flight progress (0.0-1.0)
    /// Only seeks when there's significant drift to avoid corrupting audio decoding
    pub fn sync_to_progress(&mut self, progress: f32, settings: &AudioSettings) {
        // Threshold: only seek if drift is more than 2 seconds worth of progress
        const SEEK_THRESHOLD: f32 = 0.05; // 5% of total duration

        for source in self.sources.values_mut() {
            if let Some(ref sink) = source.sink {
                // Only update volume, don't constantly seek
                sink.set_volume(settings.master_volume * source.volume);

                // Check if we need to seek (large drift or initial sync)
                let drift = (progress - source.last_seek_progress).abs();
                if drift > SEEK_THRESHOLD {
                    let target_position = std::time::Duration::from_secs_f32(
                        progress * source.duration_secs
                    );
                    info!(
                        "Seeking {}: drift={:.1}%, target={:.2}s",
                        source.source_id, drift * 100.0, target_position.as_secs_f32()
                    );
                    let _ = sink.try_seek(target_position);
                    source.last_seek_progress = progress;
                }
            }
        }
    }

    /// Update volumes based on mixing mode and camera position
    pub fn update_mixing(
        &mut self,
        settings: &AudioSettings,
        camera_pos: [f32; 3],
        walk_positions: &HashMap<String, [f32; 3]>,
    ) {
        match settings.mixing_mode {
            MixingMode::Simultaneous => {
                // All sources at equal volume
                let num_sources = self.sources.len().max(1) as f32;
                let per_source_volume = 1.0 / num_sources.sqrt();
                for source in self.sources.values_mut() {
                    source.volume = per_source_volume;
                }
            }
            MixingMode::CameraFocus => {
                // Find closest source, make it loudest
                let mut closest_id = None;
                let mut closest_dist = f32::MAX;

                for (id, pos) in walk_positions {
                    let dx = pos[0] - camera_pos[0];
                    let dy = pos[1] - camera_pos[1];
                    let dz = pos[2] - camera_pos[2];
                    let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                    if dist < closest_dist {
                        closest_dist = dist;
                        closest_id = Some(id.clone());
                    }
                }

                for source in self.sources.values_mut() {
                    source.volume = if Some(&source.source_id) == closest_id.as_ref() {
                        1.0
                    } else {
                        0.2 // Background volume for non-focused
                    };
                }
            }
            MixingMode::DistanceBased => {
                // Volume inversely proportional to distance
                for source in self.sources.values_mut() {
                    if let Some(pos) = walk_positions.get(&source.source_id) {
                        let dx = pos[0] - camera_pos[0];
                        let dy = pos[1] - camera_pos[1];
                        let dz = pos[2] - camera_pos[2];
                        let dist = (dx * dx + dy * dy + dz * dz).sqrt().max(1.0);
                        source.volume = (50.0 / dist).min(1.0);
                    }
                }
            }
        }

        // Apply updated volumes
        for source in self.sources.values() {
            if let Some(ref sink) = source.sink {
                sink.set_volume(settings.master_volume * source.volume);
            }
        }
    }

    /// Check if any sources are loaded
    pub fn has_sources(&self) -> bool {
        !self.sources.is_empty()
    }

    /// Get source duration for a given source ID
    pub fn get_duration(&self, source_id: &str) -> Option<f32> {
        self.sources.get(source_id).map(|s| s.duration_secs)
    }

    /// Recreate synthesized audio sinks with a new synth method
    /// Call this when the user changes the synth method during playback
    pub fn recreate_synth_sinks(&mut self, settings: &AudioSettings) {
        info!("Recreating synth sinks with method: {:?}", settings.synthesis_method);

        for source in self.sources.values_mut() {
            // Only recreate synthesized sources (not audio files)
            match &source.source_type {
                SourceType::Synthesized { base_digits } => {
                    // Stop existing sink
                    if let Some(sink) = source.sink.take() {
                        sink.stop();
                    }

                    // Create new sink with updated method
                    let sink_result = match source.synth_rate {
                        Some(rate) => synthesis::create_synth_sink_with_rate(
                            &self.stream_handle,
                            base_digits.clone(),
                            settings.synthesis_method,
                            self.stop_flag.clone(),
                            rate,
                        ),
                        None => synthesis::create_synth_sink(
                            &self.stream_handle,
                            base_digits.clone(),
                            settings.synthesis_method,
                            self.stop_flag.clone(),
                        ),
                    };

                    if let Ok(sink) = sink_result {
                        sink.set_volume(settings.master_volume * source.volume);
                        sink.play();
                        source.sink = Some(sink);
                        debug!("Recreated synth sink for {} with {:?}", source.source_id, settings.synthesis_method);
                    }
                }
                SourceType::MidiNotes { notes } => {
                    // Stop existing sink
                    if let Some(sink) = source.sink.take() {
                        sink.stop();
                    }

                    // Create new sink with updated method
                    let sink_result = match source.synth_rate {
                        Some(rate) => synthesis::create_midi_synth_sink_with_rate(
                            &self.stream_handle,
                            notes.clone(),
                            settings.synthesis_method,
                            self.stop_flag.clone(),
                            rate,
                        ),
                        None => synthesis::create_midi_synth_sink(
                            &self.stream_handle,
                            notes.clone(),
                            settings.synthesis_method,
                            self.stop_flag.clone(),
                        ),
                    };

                    if let Ok(sink) = sink_result {
                        sink.set_volume(settings.master_volume * source.volume);
                        sink.play();
                        source.sink = Some(sink);
                        debug!("Recreated MIDI synth sink for {} with {:?}", source.source_id, settings.synthesis_method);
                    }
                }
                SourceType::AudioFile { .. } => {
                    // Audio files don't use synthesis method, skip
                }
            }
        }
    }
}
