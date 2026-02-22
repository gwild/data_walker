#!/usr/bin/env python3
"""
DNA Walk with Base-12 Mapping (PCPRI-style)

Apply the same 3D turtle walk approach used for PCPRI to DNA sequences.
Maps DNA to base-12 using several strategies:
1. Codon-based: 64 codons → base-12 (mod 12)
2. Di-nucleotide: 16 pairs → base-12
3. GC-content sliding window → normalized to base-12

This allows direct comparison between PCPRI walks and DNA walks.
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
print("DNA WALK WITH BASE-12 MAPPING (PCPRI-STYLE)")
print("=" * 70)
print(f"Started: {datetime.now()}\n")

# PCPRI's optimal mapping
SPIRAL_MAPPING = [0, 2, 4, 6, 8, 10, 1, 3, 5, 7, 9, 11]

# Codon table - maps 64 codons to amino acids (used for grouping)
CODON_TO_AA = {
    'TTT': 'F', 'TTC': 'F', 'TTA': 'L', 'TTG': 'L',
    'TCT': 'S', 'TCC': 'S', 'TCA': 'S', 'TCG': 'S',
    'TAT': 'Y', 'TAC': 'Y', 'TAA': '*', 'TAG': '*',
    'TGT': 'C', 'TGC': 'C', 'TGA': '*', 'TGG': 'W',
    'CTT': 'L', 'CTC': 'L', 'CTA': 'L', 'CTG': 'L',
    'CCT': 'P', 'CCC': 'P', 'CCA': 'P', 'CCG': 'P',
    'CAT': 'H', 'CAC': 'H', 'CAA': 'Q', 'CAG': 'Q',
    'CGT': 'R', 'CGC': 'R', 'CGA': 'R', 'CGG': 'R',
    'ATT': 'I', 'ATC': 'I', 'ATA': 'I', 'ATG': 'M',
    'ACT': 'T', 'ACC': 'T', 'ACA': 'T', 'ACG': 'T',
    'AAT': 'N', 'AAC': 'N', 'AAA': 'K', 'AAG': 'K',
    'AGT': 'S', 'AGC': 'S', 'AGA': 'R', 'AGG': 'R',
    'GTT': 'V', 'GTC': 'V', 'GTA': 'V', 'GTG': 'V',
    'GCT': 'A', 'GCC': 'A', 'GCA': 'A', 'GCG': 'A',
    'GAT': 'D', 'GAC': 'D', 'GAA': 'E', 'GAG': 'E',
    'GGT': 'G', 'GGC': 'G', 'GGA': 'G', 'GGG': 'G',
}

# Amino acid hydrophobicity (Kyte-Doolittle scale, normalized)
AA_HYDRO = {
    'I': 4.5, 'V': 4.2, 'L': 3.8, 'F': 2.8, 'C': 2.5,
    'M': 1.9, 'A': 1.8, 'G': -0.4, 'T': -0.7, 'S': -0.8,
    'W': -0.9, 'Y': -1.3, 'P': -1.6, 'H': -3.2, 'E': -3.5,
    'Q': -3.5, 'D': -3.5, 'N': -3.5, 'K': -3.9, 'R': -4.5,
    '*': 0.0,  # Stop codon
}

class Turtle3D:
    """3D turtle for walk computation (same as PCPRI)."""
    def __init__(self):
        self.position = np.array([0.0, 0.0, 0.0])
        self.rotation = Rotation.identity()
        self.path = [self.position.copy()]

    def move(self, direction, mapping):
        direction = mapping[direction % 12]
        if direction < 6:
            local_dir = np.array([
                [1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1],
            ])[direction]
            world_dir = self.rotation.apply(local_dir)
            self.position += world_dir
        else:
            rot_axis = [[1,0,0], [-1,0,0], [0,1,0], [0,-1,0], [0,0,1], [0,0,-1]][direction - 6]
            delta_rot = Rotation.from_rotvec(np.array(rot_axis) * np.radians(15))
            self.rotation = delta_rot * self.rotation
        self.path.append(self.position.copy())

def dna_to_base12_codon(sequence):
    """Convert DNA to base-12 using codon → amino acid → hydrophobicity."""
    values = []
    for i in range(0, len(sequence) - 2, 3):
        codon = sequence[i:i+3]
        if codon in CODON_TO_AA:
            aa = CODON_TO_AA[codon]
            hydro = AA_HYDRO.get(aa, 0.0)
            values.append(hydro)

    if len(values) == 0:
        return []

    # Normalize to base-12
    arr = np.array(values)
    arr_min, arr_max = arr.min(), arr.max()
    if arr_max == arr_min:
        return [6] * len(values)
    normalized = (arr - arr_min) / (arr_max - arr_min)
    base12 = (normalized * 11.99).astype(int)
    return base12.tolist()

def dna_to_base12_dinuc(sequence):
    """Convert DNA to base-12 using di-nucleotide frequencies."""
    # Map 16 di-nucleotides to numbers 0-15, then mod 12
    dinuc_map = {}
    bases = 'ACGT'
    for i, b1 in enumerate(bases):
        for j, b2 in enumerate(bases):
            dinuc_map[b1 + b2] = i * 4 + j

    values = []
    for i in range(len(sequence) - 1):
        dinuc = sequence[i:i+2]
        if dinuc in dinuc_map:
            # Use mod 12 to map 0-15 → 0-11
            values.append(dinuc_map[dinuc] % 12)

    return values

def dna_to_base12_gc_window(sequence, window_size=30):
    """Convert DNA to base-12 using GC-content in sliding windows."""
    values = []
    for i in range(0, len(sequence) - window_size + 1, window_size // 3):
        window = sequence[i:i + window_size]
        gc = sum(1 for b in window if b in 'GC') / len(window)
        values.append(gc)

    if len(values) == 0:
        return []

    # Normalize to base-12
    arr = np.array(values)
    arr_min, arr_max = arr.min(), arr.max()
    if arr_max == arr_min:
        return [6] * len(values)
    normalized = (arr - arr_min) / (arr_max - arr_min)
    base12 = (normalized * 11.99).astype(int)
    return base12.tolist()

def compute_walk_features(base12_seq, mapping=SPIRAL_MAPPING, max_points=5000):
    """Compute 3D turtle walk features (same as PCPRI)."""
    if len(base12_seq) == 0:
        return None

    step = max(1, len(base12_seq) // max_points)
    seq = base12_seq[::step]

    turtle = Turtle3D()
    for digit in seq:
        turtle.move(digit, mapping)

    path = np.array(turtle.path)

    max_dist = float(np.max(np.linalg.norm(path, axis=1)))
    x_range = float(path[:, 0].max() - path[:, 0].min())
    y_range = float(path[:, 1].max() - path[:, 1].min())
    z_range = float(path[:, 2].max() - path[:, 2].min())
    final_dist = float(np.linalg.norm(path[-1]))
    volume = x_range * y_range * z_range if x_range * y_range * z_range > 0 else 1
    compactness = max_dist / (volume ** (1/3)) if volume > 0 else 0
    ranges = sorted([x_range, y_range, z_range])
    flatness = ranges[0] / ranges[2] if ranges[2] > 0 else 1

    return {
        'path': path.tolist(),
        'max_dist': max_dist,
        'x_range': x_range,
        'y_range': y_range,
        'z_range': z_range,
        'final_dist': final_dist,
        'compactness': compactness,
        'flatness': flatness,
        'path_length': len(seq),
    }

def parse_ncbi_title(fasta_text):
    """Extract clean title from FASTA header."""
    for line in fasta_text.strip().split('\n'):
        if line.startswith('>'):
            header = line[1:].strip()
            parts = header.split(' ', 1)
            title = parts[1] if len(parts) > 1 else parts[0]
            title = re.sub(r'\s+ORF\d+\w*\s+.*$', '', title)
            title = re.sub(r',?\s*partial\s+(cds|sequence).*$', '', title, flags=re.IGNORECASE)
            title = re.sub(r',?\s*complete\s+(genome|sequence|cds).*$', '', title, flags=re.IGNORECASE)
            title = re.sub(r',?\s*genomic\s+sequence.*$', '', title, flags=re.IGNORECASE)
            return title.strip().rstrip(',').rstrip(';')
    return None

def fetch_ncbi_sequence(accession, fallback_name=""):
    """Fetch sequence and NCBI title from NCBI."""
    ssl_context = ssl.create_default_context()
    ssl_context.check_hostname = False
    ssl_context.verify_mode = ssl.CERT_NONE

    url = f"https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?db=nuccore&id={accession}&rettype=fasta&retmode=text"
    print(f"  Fetching {fallback_name} ({accession})...")
    try:
        with urllib.request.urlopen(url, timeout=120, context=ssl_context) as response:
            text = response.read().decode('utf-8')
            title = parse_ncbi_title(text)
            lines = text.strip().split('\n')
            seq = ''.join(line for line in lines if not line.startswith('>'))
            seq = ''.join(c for c in seq.upper() if c in 'ACGTU')
            seq = seq.replace('U', 'T')
            return seq, title or fallback_name
    except Exception as e:
        print(f"    Failed: {e}")
        return None, fallback_name

# ============================================================
# FETCH DNA SEQUENCES
# ============================================================
print("[1/4] Fetching DNA sequences...")

accessions = [
    ('NC_045512.2', 'COVID Wuhan'),
    ('OK091006.1', 'COVID Delta'),
    ('NC_004718.3', 'SARS-CoV-1'),
    ('NR_024570.1', 'E. coli 16S rRNA'),
]

sequences = {}
for accession, fallback in accessions:
    seq, ncbi_title = fetch_ncbi_sequence(accession, fallback)
    if seq:
        sequences[ncbi_title] = (accession, seq)
        print(f"    Got {len(seq):,} bp")

if not sequences:
    raise RuntimeError("No real NCBI sequences were fetched. Aborting to enforce real-data-only policy.")

# ============================================================
# COMPUTE WALKS WITH DIFFERENT ENCODINGS
# ============================================================
print("\n[2/4] Computing base-12 walks...")

results = {}

for name, (accession, seq) in sequences.items():
    print(f"\n  {name} ({len(seq):,} bp):")

    results[name] = {
        'accession': accession,
        'length': len(seq),
        'encodings': {}
    }

    # Encoding 1: Codon-based (hydrophobicity)
    base12_codon = dna_to_base12_codon(seq)
    if base12_codon:
        walk_codon = compute_walk_features(base12_codon)
        walk_codon['base12'] = [int(x) for x in base12_codon]
        results[name]['encodings']['codon_hydro'] = walk_codon
        print(f"    Codon (hydro): {walk_codon['path_length']} steps, max_dist={walk_codon['max_dist']:.1f}")

    # Encoding 2: Di-nucleotide
    base12_dinuc = dna_to_base12_dinuc(seq)
    if base12_dinuc:
        walk_dinuc = compute_walk_features(base12_dinuc)
        walk_dinuc['base12'] = [int(x) for x in base12_dinuc]
        results[name]['encodings']['dinucleotide'] = walk_dinuc
        print(f"    Di-nucleotide: {walk_dinuc['path_length']} steps, max_dist={walk_dinuc['max_dist']:.1f}")

    # Encoding 3: GC-content window
    base12_gc = dna_to_base12_gc_window(seq)
    if base12_gc:
        walk_gc = compute_walk_features(base12_gc)
        walk_gc['base12'] = [int(x) for x in base12_gc]
        results[name]['encodings']['gc_window'] = walk_gc
        print(f"    GC window:     {walk_gc['path_length']} steps, max_dist={walk_gc['max_dist']:.1f}")

# ============================================================
# COMPARE WITH PCPRI WALKS
# ============================================================
print("\n[3/4] Comparing with PCPRI walk statistics...")

# Load PCPRI stats if available
try:
    with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'analysis', 'walk_failure_investigation.json'), 'r') as f:
        pcpri_data = json.load(f)

    # Get some PCPRI walk stats from successes
    if 'failure_details' in pcpri_data and len(pcpri_data['failure_details']) > 0:
        sample = pcpri_data['failure_details'][0]
        pcpri_max_dist = sample.get('gt_walk', {}).get('max_dist', 'N/A')
        print(f"  PCPRI sample max_dist: {pcpri_max_dist}")
except:
    print("  (PCPRI data not available for comparison)")

print("\n  Walk Feature Comparison:")
print("  " + "-" * 75)
print(f"  {'Sequence':<20} {'Encoding':<15} {'max_dist':>10} {'per_step':>10} {'compactness':>12}")
print("  " + "-" * 75)

for name, data in results.items():
    for enc_name, walk in data['encodings'].items():
        per_step = walk['max_dist'] / walk['path_length'] if walk['path_length'] > 0 else 0
        print(f"  {name:<20} {enc_name:<15} {walk['max_dist']:>10.1f} {per_step:>10.4f} {walk['compactness']:>12.3f}")

# Compare COVID variants using codon encoding
print("\n  COVID Variant Walk Differences (codon encoding):")
print("  " + "-" * 50)
covid_walks = {name: data['encodings'].get('codon_hydro') for name, data in results.items() if 'COVID' in name}
if len(covid_walks) >= 2:
    names = list(covid_walks.keys())
    for i in range(len(names)):
        for j in range(i+1, len(names)):
            w1, w2 = covid_walks[names[i]], covid_walks[names[j]]
            if w1 and w2:
                dist_diff = abs(w1['max_dist'] - w2['max_dist'])
                compact_diff = abs(w1['compactness'] - w2['compactness'])
                print(f"  {names[i]} vs {names[j]}:")
                print(f"    max_dist diff: {dist_diff:.2f}")
                print(f"    compactness diff: {compact_diff:.4f}")

# ============================================================
# SAVE RESULTS
# ============================================================
print("\n[4/4] Saving results...")

# Save walk data (without full paths for size)
output_data = {}
for name, data in results.items():
    output_data[name] = {
        'accession': data['accession'],
        'length': data['length'],
        'encodings': {}
    }
    for enc_name, walk in data['encodings'].items():
        output_data[name]['encodings'][enc_name] = {
            'max_dist': walk['max_dist'],
            'x_range': walk['x_range'],
            'y_range': walk['y_range'],
            'z_range': walk['z_range'],
            'final_dist': walk['final_dist'],
            'compactness': walk['compactness'],
            'flatness': walk['flatness'],
            'path_length': walk['path_length'],
        }

with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'analysis', 'dna_walk_base12_results.json'), 'w') as f:
    json.dump(output_data, f, indent=2)
print("  Saved: analysis/dna_walk_base12_results.json")

# Save one walk path for visualization
if results:
    first_name = list(results.keys())[0]
    first_walk = list(results[first_name]['encodings'].values())[0]
    vis_data = {
        'name': first_name,
        'encoding': list(results[first_name]['encodings'].keys())[0],
        'points': first_walk['path'][:5000],  # Limit for visualization
    }

    js_content = f"""// DNA Walk Base-12 Data - Generated {datetime.now()}
const DNA_WALK_BASE12 = {json.dumps(vis_data, indent=2)};
"""
    with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'visualizations/data', 'dna_walk_base12.js'), 'w') as f:
        f.write(js_content)
    print("  Saved: visualizations/data/dna_walk_base12.js")

print(f"""
{'=' * 70}
SUMMARY
{'=' * 70}

DNA sequences converted to base-12 using three encodings:
1. Codon (hydrophobicity): 3-letter codons -> amino acid -> hydrophobicity -> base-12
2. Di-nucleotide: 2-letter pairs (16 values) mod 12
3. GC window: GC-content in sliding windows -> normalized to base-12

All encodings use the PCPRI Spiral mapping: {SPIRAL_MAPPING}

This enables direct comparison between PCPRI and DNA walk structures.
""")

print(f"Completed: {datetime.now()}")
