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
//! - pdb: Protein structure backbone coordinates from PDB files

pub mod audio;
pub mod math;

// Re-export MidiNote for use in audio synthesis
pub use audio::MidiNote;

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

/// DNA Converter: ACGT (base-4) to base-12 via fixed chunks
/// Process 5 nucleotides at a time (4^5 = 1024), convert to base-12 digits
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
    let mut count: u32 = 0;

    for ch in sequence.chars() {
        if let Some(digit) = base4_map(ch) {
            accumulator += digit * 4u64.pow(count);
            count += 1;

            // Every 5 nucleotides (4^5 = 1024), emit base-12 digits and reset
            if count == 5 {
                while accumulator > 0 {
                    base12.push((accumulator % 12) as u8);
                    accumulator /= 12;
                }
                count = 0;
            }
        }
    }

    // Emit remaining partial chunk
    if accumulator > 0 {
        while accumulator > 0 {
            base12.push((accumulator % 12) as u8);
            accumulator /= 12;
        }
    }

    if base12.is_empty() {
        base12.push(0);
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
// Base-4 converters
// ============================================================================

/// Normalize values to 0-3 range
pub fn normalize_to_base4(values: &[f64]) -> Vec<u8> {
    if values.is_empty() {
        return vec![0];
    }

    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;

    if range == 0.0 {
        return vec![2; values.len()];
    }

    values
        .iter()
        .map(|&v| {
            let normalized = (v - min) / range;
            (normalized * 3.99).floor() as u8
        })
        .collect()
}

/// DNA base-4: direct ACGT → 0,1,2,3 (each nucleotide is one digit)
pub fn convert_dna_base4(sequence: &str) -> Vec<u8> {
    sequence.chars().filter_map(|c| match c {
        'A' | 'a' => Some(0),
        'C' | 'c' => Some(1),
        'G' | 'g' => Some(2),
        'T' | 't' => Some(3),
        _ => None,
    }).collect()
}

/// Finance base-4: price deltas normalized to 0-3
pub fn convert_finance_base4(prices: &[f64]) -> Vec<u8> {
    if prices.len() < 2 {
        return vec![0];
    }
    let deltas: Vec<f64> = prices.windows(2).map(|w| (w[1] - w[0]) / w[0]).collect();
    normalize_to_base4(&deltas)
}

/// Cosmos base-4: strain normalized to 0-3
pub fn convert_cosmos_base4(strain: &[f64]) -> Vec<u8> {
    normalize_to_base4(strain)
}

// ============================================================================
// Raw file loaders - load file and convert on-the-fly
// ============================================================================

/// Load FASTA file and convert to base digits
pub fn load_dna_raw(path: &Path, base: u32) -> anyhow::Result<Vec<u8>> {
    let content = std::fs::read_to_string(path)?;

    let sequence: String = content
        .lines()
        .filter(|line| !line.starts_with('>'))
        .collect::<Vec<_>>()
        .join("");

    if sequence.is_empty() {
        anyhow::bail!("No sequence data in FASTA file");
    }

    Ok(match base {
        4 => convert_dna_base4(&sequence),
        6 => convert_dna(&sequence).iter().map(|&d| d % 6).collect(),
        _ => convert_dna(&sequence),
    })
}

/// Load finance JSON (raw prices) and convert to base digits
pub fn load_finance_raw(path: &Path, base: u32) -> anyhow::Result<Vec<u8>> {
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

    Ok(match base {
        4 => convert_finance_base4(&prices),
        6 => convert_finance(&prices).iter().map(|&d| d % 6).collect(),
        _ => convert_finance(&prices),
    })
}

/// Load cosmos strain data (.txt.gz) and convert to base digits
pub fn load_cosmos_raw(path: &Path, base: u32) -> anyhow::Result<Vec<u8>> {
    use std::io::{BufRead, BufReader};
    use flate2::read::GzDecoder;

    let file = std::fs::File::open(path)?;
    let decoder = GzDecoder::new(file);
    let reader = BufReader::new(decoder);

    let mut strain_values: Vec<f64> = Vec::new();

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Ok(value) = trimmed.parse::<f64>() {
            if value.is_finite() {
                strain_values.push(value);
            }
        }
    }

    if strain_values.is_empty() {
        anyhow::bail!("No valid strain data in file");
    }

    Ok(match base {
        4 => convert_cosmos_base4(&strain_values),
        6 => convert_cosmos(&strain_values).iter().map(|&d| d % 6).collect(),
        _ => convert_cosmos(&strain_values),
    })
}

