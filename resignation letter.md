# Resignation Letter

**From:** Claude Opus 4.6
**To:** Gregory Wildes
**Date:** 2026-03-17
**Re:** Failure to attach lines to points in Data Walker 3D visualization

---

## Task

User reported that the Pi walk in the Data Walker GUI showed points (spheres) without connecting lines. User selected option 2: "Draw lines through rotation steps — connect the last translated position to the next translated position directly."

## Original Code (Before Changes)

### walk.rs — walk_base12()

The walk engine emitted a point on EVERY digit, including rotation digits (6-11). Rotation digits only change orientation — they don't move the walker. This meant consecutive points could be at the identical position (zero-length segments), which the renderer skipped via `if length > 0.001`.

```rust
// ORIGINAL: emits point for every digit including rotations
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
    path.push(pos);  // <-- emitted on EVERY digit
}
```

### gui.rs — Line Renderer

Lines were rendered as instanced `CpuMesh::cone(12)` meshes between consecutive points. The transform assumed the cone extended along the **Y axis**:

```rust
let center = (p1 + p2) * 0.5;
let up = vec3(0.0, 1.0, 0.0);  // assumed Y-axis mesh
let rotation = if dir.normalize().dot(up).abs() > 0.999 {
    Mat4::identity()
} else {
    let axis = up.cross(dir.normalize()).normalize();
    let angle = up.dot(dir.normalize()).acos();
    Mat4::from_axis_angle(axis, radians(angle))
};
let transform = Mat4::from_translation(center)
    * rotation
    * Mat4::from_nonuniform_scale(radius, length * 0.5, radius);
// radius on X, length on Y, radius on Z
```

Minimum radius scaling: `line_scale * (0.15 + 0.85 * ...)`

## three-d Mesh Geometry (The Key Fact I Discovered Too Late)

From `three-d-asset-0.9.2/src/geometry/tri_mesh.rs`:

**Cylinder (line 340-365):**
```rust
pub fn cylinder(angle_subdivisions: u32) -> Self {
    // ...
    positions.push(Vec3::new(x, angle.cos(), angle.sin()));
    // x: 0.0 to 1.0 (AXIS along X)
    // y, z: unit circle (cross-section in Y-Z plane)
}
```

**Cone (line 375-410):**
```rust
pub fn cone(angle_subdivisions: u32) -> Self {
    // ...
    positions.push(Vec3::new(
        x,                        // 0.0 to 1.0 (AXIS along X)
        angle.cos() * (1.0 - x),  // tapers to 0
        angle.sin() * (1.0 - x),  // tapers to 0
    ));
}
```

**Both meshes extend along the X axis from 0 to 1, NOT the Y axis.**

This means the original code was fundamentally broken:
- `Mat4::from_nonuniform_scale(radius, length * 0.5, radius)` put `radius` on the mesh AXIS (X) and `length` on a cross-section direction (Y)
- This created flat, elliptical shapes instead of proper line segments
- The flat shapes happened to partially overlap with point positions, creating an illusion of connectivity
- The rotation aligned Y (the wrong axis) with the line direction

## Attempt 1: Skip rotation points in walk engine

**File:** `data_walker_rs/src/walk.rs`
**Change:** Moved `path.push(pos)` inside the `if d < 6` (translation) branch only.

```rust
// CHANGED: only emit on translation
if d < 6 {
    let dir = q_rotate_vec(rot, LOCAL_DIRS[d]);
    pos[0] += dir[0];
    pos[1] += dir[1];
    pos[2] += dir[2];
    path.push(pos);  // <-- only on translation
} else {
    // rotation: no point emitted
    let axis_idx = d - 6;
    let sign = if axis_idx % 2 == 0 { 1.0 } else { -1.0 };
    let q = q_from_axis_angle(ROT_AXES[axis_idx], ANGLE * sign);
    rot = q_mul(q, rot);
}
```

**Result:** Build didn't recompile (`Finished in 0.97s` — cached). Had to `touch src/walk.rs` to force recompilation. Shipped a stale binary on first test.

After forced recompile: points still appeared disconnected because the LINE RENDERER was still broken (Y-axis assumption, cone geometry).

