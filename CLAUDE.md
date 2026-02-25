# Data Walker - Claude Instructions

## ABSOLUTE RULE: NO FAKE DATA

**ALL data must be DOWNLOADED from a real, documented source with a URL.**

### What CAN be computed:
- Pure math only: pi digits, Mandelbrot coordinates, fractals from formulas

### What MUST be downloaded:
- Audio → ESC-50, Xeno-canto, NOAA, Archive.org
- DNA → NCBI GenBank
- Stocks → Yahoo Finance (yfinance)
- Cosmos → LIGO/GWOSC
- Music scores → IMSLP

### FORBIDDEN:
- `np.random` for any data
- "generated", "simulated", "synthetic" data
- "based on characteristics"
- "fallback" or "legacy" data
- Placeholder data with real-sounding names

### If data is missing:
1. Find the real source URL
2. Write a download script
3. Document the URL in code comments
4. If no source exists: **STOP AND ASK THE USER**

---

## ABSOLUTE RULE: STORE RAW DATA ONLY

**Disk stores ONLY raw downloaded data. Everything else is computed on the fly.**

### What gets stored to disk:
- Raw audio files (.wav, .mp3)
- Raw DNA sequences (.fasta)
- Raw strain data (.txt.gz)
- Raw price data (JSON from API)

### What is NEVER stored to disk:
- Base-12 sequences (computed on the fly from raw data)
- 3D points (computed on the fly from base-12 + mapping)
- Walk metadata duplicating sources.yaml

### The pipeline:
```
DISK: raw data files only
  → ON REQUEST: load raw → convert to base-12 → cache in memory
  → ON REQUEST: base-12 + mapping → walk engine → 3D points (never stored)
```

### FORBIDDEN:
- Saving base-12 arrays to JSON files
- Saving 3D point arrays anywhere
- Pre-computing walks and storing results
- Any "pre-processed" data files

---

## Architecture: Clean and Simple

```
Download → Convert to base-12 → Walk Engine → REST API → Web UI
   ↓            ↓                   ↓            ↓          ↓
(real URLs)  (converters)      (quaternions)  (Axum)    (Three.js)
```

### NO hidden layers:
- No fake data
- No workarounds
- No hidden filters

If data shouldn't be shown, don't put it in sources.yaml. Period.

### Single Source of Truth:
- `data_walker_rs/sources.yaml` - all source definitions, mappings, categories
- `data_walker_rs/src/` - all Rust source code
- `data_walker_rs/web/` - HTML/JS web viewers

---

## 3D Turtle Walk

Each data source is converted to base-12, then walked through 3D space.

### Base-12 Mapping:
- Values 0-5: translations (+X, -X, +Y, -Y, +Z, -Z)
- Values 6-11: rotations (+RX, -RX, +RY, -RY, +RZ, -RZ at 15°)

### Named Mappings:
| Mapping | Array | Use |
|---------|-------|-----|
| **Optimal** | `[0,1,2,3,4,5,6,7,10,9,8,11]` | General purpose |
| **Spiral** | `[0,2,4,6,8,10,1,3,5,7,9,11]` | Interleaved trans/rot |
| **Identity** | `[0,1,2,3,4,5,6,7,8,9,10,11]` | No remapping |
| **Stock-opt** | `[1,0,2,4,10,5,6,9,8,7,3,11]` | Stock data |

### Data Flow:
```
sources.yaml → Config → AppState.load_walk(id) → Converter → base12 → walk_base12() → 3D points
```

Each source entry in `sources.yaml` specifies: id, name, category, subcategory, converter type, default mapping, source URL.

---

## Directory Structure

```
data_walker/
├── data_walker_rs/
│   ├── Cargo.toml             # Rust dependencies
│   ├── sources.yaml           # SSOT: all data sources, mappings, categories
│   ├── src/
│   │   ├── main.rs            # CLI (serve, gui, generate-math, list, download)
│   │   ├── config.rs          # YAML config loader
│   │   ├── walk.rs            # 3D turtle walk engine (quaternions)
│   │   ├── state.rs           # Thread-safe walk cache (AppState)
│   │   ├── server.rs          # Axum REST API
│   │   ├── download.rs        # Downloaders (NCBI, Yahoo, GWOSC, Archive.org)
│   │   ├── gui.rs             # Native GUI (egui + three-d)
│   │   ├── logging.rs         # Rotating file logs
│   │   └── converters/
│   │       ├── mod.rs          # DNA, finance, cosmos converters
│   │       ├── audio.rs        # FFT spectrogram to base-12
│   │       └── math/           # Constants, fractals, Mandelbrot, sequences
│   └── web/
│       ├── index.html          # 3D walk viewer (Three.js)
│       └── compare.html        # Multi-walk comparison tool
├── CLAUDE.md
└── README.md
```

---

## Real Data Sources

| Category | Source | URL |
|----------|--------|-----|
| Animals | ESC-50 | https://github.com/karolpiczak/ESC-50 |
| Birdsong | Xeno-canto | https://xeno-canto.org/ |
| Whales | NOAA | https://www.pmel.noaa.gov/acoustics/whales/sounds/ |
| Indigenous Music | Archive.org | https://archive.org/details/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr |
| DNA | NCBI GenBank | https://www.ncbi.nlm.nih.gov/nuccore/ |
| Cosmos | LIGO | https://gwosc.org/ |
| Stocks | Yahoo Finance | https://finance.yahoo.com/ |
| Music Scores | IMSLP | https://imslp.org/ |
| Math | Computed | (pi, e, fractals - these are the ONLY computed data)

---

## Commands

```bash
cd data_walker_rs

# Build
cargo build --release

# Serve web gallery at http://localhost:8080
cargo run -- serve
cargo run -- serve --port 9000

# Download real data from sources
cargo run -- download --all
cargo run -- download --category dna
cargo run -- download --source gw150914_h1

# Generate math walks (no network needed)
cargo run -- generate-math

# List sources
cargo run -- list
cargo run -- list --category cosmos

# Launch native GUI
cargo run -- gui

# Run tests
cargo test
```

---

## Lessons Learned

1. **No workarounds** - Fix problems at the source, not with filters
2. **No hidden layers** - If it exists, it's visible; if it shouldn't exist, delete it
3. **SSOT** - One config file, one manifest, one truth
4. **Verify** - "Similar" is not "same"; check the actual output
5. **Clean data** - Generators output only real data, no baselines or junk
