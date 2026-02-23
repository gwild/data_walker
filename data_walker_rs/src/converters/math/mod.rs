//! Math Converters - Standalone module (no external downloads)
//!
//! Generates base-12 sequences from mathematical constructs:
//! - Constants: pi, e, sqrt(2), phi, ln(2)
//! - Fractals: Dragon curve, Koch snowflake, etc.
//! - Mandelbrot/Julia orbits
//! - Sequences: Fibonacci word, Thue-Morse, logistic map

pub mod constants;
pub mod fractals;
pub mod mandelbrot;
pub mod sequences;

pub use constants::*;
pub use fractals::*;
pub use mandelbrot::*;
pub use sequences::*;

/// All available math generators
#[derive(Debug, Clone)]
pub enum MathGenerator {
    // Constants
    Pi,
    E,
    Sqrt2,
    Phi,
    Ln2,

    // Fractals
    DragonCurve,
    KochSnowflake,
    SierpinskiArrowhead,
    HilbertCurve,
    PeanoCurve,

    // Mandelbrot/Julia
    Mandelbrot { c_re: f64, c_im: f64 },
    Julia { c_re: f64, c_im: f64, z0_re: f64, z0_im: f64 },

    // Sequences
    FibonacciWord,
    ThueMorse,
    LogisticMap { r: f64, x0: f64 },
}

impl MathGenerator {
    /// Generate base-12 sequence with specified length
    pub fn generate(&self, n_digits: usize) -> Vec<u8> {
        match self {
            MathGenerator::Pi => constants::pi_base12(n_digits),
            MathGenerator::E => constants::e_base12(n_digits),
            MathGenerator::Sqrt2 => constants::sqrt2_base12(n_digits),
            MathGenerator::Phi => constants::phi_base12(n_digits),
            MathGenerator::Ln2 => constants::ln2_base12(n_digits),

            MathGenerator::DragonCurve => fractals::dragon_curve(14),
            MathGenerator::KochSnowflake => fractals::koch_snowflake(5),
            MathGenerator::SierpinskiArrowhead => fractals::sierpinski_arrowhead(9),
            MathGenerator::HilbertCurve => fractals::hilbert_curve(6),
            MathGenerator::PeanoCurve => fractals::peano_curve(4),

            MathGenerator::Mandelbrot { c_re, c_im } => {
                mandelbrot::mandelbrot_orbit(*c_re, *c_im, n_digits)
            }
            MathGenerator::Julia { c_re, c_im, z0_re, z0_im } => {
                mandelbrot::julia_orbit(*c_re, *c_im, *z0_re, *z0_im, n_digits)
            }

            MathGenerator::FibonacciWord => sequences::fibonacci_word(n_digits),
            MathGenerator::ThueMorse => sequences::thue_morse(n_digits),
            MathGenerator::LogisticMap { r, x0 } => {
                sequences::logistic_map(*r, *x0, n_digits)
            }
        }
    }

    /// Parse from converter string like "math.constant.pi"
    pub fn from_converter_string(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.first() != Some(&"math") {
            return None;
        }

        match (parts.get(1), parts.get(2)) {
            (Some(&"constant"), Some(&"pi")) => Some(MathGenerator::Pi),
            (Some(&"constant"), Some(&"e")) => Some(MathGenerator::E),
            (Some(&"constant"), Some(&"sqrt2")) => Some(MathGenerator::Sqrt2),
            (Some(&"constant"), Some(&"phi")) => Some(MathGenerator::Phi),
            (Some(&"constant"), Some(&"ln2")) => Some(MathGenerator::Ln2),

            (Some(&"fractal"), Some(&"dragon")) => Some(MathGenerator::DragonCurve),
            (Some(&"fractal"), Some(&"koch")) => Some(MathGenerator::KochSnowflake),
            (Some(&"fractal"), Some(&"sierpinski")) => Some(MathGenerator::SierpinskiArrowhead),
            (Some(&"fractal"), Some(&"hilbert")) => Some(MathGenerator::HilbertCurve),
            (Some(&"fractal"), Some(&"peano")) => Some(MathGenerator::PeanoCurve),

            (Some(&"sequence"), Some(&"fibonacci")) => Some(MathGenerator::FibonacciWord),
            (Some(&"sequence"), Some(&"thue_morse")) => Some(MathGenerator::ThueMorse),
            (Some(&"sequence"), Some(&"logistic")) => {
                Some(MathGenerator::LogisticMap { r: 3.99, x0: 0.5 })
            }
            // Logistic map variants
            (Some(&"sequence"), Some(&"logistic_chaos")) => {
                Some(MathGenerator::LogisticMap { r: 3.99, x0: 0.5 })
            }
            (Some(&"sequence"), Some(&"logistic_periodic")) => {
                Some(MathGenerator::LogisticMap { r: 3.5, x0: 0.5 })
            }
            (Some(&"sequence"), Some(&"logistic_period3")) => {
                Some(MathGenerator::LogisticMap { r: 3.8284, x0: 0.5 })
            }

            // Mandelbrot named points (interesting locations on the boundary)
            (Some(&"mandelbrot"), Some(&"cardioid")) => {
                // Main cardioid boundary
                Some(MathGenerator::Mandelbrot { c_re: 0.25, c_im: 0.5 })
            }
            (Some(&"mandelbrot"), Some(&"spiral")) => {
                // Spiral region
                Some(MathGenerator::Mandelbrot { c_re: -0.75, c_im: 0.1 })
            }
            (Some(&"mandelbrot"), Some(&"antenna")) => {
                // Near the antenna
                Some(MathGenerator::Mandelbrot { c_re: -1.75, c_im: 0.0 })
            }
            (Some(&"mandelbrot"), Some(&"period3")) => {
                // Period-3 bulb
                Some(MathGenerator::Mandelbrot { c_re: -0.122, c_im: 0.745 })
            }

            // Julia set named points (classic Julia set parameters)
            (Some(&"julia"), Some(&"rabbit")) => {
                // Douady's rabbit
                Some(MathGenerator::Julia { c_re: -0.123, c_im: 0.745, z0_re: 0.0, z0_im: 0.0 })
            }
            (Some(&"julia"), Some(&"dragon")) => {
                // Dragon Julia
                Some(MathGenerator::Julia { c_re: -0.8, c_im: 0.156, z0_re: 0.0, z0_im: 0.0 })
            }
            (Some(&"julia"), Some(&"spiral")) => {
                // Spiral Julia
                Some(MathGenerator::Julia { c_re: -0.4, c_im: 0.6, z0_re: 0.0, z0_im: 0.0 })
            }
            (Some(&"julia"), Some(&"siegel")) => {
                // Siegel disk
                Some(MathGenerator::Julia { c_re: -0.391, c_im: -0.587, z0_re: 0.0, z0_im: 0.0 })
            }

            _ => None,
        }
    }
}
