# Data Walker

3D turtle walk visualizations of real-world data converted to base-12.

## What is this?

Data from diverse sources (audio, DNA, stocks, gravitational waves, music scores) is:
1. Downloaded from documented sources
2. Converted to base-12 values
3. Walked through 3D space using a turtle graphics algorithm
4. Visualized as interactive 3D paths

## Quick Start

```bash
# Install dependencies
pip install numpy scipy soundfile yfinance requests

# Generate data (downloads from real sources)
python scripts/generate_audio_walks.py

# Serve the gallery
npx http-server web -p 8080

# Open http://localhost:8080/sources.html
```

## Data Sources

All data is downloaded from real, documented sources:

| Category | Source | Examples |
|----------|--------|----------|
| **Animals** | [ESC-50](https://github.com/karolpiczak/ESC-50) | Dog, Cat, Frog, Insects |
| **Birdsong** | [Xeno-canto](https://xeno-canto.org/) | Dawn Chorus, Forest Birds |
| **Whales** | [NOAA](https://www.pmel.noaa.gov/acoustics/whales/sounds/) | Blue Whale, Humpback, Orca |
| **Indigenous Music** | [Archive.org](https://archive.org/) | Karaja, Kraho, Suya tribes |
| **DNA** | [NCBI GenBank](https://www.ncbi.nlm.nih.gov/nuccore/) | SARS-CoV-2, Plant chloroplasts |
| **Cosmos** | [LIGO](https://gwosc.org/) | GW150914 gravitational wave |
| **Stocks** | [Yahoo Finance](https://finance.yahoo.com/) | NASDAQ, DOW, S&P500, Bitcoin |
| **Music Scores** | [IMSLP](https://imslp.org/) | Bach, Beethoven, Schoenberg |
| **Math** | Computed | Pi, e, Mandelbrot, Fractals |

## How It Works

### Base-12 Conversion
Each data source is converted to values 0-11:
- **Audio**: Dominant frequency bins from spectrogram
- **DNA**: Codon hydrophobicity or nucleotide encoding
- **Stocks**: Price changes normalized to 12 buckets
- **Music**: MIDI note mod 12 (pitch class)

### 3D Turtle Walk
Values 0-11 map to turtle commands:
- **0-5**: Translations (+X, -X, +Y, -Y, +Z, -Z)
- **6-11**: Rotations (±15° around X, Y, Z axes)

Different **mappings** reorder which value triggers which action, revealing different structural patterns.

## Project Structure

```
data_walker/
├── scripts/
│   ├── generate_audio_walks.py    # ESC-50 animals, frogs, environment
│   ├── generate_composers_walk.py # IMSLP music scores
│   ├── generate_math_walks.py     # Pi, fractals, Mandelbrot
│   ├── download_amazon.py         # Indigenous music, Amazon birds
│   ├── download_birdsong.py       # Xeno-canto recordings
│   ├── download_cosmos.py         # LIGO gravitational waves
│   ├── download_whales.py         # NOAA whale recordings
│   ├── dna_walk_base12.py         # NCBI DNA sequences
│   └── generate_thumbnails.js     # Puppeteer thumbnail generator
├── web/
│   ├── sources.html               # Gallery page
│   ├── neural_walks_compare.html  # Interactive 3D comparison
│   ├── walk_config.js             # Configuration (SSOT)
│   ├── walk_renderer.js           # Three.js renderer
│   ├── data/                      # Generated walk data (gitignored)
│   └── thumbnails/                # Generated thumbnails (gitignored)
└── .gitignore
```

## Gallery Features

- **Category filters**: Animals, Audio, DNA, Cosmos, Finance, Math
- **Subcategory organization**: Whales, Birdsong, Composers, etc.
- **Mapping selector**: Switch between different base-12 mappings
- **3D comparison tool**: Compare multiple walks side-by-side

## No Fake Data Policy

This project strictly uses real data:
- ✅ Downloaded from documented URLs
- ✅ Processed algorithmically
- ✅ Source attribution in code

The only computed data is pure mathematics (pi digits, fractal coordinates).

## License

Data sources retain their original licenses. Code is provided as-is.
