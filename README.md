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
cd data_walker_rs

# Build
cargo build --release

# Launch the native GUI
cargo run -- gui

# Relaunch GUI cleanly during development
cargo run --bin launch_gui

# Download data from real sources
cargo run -- download --all

# Generate math walks only (no network needed)
cargo run -- generate-math

# Generate thumbnails
cargo run -- generate-thumbnails

# List available sources
cargo run -- list
cargo run -- list --category dna
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `gui` | Launch native GUI viewer (egui + three-d) |
| `launch_gui` | Development wrapper: stop existing GUI, build, and relaunch |
| `generate-math [--output data/math]` | Generate math walks locally |
| `generate-thumbnails [--output thumbnails] [--size 512]` | Render thumbnail gallery images |
| `list [--category <name>]` | List available data sources |
| `download --source <id>` | Download a single source |
| `download --category <cat>` | Download all sources in a category |
| `download --all` | Download everything |
| `freesound search <query>` | Search Freesound for additional audio sources |
| `freesound download <ID>` | Download a Freesound clip into the raw audio store |
| `test-notes <file>` | Inspect extracted note data from an audio file |

## Data Sources

All data is downloaded from real, documented sources:

| Category | Source | Examples |
|----------|--------|----------|
| **Animals** | [ESC-50](https://github.com/karolpiczak/ESC-50) | Dog, Cat, Frog, Insects |
| **Birdsong** | [Archive.org](https://archive.org/) | Dawn Chorus, Forest Birds |
| **Whales** | [Archive.org](https://archive.org/) | Humpback, Blue, Orca, Sperm, Beluga |
| **Indigenous Music** | [Archive.org](https://archive.org/) | Karaja solo, dance, choir |
| **Composers** | [Archive.org](https://archive.org/) | Bach, Beethoven, Schoenberg |
| **DNA** | [NCBI GenBank](https://www.ncbi.nlm.nih.gov/nuccore/) | SARS-CoV-2, Human mitochondria |
| **Cosmos** | [LIGO/GWOSC](https://gwosc.org/) | GW150914 gravitational wave |
| **Stocks** | [Yahoo Finance](https://finance.yahoo.com/) | S&P 500, NASDAQ, Dow, Bitcoin |
| **Math** | Computed | Pi, e, fractals, Mandelbrot, sequences |

## How It Works

### Base-12 Conversion
Each data source is converted to values 0-11:
- **Audio**: Dominant frequency bins from FFT spectrogram
- **DNA**: ACGT base-4 streaming to base-12
- **Stocks**: Log returns normalized to 12 buckets
- **Cosmos**: Strain amplitude normalized to 12 buckets
- **Music**: Pitch class (MIDI note mod 12)

### 3D Turtle Walk
Values 0-11 map to turtle commands:
- **0-5**: Translations (+X, -X, +Y, -Y, +Z, -Z)
- **6-11**: Rotations (+/-15 degrees around X, Y, Z axes)

Different **mappings** reorder which value triggers which action, revealing different structural patterns.

### Named Mappings
| Mapping | Use |
|---------|-----|
| **Identity** | No remapping (direct 0-11) |
| **Optimal** | General purpose |
| **Spiral** | Interleaved translation/rotation |
| **Stock-opt** | Tuned for financial data |

## Project Structure

```
data_walker/
├── data_walker_rs/
│   ├── Cargo.toml          # Dependencies
│   ├── sources.yaml        # SSOT: all data source definitions
│   ├── src/
│   │   ├── main.rs         # CLI entry point
│   │   ├── config.rs       # YAML config loader
│   │   ├── walk.rs         # 3D turtle walk engine (quaternions)
│   │   ├── download.rs     # Real data downloaders (NCBI, Yahoo, GWOSC, Archive.org)
│   │   ├── gui.rs          # Native GUI (egui + three-d)
│   │   ├── thumbnail.rs    # Offscreen thumbnail renderer
│   │   ├── automation.rs   # GUI automation IPC
│   │   ├── logging.rs      # Rotating file logs
│   │   └── converters/
│   │       ├── mod.rs       # DNA, finance, cosmos converters
│   │       ├── audio.rs     # FFT spectrogram to base-12
│   │       └── math/        # Constants, fractals, Mandelbrot, sequences
├── CLAUDE.md
├── README.md
└── confessions_of_a_liar.md
```

## No Fake Data Policy

This project strictly uses real data:
- Downloaded from documented URLs
- Processed algorithmically
- Source attribution in code

The only computed data is pure mathematics (pi digits, fractal coordinates, number-theoretic sequences).

## License

Data sources retain their original licenses. Code is provided as-is.
