#!/usr/bin/env python3
"""
Generate mathematical walk data for the gallery.

ALL data is deterministically computed from mathematical definitions.
No random number generation. No np.random. No fake data.

Generates:
  - visualizations/data/pi_walk_data.js      (Constants: pi, e, sqrt2, phi, ln2)
  - visualizations/data/fractal_walk_data.js  (L-system fractals, Fibonacci word, Thue-Morse)
  - visualizations/data/mandelbrot_walk_data.js (Mandelbrot/Julia orbits, logistic map)
"""

import sys
sys.stdout.reconfigure(line_buffering=True)

import numpy as np
import json
import os
from datetime import datetime
from scipy.spatial.transform import Rotation

print("=" * 70)
print("MATHEMATICAL WALK DATA — ALL COMPUTED, NO RANDOM")
print("=" * 70)
print(f"Started: {datetime.now()}\n")

DATA_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'visualizations', 'data')
os.makedirs(DATA_DIR, exist_ok=True)

IDENTITY_MAPPING = list(range(12))


class Turtle3D:
    """3D turtle walker — same as all PCPRI generators."""
    def __init__(self):
        self.position = np.array([0.0, 0.0, 0.0])
        self.rotation = Rotation.identity()
        self.path = [self.position.copy()]

    def move(self, direction, mapping):
        d = mapping[direction % 12]
        if d < 6:
            local_dirs = np.array([
                [1,0,0], [-1,0,0], [0,1,0], [0,-1,0], [0,0,1], [0,0,-1],
            ])
            world_dir = self.rotation.apply(local_dirs[d])
            self.position = self.position + world_dir
        else:
            axes = [[1,0,0], [-1,0,0], [0,1,0], [0,-1,0], [0,0,1], [0,0,-1]]
            delta = Rotation.from_rotvec(np.array(axes[d - 6]) * np.radians(15))
            self.rotation = delta * self.rotation
        self.path.append(self.position.copy())