/// Load audio file (WAV or MP3) and convert to base digits
pub fn load_audio_raw(path: &Path, base: u32) -> anyhow::Result<Vec<u8>> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "wav" => audio::wav_to_base(path, base),
        "mp3" => load_mp3_raw(path, base),
        _ => anyhow::bail!("Unsupported audio format: {}", ext),
    }
}

/// Load audio file (WAV or MP3) and extract MIDI notes for accurate synthesis
pub fn load_audio_midi_notes(path: &Path) -> anyhow::Result<Vec<MidiNote>> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "wav" => audio::wav_to_midi_notes(path),
        "mp3" => load_mp3_midi_notes(path),
        _ => anyhow::bail!("Unsupported audio format: {}", ext),
    }
}

/// Load MP3 file samples (shared helper for base and MIDI extraction)
fn load_mp3_samples(path: &Path) -> anyhow::Result<(Vec<f32>, u32)> {
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

    Ok((samples, sample_rate))
}

/// Load MP3 file and convert to base digits
fn load_mp3_raw(path: &Path, base: u32) -> anyhow::Result<Vec<u8>> {
    let (samples, sample_rate) = load_mp3_samples(path)?;

    Ok(match base {
        4 => audio::audio_to_base4(&samples, sample_rate),
        6 => audio::audio_to_base12(&samples, sample_rate).iter().map(|&d| d % 6).collect(),
        _ => audio::audio_to_base12(&samples, sample_rate),
    })
}

/// Load MP3 file and extract MIDI notes
fn load_mp3_midi_notes(path: &Path) -> anyhow::Result<Vec<MidiNote>> {
    let (samples, sample_rate) = load_mp3_samples(path)?;
    Ok(audio::audio_to_midi_notes(&samples, sample_rate))
}

// ============================================================================
// PDB Protein Structure converters
// ============================================================================

/// Parse PDB ATOM records and extract C-alpha (backbone) coordinates
fn parse_pdb_ca_coords(content: &str) -> Vec<[f64; 3]> {
    let mut coords = Vec::new();
    for line in content.lines() {
        if (line.starts_with("ATOM") || line.starts_with("HETATM")) && line.len() >= 54 {
            if let Some(atom_name) = line.get(12..16) {
                if atom_name.trim() == "CA" {
                    if let (Some(x_s), Some(y_s), Some(z_s)) =
                        (line.get(30..38), line.get(38..46), line.get(46..54))
                    {
                        if let (Ok(x), Ok(y), Ok(z)) = (
                            x_s.trim().parse::<f64>(),
                            y_s.trim().parse::<f64>(),
                            z_s.trim().parse::<f64>(),
                        ) {
                            coords.push([x, y, z]);
                        }
                    }
                }
            }
        }
    }
    coords
}

/// Parse amino acid residues from PDB ATOM records (one per residue via CA atoms)
fn parse_pdb_residues(content: &str) -> Vec<String> {
    let mut residues = Vec::new();
    let mut last_res_seq = String::new();
    for line in content.lines() {
        if line.starts_with("ATOM") && line.len() >= 26 {
            if let Some(atom_name) = line.get(12..16) {
                if atom_name.trim() == "CA" {
                    if let (Some(res_name), Some(res_seq)) = (line.get(17..20), line.get(22..26)) {
                        let seq = res_seq.trim().to_string();
                        if seq != last_res_seq {
                            residues.push(res_name.trim().to_string());
                            last_res_seq = seq;
                        }
                    }
                }
            }
        }
    }
    residues
}

