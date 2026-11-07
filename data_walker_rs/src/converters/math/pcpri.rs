//! PCPRI - Phase-Coherent Pseudorandom Recursive Input
//!
//! A faithful port of the PCPRI generator from Gregory Wildes' pcpri repo
//! (https://github.com/gwild/pcpri), so Data Walker can walk PCPRI data
//! locally, the same way it computes pi digits or logistic-map sequences.
//!
//! ## Provenance / fidelity
//!
//! The generator core below (state, branch operators, conductor, phase
//! analyzer, feedback extraction, and the step evolution in
//! `PcpriGenerator::step`) is ported line-for-line from pcpri's
//! `src/state.rs`, `src/conductor.rs`, `src/phase.rs`, and
//! `src/generator.rs` at pcpri commit 2d622104f4543bcac7530b61b683ac70dfafb9d3.
//! The algorithm is unchanged. The only omissions are the original's
//! LLM-token string encoding (base64 state + hex feedback), which Data
//! Walker does not need, and the original's DB/MindsDB modules, which are
//! unrelated to generation.
//!
//! This is NOT a "PCPRI-like" simulation: earlier walk experiments used a
//! Python stand-in (an LCG labelled as PCPRI) when the real generator was
//! unavailable, which produced misleading results. This module exists so
//! that never happens again - if these digits say PCPRI, they came out of
//! the real recursive phase-feedback dynamics, and a test harness comparing
//! this port against the original crate step-for-step (conductor value,
//! feedback word, and packed state bytes) is part of the change that
//! introduced it.
//!
//! ## Digit streams
//!
//! Each generator step produces three observable signals, exposed as three
//! base-12 digit streams:
//!
//! - **Conductor** (`conductor_base12`): the conductor value c in [-1, 1],
//!   mapped linearly to 0-11. This is the signal the phase analyzer runs
//!   on, analogous to walking LIGO strain amplitude.
//! - **Feedback** (`feedback_base12`): the u64 feedback word z, emitted
//!   as base-12 digits, least-significant first (the same chunking
//!   convention as the DNA converter).
//! - **State** (`state_base12`): the packed state bytes, each byte emitted
//!   as three base-12 digits (b % 12, (b / 12) % 12, b / 144), which is an
//!   exact base-12 expansion of the byte.
//!
//! ## Warm-up (feedback and state streams only)
//!
//! Phase analysis only starts once `fft_size` conductor samples have been
//! accumulated; before that the feedback word is 0 and (with zero feedback)
//! the branch operators leave the all-zero initial state unchanged. Those
//! warm-up steps carry no PCPRI structure, so the feedback and state
//! streams discard the first `fft_size` steps before emitting digits.
//! This discard is explicit and deliberate, not a silent filter: it is the
//! generator's own documented start-up transient. The conductor stream is
//! emitted from step 0 because its initial ramp IS the signal.

use num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::collections::VecDeque;
use std::sync::Arc;

// ============================================================================
// State and branch operators (ported from pcpri src/state.rs)
// ============================================================================

/// Core state: a vector of boolean values representing the internal state
pub type State = Vec<bool>;

/// Branching operator function type
pub type BranchOp = fn(&State, u64) -> State;

/// Branching operator f1: XOR each bit with a rotated pattern from z
pub fn f1(s: &State, z: u64) -> State {
    s.iter()
        .enumerate()
        .map(|(i, b)| b ^ ((z >> (i % 64)) & 1 == 1))
        .collect()
}

/// Branching operator f2: XOR reversed state with rotated pattern from z
pub fn f2(s: &State, z: u64) -> State {
    s.iter()
        .rev()
        .enumerate()
        .map(|(i, b)| b ^ ((z >> (i % 64)) & 1 == 1))
        .collect()
}

/// Branching operator f3: XOR with shifted pattern
pub fn f3(s: &State, z: u64) -> State {
    s.iter()
        .enumerate()
        .map(|(i, b)| {
            let shift = (z >> (i % 64)) & 63;
            b ^ ((shift & 1) == 1)
        })
        .collect()
}

/// Default branching function set
pub fn default_branch_set() -> Vec<BranchOp> {
    vec![f1, f2, f3]
}

/// Compute bit-balance emission signal
/// Returns the normalized sum of (-1)^s_i for each bit
pub fn bit_balance(s: &State) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let sum: i32 = s.iter().map(|b| if *b { 1 } else { -1 }).sum();
    sum as f64 / s.len() as f64
}

