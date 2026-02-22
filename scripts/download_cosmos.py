#!/usr/bin/env python3
"""
Download real gravitational wave data from LIGO/GWOSC.

Source: Gravitational Wave Open Science Center (GWOSC)
https://gwosc.org/

License: CC BY 4.0

Output: visualizations/data/cosmos_real_walk_data.js
"""

import os
import json
import gzip
import urllib.request
import ssl
import tempfile
import numpy as np
from datetime import datetime
from scipy.spatial.transform import Rotation

DATA_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'visualizations', 'data')
os.makedirs(DATA_DIR, exist_ok=True)

SSL_CTX = ssl.create_default_context()
SSL_CTX.check_hostname = False
SSL_CTX.verify_mode = ssl.CERT_NONE

IDENTITY_MAPPING = list(range(12))

# LIGO gravitational wave events - strain data (TXT format, gzipped)
# Source: https://gwosc.org/
GWOSC_EVENTS = {
    # GW150914 - First detection of gravitational waves (binary black hole merger)
    'GW150914 (H1)': 'https://gwosc.org/eventapi/html/GWTC-1-confident/GW150914/v3/H-H1_GWOSC_4KHZ_R1-1126259447-32.txt.gz',
    'GW150914 (L1)': 'https://gwosc.org/eventapi/html/GWTC-1-confident/GW150914/v3/L-L1_GWOSC_4KHZ_R1-1126259447-32.txt.gz',

    # GW170817 - First binary neutron star merger
    'GW170817 (H1)': 'https://gwosc.org/eventapi/html/GWTC-1-confident/GW170817/v3/H-H1_GWOSC_4KHZ_R1-1187008882-32.txt.gz',
    'GW170817 (L1)': 'https://gwosc.org/eventapi/html/GWTC-1-confident/GW170817/v3/L-L1_GWOSC_4KHZ_R1-1187008882-32.txt.gz',

    # GW170104 - Binary black hole
    'GW170104 (H1)': 'https://gwosc.org/eventapi/html/GWTC-1-confident/GW170104/v2/H-H1_GWOSC_4KHZ_R1-1167559936-32.txt.gz',
    'GW170104 (L1)': 'https://gwosc.org/eventapi/html/GWTC-1-confident/GW170104/v2/L-L1_GWOSC_4KHZ_R1-1167559936-32.txt.gz',
}


class Turtle3D:
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
    if mapping is None:
        mapping = list(range(12))
    step = max(1, len(base12_seq) // max_points)
    seq = base12_seq[::step]
    t = Turtle3D()
    for d in seq:
        t.move(d, mapping)
    return {
        'points': np.array(t.path).tolist(),
        'base12': [int(x) for x in base12_seq],
    }


def strain_to_base12(strain_data):
    """Convert gravitational wave strain data to base12."""
    strain = np.array(strain_data)

    # Normalize to 0-11
    s_min, s_max = strain.min(), strain.max()
    if s_max == s_min:
        return [6] * len(strain)

    base12 = []
    for s in strain:
        normalized = (s - s_min) / (s_max - s_min)
        b12 = int(normalized * 11.99)
        base12.append(min(11, max(0, b12)))

    return base12


def download_strain_data(url, name):
    """Download LIGO strain data from GWOSC."""
    print(f"  Downloading: {name}")
    print(f"    URL: {url}")

    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        with urllib.request.urlopen(req, context=SSL_CTX, timeout=60) as response:
            compressed_data = response.read()

        # Decompress gzip
        data = gzip.decompress(compressed_data).decode('utf-8')

        # Parse strain values (one per line, skip header lines starting with #)
        strain_values = []
        for line in data.strip().split('\n'):
            line = line.strip()
            if line and not line.startswith('#'):
                try:
                    strain_values.append(float(line))
                except ValueError:
                    continue

        print(f"    Got {len(strain_values)} strain values")
        return strain_values

    except Exception as e:
        print(f"    Download error: {e}")
        return None


def main():
    print("=" * 70)
    print("DOWNLOADING GRAVITATIONAL WAVE DATA FROM LIGO/GWOSC")
    print("Source: https://gwosc.org/")
    print("License: CC BY 4.0")
    print("=" * 70)
    print(f"Started: {datetime.now()}\n")

    cosmos_data = {}

    for name, url in GWOSC_EVENTS.items():
        print(f"\n{name}:")
        strain_values = download_strain_data(url, name)

        if strain_values is None or len(strain_values) < 100:
            print("    Failed or insufficient data")
            continue

        base12 = strain_to_base12(strain_values)
        walk = compute_walk(base12, IDENTITY_MAPPING)
        walk['source'] = url
        walk['event'] = name.split(' ')[0]
        cosmos_data[name] = walk

    if cosmos_data:
        output_path = os.path.join(DATA_DIR, 'cosmos_real_walk_data.js')
        content = f"""// COSMOS_REAL_WALK_DATA - Real gravitational wave data from LIGO
// Downloaded: {datetime.now().isoformat()}
// Source: GWOSC (https://gwosc.org/)
// License: CC BY 4.0
// NO FAKE DATA - all from real LIGO observations

const COSMOS_REAL_WALK_DATA = {json.dumps(cosmos_data, indent=2)};
"""
        with open(output_path, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"\nSaved: cosmos_real_walk_data.js ({len(cosmos_data)} walks)")
    else:
        print("\nERROR: No cosmos data downloaded!")

    print("\n" + "=" * 70)
    print("COMPLETE")
    print("=" * 70)


if __name__ == '__main__':
    main()
