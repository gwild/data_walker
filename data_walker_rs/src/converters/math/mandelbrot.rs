//! Mandelbrot and Julia Set Orbits
//!
//! Computes complex iteration orbits and encodes them as base-12 sequences.
//! z_{n+1} = z_n^2 + c

use std::f64::consts::PI;

/// Compute Mandelbrot orbit: z = z² + c, starting from z = 0
///
/// Returns base-12 sequence based on orbit angles
pub fn mandelbrot_orbit(c_re: f64, c_im: f64, max_iter: usize) -> Vec<u8> {
    let c = (c_re, c_im);
    let mut z = (0.0, 0.0);
    let mut orbit = Vec::with_capacity(max_iter);

    for _ in 0..max_iter {
        // z = z² + c
        let z_new = (
            z.0 * z.0 - z.1 * z.1 + c.0,
            2.0 * z.0 * z.1 + c.1,
        );
        z = z_new;

        // Escape check
        if z.0 * z.0 + z.1 * z.1 > 1e12 {
            break;
        }

        // Encode angle to base-12
        let angle = z.1.atan2(z.0); // [-π, π]
        let normalized = (angle + PI) / (2.0 * PI); // [0, 1]
        let digit = (normalized * 11.99).floor() as u8;
        orbit.push(digit.min(11));
    }

    if orbit.is_empty() {
        orbit.push(0);
    }

    orbit
}

/// Compute Julia set orbit: z = z² + c, starting from z = z0
pub fn julia_orbit(c_re: f64, c_im: f64, z0_re: f64, z0_im: f64, max_iter: usize) -> Vec<u8> {
    let c = (c_re, c_im);
    let mut z = (z0_re, z0_im);
    let mut orbit = Vec::with_capacity(max_iter);

    for _ in 0..max_iter {
        // z = z² + c
        let z_new = (
            z.0 * z.0 - z.1 * z.1 + c.0,
            2.0 * z.0 * z.1 + c.1,
        );
        z = z_new;

        // Escape check
        if z.0 * z.0 + z.1 * z.1 > 1e12 {
            break;
        }

        // Encode angle to base-12
        let angle = z.1.atan2(z.0);
        let normalized = (angle + PI) / (2.0 * PI);
        let digit = (normalized * 11.99).floor() as u8;
        orbit.push(digit.min(11));
    }

    if orbit.is_empty() {
        orbit.push(0);
    }

    orbit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mandelbrot_bounded() {
        // Point in main cardioid should produce long orbit
        let orbit = mandelbrot_orbit(-0.5, 0.0, 1000);
        assert!(orbit.len() >= 100);
    }

    #[test]
    fn test_mandelbrot_escape() {
        // Point outside set should escape quickly
        let orbit = mandelbrot_orbit(2.0, 0.0, 1000);
        assert!(orbit.len() < 10);
    }

    #[test]
    fn test_all_digits_valid() {
        let orbit = mandelbrot_orbit(-0.75, 0.1, 500);
        assert!(orbit.iter().all(|&d| d < 12));
    }

    #[test]
    fn test_julia_orbit() {
        let orbit = julia_orbit(-0.123, 0.745, 0.1, 0.1, 500);
        assert!(!orbit.is_empty());
        assert!(orbit.iter().all(|&d| d < 12));
    }
}
