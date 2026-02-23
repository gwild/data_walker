//! L-System Fractals - Deterministic string rewriting to base-12
//!
//! Generates fractal patterns using Lindenmayer systems,
//! then converts to turtle walk commands.

/// Dragon Curve (Heighway Dragon)
/// Axiom: F, Rules: F → F+G, G → F-G, Angle: 90°
pub fn dragon_curve(iterations: u32) -> Vec<u8> {
    let mut s = String::from("F");

    for _ in 0..iterations {
        let mut next = String::with_capacity(s.len() * 2);
        for c in s.chars() {
            match c {
                'F' => next.push_str("F+G"),
                'G' => next.push_str("F-G"),
                _ => next.push(c),
            }
        }
        s = next;
    }

    lsystem_to_base12(&s, 90)
}

/// Koch Snowflake
/// Axiom: F--F--F, Rules: F → F+F--F+F, Angle: 60°
pub fn koch_snowflake(iterations: u32) -> Vec<u8> {
    let mut s = String::from("F--F--F");

    for _ in 0..iterations {
        let mut next = String::with_capacity(s.len() * 4);
        for c in s.chars() {
            match c {
                'F' => next.push_str("F+F--F+F"),
                _ => next.push(c),
            }
        }
        s = next;
    }

    lsystem_to_base12(&s, 60)
}

/// Sierpinski Arrowhead Curve
/// Axiom: F, Rules: F → G-F-G, G → F+G+F, Angle: 60°
pub fn sierpinski_arrowhead(iterations: u32) -> Vec<u8> {
    let mut s = String::from("F");

    for _ in 0..iterations {
        let mut next = String::with_capacity(s.len() * 3);
        for c in s.chars() {
            match c {
                'F' => next.push_str("G-F-G"),
                'G' => next.push_str("F+G+F"),
                _ => next.push(c),
            }
        }
        s = next;
    }

    lsystem_to_base12(&s, 60)
}

/// Hilbert Curve
/// Axiom: A, Rules: A → -BF+AFA+FB-, B → +AF-BFB-FA+, Angle: 90°
pub fn hilbert_curve(iterations: u32) -> Vec<u8> {
    let mut s = String::from("A");

    for _ in 0..iterations {
        let mut next = String::with_capacity(s.len() * 8);
        for c in s.chars() {
            match c {
                'A' => next.push_str("-BF+AFA+FB-"),
                'B' => next.push_str("+AF-BFB-FA+"),
                _ => next.push(c),
            }
        }
        s = next;
    }

    lsystem_to_base12(&s, 90)
}

/// Peano Curve
/// Axiom: F, Rules: F → F+F-F-F-F+F+F+F-F, Angle: 90°
pub fn peano_curve(iterations: u32) -> Vec<u8> {
    let mut s = String::from("F");

    for _ in 0..iterations {
        let mut next = String::with_capacity(s.len() * 10);
        for c in s.chars() {
            match c {
                'F' => next.push_str("F+F-F-F-F+F+F+F-F"),
                _ => next.push(c),
            }
        }
        s = next;
    }

    lsystem_to_base12(&s, 90)
}

/// Gosper Curve (Flowsnake)
/// Axiom: A, Rules: A → A-B--B+A++AA+B-, B → +A-BB--B-A++A+B
pub fn gosper_curve(iterations: u32) -> Vec<u8> {
    let mut s = String::from("A");

    for _ in 0..iterations {
        let mut next = String::with_capacity(s.len() * 8);
        for c in s.chars() {
            match c {
                'A' => next.push_str("A-B--B+A++AA+B-"),
                'B' => next.push_str("+A-BB--B-A++A+B"),
                _ => next.push(c),
            }
        }
        s = next;
    }

    lsystem_to_base12(&s, 60)
}

/// Convert L-system string to base-12 walk sequence
fn lsystem_to_base12(s: &str, angle_degrees: u32) -> Vec<u8> {
    // How many 15° rotations per turn
    let n_rot = (angle_degrees / 15).max(1) as usize;

    let mut result = Vec::with_capacity(s.len());

    for c in s.chars() {
        match c {
            'F' | 'G' | 'A' | 'B' => {
                // Forward movement: translate +X
                result.push(0);
            }
            '+' => {
                // Turn right: rotate +Z (digit 10)
                for _ in 0..n_rot {
                    result.push(10);
                }
            }
            '-' => {
                // Turn left: rotate -Z (digit 11)
                for _ in 0..n_rot {
                    result.push(11);
                }
            }
            _ => {} // Ignore other characters
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dragon_curve_growth() {
        let d1 = dragon_curve(1);
        let d2 = dragon_curve(2);
        assert!(d2.len() > d1.len());
    }

    #[test]
    fn test_all_digits_valid() {
        let dragon = dragon_curve(10);
        assert!(dragon.iter().all(|&d| d < 12));

        let koch = koch_snowflake(4);
        assert!(koch.iter().all(|&d| d < 12));
    }

    #[test]
    fn test_koch_starts_with_forward() {
        let koch = koch_snowflake(1);
        assert_eq!(koch[0], 0); // First move is forward
    }
}