def compute_walk(base12_seq, mapping=None, max_points=5000):
    """Run turtle walk and return walk dict with points + base12."""
    if mapping is None:
        mapping = IDENTITY_MAPPING
    step = max(1, len(base12_seq) // max_points)
    seq = base12_seq[::step]
    t = Turtle3D()
    for d in seq:
        t.move(d, mapping)
    path = np.array(t.path)
    return {
        'points': path.tolist(),
        'base12': [int(x) for x in base12_seq],
    }


# ============================================================
# PART 1: MATHEMATICAL CONSTANTS (pi, e, sqrt2, phi, ln2)
# ============================================================

def constant_to_base12(value, n_digits=2000):
    """Convert an mpmath value to its base-12 digit representation.

    This computes the ACTUAL base-12 expansion of the constant.
    Pi in base 12 starts: 3.184809493B918...
    """
    from mpmath import floor
    digits = []
    int_val = int(floor(value))
    frac = value - int_val

    # Integer part → base 12
    if int_val == 0:
        digits.append(0)
    else:
        int_digits = []
        while int_val > 0:
            int_digits.append(int(int_val % 12))
            int_val //= 12
        digits.extend(reversed(int_digits))

    # Fractional part → base 12
    while len(digits) < n_digits:
        frac = frac * 12
        d = int(floor(frac))
        digits.append(d)
        frac = frac - d

    return digits[:n_digits]


def generate_constants():
    """Generate walks from mathematical constant digits in base 12."""
    from mpmath import mp, pi, e, sqrt, phi, ln

    mp.dps = 5000  # 5000 decimal digits of precision

    constants = {
        'Pi':       lambda: pi,
        'e':        lambda: e,
        'sqrt(2)':  lambda: sqrt(2),
        'phi':      lambda: phi,
        'ln(2)':    lambda: ln(2),
    }

    walk_data = {}
    for name, func in constants.items():
        print(f"  Computing {name} in base 12...")
        value = func()
        b12 = constant_to_base12(value, n_digits=2000)
        walk = compute_walk(b12)
        walk_data[name] = walk
        print(f"    {len(b12)} base-12 digits, {len(walk['points'])} walk points")

    return walk_data


# ============================================================
# PART 2: L-SYSTEM FRACTALS
# ============================================================

def lsystem_expand(axiom, rules, iterations):
    """Deterministically expand an L-system string."""
    s = axiom
    for _ in range(iterations):
        s = ''.join(rules.get(c, c) for c in s)
    return s


def lsystem_to_base12(s, angle_degrees):
    """Convert L-system string to base-12 walk sequence.

    F/G/A/B → 0 (translate forward along local +X)
    + → [10] × n (rotate around local +Z, i.e. yaw left)
    - → [11] × n (rotate around local -Z, i.e. yaw right)

    The rotation count n = angle / 15° ensures geometrically correct angles.
    """
    n_rot = max(1, round(angle_degrees / 15))
    result = []
    for c in s:
        if c in ('F', 'G'):
            result.append(0)
        elif c == '+':
            result.extend([10] * n_rot)
        elif c == '-':
            result.extend([11] * n_rot)
        # Skip non-drawing symbols (A, B, X, Y, [, ])
    return result


def generate_fractals():
    """Generate walks from deterministic L-system fractals and sequences."""

    # L-system definitions — all are deterministic rewriting rules
    LSYSTEMS = {
        'Dragon Curve': {
            'axiom': 'F',
            'rules': {'F': 'F+G', 'G': 'F-G'},
            'angle': 90,
            'iterations': 14,
        },
        'Koch Snowflake': {
            'axiom': 'F--F--F',
            'rules': {'F': 'F+F--F+F'},
            'angle': 60,
            'iterations': 5,
        },
        'Sierpinski Arrowhead': {
            'axiom': 'F',
            'rules': {'F': 'G-F-G', 'G': 'F+G+F'},
            'angle': 60,
            'iterations': 9,
        },
        'Hilbert Curve': {
            'axiom': 'F',
            'rules': {'F': '-G+F+G-', 'G': '+F-G-F+'},
            'angle': 90,
            'iterations': 8,
        },
        'Peano Curve': {
            'axiom': 'F',
            'rules': {'F': 'F+G+F-G-F-G-F+G+F', 'G': 'G-F-G+F+G+F+G-F-G'},
            'angle': 90,
            'iterations': 4,
        },
    }

    walk_data = {}

    for name, spec in LSYSTEMS.items():
        print(f"  L-system: {name}...")
        s = lsystem_expand(spec['axiom'], spec['rules'], spec['iterations'])
        b12 = lsystem_to_base12(s, spec['angle'])
        # Cap sequence length for reasonable walks
        if len(b12) > 10000:
            step = len(b12) // 10000
            b12 = b12[::step]
        walk = compute_walk(b12)
        walk_data[name] = walk
        print(f"    {len(b12)} base-12 values, {len(walk['points'])} walk points")

    # Fibonacci word — deterministic binary sequence
    print("  Fibonacci word...")
    a_str, b_str = "0", "01"
    while len(b_str) < 8000:
        a_str, b_str = b_str, b_str + a_str
    fib_bits = [int(c) for c in b_str[:8000]]
    # Convert to base-12 using sliding window of 4 bits
    fib_b12 = []
    for i in range(0, len(fib_bits) - 3, 1):
        val = fib_bits[i]*8 + fib_bits[i+1]*4 + fib_bits[i+2]*2 + fib_bits[i+3]
        fib_b12.append(val % 12)
    walk = compute_walk(fib_b12[:3000])
    walk_data['Fibonacci word'] = walk
    print(f"    {len(fib_b12)} base-12 values, {len(walk['points'])} walk points")

    # Thue-Morse — deterministic binary sequence
    print("  Thue-Morse...")
    tm = [0]
    while len(tm) < 8192:
        tm = tm + [1 - x for x in tm]
    tm_b12 = []
    for i in range(0, len(tm) - 3, 1):
        val = tm[i]*8 + tm[i+1]*4 + tm[i+2]*2 + tm[i+3]
        tm_b12.append(val % 12)
    walk = compute_walk(tm_b12[:3000])
    walk_data['Thue-Morse'] = walk
    print(f"    {len(tm_b12)} base-12 values, {len(walk['points'])} walk points")

    return walk_data


# ============================================================
# PART 3: MANDELBROT / JULIA / LOGISTIC MAP
# ============================================================

def mandelbrot_orbit_base12(c, max_iter=3000):
    """Compute Mandelbrot orbit of z=z²+c and encode as base-12.

    Uses the angle of each orbit point normalized to [0, 11].
    Real iteration — no randomness.
    """
    z = complex(0, 0)
    orbit = []
    for _ in range(max_iter):
        z = z * z + c
        if abs(z) > 1e6:
            break
        angle = np.arctan2(z.imag, z.real)  # [-pi, pi]
        normalized = (angle + np.pi) / (2 * np.pi) * 11.99
        orbit.append(int(normalized))
    return orbit


def julia_orbit_base12(c, z0, max_iter=3000):
    """Compute Julia set orbit from z0 under z=z²+c."""
    z = z0
    orbit = []
    for _ in range(max_iter):
        z = z * z + c
        if abs(z) > 1e6:
            break
        angle = np.arctan2(z.imag, z.real)
        normalized = (angle + np.pi) / (2 * np.pi) * 11.99
        orbit.append(int(normalized))
    return orbit


def logistic_map_base12(r, x0, n=3000):
    """Compute logistic map x_{n+1} = r*x_n*(1-x_n).

    Deterministic for given r and x0. No randomness.
    r=3.99: chaotic regime. r=3.56995: onset of chaos.
    """
    x = x0
    seq = []
    for _ in range(n):
        x = r * x * (1.0 - x)
        seq.append(int(x * 11.99))
    return seq


def generate_mandelbrot():
    """Generate walks from Mandelbrot/Julia orbits and logistic map."""
    walk_data = {}

    # Mandelbrot orbits — points chosen near the boundary for long orbits
    mandelbrot_points = {
        'Mandelbrot cardioid':   complex(-0.75, 0.01),
        'Mandelbrot spiral':    complex(-0.7463, 0.1102),
        'Mandelbrot seahorse':  complex(-0.75, 0.1),
        'Mandelbrot antenna':   complex(-1.768, 0.001),
        'Mandelbrot period-3':  complex(-0.1225, 0.7449),
    }

    for name, c in mandelbrot_points.items():
        print(f"  {name} (c={c})...")
        b12 = mandelbrot_orbit_base12(c, max_iter=3000)
        if len(b12) < 50:
            print(f"    Only {len(b12)} points (escaped), skipping")
            continue
        walk = compute_walk(b12)
        walk_data[name] = walk
        print(f"    {len(b12)} orbit points, {len(walk['points'])} walk points")

    # Julia set orbits — iterate from specific starting points
    julia_sets = {
        'Julia rabbit':   (complex(-0.123, 0.745), complex(0.1, 0.1)),
        'Julia dendrite': (complex(0.0, 1.0), complex(0.01, 0.01)),
        'Julia dragon':   (complex(-0.8, 0.156), complex(0.1, 0.0)),
        'Julia spiral':   (complex(-0.4, 0.6), complex(0.0, 0.1)),
        'Julia siegel':   (complex(-0.391, -0.587), complex(0.1, 0.1)),
    }

    for name, (c, z0) in julia_sets.items():
        print(f"  {name} (c={c}, z0={z0})...")
        b12 = julia_orbit_base12(c, z0, max_iter=3000)
        if len(b12) < 50:
            print(f"    Only {len(b12)} points (escaped), skipping")
            continue
        walk = compute_walk(b12)
        walk_data[name] = walk
        print(f"    {len(b12)} orbit points, {len(walk['points'])} walk points")

    # Logistic map — deterministic chaos
    logistic_params = {
        'Logistic map (chaos)':    (3.99, 0.5),
        'Logistic map (periodic)': (3.56995, 0.5),
        'Logistic map (period-3)': (3.8284, 0.5),
    }

    for name, (r, x0) in logistic_params.items():
        print(f"  {name} (r={r}, x0={x0})...")
        b12 = logistic_map_base12(r, x0, n=3000)
        walk = compute_walk(b12)
        walk_data[name] = walk
        print(f"    {len(b12)} values, {len(walk['points'])} walk points")

    return walk_data


# ============================================================
# MAIN
# ============================================================

print("[1/4] Computing mathematical constants in base 12...")
pi_walks = generate_constants()

print(f"\n[2/4] Computing L-system fractals...")
fractal_walks = generate_fractals()

print(f"\n[3/4] Computing Mandelbrot/Julia orbits and logistic map...")
mandelbrot_walks = generate_mandelbrot()

print(f"\n[4/4] Saving data files...")

# pi_walk_data.js
js = f"""// Mathematical Constants — Base-12 Digit Walks
// Generated {datetime.now()}
// Source: mpmath arbitrary-precision computation
// Each constant is converted to its TRUE base-12 digit representation
// NO random data — all digits are deterministically computed

const PI_WALK_DATA = {json.dumps(pi_walks, indent=2)};
"""
out = os.path.join(DATA_DIR, 'pi_walk_data.js')
with open(out, 'w') as f:
    f.write(js)
print(f"  Saved: {out} ({len(pi_walks)} walks)")

# fractal_walk_data.js
js = f"""// L-System Fractals and Deterministic Sequences
// Generated {datetime.now()}
// Source: deterministic L-system rewriting rules, Fibonacci word, Thue-Morse
// NO random data — all sequences are computed from deterministic rules

const FRACTAL_WALK_DATA = {json.dumps(fractal_walks, indent=2)};
"""
out = os.path.join(DATA_DIR, 'fractal_walk_data.js')
with open(out, 'w') as f:
    f.write(js)
print(f"  Saved: {out} ({len(fractal_walks)} walks)")

# mandelbrot_walk_data.js
js = f"""// Mandelbrot/Julia Orbits and Logistic Map
// Generated {datetime.now()}
// Source: deterministic complex iteration z=z²+c, logistic map x=rx(1-x)
// NO random data — all orbits are computed from mathematical iteration

const MANDELBROT_WALK_DATA = {json.dumps(mandelbrot_walks, indent=2)};
"""
out = os.path.join(DATA_DIR, 'mandelbrot_walk_data.js')
with open(out, 'w') as f:
    f.write(js)
print(f"  Saved: {out} ({len(mandelbrot_walks)} walks)")

print(f"\nCompleted: {datetime.now()}")
total = len(pi_walks) + len(fractal_walks) + len(mandelbrot_walks)
print(f"Total: {total} mathematical walks (zero random data)")
