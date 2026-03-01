# Authenticate Sources

Verify all raw source files are authentic and URLs are valid.

## Usage

```
/auth [options]
```

## Options

- `--all` - Check all sources (default)
- `--category <name>` - Check only sources in a category
- `--source <id>` - Check a specific source
- `--fix` - Attempt to fix broken URLs by searching for alternatives

## Instructions

Perform the following checks on data sources:

### 1. URL Validation
For each source in `sources.yaml`, verify the source URL is still accessible:
```bash
cd data_walker_rs && cargo run -- list
```
Then check each URL with HTTP HEAD requests to verify they're still valid.

### 2. File Existence Check
Verify downloaded files exist in `data_walker_rs/data/`:
- `data/audio/` - Check for `.wav` and `.mp3` files
- `data/dna/` - Check for `.fasta` files
- `data/cosmos/` - Check for `.txt.gz` files
- `data/finance/` - Check for `.json` files

### 3. Duplicate Detection
Check for duplicate files (same content with different names):
```bash
cd data_walker_rs/data/audio && md5sum *.mp3 *.wav 2>/dev/null | sort | uniq -d -w32
```

### 4. File Size Validation
Flag suspiciously small files that may be error pages or corrupted:
- Audio files < 10KB are suspect
- DNA files < 100 bytes are suspect
- Cosmos files < 1KB are suspect

### 5. Download URL Verification
Check that the hardcoded URLs in `src/download.rs` match unique content:
- Each source ID should map to a unique URL
- No two sources should download the same file

## Report Format

Generate a report with:
```
=== SOURCE AUTHENTICATION REPORT ===

VALID SOURCES: X
MISSING FILES: X
BROKEN URLS: X
DUPLICATE FILES: X
SUSPECT FILES: X

--- ISSUES ---
[MISSING] source_id - file not found
[BROKEN_URL] source_id - HTTP 404
[DUPLICATE] source_id1, source_id2 - same file content
[SUSPECT] source_id - file too small (X bytes)
```

## Fixing Issues

If `--fix` is specified:
1. For broken URLs, search Archive.org for alternatives
2. For duplicates, find unique source files
3. Re-download fixed sources

## Examples

Check all sources:
```
/auth
```

Check only audio:
```
/auth --category audio
```

Check and fix issues:
```
/auth --fix
```
