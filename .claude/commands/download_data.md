# Download Data

Download raw data from external sources for Data Walker.

## Usage

```
/download_data [options]
```

## Options

- `--all` - Download all available data sources
- `--category <name>` - Download all sources in a category (dna, audio, cosmos, finance)
- `--source <id>` - Download a specific source by ID

## Instructions

Run the data_walker Rust CLI to download raw data files.

**IMPORTANT: This downloads RAW data only. No base-12 conversion happens during download.**
All conversion to base-12 happens on-the-fly during plotting.

Raw data formats stored:
- DNA: `.fasta` files (ACGT sequences from NCBI)
- Audio: `.wav` or `.mp3` files (ESC-50, Archive.org)
- Cosmos: `.txt.gz` files (LIGO strain data)
- Finance: `.json` files (raw price arrays from Yahoo Finance)

## Examples

Download all data:
```bash
cd data_walker_rs && cargo run -- download --all
```

Download a specific category:
```bash
cd data_walker_rs && cargo run -- download --category dna
cd data_walker_rs && cargo run -- download --category audio
cd data_walker_rs && cargo run -- download --category cosmos
cd data_walker_rs && cargo run -- download --category finance
```

Download a specific source:
```bash
cd data_walker_rs && cargo run -- download --source dog
cd data_walker_rs && cargo run -- download --source sars_cov_2
cd data_walker_rs && cargo run -- download --source gw150914_h1
```

## Output

Downloaded files are saved to:
- `data_walker_rs/data/audio/` - Audio files
- `data_walker_rs/data/dna/` - FASTA files
- `data_walker_rs/data/cosmos/` - Strain data files
- `data_walker_rs/data/finance/` - Price data files

Math sources do NOT require downloading - they are computed on-the-fly during plotting.
