//! Mathematical Constants - true on-demand base-12 digit generation
//!
//! Computes base-12 digits from real arbitrary-precision values using
//! `astro-float` rather than precomputed tables or synthetic extension logic.

use astro_float::{BigFloat, Consts, RoundingMode};

const LOG2_12: f64 = 3.584_962_500_721_156_5;
const EXTRA_GUARD_BITS: usize = 128;
const BASE12_U8: u8 = 12;

/// Pi in base-12.
pub fn pi_base12(n_digits: usize) -> Vec<u8> {
    generate_digits(n_digits, |precision, consts| {
        consts.pi(precision, RoundingMode::ToEven)
    })
}

/// e (Euler's number) in base-12.
pub fn e_base12(n_digits: usize) -> Vec<u8> {
    generate_digits(n_digits, |precision, consts| {
        consts.e(precision, RoundingMode::ToEven)
    })
}

/// sqrt(2) in base-12.
pub fn sqrt2_base12(n_digits: usize) -> Vec<u8> {
    generate_digits(n_digits, |precision, _consts| {
        BigFloat::from_u8(2, precision).sqrt(precision, RoundingMode::ToEven)
    })
}

/// Golden ratio phi in base-12.
pub fn phi_base12(n_digits: usize) -> Vec<u8> {
    generate_digits(n_digits, |precision, _consts| {
        let one = BigFloat::from_u8(1, precision);
        let two = BigFloat::from_u8(2, precision);
        let sqrt5 = BigFloat::from_u8(5, precision).sqrt(precision, RoundingMode::None);
        let numerator = one.add(&sqrt5, precision, RoundingMode::None);
        numerator.div(&two, precision, RoundingMode::ToEven)
    })
}

/// Natural log of 2 in base-12.
pub fn ln2_base12(n_digits: usize) -> Vec<u8> {
    generate_digits(n_digits, |precision, consts| {
        consts.ln_2(precision, RoundingMode::ToEven)
    })
}

fn generate_digits<F>(n_digits: usize, build_value: F) -> Vec<u8>
where
    F: FnOnce(usize, &mut Consts) -> BigFloat,
{
    if n_digits == 0 {
        return vec![];
    }

    let precision = precision_bits(n_digits);
    let mut consts = Consts::new().expect("constants cache should initialize");
    let value = build_value(precision, &mut consts);
    digits_from_positive_bigfloat(&value, n_digits, precision)
}

fn precision_bits(n_digits: usize) -> usize {
    (((n_digits.max(1)) as f64 * LOG2_12).ceil() as usize) + EXTRA_GUARD_BITS
}

fn digits_from_positive_bigfloat(value: &BigFloat, n_digits: usize, precision: usize) -> Vec<u8> {
    let mut result = Vec::with_capacity(n_digits);
    result.push(small_bigfloat_to_digit(&value.int(), precision));

    let twelve = BigFloat::from_u8(BASE12_U8, precision);
    let mut fractional = value.fract();
    for _ in 1..n_digits {
        let shifted = fractional.mul(&twelve, precision, RoundingMode::None);
        let digit_part = shifted.int();
        let digit = small_bigfloat_to_digit(&digit_part, precision);
        result.push(digit);

        let digit_value = BigFloat::from_u8(digit, precision);
        fractional = shifted.sub(&digit_value, precision, RoundingMode::None);
    }

    result
}

fn small_bigfloat_to_digit(value: &BigFloat, precision: usize) -> u8 {
    for digit in (0..BASE12_U8).rev() {
        let candidate = BigFloat::from_u8(digit, precision);
        if matches!(value.cmp(&candidate), Some(ordering) if ordering >= 0) {
            return digit;
        }
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pi_starts_with_3() {
        let pi = pi_base12(10);
        assert_eq!(pi[0], 3);
        assert_eq!(&pi[..6], &[3, 1, 8, 4, 8, 0]);
    }

    #[test]
    fn test_e_starts_with_2() {
        let e = e_base12(10);
        assert_eq!(e[0], 2);
        assert_eq!(&e[..6], &[2, 8, 7, 5, 2, 3]);
    }

    #[test]
    fn test_all_digits_valid() {
        let pi = pi_base12(1000);
        assert!(pi.iter().all(|&d| d < 12));
    }

    #[test]
    fn test_constant_prefixes_are_stable() {
        let pi_200 = pi_base12(200);
        let pi_220 = pi_base12(220);
        assert_eq!(pi_200, pi_220[..200]);

        let phi_200 = phi_base12(200);
        let phi_220 = phi_base12(220);
        assert_eq!(phi_200, phi_220[..200]);

        let ln2_200 = ln2_base12(200);
        let ln2_220 = ln2_base12(220);
        assert_eq!(ln2_200, ln2_220[..200]);
    }
}