// ============================================================================
// Conductor (ported from pcpri src/conductor.rs)
// ============================================================================

/// Conductor accumulates bit-balance signals with exponential smoothing
pub struct Conductor {
    /// Feedback integration constant (0 < alpha <= 1)
    alpha: f64,
    /// Current accumulated value
    c: f64,
}

impl Conductor {
    /// Create a new conductor with given alpha
    ///
    /// # Panics
    /// Panics if alpha is not in (0, 1]
    pub fn new(alpha: f64) -> Self {
        assert!(alpha > 0.0 && alpha <= 1.0, "alpha must be in (0, 1]");
        Self { alpha, c: 0.0 }
    }

    /// Update the conductor with a new bit-balance emission
    /// Formula: c_{t+1} = (1 - alpha) * c_t + alpha * y_t
    pub fn update(&mut self, y: f64) {
        self.c = (1.0 - self.alpha) * self.c + self.alpha * y;
    }

    /// Get the current accumulated value
    pub fn value(&self) -> f64 {
        self.c
    }

    /// Reset the conductor to zero
    #[allow(dead_code)] // ported from pcpri; kept for API fidelity
    pub fn reset(&mut self) {
        self.c = 0.0;
    }
}

// ============================================================================
// Phase analysis (ported from pcpri src/phase.rs)
// ============================================================================

/// Phase feature for a frequency bin
#[derive(Debug, Clone)]
#[allow(dead_code)] // `k` mirrors the original pcpri struct; the walk extraction does not read it
pub struct BinFeature {
    /// Frequency bin index
    pub k: usize,
    /// Demodulated phase delta
    pub delta_phi: f64,
    /// Coherency value
    pub coherency: f64,
    /// Amplitude
    pub amplitude: f64,
}

/// Phase analyzer for Snail-style analysis
pub struct PhaseAnalyzer {
    /// FFT planner
    fft: Arc<dyn Fft<f64>>,
    /// FFT size
    n: usize,
    /// Hop size (samples between frames)
    hop: usize,
    /// Previous phase for each bin
    prev_phases: Vec<f64>,
    /// Phase delta history for coherency computation (VecDeque for O(1) pop_front)
    phase_deltas_history: Vec<VecDeque<f64>>,
    /// Coherency window length
    coherency_window: usize,
    /// Pre-allocated FFT buffer to avoid per-frame allocations
    fft_buffer: Vec<Complex<f64>>,
}

impl PhaseAnalyzer {
    /// Create a new phase analyzer
    pub fn new(n: usize, hop: usize, coherency_window: usize) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(n);

        // Pre-allocate VecDeques with capacity for coherency_window
        let mut phase_deltas_history = Vec::with_capacity(n);
        for _ in 0..n {
            phase_deltas_history.push(VecDeque::with_capacity(coherency_window));
        }

        Self {
            fft,
            n,
            hop,
            prev_phases: vec![0.0; n],
            phase_deltas_history,
            coherency_window,
            fft_buffer: vec![Complex::new(0.0, 0.0); n],
        }
    }

    /// Process a frame of signal and compute phase features
    /// Frame length must equal the FFT size.
    pub fn process_frame(&mut self, frame: &[f64]) -> Vec<BinFeature> {
        assert_eq!(frame.len(), self.n, "Frame length must match FFT size");

        // Reuse pre-allocated buffer: copy frame data and perform FFT
        for (i, x) in frame.iter().enumerate() {
            self.fft_buffer[i] = Complex::new(*x, 0.0);
        }
        self.fft.process(&mut self.fft_buffer);

        // Compute demodulated phase for each bin (pre-allocate features vector)
        let mut features = Vec::with_capacity(self.n);
        for k in 0..self.n {
            let current_phase = self.fft_buffer[k].arg();
            let prev_phase = self.prev_phases[k];

            // Demodulated phase: remove expected phase advance
            let expected_advance =
                2.0 * std::f64::consts::PI * k as f64 * self.hop as f64 / self.n as f64;
            let delta_phi = wrap_phase(current_phase - prev_phase - expected_advance);

            // Update phase delta history (O(1) operations with VecDeque)
            self.phase_deltas_history[k].push_back(delta_phi);
            if self.phase_deltas_history[k].len() > self.coherency_window {
                self.phase_deltas_history[k].pop_front();
            }

            // Compute coherency from recent phase deltas
            let coherency = self.compute_coherency_deque(&self.phase_deltas_history[k]);
            let amplitude = self.fft_buffer[k].norm();

            features.push(BinFeature {
                k,
                delta_phi,
                coherency,
                amplitude,
            });

            // Update previous phase
            self.prev_phases[k] = current_phase;
        }

        features
    }

    /// Compute coherency from phase deltas (VecDeque version)
    /// Coherency is in [0, 1] where 1 means perfectly coherent.
    fn compute_coherency_deque(&self, deltas: &VecDeque<f64>) -> f64 {
        if deltas.is_empty() {
            return 0.0;
        }

        // Sum complex exponentials of phase deltas
        let mut sum = Complex::new(0.0, 0.0);
        for d in deltas.iter() {
            sum += Complex::from_polar(1.0, *d);
        }

        // Coherency is the normalized magnitude
        sum.norm() / deltas.len() as f64
    }

    /// Reset the analyzer state
    #[allow(dead_code)] // ported from pcpri; kept for API fidelity
    pub fn reset(&mut self) {
        self.prev_phases.fill(0.0);
        for history in &mut self.phase_deltas_history {
            history.clear();
        }
    }
}

