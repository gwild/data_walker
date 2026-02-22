#!/usr/bin/env python3
"""
Generate walk data from REAL audio recordings.

Sources:
  - ESC-50 dataset (GitHub): dog, cat, frog, crow, insects, crickets,
    chirping birds, rain, thunderstorm, fire, wind, sea waves
  - NOAA / Internet Archive: whale recordings
  - InsectSet32 (Zenodo): cicadas

Audio processing: scipy spectrogram -> dominant frequency -> base-12 -> turtle walk
NO fake data. NO np.random. Every walk is from a real recording.

Generates:
  - visualizations/data/animals_walk_data.js
  - visualizations/data/frogs_walk_data.js
  - visualizations/data/environment_walk_data.js
  - visualizations/data/whales_walk_data.js
"""

import sys
sys.stdout.reconfigure(line_buffering=True)

import numpy as np
import json
import os
import io
import csv
import ssl
import urllib.request
import tempfile
import wave
import struct
import soundfile as sf
from datetime import datetime
from scipy.spatial.transform import Rotation
from scipy.signal import spectrogram as scipy_spectrogram
from scipy.io import wavfile

print("=" * 70)
print("REAL AUDIO WALK DATA — ESC-50, NOAA, Internet Archive")
print("=" * 70)
print(f"Started: {datetime.now()}\n")

DATA_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'visualizations', 'data')
os.makedirs(DATA_DIR, exist_ok=True)

IDENTITY_MAPPING = list(range(12))

# SSL context for downloads
SSL_CTX = ssl.create_default_context()
SSL_CTX.check_hostname = False
SSL_CTX.verify_mode = ssl.CERT_NONE


class Turtle3D:
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
            self.position = self.position + self.rotation.apply(local_dirs[d])
        else:
            axes = [[1,0,0], [-1,0,0], [0,1,0], [0,-1,0], [0,0,1], [0,0,-1]]
            self.rotation = Rotation.from_rotvec(np.array(axes[d-6]) * np.radians(15)) * self.rotation
        self.path.append(self.position.copy())