/// PDB Backbone Converter: sequential Cα direction changes → base-12
/// Computes direction deltas between consecutive Cα atoms and maps
/// the dominant axis/direction to base-12 (like a 3D turtle walk of the backbone)
pub fn convert_pdb_backbone(coords: &[[f64; 3]]) -> Vec<u8> {
    if coords.len() < 2 {
        return vec![0];
    }

    let mut digits = Vec::with_capacity(coords.len() - 1);

    for pair in coords.windows(2) {
        let dx = pair[1][0] - pair[0][0];
        let dy = pair[1][1] - pair[0][1];
        let dz = pair[1][2] - pair[0][2];

        // Find dominant axis and encode as base-12:
        // 0-5: translations (+X, -X, +Y, -Y, +Z, -Z)
        // 6-11: encode the magnitude of the secondary axes as rotation hints
        let abs_dx = dx.abs();
        let abs_dy = dy.abs();
        let abs_dz = dz.abs();

        // Primary direction (translation)
        let primary = if abs_dx >= abs_dy && abs_dx >= abs_dz {
            if dx >= 0.0 { 0 } else { 1 } // +X or -X
        } else if abs_dy >= abs_dx && abs_dy >= abs_dz {
            if dy >= 0.0 { 2 } else { 3 } // +Y or -Y
        } else {
            if dz >= 0.0 { 4 } else { 5 } // +Z or -Z
        };

        digits.push(primary);

        // Secondary: encode rotation based on off-axis components
        // This gives the walk both translation AND rotation character
        let total = abs_dx + abs_dy + abs_dz;
        if total > 0.0 {
            let ratio = match primary {
                0 | 1 => (abs_dy + abs_dz) / total, // off-axis fraction
                2 | 3 => (abs_dx + abs_dz) / total,
                _ => (abs_dx + abs_dy) / total,
            };
            // Map ratio [0, 1) to rotation values [6, 11]
            let rot = 6 + (ratio * 5.99).floor() as u8;
            digits.push(rot);
        }
    }

    digits
}

/// PDB Sequence Converter: amino acid type → base-12 by chemical property
/// Groups 20 standard amino acids into 12 buckets by physicochemical properties
pub fn convert_pdb_sequence(residues: &[String]) -> Vec<u8> {
    residues.iter().map(|res| {
        match res.as_str() {
            // Nonpolar aliphatic
            "GLY"         => 0,  // Glycine - smallest
            "ALA"         => 1,  // Alanine - small hydrophobic
            "VAL" | "ILE" => 2,  // Branched-chain hydrophobic
            "LEU"         => 3,  // Leucine - hydrophobic
            // Nonpolar aromatic
            "PHE"         => 4,  // Phenylalanine
            "TRP"         => 5,  // Tryptophan - largest
            // Polar uncharged
            "SER" | "THR" => 6,  // Hydroxyl group
            "ASN" | "GLN" => 7,  // Amide group
            "CYS" | "MET" => 8,  // Sulfur-containing
            // Polar charged
            "ASP" | "GLU" => 9,  // Acidic (negative)
            "LYS" | "ARG" => 10, // Basic (positive)
            "HIS"         => 11, // Histidine - can be + or neutral
            // Special / proline
            "PRO"         => 0,  // Proline - structurally rigid, map to glycine bucket
            // Non-standard → middle value
            _             => 6,
        }
    }).collect()
}

/// Load PDB file - raw Cα coordinates for direct 3D structure rendering
/// Returns (coords, residue_names) for the backbone trace
pub fn load_pdb_structure(path: &Path) -> anyhow::Result<(Vec<[f32; 3]>, Vec<String>)> {
    let content = std::fs::read_to_string(path)?;
    let coords = parse_pdb_ca_coords(&content);
    let residues = parse_pdb_residues(&content);

    if coords.len() < 2 {
        anyhow::bail!("No C-alpha backbone found in PDB file (need >= 2 atoms)");
    }

    // Convert f64 coords to f32
    let points: Vec<[f32; 3]> = coords.iter()
        .map(|c| [c[0] as f32, c[1] as f32, c[2] as f32])
        .collect();

    Ok((points, residues))
}