/// Wrap phase to [-π, π]
fn wrap_phase(phi: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    let mut wrapped = phi % two_pi;
    if wrapped > std::f64::consts::PI {
        wrapped -= two_pi;
    } else if wrapped < -std::f64::consts::PI {
        wrapped += two_pi;
    }
    wrapped
}

/// Extract feedback word from phase features
/// Selects top-M bins by score (amplitude * coherency) and encodes
/// their phase deltas into a u64 feedback word.
pub fn build_feedback(features: &[BinFeature], top_m: usize) -> u64 {
    // Score and sort features
    let mut scored: Vec<(usize, f64)> = features
        .iter()
        .enumerate()
        .map(|(i, f)| (i, f.amplitude * f.coherency))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    // Take top M and build feedback word
    let mut feedback = 0u64;
    for (idx, _score) in scored.iter().take(top_m.min(64)) {
        let feature = &features[*idx];
        let bit = (feature.delta_phi > 0.0) as u64;
        feedback = (feedback << 1) | bit;
    }

    feedback
}

// ============================================================================
// Generator (ported from pcpri src/generator.rs)
//
// The original's `next_token()` both advanced the state and encoded a token
// string. Here `step()` performs the identical state evolution (steps 1-5 of
// the original) and the caller reads the resulting signals via
// `conductor_value()`, `last_feedback()`, and `packed_state_bytes()`.
// ============================================================================

/// Configuration parameters for PCPRI generator
#[derive(Debug, Clone)]
pub struct PcpriConfig {
    /// Core state size (bits)
    pub n: usize,
    /// Branching factor (number of branch operators)
    pub branching_factor: usize,
    /// Feedback integration constant
    pub alpha: f64,
    /// FFT size for phase analysis
    pub fft_size: usize,
    /// Hop size between frames
    pub hop_size: usize,
    /// Coherency window length
    pub coherency_window: usize,
    /// Number of top bins to use for feedback
    pub top_m: usize,
}

impl PcpriConfig {
    /// Validate configuration invariants (fail-fast, no silent defaults).
    pub fn validate(&self) -> Result<(), String> {
        if self.n == 0 {
            return Err("n must be > 0".to_string());
        }
        if self.branching_factor == 0 {
            return Err("branching_factor must be > 0".to_string());
        }
        if !self.alpha.is_finite() || self.alpha <= 0.0 || self.alpha > 1.0 {
            return Err("alpha must be in (0, 1]".to_string());
        }
        if self.fft_size == 0 {
            return Err("fft_size must be > 0".to_string());
        }
        if self.hop_size == 0 {
            return Err("hop_size must be > 0".to_string());
        }
        if self.hop_size > self.fft_size {
            return Err("hop_size must be <= fft_size".to_string());
        }
        if self.coherency_window == 0 {
            return Err("coherency_window must be > 0".to_string());
        }
        if self.top_m == 0 {
            return Err("top_m must be > 0".to_string());
        }
        if self.top_m > 64 {
            return Err("top_m must be <= 64 (feedback word is u64)".to_string());
        }
        if self.top_m > self.fft_size {
            return Err("top_m must be <= fft_size".to_string());
        }

        let available = default_branch_set().len();
        if self.branching_factor > available {
            return Err(format!(
                "branching_factor={}: exceeds available branch operators ({available})",
                self.branching_factor
            ));
        }

        Ok(())
    }
}

