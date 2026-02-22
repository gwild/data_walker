#!/usr/bin/env python3
"""
Download real whale recordings from NOAA and Internet Archive.

Sources:
- NOAA PMEL Acoustics: https://www.pmel.noaa.gov/acoustics/whales/sounds/
- Internet Archive: https://archive.org/details/whale-songs-whale-sound-effects

Output: visualizations/data/whales_walk_data.js
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

DATA_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'visualizations', 'data')
os.makedirs(DATA_DIR, exist_ok=True)

# SSL context
SSL_CTX = ssl.create_default_context()
SSL_CTX.check_hostname = False
SSL_CTX.verify_mode = ssl.CERT_NONE

OPTIMAL_MAPPING = [0, 1, 2, 3, 4, 5, 6, 7, 10, 9, 8, 11]

# Real whale sound URLs - WAV files from NOAA PMEL
# Source: https://www.pmel.noaa.gov/acoustics/whales/sounds/sounds_whales_blue.html
WHALE_URLS = {
    # NOAA PMEL whale sounds (WAV format)
    'Blue Whale NE Pacific': 'https://www.pmel.noaa.gov/acoustics/whales/sounds/whalewav/nepblue24s10x.wav',
    'Blue Whale West Pacific': 'https://www.pmel.noaa.gov/acoustics/whales/sounds/whalewav/wblue26s10x.wav',
    'Blue Whale South Pacific': 'https://www.pmel.noaa.gov/acoustics/whales/sounds/whalewav/etpb3_10xc-BlueWhaleSouthPacific-10x.wav',
    'Blue Whale Atlantic': 'https://www.pmel.noaa.gov/acoustics/whales/sounds/whalewav/atlblue_512_64_0-50_10x.wav',
    '52 Hz Whale': 'https://www.pmel.noaa.gov/acoustics/whales/sounds/whalewav/ak52_10x.wav',
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


def download_and_process_wav(url, name):
    """Download WAV and convert to base12."""
    print(f"  Downloading: {name}")
    print(f"    URL: {url}")

    try:
        # Download WAV file
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        with urllib.request.urlopen(req, context=SSL_CTX, timeout=60) as response:
            wav_data = response.read()

        # Save temp WAV
        with tempfile.NamedTemporaryFile(suffix='.wav', delete=False) as f:
            f.write(wav_data)
            wav_path = f.name

        # Read WAV file
        try:
            sample_rate, audio_data = wavfile.read(wav_path)
        except Exception:
            import soundfile as sf
            audio_data, sample_rate = sf.read(wav_path)

        os.unlink(wav_path)

        base12 = audio_to_base12(audio_data, sample_rate)
        print(f"    Got {len(base12)} base12 values")
        return base12

    except Exception as e:
        print(f"    ERROR: {e}")
        return None


def main():
    print("=" * 70)
    print("DOWNLOADING WHALE RECORDINGS")
    print("Sources: NOAA, Internet Archive")
    print("=" * 70)
    print(f"Started: {datetime.now()}\n")

    whales_data = {}

    for name, url in WHALE_URLS.items():
        base12 = download_and_process_wav(url, name)
        if base12 and len(base12) > 100:
            walk = compute_walk(base12, OPTIMAL_MAPPING)
            walk['source'] = url
            whales_data[name] = walk

    if whales_data:
        output_path = os.path.join(DATA_DIR, 'whales_walk_data.js')
        content = f"""// WHALES_WALK_DATA - Real whale recordings
// Downloaded: {datetime.now().isoformat()}
// Sources: Internet Archive whale recordings
// NO FAKE DATA - all from real recordings

const WHALES_WALK_DATA = {json.dumps(whales_data, indent=2)};
"""
        with open(output_path, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"\nSaved: whales_walk_data.js ({len(whales_data)} walks)")
    else:
        print("\nERROR: No whale data downloaded!")

    print("\n" + "=" * 70)
    print("COMPLETE")
    print("=" * 70)


if __name__ == '__main__':
    main()
