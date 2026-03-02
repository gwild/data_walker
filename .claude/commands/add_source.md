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
- `--category <cat>` - Force category (audio, dna, cosmos, finance)
- `--subcategory <sub>` - Set subcategory for organization

## Instructions

### 1. Validate URL

First, verify the URL is accessible and determine file type:

```bash
curl -sI "<url>" | head -20
```

Check for:
- HTTP 200 response
- Content-Type header to identify file type
- Content-Length for sanity check

### 2. Determine Data Type

Based on URL and content-type, determine the converter:

| File Extension / Content-Type | Converter | Data Folder |
|------------------------------|-----------|-------------|
| `.mp3`, `.wav`, `audio/*` | `audio` | `data/audio/` |
| `.fasta`, `.fa`, `.fna` | `dna` | `data/dna/` |
| `.txt.gz` (LIGO strain) | `cosmos` | `data/cosmos/` |
| `.json` (price data) | `finance` | `data/finance/` |

If unclear, ask user to specify `--category`.

### 3. Generate Source ID

Create a valid source ID from the URL or filename:
- Extract filename from URL
- Convert to lowercase
- Replace spaces/special chars with underscores
- Remove file extension
- Ensure unique (check existing sources.yaml)

Example: `Blue_Whale_Song.mp3` -> `blue_whale_song`

### 4. Download File

Download to appropriate data folder:

```bash
cd data_walker_rs

# For audio
curl -L -o "data/audio/<id>.mp3" "<url>"
# or for wav
curl -L -o "data/audio/<id>.wav" "<url>"

# For DNA (NCBI)
curl -L -o "data/dna/<accession>.fasta" "<url>"

# For cosmos
curl -L -o "data/cosmos/<id>.txt.gz" "<url>"

# For finance (usually via yfinance, not direct download)
# Requires special handling
```

Verify download succeeded and file is valid:
```bash
file data/audio/<id>.mp3  # Should show audio format
wc -c data/audio/<id>.mp3  # Check file size > 10KB
```

### 5. Add to sources.yaml

Add new entry to `data_walker_rs/sources.yaml` in the appropriate section:

```yaml
  - id: <generated_id>
    name: "<Display Name>"
    category: <category>
    subcategory: <subcategory>
    converter: <converter_type>
    mapping: Optimal  # or Identity for math-like data
    url: "<original_url>"
```

Place the entry in the correct section based on category (look for existing entries of same type).

### 6. Generate Thumbnail

Run thumbnail generation for the new source:

```bash
cd data_walker_rs
cargo run --release -- thumbnail --source <id>
```

Or regenerate all thumbnails if that command doesn't exist:
```bash
cargo run --release -- thumbnails
```

### 7. Verify Integration

1. Run the GUI and verify the new source appears in the list
2. Select it and verify the walk renders correctly
3. Check thumbnail appears in gallery

```bash
cargo run --release -- gui
```

## Examples

### Add whale song from Archive.org

```
/add-source https://archive.org/download/whale-songs/humpback_whale.mp3 --name "Humpback Whale" --subcategory "Whales"
```

### Add DNA sequence from NCBI

```
/add-source https://www.ncbi.nlm.nih.gov/nuccore/NC_045512.2?report=fasta --name "SARS-CoV-2" --subcategory "Coronaviruses"
```

### Add gravitational wave data

```
/add-source https://gwosc.org/eventapi/json/GWTC-1-confident/GW150914/v3/H-H1_GWOSC_4KHZ_R1-1126259447-32.txt.gz --name "GW150914 H1" --subcategory "LIGO Events"
```

## Supported Source Types

### Audio (ESC-50, Archive.org, Xeno-canto)
- Formats: MP3, WAV
- Min size: 10KB
- Converts via FFT spectrogram to base-12

### DNA (NCBI GenBank)
- Formats: FASTA
- Min size: 100 bytes
- Converts ACGT -> base-4 -> base-12

### Cosmos (LIGO/GWOSC)
- Formats: .txt.gz strain data
- Min size: 1KB
- Converts strain amplitude to base-12

### Finance (Yahoo Finance)
- Usually downloaded via yfinance API, not direct URL
- Use `cargo run -- download --source <symbol>` instead

## Error Handling

- **404/URL not accessible**: Verify URL is correct and publicly accessible
- **Unknown file type**: Specify `--category` explicitly
- **Duplicate ID**: Choose different `--id` or modify existing
- **Download failed**: Check network, try again, verify URL allows direct download
- **Invalid file**: Check file isn't HTML error page, verify source authenticity

## Notes

- Always verify data is from a legitimate source
- Include proper attribution in the URL field
- For audio, prefer MP3 for smaller file sizes
- For DNA, NCBI accession numbers are preferred in URLs