impl Default for PcpriConfig {
    fn default() -> Self {
        Self {
            n: 256,
            branching_factor: 3,
            alpha: 0.1,
            fft_size: 256,
            hop_size: 128,
            coherency_window: 10,
            top_m: 8,
        }
    }
}

/// Main PCPRI generator
pub struct PcpriGenerator {
    config: PcpriConfig,
    /// Current state
    state: State,
    /// Conductor for accumulation
    conductor: Conductor,
    /// Phase analyzer
    phase_analyzer: PhaseAnalyzer,
    /// Branch operators
    branch_ops: Vec<BranchOp>,
    /// Last FFT bin features (only present if the last step had enough samples).
    last_features: Option<Vec<BinFeature>>,
    /// Signal history for phase analysis (circular buffer, max fft_size elements)
    signal_history: VecDeque<f64>,
    /// Current time step
    t: usize,
    /// Accumulated branch entropy (combined with state for branch selection)
    branch_entropy: u64,
    /// Last feedback word used to update state
    last_feedback: u64,
    /// Whether phase analysis ran on the last step (i.e., we had enough samples)
    last_has_fft: bool,
    /// The branch index that was used for the last state update
    last_branch_used: usize,
}

impl PcpriGenerator {
    /// Create a new PCPRI generator with given configuration
    pub fn new(config: PcpriConfig) -> Self {
        if let Err(e) = config.validate() {
            panic!("Invalid PcpriConfig: {e}");
        }
        let state = vec![false; config.n];
        let conductor = Conductor::new(config.alpha);
        let phase_analyzer =
            PhaseAnalyzer::new(config.fft_size, config.hop_size, config.coherency_window);
        let all_branch_ops = default_branch_set();
        let mut branch_ops: Vec<BranchOp> = Vec::new();
        let mut i: usize = 0;
        while i < config.branching_factor {
            branch_ops.push(all_branch_ops[i]);
            i += 1;
        }

        let fft_size = config.fft_size;
        Self {
            config,
            state,
            conductor,
            phase_analyzer,
            branch_ops,
            last_features: None,
            signal_history: VecDeque::with_capacity(fft_size),
            t: 0,
            branch_entropy: 0,
            last_feedback: 0,
            last_has_fft: false,
            last_branch_used: 0,
        }
    }

    /// Advance the generator by one step.
    ///
    /// Identical evolution to the original's `next_token()`, minus the
    /// token-string encoding: after calling this, read the step's signals
    /// via `conductor_value()`, `last_feedback()`, and `packed_state_bytes()`.
    pub fn step(&mut self) {
        // 1. Compute bit-balance emission
        let y = bit_balance(&self.state);

        // 2. Update conductor
        self.conductor.update(y);
        let c = self.conductor.value();

        // 3. Accumulate signal history (circular buffer - keeps only fft_size samples)
        self.signal_history.push_back(c);
        while self.signal_history.len() > self.config.fft_size {
            self.signal_history.pop_front();
        }

        // 4. Perform phase analysis when we have enough samples
        let (feedback, has_fft) = if self.signal_history.len() >= self.config.fft_size {
            // Extract frame as contiguous slice
            let frame: Vec<f64> = self.signal_history.iter().copied().collect();

            // Process frame
            let features = self.phase_analyzer.process_frame(&frame);
            let feedback = build_feedback(&features, self.config.top_m);
            self.last_features = Some(features);
            (feedback, true)
        } else {
            self.last_features = None;
            (0u64, false)
        };
        self.last_feedback = feedback;
        self.last_has_fft = has_fft;

        // 5. Update state using branch operator selected by state-derived entropy
        // Hash current state bits to get entropy for branch selection
        let state_hash = self.hash_state();
        // Combine state hash with feedback, time, and accumulated entropy
        // Include time_step to ensure variation even when state is static
        let combined = state_hash
            ^ feedback
            ^ self.branch_entropy
            ^ (self.t as u64).wrapping_mul(0x517cc1b727220a95); // Mix in time
                                                                // Use bit mixing to improve uniformity before modulo
        let mixed = combined.wrapping_mul(0x9e3779b97f4a7c15); // Golden ratio constant
        let used_branch = ((mixed >> 32) as usize) % self.branch_ops.len();
        let branch_op = self.branch_ops[used_branch];
        self.state = branch_op(&self.state, feedback);
        self.last_branch_used = used_branch;
        // Update entropy accumulator for next step (mix in current selection)
        self.branch_entropy = self
            .branch_entropy
            .wrapping_mul(6364136223846793005)
            .wrapping_add(combined);

        self.t += 1;
    }

