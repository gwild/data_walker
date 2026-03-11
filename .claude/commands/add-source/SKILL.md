---
name: add-source
description: Add a new data source from a URL to the Data Walker. Use when user says "add source", "add a new walk", "add data from URL", "import audio/dna/cosmos data", or invokes /add-source. Takes a URL and optional --id, --name, --category, --subcategory arguments.
metadata:
  author: Data Walker
  version: 1.0.0
---

# Add Source

Add a new data source from a URL to the Data Walker.

## Usage

```
/add-source <url> [options]
```

## Arguments

- `<url>` - Direct download URL for the data file

## Options

- `--id <id>` - Override auto-generated source ID
- `--name <name>` - Override auto-generated display name
- `--category <cat>` - Force category (audio, dna, cosmos, finance, proteins)
- `--subcategory <sub>` - Set subcategory for organization

## Instructions

### Step 1: Validate URL

Verify the URL is accessible and determine file type:
```bash
curl -sI "<url>" | head -20
```
Check for HTTP 200, Content-Type, and Content-Length.

### Step 2: Determine Data Type

Based on URL and content-type, determine the converter:

| File Extension / Content-Type | Converter | Data Folder |
|------------------------------|-----------|-------------|
| `.mp3`, `.wav`, `audio/*` | `audio` | `data/audio/` |
| `.fasta`, `.fa`, `.fna` | `dna` | `data/dna/` |
| `.txt.gz` (LIGO strain) | `cosmos` | `data/cosmos/` |
| `.json` (price data) | `finance` | `data/finance/` |
| `.pdb` (protein structure) | `pdb_backbone` or `pdb_sequence` | `data/proteins/` |

If unclear, ask user to specify `--category`.

### Step 3: Generate Source ID

Create a valid source ID from the URL or filename:
- Extract filename from URL
- Convert to lowercase
- Replace spaces/special chars with underscores
- Remove file extension
- Ensure unique (check existing sources.yaml)

### Step 4: Download File

Download to appropriate data folder:
```bash
cd data_walker_rs
curl -L -o "data/audio/<id>.mp3" "<url>"
```

Verify download succeeded and file is valid:
```bash
file data/audio/<id>.mp3
wc -c data/audio/<id>.mp3
```

### Step 5: Add to sources.yaml

Add new entry to `data_walker_rs/sources.yaml`:
```yaml
  - id: <generated_id>
    name: "<Display Name>"
    category: <category>
    subcategory: <subcategory>
    converter: <converter_type>
    mapping: Optimal
    url: "<original_url>"
```

### Step 6: Verify Integration

Run the GUI and verify the new source appears and renders:
```bash
cargo run --release -- gui
```

## Supported Source Types

| Type | Formats | Min Size | Conversion |
|------|---------|----------|------------|
| Audio | MP3, WAV | 10KB | FFT spectrogram to base-12 |
| DNA | FASTA | 100B | ACGT → base-4 → base-12 |
| Cosmos | .txt.gz | 1KB | Strain amplitude to base-12 |
| Finance | JSON | 1KB | Price deltas to base-12 |
| Proteins | .pdb | 1KB | Backbone or sequence to base-12 |

## Troubleshooting

Error: 404/URL not accessible
Solution: Verify URL is correct and publicly accessible

Error: Unknown file type
Solution: Specify `--category` explicitly

Error: Duplicate ID
Solution: Choose different `--id` or modify existing