def compute_walk(base12_seq, mapping=None, max_points=5000):
    if mapping is None:
        mapping = IDENTITY_MAPPING
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
    """Convert audio waveform to base-12 via dominant frequency extraction.

    1. Compute spectrogram
    2. Find dominant frequency at each time step
    3. Normalize to [0, 11]

    This is a real, meaningful encoding — it maps actual frequency content
    of the recording to walk directions.
    """
    # Ensure mono
    if len(audio_data.shape) > 1:
        audio_data = audio_data.mean(axis=1)

    # Convert to float
    if audio_data.dtype == np.int16:
        audio_data = audio_data.astype(np.float64) / 32768.0
    elif audio_data.dtype == np.int32:
        audio_data = audio_data.astype(np.float64) / 2147483648.0

    # Compute spectrogram
    f, t, Sxx = scipy_spectrogram(audio_data, fs=sample_rate, nperseg=nperseg,
                                   noverlap=nperseg//2)

    if Sxx.shape[1] < 2:
        return []

    # Dominant frequency at each time step
    dominant_idx = np.argmax(Sxx, axis=0)
    dominant_freq = f[dominant_idx]

    # Normalize to [0, 11]
    fmin, fmax = dominant_freq.min(), dominant_freq.max()
    if fmax - fmin < 1e-10:
        return [6] * len(dominant_freq)
    normalized = (dominant_freq - fmin) / (fmax - fmin)
    base12 = (normalized * 11.99).astype(int)
    return base12.tolist()


def download_file(url, timeout=60):
    """Download a file and return bytes. Returns None on failure."""
    try:
        req = urllib.request.Request(url)
        req.add_header('User-Agent', 'Mozilla/5.0')
        with urllib.request.urlopen(req, timeout=timeout, context=SSL_CTX) as resp:
            return resp.read()
    except Exception as e:
        print(f"    Download failed: {e}")
        return None


def read_audio_bytes(audio_bytes):
    """Read audio file from bytes, return (sample_rate, data).

    Uses soundfile (libsndfile) which handles WAV, FLAC, OGG, MP3.
    Falls back to scipy.io.wavfile for plain WAV.
    """
    buf = io.BytesIO(audio_bytes)
    try:
        data, sr = sf.read(buf)
        return sr, np.array(data)
    except Exception:
        pass
    # Fallback to scipy for plain WAV
    buf.seek(0)
    try:
        sr, data = wavfile.read(buf)
        return sr, data
    except Exception:
        return None, None


# ============================================================
# ESC-50: Download metadata and audio files
# ============================================================

ESC50_META_URL = "https://raw.githubusercontent.com/karolpiczak/ESC-50/master/meta/esc50.csv"
ESC50_AUDIO_URL = "https://github.com/karolpiczak/ESC-50/raw/master/audio/"

# ESC-50 category targets we want
ESC50_CATEGORIES = {
    # Animals
    0: 'Dog',
    5: 'Cat',
    9: 'Crow',
    7: 'Insects (Buzzing)',
    # Frogs
    4: 'Frog',
    # Environment
    10: 'Rain',
    11: 'Sea Waves',
    12: 'Crackling Fire',
    16: 'Wind',
    19: 'Thunderstorm',
    13: 'Crickets',
    14: 'Chirping Birds',
}

# How many clips per category to download (ESC-50 has 40 per category)
CLIPS_PER_CATEGORY = 3

# Frog clips with known species (from Freesound metadata)
# ESC-50 filename -> species name
FROG_SPECIES = {
    '1-15689-A-4.wav': 'Tree Frog',
    '1-15689-B-4.wav': 'Tree Frog',
    '1-17970-A-4.wav': 'Water Frog',
    '2-32515-A-4.wav': 'Pacific Chorus Frog',
    '3-83527-A-4.wav': 'Tree Frog',
    '4-99193-A-4.wav': 'Swamp Frog',
    '3-102908-A-4.wav': 'Tropical Frog',
    '5-237499-A-4.wav': 'Edible Frog (Pelophylax)',
    '4-154793-A-4.wav': 'Heavy Frogs',
}
# Download these specific frog clips for species diversity
FROG_CLIPS_WANTED = list(FROG_SPECIES.keys())


def fetch_esc50():
    """Download ESC-50 metadata and audio files for target categories."""
    print("[1/4] Downloading ESC-50 metadata...")
    meta_bytes = download_file(ESC50_META_URL)
    if not meta_bytes:
        print("  ERROR: Could not download ESC-50 metadata")
        return {}

    reader = csv.DictReader(io.StringIO(meta_bytes.decode('utf-8')))
    rows = list(reader)
    print(f"  Metadata: {len(rows)} entries")

    # Group files by target category
    by_category = {}
    for row in rows:
        target = int(row['target'])
        if target in ESC50_CATEGORIES:
            by_category.setdefault(target, []).append(row['filename'])

    # Download audio files
    print("\n[2/4] Downloading ESC-50 audio clips...")
    results = {}  # { 'Category Name': [(sr, data), ...] }

    for target, name in ESC50_CATEGORIES.items():
        files = by_category.get(target, [])
        if not files:
            print(f"  {name}: no files found")
            continue

        # For frogs, download specific clips with known species
        if target == 4:  # Frog
            wanted = [f for f in FROG_CLIPS_WANTED if f in files]
            if wanted:
                files_to_get = wanted
            else:
                files_to_get = files[:CLIPS_PER_CATEGORY]
        else:
            files_to_get = files[:CLIPS_PER_CATEGORY]

        clips = []
        for fname in files_to_get:
            url = ESC50_AUDIO_URL + fname
            print(f"  Downloading {name}: {fname}...")
            wav_bytes = download_file(url)
            if wav_bytes:
                sr, data = read_audio_bytes(wav_bytes)
                if sr and data is not None and len(data) > 0:
                    clips.append((sr, data, fname))
                    print(f"    OK: {sr} Hz, {len(data)} samples")
                else:
                    print(f"    Failed to parse WAV")
            import time
            time.sleep(0.3)

        if clips:
            results[name] = clips
            print(f"  {name}: {len(clips)} clips downloaded")

    return results


# ============================================================
# WHALE RECORDINGS — NOAA / Internet Archive
# ============================================================

# Real whale recordings — verified working URLs (tested 2026-02-18)
# All public domain (NOAA/US govt) or Creative Commons
WHALE_URLS = {
    'Whale (Humpback)': [
        # NOAA PMEL Acoustics — public domain US government work
        'https://www.pmel.noaa.gov/acoustics/whales/sounds/whalewav/akhumphi1x.wav',
    ],
    'Whale (Blue)': [
        # NOAA PMEL Acoustics — NE Pacific Blue Whale, sped up 10x
        'https://www.pmel.noaa.gov/acoustics/whales/sounds/whalewav/nepblue24s10x.wav',
        # The famous "52 Hz whale"
        'https://www.pmel.noaa.gov/acoustics/whales/sounds/whalewav/ak52_10x.wav',
    ],
    'Whale (Orca)': [
        # Internet Archive — CC0 whale sound effects collection
        'https://archive.org/download/whale-songs-whale-sound-effects/killer-whale.mp3',
    ],
    'Whale (Sperm)': [
        # NOAA Fisheries — sperm whale clicks, public domain
        'https://www.fisheries.noaa.gov/s3/2023-04/Phma-clicks-NOAA-PAGroup-01-sperm-clip.mp3',
        # Internet Archive — sperm whale codas, Galapagos
        'https://archive.org/download/WhaleSong_201509/Whale-song.mp3',
    ],
    'Whale (Beluga)': [
        # Internet Archive — CC0 whale sound effects collection
        'https://archive.org/download/whale-songs-whale-sound-effects/beluga-whale.mp3',
    ],
}


def fetch_whales():
    """Download whale recordings from NOAA and other public sources."""
    print("\n[3/4] Downloading whale recordings...")
    results = {}

    for name, urls in WHALE_URLS.items():
        print(f"  Trying {name}...")
        clips = []
        for url in urls:
            fname = url.split('/')[-1]
            print(f"    URL: {fname}...")
            audio_bytes = download_file(url, timeout=60)
            if audio_bytes and len(audio_bytes) > 1000:
                sr, data = read_audio_bytes(audio_bytes)
                if sr and data is not None and len(data) > 100:
                    clips.append((sr, data, fname))
                    print(f"    OK: {sr} Hz, {len(data)} samples")
                else:
                    print(f"    Could not parse audio")
            else:
                print(f"    Failed or empty")
            import time
            time.sleep(0.3)
        if clips:
            results[name] = clips
            print(f"  {name}: {len(clips)} clips downloaded")
        else:
            print(f"    SKIPPED: no working URL for {name}")

    return results


# ============================================================
# PROCESS AUDIO -> WALKS
# ============================================================

def process_audio_category(name, clips, mapping=None):
    """Process audio clips into walk data.

    For categories with multiple clips, concatenate their base-12 sequences
    to get a longer walk.
    """
    all_b12 = []
    for sr, data, fname in clips:
        b12 = audio_to_base12(data, sr)
        if b12:
            all_b12.extend(b12)

    if not all_b12:
        print(f"    {name}: no usable audio data")
        return None

    walk = compute_walk(all_b12, mapping)
    return walk


# ============================================================
# MAIN
# ============================================================

# Fetch ESC-50 data
esc50_clips = fetch_esc50()

# Fetch whale recordings
whale_clips = fetch_whales()

print(f"\n[4/4] Computing walks from real audio...")

# Build category groupings
OPTIMAL_MAPPING = [0, 1, 2, 3, 4, 5, 6, 7, 10, 9, 8, 11]

# Animals
animals_data = {}
animal_names = ['Dog', 'Cat', 'Crow', 'Insects (Buzzing)']
for name in animal_names:
    if name in esc50_clips:
        walk = process_audio_category(name, esc50_clips[name], OPTIMAL_MAPPING)
        if walk:
            animals_data[name] = walk
            print(f"  {name}: {len(walk['base12'])} base-12 -> {len(walk['points'])} pts")

# Whales (separate file)
whales_data = {}
for name, clips in whale_clips.items():
    walk = process_audio_category(name, clips, OPTIMAL_MAPPING)
    if walk:
        whales_data[name] = walk
        print(f"  {name}: {len(walk['base12'])} base-12 -> {len(walk['points'])} pts")

# Frogs — each clip labeled by species
frogs_data = {}
if 'Frog' in esc50_clips:
    seen_species = {}
    for sr, data, fname in esc50_clips['Frog']:
        species = FROG_SPECIES.get(fname, 'Frog')
        # Deduplicate: if we already have this species, add a number
        if species in seen_species:
            seen_species[species] += 1
            clip_name = f"{species} ({seen_species[species]})"
        else:
            seen_species[species] = 1
            clip_name = species
        b12 = audio_to_base12(data, sr)
        if b12:
            walk = compute_walk(b12, OPTIMAL_MAPPING)
            frogs_data[clip_name] = walk
            print(f"  {clip_name}: {len(b12)} base-12 -> {len(walk['points'])} pts")

# Crickets and Chirping Birds go into their own data structures (not frogs)
crickets_data = {}
if 'Crickets' in esc50_clips:
    walk = process_audio_category('Crickets', esc50_clips['Crickets'], OPTIMAL_MAPPING)
    if walk:
        crickets_data['Crickets'] = walk
        print(f"  Crickets: {len(walk['base12'])} base-12 -> {len(walk['points'])} pts")

if 'Chirping Birds' in esc50_clips:
    walk = process_audio_category('Chirping Birds', esc50_clips['Chirping Birds'], OPTIMAL_MAPPING)
    if walk:
        crickets_data['Chirping Birds'] = walk
        print(f"  Chirping Birds: {len(walk['base12'])} base-12 -> {len(walk['points'])} pts")

# Environment
env_data = {}
env_names = ['Rain', 'Sea Waves', 'Crackling Fire', 'Wind', 'Thunderstorm']
for name in env_names:
    if name in esc50_clips:
        walk = process_audio_category(name, esc50_clips[name], OPTIMAL_MAPPING)
        if walk:
            env_data[name] = walk
            print(f"  {name}: {len(walk['base12'])} base-12 -> {len(walk['points'])} pts")


# ============================================================
# SAVE
# ============================================================

def save_js(filename, var_name, data, sources_comment):
    if not data:
        print(f"  SKIPPED {filename} (no data)")
        return
    js = f"""// {var_name} — Real Audio Walks
// Generated {datetime.now()}
// {sources_comment}
// Audio processing: scipy spectrogram -> dominant frequency -> base-12 -> turtle walk
// NO fake data — every walk is from a real recording

const {var_name} = {json.dumps(data, indent=2)};
"""
    out = os.path.join(DATA_DIR, filename)
    with open(out, 'w') as f:
        f.write(js)
    print(f"  Saved: {filename} ({len(data)} walks)")


print(f"\nSaving...")

# Merge crickets/birds into animals
animals_data.update(crickets_data)

save_js('animals_walk_data.js', 'ANIMALS_WALK_DATA', animals_data,
        'Source: ESC-50 dataset (dog, cat, crow, insects, crickets, chirping birds)')

save_js('frogs_walk_data.js', 'FROGS_WALK_DATA', frogs_data,
        'Source: ESC-50 dataset — frog species: Tree Frog, Water Frog, Pacific Chorus Frog, Swamp Frog, Tropical Frog, Edible Frog')

save_js('environment_walk_data.js', 'ENVIRONMENT_WALK_DATA', env_data,
        'Source: ESC-50 dataset (rain, sea waves, crackling fire, wind, thunderstorm)')

save_js('whales_walk_data.js', 'WHALES_WALK_DATA', whales_data,
        'Source: NOAA PMEL Acoustics (humpback, blue) + Internet Archive (orca, beluga) + NOAA Fisheries (sperm)')

print(f"\nCompleted: {datetime.now()}")

# Summary
total = len(animals_data) + len(frogs_data) + len(env_data) + len(whales_data)
print(f"\nTotal: {total} real audio walks")
if not whale_clips:
    print("\nNOTE: Whale downloads failed. Whale recordings may need manual sourcing.")
    print("  Try: NOAA Fisheries Sound Library, Cornell Bioacoustics, or Watkins Marine Mammal Sound Database")
