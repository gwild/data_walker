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

/// Predefined interesting Mandelbrot points
pub struct MandelbrotPoint {
    pub name: &'static str,
    pub c_re: f64,
    pub c_im: f64,
}

pub const MANDELBROT_POINTS: &[MandelbrotPoint] = &[
    MandelbrotPoint { name: "cardioid", c_re: -0.75, c_im: 0.01 },
    MandelbrotPoint { name: "spiral", c_re: -0.7463, c_im: 0.1102 },
    MandelbrotPoint { name: "seahorse", c_re: -0.75, c_im: 0.1 },
    MandelbrotPoint { name: "antenna", c_re: -1.768, c_im: 0.001 },
    MandelbrotPoint { name: "period3", c_re: -0.1225, c_im: 0.7449 },
    MandelbrotPoint { name: "elephant", c_re: 0.275, c_im: 0.0 },
    MandelbrotPoint { name: "double_spiral", c_re: -0.925, c_im: 0.266 },
    MandelbrotPoint { name: "triple_spiral", c_re: -0.1011, c_im: 0.9563 },
];

/// Predefined interesting Julia set parameters
pub struct JuliaPoint {
    pub name: &'static str,
    pub c_re: f64,
    pub c_im: f64,
    pub z0_re: f64,
    pub z0_im: f64,
}

pub const JULIA_POINTS: &[JuliaPoint] = &[
    JuliaPoint { name: "rabbit", c_re: -0.123, c_im: 0.745, z0_re: 0.1, z0_im: 0.1 },
    JuliaPoint { name: "dendrite", c_re: 0.0, c_im: 1.0, z0_re: 0.01, z0_im: 0.01 },
    JuliaPoint { name: "dragon", c_re: -0.8, c_im: 0.156, z0_re: 0.1, z0_im: 0.0 },
    JuliaPoint { name: "spiral_julia", c_re: -0.4, c_im: 0.6, z0_re: 0.0, z0_im: 0.1 },
    JuliaPoint { name: "siegel", c_re: -0.391, c_im: -0.587, z0_re: 0.1, z0_im: 0.1 },
    JuliaPoint { name: "san_marco", c_re: -0.75, c_im: 0.0, z0_re: 0.1, z0_im: 0.1 },
];

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
