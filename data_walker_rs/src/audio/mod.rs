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
use std::time::Instant;
use rodio::{OutputStream, OutputStreamHandle, Sink};
use tracing::{info, warn, debug};

pub use synthesis::SynthMethod;
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
    /// Synthesized directly from digit data.
    Synthesized { base_digits: Vec<u8> },
}

/// Per-source audio state
struct AudioSource {
    source_id: String,
    source_type: SourceType,
    sink: Option<Sink>,
    active_hit_sinks: Vec<Sink>,
    duration_secs: f32,
    volume: f32,
    synth_rate: Option<f32>,  // Notes per second for synced synthesis (None = default 10/sec)
    last_triggered_step: Option<usize>,
    last_synced_step: Option<usize>,
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
    debug_window_started_at: Instant,
    debug_triggered_hits_in_window: usize,
    debug_sync_calls_in_window: usize,
    debug_seek_events_in_window: usize,
}

impl AudioEngine {
    fn digit_note(digit: u8) -> MidiNote {
        MidiNote {
            note: 60 + (digit % 12),
            velocity: 0.85,
        }
    }

    /// Create a new audio engine
    pub fn new() -> anyhow::Result<Self> {
        let (stream, stream_handle) = OutputStream::try_default()?;
        info!("Audio engine initialized");
        Ok(Self {
            _stream: stream,
            stream_handle,
            sources: HashMap::new(),
            stop_flag: Arc::new(AtomicBool::new(false)),
            debug_window_started_at: Instant::now(),
            debug_triggered_hits_in_window: 0,
            debug_sync_calls_in_window: 0,
            debug_seek_events_in_window: 0,
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
        };

        self.sources.insert(source_id.to_string(), AudioSource {
            source_id: source_id.to_string(),
            source_type,
            sink,
            active_hit_sinks: Vec::new(),
            duration_secs: duration,
            volume: 1.0,
            synth_rate: None,
            last_triggered_step: None,
            last_synced_step: None,
        });

        debug!("Prepared audio source: {} (duration: {:.1}s)", source_id, duration);
        Ok(())
    }

    /// Prepare a generated source so its note/drum trigger rate matches
    /// the flight point rate directly.
    pub fn prepare_source_synced_to_points(
        &mut self,
        source_id: &str,
        source_type: SourceType,
        points_per_second: f32,
    ) -> anyhow::Result<()> {
        if points_per_second <= 0.0 {
            anyhow::bail!("points_per_second must be > 0 for synced generated audio");
        }

        self.remove_source(source_id);

        let (duration, synth_rate) = match &source_type {
            SourceType::Synthesized { base_digits } => {
                let duration = base_digits.len() as f32 / points_per_second;
                info!(
                    "Generated synth synced to flight: {:.3} notes/sec for {} digits (duration: {:.1}s)",
                    points_per_second,
                    base_digits.len(),
                    duration
                );
                (duration, points_per_second)
            }
            SourceType::AudioFile { .. } => {
                anyhow::bail!("prepare_source_synced_to_points does not support audio files");
            }
        };

        self.sources.insert(source_id.to_string(), AudioSource {
            source_id: source_id.to_string(),
            source_type,
            sink: None,
            active_hit_sinks: Vec::new(),
            duration_secs: duration,
            volume: 1.0,
            synth_rate: Some(synth_rate),
            last_triggered_step: None,
            last_synced_step: None,
        });

        info!(
            "Prepared generated audio source synced to points: {} ({:.3} Hz)",
            source_id,
            synth_rate
        );
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
            active_hit_sinks: Vec::new(),
            duration_secs: duration,
            volume: 1.0,
            synth_rate: None,
            last_triggered_step: None,
            last_synced_step: None,
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
        if let Some(mut source) = self.sources.remove(source_id) {
            if let Some(sink) = source.sink.take() {
                sink.stop();
            }
            for sink in source.active_hit_sinks.drain(..) {
                sink.stop();
            }
            debug!("Removed audio source: {}", source_id);
        }
    }

    /// Start or resume playback of all prepared sources
    pub fn play(&mut self, settings: &AudioSettings) {
        info!("AudioEngine::play() called with {} sources", self.sources.len());
        for source in self.sources.values_mut() {
            for sink in &source.active_hit_sinks {
                sink.set_volume(settings.master_volume * source.volume);
                sink.play();
            }

            if let Some(ref sink) = source.sink {
                info!("Playing audio source: {} (volume: {:.2})", source.source_id, settings.master_volume * source.volume);
                sink.set_volume(settings.master_volume * source.volume);
                sink.play();
            } else {
                if settings.sync_to_flight
                    && matches!(source.source_type, SourceType::Synthesized { .. })
                    && source.synth_rate.is_some()
                {
                    source.last_triggered_step = None;
                    source.last_synced_step = None;
                    continue;
                }

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
            for sink in &source.active_hit_sinks {
                sink.pause();
            }
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
            for sink in source.active_hit_sinks.drain(..) {
                sink.stop();
            }
            source.last_triggered_step = None;
            source.last_synced_step = None;
        }
        self.stop_flag.store(false, Ordering::SeqCst);
    }

    /// Sync playback to the current flight step.
    pub fn sync_to_step(&mut self, flight_step: usize, total_steps: usize, settings: &AudioSettings) {
        self.debug_sync_calls_in_window += 1;

        for source in self.sources.values_mut() {
            source.active_hit_sinks.retain(|sink| !sink.empty());
            for sink in &source.active_hit_sinks {
                sink.set_volume(settings.master_volume * source.volume);
            }

            if settings.sync_to_flight && source.synth_rate.is_some() {
                if let SourceType::Synthesized { base_digits } = &source.source_type {
                    if base_digits.is_empty() {
                        source.last_synced_step = Some(flight_step);
                        continue;
                    }

                    let target_step = flight_step.min(base_digits.len().saturating_sub(1));

                    match source.last_triggered_step {
                        None => {
                            match synthesis::create_one_shot_midi_sink(
                                &self.stream_handle,
                                Self::digit_note(base_digits[target_step]),
                                settings.synthesis_method,
                                self.stop_flag.clone(),
                            ) {
                                Ok(sink) => {
                                    sink.set_volume(settings.master_volume * source.volume);
                                    sink.play();
                                    source.active_hit_sinks.push(sink);
                                    self.debug_triggered_hits_in_window += 1;
                                }
                                Err(error) => {
                                    warn!(
                                        "Failed to create initial step trigger for {} at step {}: {}",
                                        source.source_id,
                                        target_step,
                                        error
                                    );
                                }
                            }
                        }
                        Some(last_step) if target_step > last_step => {
                            for step in (last_step + 1)..=target_step {
                                match synthesis::create_one_shot_midi_sink(
                                    &self.stream_handle,
                                    Self::digit_note(base_digits[step]),
                                    settings.synthesis_method,
                                    self.stop_flag.clone(),
                                ) {
                                    Ok(sink) => {
                                        sink.set_volume(settings.master_volume * source.volume);
                                        sink.play();
                                        source.active_hit_sinks.push(sink);
                                        self.debug_triggered_hits_in_window += 1;
                                    }
                                    Err(error) => {
                                        warn!(
                                            "Failed to create forward step trigger for {} at step {}: {}",
                                            source.source_id,
                                            step,
                                            error
                                        );
                                    }
                                }
                            }
                        }
                        Some(last_step) if target_step < last_step => {
                            for step in (target_step..last_step).rev() {
                                match synthesis::create_one_shot_midi_sink(
                                    &self.stream_handle,
                                    Self::digit_note(base_digits[step]),
                                    settings.synthesis_method,
                                    self.stop_flag.clone(),
                                ) {
                                    Ok(sink) => {
                                        sink.set_volume(settings.master_volume * source.volume);
                                        sink.play();
                                        source.active_hit_sinks.push(sink);
                                        self.debug_triggered_hits_in_window += 1;
                                    }
                                    Err(error) => {
                                        warn!(
                                            "Failed to create reverse step trigger for {} at step {}: {}",
                                            source.source_id,
                                            step,
                                            error
                                        );
                                    }
                                }
                            }
                        }
                        Some(_) => {}
                    }

                    source.last_triggered_step = Some(target_step);
                    source.last_synced_step = Some(flight_step);
                    continue;
                }
            }

            if let Some(ref sink) = source.sink {
                sink.set_volume(settings.master_volume * source.volume);
                let should_seek = match source.last_synced_step {
                    None => true,
                    Some(last_step) => last_step.abs_diff(flight_step) > 1,
                };

                if should_seek {
                    let denom = total_steps.saturating_sub(1).max(1) as f32;
                    let target_position = std::time::Duration::from_secs_f32(
                        (flight_step.min(total_steps.saturating_sub(1)) as f32 / denom) * source.duration_secs
                    );
                    match sink.try_seek(target_position) {
                        Ok(()) => {
                            source.last_synced_step = Some(flight_step);
                            self.debug_seek_events_in_window += 1;
                        }
                        Err(error) => {
                            warn!(
                                "Seek failed for {} at step {} ({:.2}s): {}",
                                source.source_id,
                                flight_step,
                                target_position.as_secs_f32(),
                                error
                            );
                        }
                    }
                } else {
                    source.last_synced_step = Some(flight_step);
                }
            }
        }

        let window_elapsed = self.debug_window_started_at.elapsed().as_secs_f32();
        if window_elapsed >= 1.0 {
            debug!(
                "[AUDIO][SYNC] elapsed={:.2}s step={} total_steps={} sync_calls={} seek_events={} triggered_sounds={} sounds_per_sec={:.2}",
                window_elapsed,
                flight_step,
                total_steps,
                self.debug_sync_calls_in_window,
                self.debug_seek_events_in_window,
                self.debug_triggered_hits_in_window,
                self.debug_triggered_hits_in_window as f32 / window_elapsed,
            );
            self.debug_window_started_at = Instant::now();
            self.debug_triggered_hits_in_window = 0;
            self.debug_sync_calls_in_window = 0;
            self.debug_seek_events_in_window = 0;
        }
    }
}
