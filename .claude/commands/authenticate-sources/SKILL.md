---
name: authenticate-sources
description: Verify all raw source files are authentic and URLs are valid. Use when user says "authenticate sources", "validate data", "check sources", "verify downloads", or invokes /authenticate-sources. Supports --all, --category, --source, and --fix options.
metadata:
  author: Data Walker
  version: 1.0.0
---

# Authenticate Sources

Verify all raw source files are authentic and URLs are valid.

## Usage

```
/authenticate-sources [options]
```

## Options

- `--all` - Check all sources (default)
- `--category <name>` - Check only sources in a category
- `--source <id>` - Check a specific source
- `--fix` - Attempt to fix broken URLs by searching for alternatives

## Instructions

### Step 1: URL Validation

For each source in `sources.yaml`, verify the source URL is still accessible:
```bash
cd data_walker_rs && cargo run -- list
```
Then check each URL with HTTP HEAD requests to verify they're still valid.

### Step 2: File Existence Check

Verify downloaded files exist in `data_walker_rs/data/`:
- `data/audio/` - Check for `.wav` and `.mp3` files
- `data/dna/` - Check for `.fasta` files
- `data/cosmos/` - Check for `.txt.gz` files
- `data/finance/` - Check for `.json` files
- `data/proteins/` - Check for `.pdb` files

### Step 3: Duplicate Detection

Check for duplicate files (same content with different names):
```bash
cd data_walker_rs/data/audio && md5sum *.mp3 *.wav 2>/dev/null | sort | uniq -d -w32
```

### Step 4: File Size Validation

Flag suspiciously small files that may be error pages or corrupted:
- Audio files < 10KB are suspect
- DNA files < 100 bytes are suspect
- Cosmos files < 1KB are suspect
- PDB files < 1KB are suspect

### Step 5: Download URL Verification

Check that the hardcoded URLs in `src/download.rs` match unique content:
- Each source ID should map to a unique URL
- No two sources should download the same file

## Report Format

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

## Troubleshooting

Error: Many URLs return 403/404
Cause: Network issues or rate limiting
Solution: Wait and retry, or check if sources have moved

Error: Duplicate content detected
Cause: Two sources point to the same file
Solution: Find unique source files or remove duplicates from sources.yaml
