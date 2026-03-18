//! Audio file playback using rodio

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use rodio::{Decoder, OutputStreamHandle, Sink, Source};
use tracing::debug;

/// Create a sink for playing an audio file
pub fn create_file_sink(
    stream_handle: &OutputStreamHandle,
    path: &Path,
) -> anyhow::Result<(Option<Sink>, f32)> {
    let duration = get_audio_duration(path)?;

    // Loop natural file playback so short real-world clips keep playing
    // until the user deselects them or a synced/stretched version replaces them.
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let source = Decoder::new_looped(reader)?;

    // Create a sink and append the source
    let sink = Sink::try_new(stream_handle)?;
    sink.append(source);
    sink.pause(); // Start paused

    debug!("Created audio sink for {:?} (duration: {:.1}s)", path, duration);
    Ok((Some(sink), duration))
}

/// Helper to get audio file duration without creating a sink
pub fn get_audio_duration(path: &Path) -> anyhow::Result<f32> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let source = Decoder::new(reader)?;

    Ok(source.total_duration()
        .map(|d| d.as_secs_f32())
        .unwrap_or(30.0))
}
