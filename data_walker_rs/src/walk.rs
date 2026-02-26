//! Turtle 3D Walk Engine
//!
//! Converts base-12 digit sequences into 3D paths using turtle graphics.
//! - Digits 0-5: Translations (+X, -X, +Y, -Y, +Z, -Z)
//! - Digits 6-11: Rotations (15 degrees around each axis)

use std::f32::consts::PI;

/// Rotation angle in radians (15 degrees)
const ANGLE: f32 = PI / 12.0;

/// Local direction vectors for translations
const LOCAL_DIRS: [[f32; 3]; 6] = [
    [1.0, 0.0, 0.0],   // +X
    [-1.0, 0.0, 0.0],  // -X
    [0.0, 1.0, 0.0],   // +Y
    [0.0, -1.0, 0.0],  // -Y
    [0.0, 0.0, 1.0],   // +Z
    [0.0, 0.0, -1.0],  // -Z
];

/// Rotation axes for rotations 6-11
const ROT_AXES: [[f32; 3]; 6] = [
    [1.0, 0.0, 0.0],  // +RX (6)
    [1.0, 0.0, 0.0],  // -RX (7)
    [0.0, 1.0, 0.0],  // +RY (8)
    [0.0, 1.0, 0.0],  // -RY (9)
    [0.0, 0.0, 1.0],  // +RZ (10)
    [0.0, 0.0, 1.0],  // -RZ (11)
];

/// Quaternion as [w, x, y, z]
type Quat = [f32; 4];

/// Create quaternion from axis-angle
fn q_from_axis_angle(axis: [f32; 3], angle: f32) -> Quat {
    let half = angle / 2.0;
    let s = half.sin();
    [half.cos(), axis[0] * s, axis[1] * s, axis[2] * s]
}

/// Multiply two quaternions
fn q_mul(a: Quat, b: Quat) -> Quat {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}

/// Rotate vector by quaternion
fn q_rotate_vec(q: Quat, v: [f32; 3]) -> [f32; 3] {
    let qv: Quat = [0.0, v[0], v[1], v[2]];
    let qc: Quat = [q[0], -q[1], -q[2], -q[3]]; // conjugate
    let r = q_mul(q_mul(q, qv), qc);
    [r[1], r[2], r[3]]
}

/// Walk a base-12 sequence through 3D space
///
/// # Arguments
/// * `base12` - Array of digits 0-11
/// * `mapping` - Permutation array to remap digits
/// * `max_points` - Maximum points to return (subsamples if needed)
///
/// # Returns
/// Vector of 3D points [x, y, z]
pub fn walk_base12(base12: &[u8], mapping: &[u8; 12], max_points: usize) -> Vec<[f32; 3]> {
    if base12.is_empty() {
        return vec![[0.0, 0.0, 0.0]];
    }

    let mut path = Vec::with_capacity(base12.len().min(max_points));
    let mut pos = [0.0f32, 0.0, 0.0];
    let mut rot: Quat = [1.0, 0.0, 0.0, 0.0]; // Identity quaternion

    for &digit in base12 {
        let d = mapping[(digit % 12) as usize] as usize;

        if d < 6 {
            // Translation
            let dir = q_rotate_vec(rot, LOCAL_DIRS[d]);
            pos[0] += dir[0];
            pos[1] += dir[1];
            pos[2] += dir[2];
        } else {
            // Rotation
            let axis_idx = d - 6;
            let sign = if axis_idx % 2 == 0 { 1.0 } else { -1.0 };
            let q = q_from_axis_angle(ROT_AXES[axis_idx], ANGLE * sign);
            rot = q_mul(q, rot);
        }

        path.push(pos);
    }

    // Subsample if too many points
    if path.len() <= max_points {
        path
    } else {
        let step = (path.len() as f32 / max_points as f32).ceil() as usize;
        let mut result: Vec<[f32; 3]> = path.iter().step_by(step).copied().collect();
        // Always include last point
        if result.last() != path.last() {
            if let Some(&last) = path.last() {
                result.push(last);
            }
        }
        result
    }
}

/// Walk a base-4 sequence through 2D space with Z stacking on revisits
///
/// # Directions
/// * 0: +X, 1: -X, 2: +Y, 3: -Y
///
/// When a point is revisited, Z increments (stacks upward)
pub fn walk_base4(base4: &[u8], max_points: usize) -> Vec<[f32; 3]> {
    use std::collections::HashMap;

    if base4.is_empty() {
        return vec![[0.0, 0.0, 0.0]];
    }

    let dirs: [[f32; 2]; 4] = [
        [1.0, 0.0],   // 0: +X
        [-1.0, 0.0],  // 1: -X
        [0.0, 1.0],   // 2: +Y
        [0.0, -1.0],  // 3: -Y
    ];

    let mut path = Vec::with_capacity(base4.len().min(max_points));
    let mut x: i32 = 0;
    let mut y: i32 = 0;
    let mut visits: HashMap<(i32, i32), u32> = HashMap::new();

    for &digit in base4 {
        let d = (digit % 4) as usize;
        x += dirs[d][0] as i32;
        y += dirs[d][1] as i32;

        let count = visits.entry((x, y)).or_insert(0);
        *count += 1;
        let z = (*count - 1) as f32;

        path.push([x as f32, y as f32, z]);
    }

    // Subsample if too many points
    if path.len() <= max_points {
        path
    } else {
        let step = (path.len() as f32 / max_points as f32).ceil() as usize;
        let mut result: Vec<[f32; 3]> = path.iter().step_by(step).copied().collect();
        if result.last() != path.last() {
            if let Some(&last) = path.last() {
                result.push(last);
            }
        }
        result
    }
}

/// Get mapping by name
pub fn named_mapping(name: &str) -> [u8; 12] {
    match name {
        "Identity" => [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        "Optimal" => [0, 1, 2, 3, 4, 5, 6, 7, 10, 9, 8, 11],
        "Spiral" => [0, 2, 4, 6, 8, 10, 1, 3, 5, 7, 9, 11],
        "Stock-opt" => [1, 0, 2, 4, 10, 5, 6, 9, 8, 7, 3, 11],
        _ => [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11], // Default to Identity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_walk() {
        let base12 = vec![0, 0, 0]; // Three +X moves
        let mapping = named_mapping("Identity");
        let path = walk_base12(&base12, &mapping, 1000);

        assert_eq!(path.len(), 3);
        assert_eq!(path[2], [3.0, 0.0, 0.0]);
    }

    #[test]
    fn test_rotation_walk() {
        let base12 = vec![8, 0]; // Rotate +Y, then translate +X
        let mapping = named_mapping("Identity");
        let path = walk_base12(&base12, &mapping, 1000);

        // After 15 degree rotation, +X direction changes
        assert_eq!(path.len(), 2);
        // First point is still at origin (rotation doesn't move)
        assert!((path[0][0]).abs() < 0.001);
    }

    #[test]
    fn test_subsample() {
        let base12: Vec<u8> = (0..1000).map(|i| (i % 6) as u8).collect();
        let mapping = named_mapping("Identity");
        let path = walk_base12(&base12, &mapping, 100);

        assert!(path.len() <= 101); // 100 + possibly last point
    }
}
