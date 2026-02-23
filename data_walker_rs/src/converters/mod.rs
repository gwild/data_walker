//! Data Converters - Transform raw data to base-12 sequences
//!
//! Each converter module handles a specific data type:
//! - math: Mathematical constants, fractals, sequences (no downloads)
//! - audio: Spectrogram analysis
//! - dna: ACGT base-4 to base-12
//! - finance: Price deltas
//! - cosmos: LIGO strain data

pub mod audio;
pub mod math;

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