    /// Returns the last FFT bin features if the last step performed phase analysis.
    #[allow(dead_code)] // inspection API from pcpri; used by fidelity checks, not digit extraction
    pub fn last_features(&self) -> Option<&[BinFeature]> {
        match &self.last_features {
            Some(v) => Some(v.as_slice()),
            None => None,
        }
    }

    /// Returns the last feedback word used to update state.
    pub fn last_feedback(&self) -> u64 {
        self.last_feedback
    }

    /// Returns whether the last step performed phase analysis (had enough samples).
    #[allow(dead_code)] // inspection API from pcpri; used by tests/fidelity checks, not digit extraction
    pub fn last_has_fft(&self) -> bool {
        self.last_has_fft
    }

    /// Returns the branch index used for the last state update.
    #[allow(dead_code)] // inspection API from pcpri; used by fidelity checks, not digit extraction
    pub fn last_branch_used(&self) -> usize {
        self.last_branch_used
    }

    /// Hash the current state into a u64 for branch selection.
    /// Uses a simple polynomial hash over state bytes.
    fn hash_state(&self) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325; // FNV-1a offset basis
        for chunk in self.state.chunks(8) {
            let mut byte: u8 = 0;
            for (i, bit) in chunk.iter().enumerate() {
                if *bit {
                    byte |= 1 << (7 - i);
                }
            }
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3); // FNV-1a prime
        }
        hash
    }

    /// Pack the current state bits into bytes (MSB-first in each byte).
    pub fn packed_state_bytes(&self) -> Vec<u8> {
        let state = &self.state;
        let mut bytes: Vec<u8> = Vec::new();
        for chunk in state.chunks(8) {
            let mut byte: u8 = 0;
            let mut i: usize = 0;
            while i < chunk.len() {
                if chunk[i] {
                    byte |= 1 << (7 - i);
                }
                i += 1;
            }
            bytes.push(byte);
        }
        bytes
    }

    /// Get current conductor value
    pub fn conductor_value(&self) -> f64 {
        self.conductor.value()
    }

    /// Get current time step
    #[allow(dead_code)] // inspection API from pcpri; used by fidelity checks, not digit extraction
    pub fn time_step(&self) -> usize {
        self.t
    }
}

// ============================================================================
// Base-12 digit streams (Data Walker extraction layer - new code)
// ============================================================================

/// Conductor stream: one digit per generator step, c in [-1, 1] mapped
/// linearly to 0-11. Emitted from step 0 (the initial ramp is the signal).
pub fn conductor_base12(length: usize) -> Vec<u8> {
    let mut gen = PcpriGenerator::new(PcpriConfig::default());
    let mut digits = Vec::with_capacity(length);
    for _ in 0..length {
        gen.step();
        let c = gen.conductor_value();
        let digit = ((c + 1.0) / 2.0 * 11.99).floor() as i32;
        digits.push(digit.clamp(0, 11) as u8);
    }
    digits
}

/// Feedback stream: base-12 digits of each step's feedback word z,
/// least-significant digit first. The first `fft_size` warm-up steps
/// (feedback identically 0, phase analysis inactive) are discarded.
pub fn feedback_base12(length: usize) -> Vec<u8> {
    let config = PcpriConfig::default();
    let warmup = config.fft_size;
    let mut gen = PcpriGenerator::new(config);
    for _ in 0..warmup {
        gen.step();
    }

    let mut digits = Vec::with_capacity(length);
    while digits.len() < length {
        gen.step();
        let mut z = gen.last_feedback();
        if z == 0 {
            digits.push(0);
        } else {
            while z > 0 {
                digits.push((z % 12) as u8);
                z /= 12;
            }
        }
    }
    digits.truncate(length);
    digits
}

