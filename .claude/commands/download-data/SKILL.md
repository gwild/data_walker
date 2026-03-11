---
name: download-data
description: Download raw data from external sources for Data Walker. Use when user says "download data", "fetch sources", "get audio/dna/cosmos/finance data", or invokes /download-data. Supports --all, --category, and --source options.
metadata:
  author: Data Walker
  version: 1.0.0
---

# Download Data

Download raw data from external sources for Data Walker.

## Usage

```
/download-data [options]
```

## Options

- `--all` - Download all available data sources
- `--category <name>` - Download all sources in a category (dna, audio, cosmos, finance, proteins)
- `--source <id>` - Download a specific source by ID

## Instructions

Run the data_walker Rust CLI to download raw data files.

**IMPORTANT: This downloads RAW data only. No base-12 conversion happens during download.**
All conversion to base-12 happens on-the-fly during plotting.

### Step 1: Determine Scope

Based on user arguments, determine what to download:
- `--all`: Download everything
- `--category <name>`: Download all sources in that category
- `--source <id>`: Download one specific source

### Step 2: Run Download

```bash
cd data_walker_rs && cargo run -- download --all
```

Or for specific categories/sources:
```bash
cd data_walker_rs && cargo run -- download --category dna
cd data_walker_rs && cargo run -- download --source dog
```

### Step 3: Verify Downloads

Check that files were saved to the correct locations.

## Raw Data Formats

- DNA: `.fasta` files (ACGT sequences from NCBI)
- Audio: `.wav` or `.mp3` files (ESC-50, Archive.org, Freesound)
- Cosmos: `.txt.gz` files (LIGO strain data)
- Finance: `.json` files (raw price arrays from Yahoo Finance)
- Proteins: `.pdb` files (RCSB PDB structural data)

## Output Locations

- `data_walker_rs/data/audio/` - Audio files
- `data_walker_rs/data/dna/` - FASTA files
- `data_walker_rs/data/cosmos/` - Strain data files
- `data_walker_rs/data/finance/` - Price data files
- `data_walker_rs/data/proteins/` - PDB files

Math sources do NOT require downloading - they are computed on-the-fly during plotting.

## Troubleshooting

Error: Freesound sources fail to download
Cause: Missing `.env` file with Freesound API credentials
Solution: Create `data_walker_rs/.env` with FREESOUND_CLIENT_ID and FREESOUND_API_KEY

Error: PDB source returns 404
Cause: Structure not yet publicly released by RCSB
Solution: Check RCSB website for release date, comment out source in sources.yaml
