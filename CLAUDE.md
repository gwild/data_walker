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

## Architecture: Clean and Simple

```
Generator → Data file → Config (FILE_INFO) → Display
   ↓           ↓              ↓                ↓
(download)  (clean)       (clean)         (no filters)
```

### NO hidden layers:
- No HIDDEN_WALKS
- No isBaseline filters
- No workarounds

If data shouldn't be shown, don't put it in FILE_INFO. Period.

### Single Source of Truth:
- `web/walk_config.js` - all configuration
- `web/data/*.js` - all walk data (gitignored)
- `web/thumbnails/` - generated thumbnails (gitignored)

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

### Generator Pattern:
```python
# 1. Download from real source (with URL)
# 2. Convert to base-12
# 3. Walk with mapping → 3D points
# 4. Save: { 'points': [...], 'base12': [...], 'source': 'URL' }
```

---

## Directory Structure

```
data_walker/
├── scripts/           # Python/JS generators
│   ├── generate_*.py  # Data generators (download + process)
│   ├── download_*.py  # Pure downloaders
│   └── generate_thumbnails.js
├── web/
│   ├── sources.html           # Gallery page
│   ├── neural_walks_compare.html  # 3D comparison tool
│   ├── walk_config.js         # SSOT config
│   ├── walk_renderer.js       # Three.js renderer
│   ├── data/                  # Generated data (gitignored)
│   └── thumbnails/            # Generated thumbnails (gitignored)
└── .gitignore
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
# Generate data (downloads from real sources)
python scripts/generate_audio_walks.py
python scripts/generate_composers_walk.py
python scripts/generate_math_walks.py
python scripts/download_amazon.py
python scripts/download_birdsong.py
python scripts/download_cosmos.py
python scripts/download_whales.py
python scripts/dna_walk_base12.py

# Generate thumbnails (requires http-server running)
npx http-server web -p 8081 &
node scripts/generate_thumbnails.js

# Serve gallery
npx http-server web -p 8080
# Open http://localhost:8080/sources.html
```

---

## Lessons Learned

1. **No workarounds** - Fix problems at the source, not with filters
2. **No hidden layers** - If it exists, it's visible; if it shouldn't exist, delete it
3. **SSOT** - One config file, one manifest, one truth
4. **Verify** - "Similar" is not "same"; check the actual output
5. **Clean data** - Generators output only real data, no baselines or junk