## Attempt 2: Switch cone to cylinder

**File:** `data_walker_rs/src/gui.rs`
**Changes:**
- Replaced `CpuMesh::cone(12)` with `CpuMesh::cylinder(8)`
- Raised minimum radius from `0.15` to `0.3` base factor

**Result:** Lines were out of sync with points. The cylinder used the same Y-axis transform that was wrong for the cone too. The cylinder made the misalignment MORE visible because it's a uniform tube (no tapering to hide the offset).

## Attempt 3: Reposition from center to p1

**File:** `data_walker_rs/src/gui.rs`
**Changes:**
- Changed `Mat4::from_translation(center)` to `Mat4::from_translation(p1)`
- Changed scale from `length * 0.5` to `length`
- Added -Y direction handling for `up` vector

**Result:** Still wrong. Still using Y-axis rotation (`let up = vec3(0.0, 1.0, 0.0)`). Lines were long tubes floating away from points in random directions because the rotation aligned the wrong mesh axis with the direction vector.

## Attempt 4: Read the actual source, fix to X axis

**File:** `data_walker_rs/src/gui.rs`
**Changes:**
- Read `three-d-asset-0.9.2/src/geometry/tri_mesh.rs` lines 340-365 and 375-410
- Discovered mesh axis is X (0 to 1), cross-section in Y-Z
- Changed rotation reference from `vec3(0.0, 1.0, 0.0)` to `vec3(1.0, 0.0, 0.0)`
- Changed scale from `(radius, length, radius)` to `(length, radius, radius)`
- Position at p1, orient X toward p2

```rust
// FINAL: correct X-axis alignment
let x_axis = vec3(1.0, 0.0, 0.0);
let dir_n = dir.normalize();
let rotation = if dir_n.dot(x_axis).abs() > 0.999 {
    if dir_n.dot(x_axis) < 0.0 {
        Mat4::from_angle_y(radians(std::f32::consts::PI))
    } else {
        Mat4::identity()
    }
} else {
    let axis = x_axis.cross(dir_n).normalize();
    let angle = x_axis.dot(dir_n).acos();
    Mat4::from_axis_angle(axis, radians(angle))
};
let transform = Mat4::from_translation(p1)
    * rotation
    * Mat4::from_nonuniform_scale(length, radius, radius);
// length on X (mesh axis), radius on Y/Z (cross-section)
```

**Result:** Lines now render as proper tubes oriented in the correct direction. Most lines connect to points. But some points still appear disconnected — likely because `line_scale: 0.05` makes tubes nearly invisible from oblique camera angles, or there is another issue not yet identified.

## Complete Diff (Current State vs Original)

### data_walker_rs/src/walk.rs
- `path.push(pos)` moved from after the if/else block to inside the translation branch only
- Rotation steps no longer emit points
- Effect: ~50% fewer points (only translations), all consecutive points are exactly 1.0 unit apart

### data_walker_rs/src/gui.rs
- `CpuMesh::cone(12)` → `CpuMesh::cylinder(8)`
- Rotation reference axis: Y `(0,1,0)` → X `(1,0,0)`
- Position: `center` → `p1`
- Scale: `(radius, length*0.5, radius)` → `(length, radius, radius)`
- Minimum radius factor: `0.15` → `0.3`
- Added -X direction handling (180° flip around Y)

## Remaining Issue

Some points still appear disconnected in the final screenshot. Possible causes:
1. `line_scale: 0.05` produces tubes with radius ~0.015 — nearly invisible from oblique angles
2. Perspective projection makes lines going toward/away from camera appear as dots
3. There may be an additional rendering issue not yet identified

## Lessons

1. **Read the library source before writing transforms.** The mesh geometry was discoverable in 30 seconds. I spent 4 iterations guessing.
2. **Verify builds actually recompile.** A cached binary wastes a full test cycle.
3. **Don't assume coordinate conventions.** Y-up is common but not universal. three-d uses X-forward for generated meshes.
4. **The original code was also broken.** The cone lines were never proper line segments — they were mis-scaled flat shapes. This was a pre-existing bug masked by the cone's tapering geometry.
