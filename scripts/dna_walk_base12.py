#!/usr/bin/env python3
"""
DNA Walk with Base-12 Mapping

Converts DNA sequences (base-4: ACGT) directly to base-12 using streaming
base conversion. No biological interpretation - just pure number conversion.

Source: NCBI GenBank
Output: web/data/dna_walk_base12.js
"""

import numpy as np
import urllib.request
import ssl
import json
import re
from datetime import datetime
import os
from scipy.spatial.transform import Rotation

print("=" * 70)
print("DNA WALK - BASE-4 TO BASE-12 CONVERSION")
print("=" * 70)
print(f"Started: {datetime.now()}\n")

DATA_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'web', 'data')
os.makedirs(DATA_DIR, exist_ok=True)

OPTIMAL_MAPPING = [0, 1, 2, 3, 4, 5, 6, 7, 10, 9, 8, 11]

# DNA nucleotides as base-4 digits
BASE4 = {'A': 0, 'C': 1, 'G': 2, 'T': 3}


def dna_to_base12(sequence):
    """Convert DNA (base-4) to base-12 using streaming base conversion.

    Reads nucleotides as base-4 digits, accumulates value, emits base-12
    digits when accumulator >= 12.

    This is a pure mathematical base conversion - no biological interpretation.
    """
    result = []
    accumulator = 0
    acc_value = 1  # Tracks the "weight" of accumulator in base-4

    for nucleotide in sequence:
        if nucleotide not in BASE4:
            continue

        # Add this nucleotide's value to accumulator
        accumulator = accumulator * 4 + BASE4[nucleotide]
        acc_value *= 4

        # Emit base-12 digits while we have enough accumulated
        while acc_value >= 12:
            # How many base-12 digits can we extract?
            result.append(accumulator % 12)
            accumulator //= 12
            acc_value //= 12

    # Handle remaining accumulator (if any significant value)
    while accumulator > 0:
        result.append(accumulator % 12)
        accumulator //= 12

    return result


class Turtle3D:
    """3D turtle walker."""
    def __init__(self):
        self.position = np.array([0.0, 0.0, 0.0])
        self.rotation = Rotation.identity()
        self.path = [self.position.copy()]

    def move(self, direction, mapping):
        d = mapping[direction % 12]
        if d < 6:
            local_dirs = np.array([
                [1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1],
            ])
            self.position = self.position + self.rotation.apply(local_dirs[d])
        else:
            axes = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]]
            self.rotation = Rotation.from_rotvec(np.array(axes[d - 6]) * np.radians(15)) * self.rotation
        self.path.append(self.position.copy())


def compute_walk(base12_seq, mapping=None, max_points=5000):
    """Compute 3D turtle walk from base-12 sequence."""
    if mapping is None:
        mapping = list(range(12))
    if len(base12_seq) == 0:
        return None

    step = max(1, len(base12_seq) // max_points)
    seq = base12_seq[::step]

    t = Turtle3D()
    for d in seq:
        t.move(d, mapping)

    return {
        'points': np.array(t.path).tolist(),
        'base12': [int(x) for x in base12_seq],
    }


def fetch_ncbi_sequence(accession, name=""):
    """Fetch DNA sequence from NCBI GenBank."""
    ssl_context = ssl.create_default_context()
    ssl_context.check_hostname = False
    ssl_context.verify_mode = ssl.CERT_NONE

    url = f"https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?db=nuccore&id={accession}&rettype=fasta&retmode=text"
    print(f"  Fetching {name} ({accession})...")
    print(f"    URL: {url}")

    try:
        with urllib.request.urlopen(url, timeout=120, context=ssl_context) as response:
            text = response.read().decode('utf-8')
            lines = text.strip().split('\n')
            seq = ''.join(line for line in lines if not line.startswith('>'))
            seq = ''.join(c for c in seq.upper() if c in 'ACGT')
            return seq
    except Exception as e:
        print(f"    Failed: {e}")
        return None


# ============================================================
# DNA SEQUENCES TO FETCH
# ============================================================

# Real DNA sequences from NCBI GenBank
SEQUENCES = {
    # Viruses
    'SARS-CoV-2 (Wuhan)': 'NC_045512.2',
    'SARS-CoV-2 (Delta)': 'OK091006.1',
    'SARS-CoV-1': 'NC_004718.3',
    'MERS-CoV': 'NC_019843.3',

    # Human
    'Human Mitochondria': 'NC_012920.1',

    # Bacteria
    'E. coli (16S rRNA)': 'NR_024570.1',
}

# ============================================================
# MAIN
# ============================================================

print("[1/3] Fetching DNA sequences from NCBI...")

walk_data = {}

for name, accession in SEQUENCES.items():
    seq = fetch_ncbi_sequence(accession, name)

    if not seq:
        print(f"    SKIPPED: could not fetch {name}")
        continue

    print(f"    Got {len(seq):,} bp")

    # Convert to base-12
    base12 = dna_to_base12(seq)
    print(f"    Converted to {len(base12):,} base-12 values")

    # Compute walk
    walk = compute_walk(base12, OPTIMAL_MAPPING)
    if walk:
        walk['source'] = f"https://www.ncbi.nlm.nih.gov/nuccore/{accession}"
        walk['accession'] = accession
        walk['bp_length'] = len(seq)
        walk_data[name] = walk
        print(f"    Walk: {len(walk['points']):,} points")

if not walk_data:
    raise RuntimeError("No sequences fetched. Aborting.")

print(f"\n[2/3] Summary...")
print(f"  {len(walk_data)} DNA walks generated")
for name, data in walk_data.items():
    print(f"    {name}: {data['bp_length']:,} bp -> {len(data['base12']):,} base-12 -> {len(data['points']):,} pts")

print(f"\n[3/3] Saving...")

js_content = f"""// DNA_WALK_BASE12 - DNA sequences converted to base-12
// Generated: {datetime.now().isoformat()}
// Source: NCBI GenBank (https://www.ncbi.nlm.nih.gov/nuccore/)
// Method: Base-4 (ACGT) → Base-12 streaming conversion
// NO FAKE DATA - all from real NCBI sequences

const DNA_WALK_BASE12 = {json.dumps(walk_data, indent=2)};
"""

output_path = os.path.join(DATA_DIR, 'dna_walk_base12.js')
with open(output_path, 'w', encoding='utf-8') as f:
    f.write(js_content)

print(f"  Saved: {output_path}")

print(f"""
{'=' * 70}
COMPLETE
{'=' * 70}
Method: DNA (ACGT = base-4) -> base-12 streaming conversion
  - No biological interpretation
  - Pure mathematical base conversion
  - A=0, C=1, G=2, T=3 treated as base-4 digits

Completed: {datetime.now()}
""")