/// State stream: exact base-12 expansion of the packed state bytes
/// (three digits per byte: b % 12, (b / 12) % 12, b / 144).
/// The first `fft_size` warm-up steps are discarded: with zero feedback
/// the branch operators leave the all-zero initial state unchanged, so
/// warm-up state carries no PCPRI structure.
pub fn state_base12(length: usize) -> Vec<u8> {
    let config = PcpriConfig::default();
    let warmup = config.fft_size;
    let mut gen = PcpriGenerator::new(config);
    for _ in 0..warmup {
        gen.step();
    }

    let mut digits = Vec::with_capacity(length);
    while digits.len() < length {
        gen.step();
        for b in gen.packed_state_bytes() {
            digits.push(b % 12);
            digits.push((b / 12) % 12);
            digits.push(b / 144);
        }
    }
    digits.truncate(length);
    digits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bit_balance() {
        assert_eq!(bit_balance(&vec![false; 8]), -1.0);
        assert_eq!(bit_balance(&vec![true; 8]), 1.0);
        assert_eq!(bit_balance(&vec![true, false, true, false]), 0.0);
    }

    #[test]
    fn test_conductor_update() {
        let mut cond = Conductor::new(0.5);
        cond.update(1.0);
        assert_eq!(cond.value(), 0.5);
        cond.update(1.0);
        assert_eq!(cond.value(), 0.75);
    }

    #[test]
    fn test_generator_deterministic() {
        let mut a = PcpriGenerator::new(PcpriConfig::default());
        let mut b = PcpriGenerator::new(PcpriConfig::default());
        for _ in 0..600 {
            a.step();
            b.step();
            assert_eq!(a.conductor_value(), b.conductor_value());
            assert_eq!(a.last_feedback(), b.last_feedback());
            assert_eq!(a.packed_state_bytes(), b.packed_state_bytes());
        }
    }

    #[test]
    fn test_feedback_zero_during_warmup_then_active() {
        let mut gen = PcpriGenerator::new(PcpriConfig::default());
        for _ in 0..255 {
            gen.step();
            assert!(!gen.last_has_fft());
            assert_eq!(gen.last_feedback(), 0);
        }
        gen.step();
        assert!(gen.last_has_fft());
    }

    #[test]
    fn test_conductor_digits() {
        let digits = conductor_base12(2000);
        assert_eq!(digits.len(), 2000);
        assert!(digits.iter().all(|&d| d < 12));
        let unique: std::collections::HashSet<_> = digits.iter().collect();
        assert!(unique.len() > 3, "conductor digits should vary");
    }

    #[test]
    fn test_feedback_digits_not_all_zero() {
        let digits = feedback_base12(2000);
        assert_eq!(digits.len(), 2000);
        assert!(digits.iter().all(|&d| d < 12));
        assert!(
            digits.iter().any(|&d| d != 0),
            "feedback digits after warm-up should not be all zero"
        );
    }

    #[test]
    fn test_state_digits_not_all_zero() {
        let digits = state_base12(2000);
        assert_eq!(digits.len(), 2000);
        assert!(digits.iter().all(|&d| d < 12));
        assert!(
            digits.iter().any(|&d| d != 0),
            "state digits after warm-up should not be all zero"
        );
    }

    #[test]
    fn test_validate_rejects_non_finite_alpha() {
        // Regression: NaN comparisons are always false, so a bare
        // `alpha <= 0.0 || alpha > 1.0` check accepted NaN, and
        // PcpriGenerator::new() then panicked in Conductor::new().
        for alpha in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let config = PcpriConfig {
                alpha,
                ..PcpriConfig::default()
            };
            assert!(
                config.validate().is_err(),
                "alpha={alpha} must be rejected by validate()"
            );
        }
    }

    #[test]
    fn test_validate_alpha_bounds() {
        for alpha in [0.0, -0.5, 1.5] {
            let config = PcpriConfig {
                alpha,
                ..PcpriConfig::default()
            };
            assert!(config.validate().is_err(), "alpha={alpha} must be rejected");
        }
        for alpha in [1.0, 0.1] {
            let config = PcpriConfig {
                alpha,
                ..PcpriConfig::default()
            };
            assert!(config.validate().is_ok(), "alpha={alpha} must be accepted");
        }
    }
}