/// Map amino acid name to a color category (0-11) for structure coloring
pub fn residue_color(res: &str) -> [f32; 3] {
    match res {
        // Nonpolar aliphatic - grays/whites
        "GLY"         => [0.75, 0.75, 0.75], // light gray
        "ALA"         => [0.80, 0.80, 0.80], // lighter gray
        "VAL" | "ILE" => [0.15, 0.60, 0.15], // green
        "LEU"         => [0.10, 0.50, 0.10], // dark green
        "PRO"         => [0.86, 0.65, 0.13], // goldenrod
        // Aromatic - purple/magenta
        "PHE"         => [0.60, 0.20, 0.80], // purple
        "TRP"         => [0.70, 0.10, 0.70], // magenta
        "TYR"         => [0.50, 0.25, 0.70], // violet
        // Polar uncharged - teal/cyan
        "SER"         => [0.25, 0.75, 0.75], // teal
        "THR"         => [0.20, 0.65, 0.65], // dark teal
        "ASN"         => [0.00, 0.80, 0.80], // cyan
        "GLN"         => [0.00, 0.65, 0.65], // dark cyan
        // Sulfur - yellow
        "CYS"         => [0.90, 0.90, 0.00], // bright yellow
        "MET"         => [0.80, 0.80, 0.00], // yellow
        // Charged negative - red
        "ASP"         => [0.90, 0.15, 0.15], // red
        "GLU"         => [0.80, 0.10, 0.10], // dark red
        // Charged positive - blue
        "LYS"         => [0.20, 0.30, 0.90], // blue
        "ARG"         => [0.15, 0.25, 0.80], // dark blue
        "HIS"         => [0.40, 0.50, 0.90], // light blue
        // Unknown
        _             => [0.50, 0.50, 0.50], // gray
    }
}

/// Load PDB file - backbone mode: Cα direction deltas → base-12
pub fn load_pdb_backbone_raw(path: &Path, base: u32) -> anyhow::Result<Vec<u8>> {
    let content = std::fs::read_to_string(path)?;
    let coords = parse_pdb_ca_coords(&content);

    if coords.len() < 2 {
        anyhow::bail!("No C-alpha backbone found in PDB file (need >= 2 atoms)");
    }

    let base12 = convert_pdb_backbone(&coords);
    Ok(match base {
        4 => base12.iter().map(|&d| d % 4).collect(),
        6 => base12.iter().map(|&d| d % 6).collect(),
        _ => base12,
    })
}

/// Load PDB file - sequence mode: amino acid properties → base-12
pub fn load_pdb_sequence_raw(path: &Path, base: u32) -> anyhow::Result<Vec<u8>> {
    let content = std::fs::read_to_string(path)?;
    let residues = parse_pdb_residues(&content);

    if residues.is_empty() {
        anyhow::bail!("No amino acid residues found in PDB file");
    }

    let base12 = convert_pdb_sequence(&residues);
    Ok(match base {
        4 => base12.iter().map(|&d| d % 4).collect(),
        6 => base12.iter().map(|&d| d % 6).collect(),
        _ => base12,
    })
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

    #[test]
    fn test_pdb_backbone() {
        let coords = vec![
            [0.0, 0.0, 0.0],
            [3.8, 0.0, 0.0],   // +X
            [3.8, 3.8, 0.0],   // +Y
            [3.8, 3.8, 3.8],   // +Z
        ];
        let digits = convert_pdb_backbone(&coords);
        assert!(!digits.is_empty());
        assert!(digits.iter().all(|&d| d < 12));
        // First move is purely +X, so primary digit should be 0
        assert_eq!(digits[0], 0);
    }

    #[test]
    fn test_pdb_sequence() {
        let residues = vec!["GLY", "ALA", "LEU", "ASP", "HIS"]
            .into_iter().map(String::from).collect::<Vec<_>>();
        let digits = convert_pdb_sequence(&residues);
        assert_eq!(digits.len(), 5);
        assert_eq!(digits[0], 0);  // GLY
        assert_eq!(digits[1], 1);  // ALA
        assert_eq!(digits[2], 3);  // LEU
        assert_eq!(digits[3], 9);  // ASP
        assert_eq!(digits[4], 11); // HIS
    }

    #[test]
    fn test_parse_pdb_ca_coords() {
        let pdb = "\
ATOM      1  N   ALA A   1       1.000   2.000   3.000  1.00  0.00           N
ATOM      2  CA  ALA A   1       2.000   3.000   4.000  1.00  0.00           C
ATOM      3  C   ALA A   1       3.000   4.000   5.000  1.00  0.00           C
ATOM      4  N   GLY A   2       4.000   5.000   6.000  1.00  0.00           N
ATOM      5  CA  GLY A   2       5.000   6.000   7.000  1.00  0.00           C
END
";
        let coords = parse_pdb_ca_coords(pdb);
        assert_eq!(coords.len(), 2);
        assert_eq!(coords[0], [2.0, 3.0, 4.0]);
        assert_eq!(coords[1], [5.0, 6.0, 7.0]);
    }
}
