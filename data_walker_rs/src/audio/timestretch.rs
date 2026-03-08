//! Time-stretching support using WSOLA (Waveform Similarity Overlap-Add)
//!
//! Pure Rust implementation - no FFI dependencies.
//! Provides tempo changes without pitch shifting for audio file playback.

use std::path::Path;
use tracing::{debug, info};

/// Load an audio file and time-stretch it to a target duration
/// Returns interleaved stereo f32 samples at the original sample rate
pub fn load_and_stretch(
    path: &Path,
    target_duration_secs: f32,
) -> anyhow::Result<(Vec<f32>, u32, u16)> {
    // Load the audio file
    let (samples, sample_rate, channels) = load_audio_file(path)?;

    let original_duration = samples.len() as f32 / (sample_rate as f32 * channels as f32);
    let stretch_factor = target_duration_secs / original_duration;

    info!(
        "Time-stretching {:?}: {:.1}s -> {:.1}s (factor: {:.2}x)",
        path.file_name().unwrap_or_default(),
        original_duration,
        target_duration_secs,
        stretch_factor
    );

    // If stretch factor is close to 1.0, skip processing
    if (stretch_factor - 1.0).abs() < 0.01 {
        debug!("Stretch factor ~1.0, returning original audio");
        return Ok((samples, sample_rate, channels));
    }

    // Time-stretch the audio using WSOLA
    let stretched = wsola_stretch(&samples, channels, sample_rate, stretch_factor)?;

    Ok((stretched, sample_rate, channels))
}

/// Load audio file into f32 samples
fn load_audio_file(path: &Path) -> anyhow::Result<(Vec<f32>, u32, u16)> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "wav" => load_wav(path),
        "mp3" => load_mp3(path),
        _ => anyhow::bail!("Unsupported audio format: {}", ext),
    }
}

/// Load WAV file
fn load_wav(path: &Path) -> anyhow::Result<(Vec<f32>, u32, u16)> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            reader.samples::<f32>().filter_map(|s| s.ok()).collect()
        }
        hound::SampleFormat::Int => {
            let max_val = (1 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect()
        }
    };

    Ok((samples, spec.sample_rate, spec.channels))
}

/// Load MP3 file using symphonia
fn load_mp3(path: &Path) -> anyhow::Result<(Vec<f32>, u32, u16)> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("mp3");

    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;

    let mut format = probed.format;
    let track = format.default_track()
        .ok_or_else(|| anyhow::anyhow!("No audio track found"))?;

    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2) as u16;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())?;

    let mut samples = Vec::new();

    loop {
        match format.next_packet() {
            Ok(packet) => {
                if let Ok(decoded) = decoder.decode(&packet) {
                    let spec = *decoded.spec();
                    let duration = decoded.capacity() as u64;
                    let mut sample_buf = SampleBuffer::<f32>::new(duration, spec);
                    sample_buf.copy_interleaved_ref(decoded);
                    samples.extend_from_slice(sample_buf.samples());
                }
            }
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => break,
        }
    }

    Ok((samples, sample_rate, channels))
}

/// WSOLA (Waveform Similarity Overlap-Add) time-stretching
///
/// This algorithm maintains pitch while changing tempo by:
/// 1. Analyzing overlapping frames of the input
/// 2. Finding optimal splice points using cross-correlation
/// 3. Overlapping and adding frames with crossfade
fn wsola_stretch(
    samples: &[f32],
    channels: u16,
    sample_rate: u32,
    stretch_factor: f32,
) -> anyhow::Result<Vec<f32>> {
    let channels = channels as usize;

    // WSOLA parameters (in frames, i.e., sample groups for all channels)
    let frame_size_ms = 50.0; // 50ms frames
    let frame_size = ((sample_rate as f32 * frame_size_ms / 1000.0) as usize) * channels;
    let hop_size = frame_size / 4; // 75% overlap
    let search_range = hop_size; // Search +/- this amount for best match

    // Output size
    let output_frames = (samples.len() as f32 * stretch_factor) as usize;
    let mut output = vec![0.0f32; output_frames];

    // Input position (fractional to handle non-integer stretch)
    let mut input_pos: f64 = 0.0;
    let input_hop = hop_size as f64 / stretch_factor as f64;

    // Output position
    let mut output_pos: usize = 0;

    // Previous frame for cross-correlation matching
    let mut prev_frame: Option<Vec<f32>> = None;

    while output_pos + frame_size <= output.len() {
        // Calculate nominal input position
        let nominal_pos = input_pos as usize;

        // Find best match position using cross-correlation with previous frame
        let best_pos = if let Some(ref prev) = prev_frame {
            find_best_match(samples, prev, nominal_pos, search_range, frame_size, channels)
        } else {
            nominal_pos.min(samples.len().saturating_sub(frame_size))
        };

        // Extract frame from input
        let frame_end = (best_pos + frame_size).min(samples.len());
        if best_pos >= samples.len() {
            break;
        }
        let frame = &samples[best_pos..frame_end];

        // Apply Hann window for smooth crossfade
        for i in 0..frame.len().min(frame_size) {
            let window = hann_window(i, frame_size);
            let out_idx = output_pos + i;
            if out_idx < output.len() {
                output[out_idx] += frame[i] * window;
            }
        }

        // Store frame for next iteration's matching
        prev_frame = Some(frame.to_vec());

        // Advance positions
        input_pos += input_hop;
        output_pos += hop_size;
    }

    // Normalize overlapped regions (since we're adding windowed frames)
    // The Hann window overlap-add should sum to 1.0 with 75% overlap,
    // but we'll normalize anyway for safety
    normalize_ola(&mut output, frame_size, hop_size);

    debug!("WSOLA stretched {} -> {} samples", samples.len(), output.len());
    Ok(output)
}

