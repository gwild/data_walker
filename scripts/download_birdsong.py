#!/usr/bin/env python3
"""
Download real birdsong recordings from Internet Archive.

Source: https://archive.org/details/various-bird-sounds
License: CC0 1.0 Universal (Public Domain)

Output: visualizations/data/real_birdsong_walk_data.js
"""

import os
import json
import urllib.request
import ssl
import tempfile
import numpy as np
from datetime import datetime
from scipy.io import wavfile
from scipy.signal import spectrogram as scipy_spectrogram
from scipy.spatial.transform import Rotation
import soundfile as sf

DATA_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'visualizations', 'data')
os.makedirs(DATA_DIR, exist_ok=True)

SSL_CTX = ssl.create_default_context()
SSL_CTX.check_hostname = False
SSL_CTX.verify_mode = ssl.CERT_NONE

OPTIMAL_MAPPING = [0, 1, 2, 3, 4, 5, 6, 7, 10, 9, 8, 11]

# Bird recordings from Internet Archive (CC0 public domain)
# Source: https://archive.org/details/various-bird-sounds
BIRD_RECORDINGS = {
    'Forest Birds Ambience': 'https://archive.org/download/various-bird-sounds/mixkit-forest-birds-ambience-1210.wav',
    'Sea Birds': 'https://archive.org/download/various-bird-sounds/mixkit-sea-birds-squeak-1188.wav',
    'Amazon Jungle Morning': 'https://archive.org/download/various-bird-sounds/amazon-jungle-morning-24939.mp3',
    'Birds in Forest Sunny Day': 'https://archive.org/download/various-bird-sounds/birds-in-forest-on-sunny-day-14444.mp3',
    'Dawn Chorus': 'https://archive.org/download/various-bird-sounds/dawn-chorus-18265.mp3',
    'Florida Birds': 'https://archive.org/download/various-bird-sounds/FloridaBirds.mp3',
    'Meadow Morning Birds': 'https://archive.org/download/various-bird-sounds/Meadow_Morning_Birds.mp3',
    'Spring Birds': 'https://archive.org/download/various-bird-sounds/Spring_Birds_2.mp3',
    'Garden Birdsong': 'https://archive.org/download/various-bird-sounds/Garden_Birdsong.mp3',
    'Hawaii Birds': 'https://archive.org/download/various-bird-sounds/Hawaii_Birds.mp3',
    'Back Garden Birds': 'https://archive.org/download/various-bird-sounds/Back_Garden_Jan_11.mp3',
    'Forest Birds Morning': 'https://archive.org/download/various-bird-sounds/forest-birds-in-morning-135658.mp3',
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


def audio_to_base12(audio_data, sample_rate, nperseg=1024):
    """Convert audio to base12 via spectrogram dominant frequency."""
    if len(audio_data.shape) > 1:
        audio_data = audio_data.mean(axis=1)

    audio_data = audio_data.astype(np.float32)
    if np.max(np.abs(audio_data)) > 0:
        audio_data = audio_data / np.max(np.abs(audio_data))

    f, t, Sxx = scipy_spectrogram(audio_data, fs=sample_rate, nperseg=nperseg)
    dominant_freqs = f[np.argmax(Sxx, axis=0)]

    nonzero = dominant_freqs[dominant_freqs > 0]
    if len(nonzero) == 0:
        return [6] * len(dominant_freqs)

    f_min, f_max = nonzero.min(), nonzero.max()
    if f_max == f_min:
        return [6] * len(dominant_freqs)

    base12 = []
    for freq in dominant_freqs:
        if freq <= 0:
            base12.append(6)
        else:
            normalized = (freq - f_min) / (f_max - f_min)
            b12 = int(normalized * 11.99)
            base12.append(min(11, max(0, b12)))

    return base12


def download_audio(url, name):
    """Download audio file and convert to base12."""
    print(f"  Downloading: {name}")
    print(f"    URL: {url}")

    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        with urllib.request.urlopen(req, context=SSL_CTX, timeout=60) as response:
            audio_data = response.read()

        # Save to temp file
        suffix = '.mp3' if 'mp3' in url else '.wav'
        with tempfile.NamedTemporaryFile(suffix=suffix, delete=False) as f:
            f.write(audio_data)
            temp_path = f.name

        # Read audio
        try:
            data, rate = sf.read(temp_path)
        except Exception as e:
            print(f"    Read error: {e}")
            os.unlink(temp_path)
            return None

        os.unlink(temp_path)
        return data, rate

    except Exception as e:
        print(f"    Download error: {e}")
        return None


def main():
    print("=" * 70)
    print("DOWNLOADING BIRDSONG FROM INTERNET ARCHIVE")
    print("Source: https://archive.org/details/various-bird-sounds")
    print("License: CC0 1.0 Universal (Public Domain)")
    print("=" * 70)
    print(f"Started: {datetime.now()}\n")

    birdsong_data = {}

    for name, url in BIRD_RECORDINGS.items():
        print(f"\n{name}:")
        result = download_audio(url, name)

        if result is None:
            print("    Failed to download")
            continue

        audio_data, sample_rate = result
        base12 = audio_to_base12(audio_data, sample_rate)
        print(f"    Got {len(base12)} base12 values")

        if len(base12) > 50:
            walk = compute_walk(base12, OPTIMAL_MAPPING)
            walk['source'] = url
            birdsong_data[name] = walk

    if birdsong_data:
        output_path = os.path.join(DATA_DIR, 'real_birdsong_walk_data.js')
        content = f"""// REAL_BIRDSONG_WALK_DATA - Real bird recordings from Internet Archive
// Downloaded: {datetime.now().isoformat()}
// Source: https://archive.org/details/various-bird-sounds
// License: CC0 1.0 Universal (Public Domain)
// NO FAKE DATA - all from real recordings

const REAL_BIRDSONG_WALK_DATA = {json.dumps(birdsong_data, indent=2)};
"""
        with open(output_path, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"\nSaved: real_birdsong_walk_data.js ({len(birdsong_data)} walks)")
    else:
        print("\nERROR: No birdsong data downloaded!")

    print("\n" + "=" * 70)
    print("COMPLETE")
    print("=" * 70)


if __name__ == '__main__':
    main()
