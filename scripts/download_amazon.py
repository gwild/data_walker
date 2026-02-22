#!/usr/bin/env python3
"""
Download Amazon rainforest and indigenous music recordings.

Sources:
- Internet Archive: Anthology of Brazilian Indian Music
  https://archive.org/details/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr
- Internet Archive: Various bird sounds (Amazon jungle recordings)
  https://archive.org/details/various-bird-sounds

Output: visualizations/data/amazon_comparison_data.js
"""

import os
import json
import urllib.request
import ssl
import tempfile
import numpy as np
from datetime import datetime
from scipy.signal import spectrogram as scipy_spectrogram
from scipy.spatial.transform import Rotation
import soundfile as sf

DATA_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'visualizations', 'data')
os.makedirs(DATA_DIR, exist_ok=True)

SSL_CTX = ssl.create_default_context()
SSL_CTX.check_hostname = False
SSL_CTX.verify_mode = ssl.CERT_NONE

OPTIMAL_MAPPING = [0, 1, 2, 3, 4, 5, 6, 7, 10, 9, 8, 11]

# Amazon audio sources
AMAZON_RECORDINGS = {
    # Amazon nature sounds
    'Amazon Jungle Morning (Birds)': 'https://archive.org/download/various-bird-sounds/amazon-jungle-morning-24939.mp3',

    # Brazilian Indian Music anthology (Karaja tribe)
    'Karaja - Solo Song Man': 'https://archive.org/download/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr/disc1/01.01.%20Solo%20Song%2C%20Man.mp3',
    'Karaja - Sacred Dance Aruana': 'https://archive.org/download/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr/disc1/01.02.%20Jahave%20%28Sacred%20Masked%20Dance%2C%20Songs%2C%20%22Aruana%22%2C%20Two%20Masks%20Dancing%29.mp3',
    'Karaja - Boys Girls Choir': 'https://archive.org/download/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr/disc1/01.03.%20Women%20And%20Men%27s%20Choir%2C%20Conducted%20By%20Traditional%20Choir%20.mp3',

    # Kraho tribe
    'Kraho - Reversal Singing': 'https://archive.org/download/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr/disc1/02.01.%20%22Reversal%22%20Singing.mp3',
    'Kraho - Hoof Rattle': 'https://archive.org/download/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr/disc1/02.02.%20Hoof%20Rattle%2C%20Rattles%20Made%20Of%20Animal%20Hooves.mp3',

    # Suya tribe
    'Suya - Shukarramae Solo': 'https://archive.org/download/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr/disc2/03.01.%20%22Shukarramae%22%20Solo%20Voice%20Song.mp3',

    # Additional nature
    'Amazon Forest Birds': 'https://archive.org/download/various-bird-sounds/birds-in-forest-on-sunny-day-14444.mp3',
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
    print(f"    URL: {url[:80]}...")

    try:
        req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0'})
        with urllib.request.urlopen(req, context=SSL_CTX, timeout=60) as response:
            audio_data = response.read()

        suffix = '.mp3' if 'mp3' in url else '.wav'
        with tempfile.NamedTemporaryFile(suffix=suffix, delete=False) as f:
            f.write(audio_data)
            temp_path = f.name

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
    print("DOWNLOADING AMAZON AUDIO")
    print("Sources:")
    print("  - Internet Archive: Anthology of Brazilian Indian Music")
    print("  - Internet Archive: Amazon jungle nature sounds")
    print("=" * 70)
    print(f"Started: {datetime.now()}\n")

    amazon_data = {}

    for name, url in AMAZON_RECORDINGS.items():
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
            amazon_data[name] = walk

    if amazon_data:
        output_path = os.path.join(DATA_DIR, 'amazon_comparison_data.js')
        content = f"""// AMAZON_COMPARISON_DATA - Amazon rainforest and indigenous music
// Downloaded: {datetime.now().isoformat()}
// Sources:
//   - https://archive.org/details/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr
//   - https://archive.org/details/various-bird-sounds
// NO FAKE DATA - all from real recordings

const AMAZON_COMPARISON_DATA = {json.dumps(amazon_data, indent=2)};
"""
        with open(output_path, 'w', encoding='utf-8') as f:
            f.write(content)
        print(f"\nSaved: amazon_comparison_data.js ({len(amazon_data)} walks)")
    else:
        print("\nERROR: No Amazon data downloaded!")

    print("\n" + "=" * 70)
    print("COMPLETE")
    print("=" * 70)


if __name__ == '__main__':
    main()