/// Hann window function
fn hann_window(i: usize, size: usize) -> f32 {
    let x = i as f32 / size as f32;
    0.5 * (1.0 - (2.0 * std::f32::consts::PI * x).cos())
}

/// Find best match position using simplified cross-correlation
fn find_best_match(
    samples: &[f32],
    prev_frame: &[f32],
    nominal_pos: usize,
    search_range: usize,
    frame_size: usize,
    channels: usize,
) -> usize {
    let mut best_pos = nominal_pos;
    let mut best_score = f32::MIN;

    // Search window around nominal position
    let start = nominal_pos.saturating_sub(search_range);
    let end = (nominal_pos + search_range).min(samples.len().saturating_sub(frame_size));

    if start >= end {
        return nominal_pos.min(samples.len().saturating_sub(frame_size));
    }

    // Use only beginning of frames for faster correlation
    let compare_len = (frame_size / 4).min(prev_frame.len());

    for pos in start..end {
        if pos + compare_len > samples.len() {
            break;
        }

        // Compute normalized cross-correlation
        let mut sum = 0.0f32;
        let mut sum_sq1 = 0.0f32;
        let mut sum_sq2 = 0.0f32;

        for i in 0..compare_len {
            let s1 = samples[pos + i];
            let s2 = if i < prev_frame.len() { prev_frame[prev_frame.len() - compare_len + i] } else { 0.0 };
            sum += s1 * s2;
            sum_sq1 += s1 * s1;
            sum_sq2 += s2 * s2;
        }

        let norm = (sum_sq1 * sum_sq2).sqrt();
        let score = if norm > 1e-10 { sum / norm } else { 0.0 };

        if score > best_score {
            best_score = score;
            best_pos = pos;
        }
    }

    best_pos
}

/// Normalize overlap-added output
fn normalize_ola(output: &mut [f32], frame_size: usize, hop_size: usize) {
    // With 75% overlap and Hann window, the OLA sum should be ~1.0
    // But edges need special handling
    let overlap_factor = frame_size / hop_size; // 4 with 75% overlap

    // Ramp up at start
    for i in 0..frame_size.min(output.len()) {
        let num_overlaps = ((i as f32 / hop_size as f32) + 1.0).min(overlap_factor as f32);
        let norm = hann_ola_sum(num_overlaps as usize);
        if norm > 0.1 {
            output[i] /= norm;
        }
    }

    // Middle section (fully overlapped)
    let middle_norm = hann_ola_sum(overlap_factor);
    let middle_end = output.len().saturating_sub(frame_size);
    if middle_norm > 0.1 && middle_end > frame_size {
        for sample in output[frame_size..middle_end].iter_mut() {
            *sample /= middle_norm;
        }
    }

    // Ramp down at end
    if output.len() > frame_size {
        let end_start = output.len() - frame_size;
        for i in 0..frame_size {
            let idx = end_start + i;
            if idx < output.len() {
                let num_overlaps = ((frame_size - i) as f32 / hop_size as f32).min(overlap_factor as f32).max(1.0);
                let norm = hann_ola_sum(num_overlaps as usize);
                if norm > 0.1 {
                    output[idx] /= norm;
                }
            }
        }
    }
}

/// Sum of Hann windows at a given overlap count
fn hann_ola_sum(num_overlaps: usize) -> f32 {
    // Approximate sum of overlapping Hann windows
    // With 75% overlap, sum is approximately 1.0 when fully overlapped
    match num_overlaps {
        0 => 0.0,
        1 => 0.5,  // Single Hann window average
        2 => 0.75,
        3 => 0.9,
        _ => 1.0,  // Full overlap
    }
}

/// A rodio Source that plays pre-stretched audio
pub struct StretchedSource {
    samples: Vec<f32>,
    position: usize,
    channels: u16,
    sample_rate: u32,
}

impl StretchedSource {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u16) -> Self {
        Self {
            samples,
            position: 0,
            channels,
            sample_rate,
        }
    }

    pub fn duration(&self) -> std::time::Duration {
        let total_frames = self.samples.len() / self.channels as usize;
        std::time::Duration::from_secs_f32(total_frames as f32 / self.sample_rate as f32)
    }
}

impl Iterator for StretchedSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position < self.samples.len() {
            let sample = self.samples[self.position];
            self.position += 1;
            Some(sample)
        } else {
            None
        }
    }
}

impl rodio::Source for StretchedSource {
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.samples.len() - self.position)
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        Some(self.duration())
    }
}
