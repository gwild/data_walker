//! Data Walker - Rust Implementation
//!
//! CLI commands:
//! - gui: Launch native 3D viewer
//! - generate-thumbnails: Render walk thumbnails
//! - generate-math: Generate math-based walks
//! - list: List available sources
//! - download: Download data from sources

mod audio;
mod automation;
mod config;
mod converters;
mod download;
mod gui;
mod logging;
mod thumbnail;
mod walk;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "data_walker")]
#[command(about = "3D walk visualizations from real-world data")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to sources.yaml config
    #[arg(short, long, default_value = "sources.yaml")]
    config: PathBuf,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch native GUI viewer
    Gui {
        /// Auto-select sources on startup (comma-separated IDs)
        #[arg(long)]
        select: Option<String>,

        /// Auto-enable flight mode
        #[arg(long)]
        flight: bool,

        /// Auto-start flight playback
        #[arg(long)]
        play: bool,

        /// Quit after N seconds
        #[arg(long)]
        quit_after: Option<f64>,

        /// Take screenshot and quit (specify output path)
        #[arg(long)]
        screenshot: Option<String>,

        /// Enable IPC server on this port
        #[arg(long, default_value = "0")]
        ipc_port: u16,

        /// Log all GUI events as JSON to stdout
        #[arg(long)]
        json_events: bool,
    },

    /// Generate math-based walks (no downloads needed)
    GenerateMath {
        /// Output directory
        #[arg(short, long, default_value = "data/math")]
        output: PathBuf,
    },

    /// List available sources
    List {
        /// Filter by category
        #[arg(short, long)]
        category: Option<String>,
    },

    /// Generate thumbnail images for all sources
    GenerateThumbnails {
        /// Output directory for thumbnails
        #[arg(short, long, default_value = "thumbnails")]
        output: PathBuf,

        /// Thumbnail size in pixels
        #[arg(long, default_value = "512")]
        size: u32,
    },

    /// Download data from sources
    Download {
        /// Download specific source by ID
        #[arg(long)]
        source: Option<String>,

        /// Download all sources in category
        #[arg(long)]
        category: Option<String>,

        /// Download all sources
        #[arg(long)]
        all: bool,
    },

    /// Search and download from Freesound
    Freesound {
        #[command(subcommand)]
        action: FreesoundAction,
    },

    /// Test MIDI note extraction from audio file
    TestNotes {
        /// Audio file to analyze (WAV or MP3)
        file: PathBuf,

        /// Number of notes to display
        #[arg(short, long, default_value = "50")]
        count: usize,
    },
}

#[derive(Subcommand)]
enum FreesoundAction {
    /// Search for sounds
    Search {
        /// Search query
        query: String,

        /// Maximum results
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },

    /// Download a sound by ID
    Download {
        /// Freesound sound ID
        sound_id: u64,

        /// Output source ID (filename without extension)
        #[arg(short, long)]
        id: Option<String>,

        /// Add to sources.yaml with this name
        #[arg(short, long)]
        name: Option<String>,

        /// Category for sources.yaml
        #[arg(long, default_value = "audio")]
        category: String,

        /// Subcategory for sources.yaml
        #[arg(long)]
        subcategory: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file (look in current dir and parent)
    let _ = dotenvy::from_filename(".env")
        .or_else(|_| dotenvy::from_filename("../.env"));

    // Initialize logging first
    logging::init_logging("logs");
    tracing::info!("Data Walker starting up");

    // Install panic handler to log crashes with full backtrace
    std::panic::set_hook(Box::new(|panic_info| {
        let backtrace = std::backtrace::Backtrace::capture();
        let location = panic_info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_else(|| "unknown".to_string());
        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        tracing::error!("PANIC at {}: {}", location, message);
        tracing::error!("Backtrace:\n{}", backtrace);
        eprintln!("\n=== CRASH DETECTED ===");
        eprintln!("Location: {}", location);
        eprintln!("Message: {}", message);
        eprintln!("Backtrace:\n{}", backtrace);
        eprintln!("=== Check logs/ for full details ===\n");
    }));

    let cli = Cli::parse();
    tracing::debug!("CLI args parsed: config={:?}", cli.config);

    // Load config
    let config = if cli.config.exists() {
        tracing::info!("Loading config from {:?}", cli.config);
        config::Config::load(&cli.config)?
    } else {
        tracing::warn!("Config file not found: {:?}, using defaults", cli.config);
        default_config()
    };
    tracing::info!("Config loaded: {} sources, {} mappings",
        config.sources.len(), config.mappings.len());

    // Load secrets
    let secrets = config::Secrets::load();

    match cli.command {
        Commands::Gui { select, flight, play, quit_after, screenshot, ipc_port, json_events } => {
            // Kill any existing instance first
            kill_existing_instances();

            // Build automation config
            let auto_config = automation::AutomationConfig {
                auto_select: select.map(|s| s.split(',').map(|x| x.trim().to_string()).collect()).unwrap_or_default(),
                auto_flight: flight,
                auto_play: play,
                quit_after_secs: quit_after.unwrap_or(0.0),
                screenshot_and_quit: screenshot,
                ipc_port,
                json_events,
            };

            if !auto_config.auto_select.is_empty() {
                tracing::info!("Auto-selecting sources: {:?}", auto_config.auto_select);
            }
            if auto_config.ipc_port > 0 {
                tracing::info!("IPC server will listen on port {}", auto_config.ipc_port);
            }

            tracing::info!("Launching native GUI viewer");
            gui::run_viewer(config, auto_config)?;
        }

        Commands::GenerateMath { output } => {
            generate_math(&config, &output)?;
        }

        Commands::GenerateThumbnails { output, size } => {
            thumbnail::generate(&config, &output, size)?;
        }

        Commands::List { category } => {
            list_sources(&config, category.as_deref());
        }

        Commands::Download { source, category, all } => {
            let data_dir = PathBuf::from(&secrets.data_dir);

            if all {
                download_all(&config, &data_dir).await?;
            } else if let Some(cat) = category {
                download_category(&config, &cat, &data_dir).await?;
            } else if let Some(id) = source {
                download_source(&config, &id, &data_dir).await?;
            } else {
                println!("Specify --source, --category, or --all");
            }
        }

        Commands::Freesound { action } => {
            match action {
                FreesoundAction::Search { query, limit } => {
                    println!("Searching Freesound for: {}", query);
                    let results = download::search_freesound(&query, limit).await?;
                    println!("\nFound {} results:\n", results.len());
                    println!("{:>8}  {:>6}  {:<30}  {:<20}  {}", "ID", "Dur(s)", "Name", "User", "License");
                    println!("{}", "-".repeat(100));
                    for r in results {
                        let license_short = r.license.rsplit('/').nth(1).unwrap_or(&r.license);
                        let name_disp = if r.name.len() > 30 {
                            format!("{}...", &r.name[..27])
                        } else {
                            r.name.clone()
                        };
                        let user_disp = if r.username.len() > 20 {
                            format!("{}...", &r.username[..17])
                        } else {
                            r.username.clone()
                        };
                        println!("{:>8}  {:>6.1}  {:<30}  {:<20}  {}",
                            r.id, r.duration, name_disp, user_disp, license_short
                        );
                    }
                    println!("\nTo download: cargo run -- freesound download <ID> --id <source_id> --name \"Display Name\"");
                }
                FreesoundAction::Download { sound_id, id, name, category, subcategory } => {
                    let output_id = id.unwrap_or_else(|| format!("freesound_{}", sound_id));
                    let output_dir = PathBuf::from("data/audio");

                    let path = download::download_freesound(sound_id, &output_id, &output_dir).await?;
                    println!("Downloaded to: {:?}", path);

                    // Add to sources.yaml if name provided
                    if let Some(display_name) = name {
                        let subcat = subcategory.unwrap_or_else(|| "Freesound".to_string());
                        println!("\nAdd this to sources.yaml:\n");
                        println!("  - id: {}", output_id);
                        println!("    name: \"{}\"", display_name);
                        println!("    category: {}", category);
                        println!("    subcategory: {}", subcat);
                        println!("    converter: audio");
                        println!("    mapping: Optimal");
                        println!("    url: \"https://freesound.org/s/{}\"", sound_id);
                    }
                }
            }
        }

        Commands::TestNotes { file, count } => {
            test_note_extraction(&file, count)?;
        }
    }

    Ok(())
}

/// Generate all math walks - now computed on-the-fly in GUI
/// This command is kept for compatibility but math is computed during plotting
fn generate_math(config: &config::Config, _output: &PathBuf) -> anyhow::Result<()> {
    let math_sources: Vec<_> = config
        .sources
        .iter()
        .filter(|s| s.converter.starts_with("math."))
        .collect();

    println!("Math sources ({}) are now computed on-the-fly during plotting:", math_sources.len());
    for source in &math_sources {
        println!("  - {} ({})", source.name, source.converter);
    }
    println!("\nNo files generated - math data is computed when needed.");
    Ok(())
}

/// List available sources
fn list_sources(config: &config::Config, category: Option<&str>) {
    let sources: Vec<_> = if let Some(cat) = category {
        config.sources.iter().filter(|s| s.category == cat).collect()
    } else {
        config.sources.iter().collect()
    };

    println!("Available sources ({}):", sources.len());
    println!();

    let mut current_cat = String::new();
    for source in sources {
        if source.category != current_cat {
            current_cat = source.category.clone();
            println!("## {}", config.categories.get(&current_cat).unwrap_or(&current_cat));
        }
        println!("  - {} [{}] ({})", source.name, source.id, source.converter);
    }
}

/// Download all available sources
async fn download_all(config: &config::Config, data_dir: &PathBuf) -> anyhow::Result<()> {
    println!("Downloading all sources to {:?}...", data_dir);
    println!();

    // Group by converter type
    let mut dna_sources = vec![];
    let mut math_sources = vec![];
    let mut finance_sources = vec![];
    let mut audio_sources = vec![];
    let mut cosmos_sources = vec![];
    let mut skipped = vec![];

    for source in &config.sources {
        if source.converter == "dna" {
            dna_sources.push(source);
        } else if source.converter.starts_with("math.") {
            math_sources.push(source);
        } else if source.converter == "finance" {
            finance_sources.push(source);
        } else if source.converter == "audio" {
            audio_sources.push(source);
        } else if source.converter == "cosmos" {
            cosmos_sources.push(source);
        } else {
            skipped.push(source);
        }
    }

    // Math sources are computed on-the-fly during plotting - no download needed
    if !math_sources.is_empty() {
        println!("=== MATH ({} sources) ===", math_sources.len());
        println!("  Math data is computed on-the-fly during plotting.");
        for source in &math_sources {
            println!("  [COMPUTED] {}", source.name);
        }
        println!();
    }

    // Download DNA from NCBI - stores raw FASTA files
    if !dna_sources.is_empty() {
        println!("=== DNA ({} sources) ===", dna_sources.len());
        let dna_dir = data_dir.join("dna");
        std::fs::create_dir_all(&dna_dir)?;

        for source in &dna_sources {
            // Extract accession from URL
            let accession = source.url
                .split('/')
                .last()
                .unwrap_or(&source.id);

            match download::download_dna(accession, &dna_dir).await {
                Ok(path) => {
                    println!("  [OK] {} -> {:?}", source.name, path);
                }
                Err(e) => {
                    println!("  [FAIL] {}: {}", source.name, e);
                }
            }

            // Rate limit: NCBI requests 3 requests/second max
            tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
        }
        println!();
    }

    // Download Finance from Yahoo - stores raw price data
    if !finance_sources.is_empty() {
        println!("=== FINANCE ({} sources) ===", finance_sources.len());
        let finance_dir = data_dir.join("finance");
        std::fs::create_dir_all(&finance_dir)?;

        for source in &finance_sources {
            // Extract symbol from URL (e.g., "https://finance.yahoo.com/quote/BTC-USD" -> "BTC-USD")
            let symbol = source.url
                .split('/')
                .last()
                .unwrap_or(&source.id)
                .replace("%5E", "^"); // Handle encoded ^ for indices

            match download::download_finance(&symbol, &finance_dir).await {
                Ok(path) => {
                    println!("  [OK] {} -> {:?}", source.name, path);
                }
                Err(e) => {
                    println!("  [FAIL] {}: {}", source.name, e);
                }
            }

            // Rate limit
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
        println!();
    }

    // Download Audio from ESC-50 and other sources - stores raw WAV/MP3 files
    if !audio_sources.is_empty() {
        println!("=== AUDIO ({} sources) ===", audio_sources.len());
        let audio_dir = data_dir.join("audio");
        std::fs::create_dir_all(&audio_dir)?;

        for source in &audio_sources {
            match download::download_audio(&source.id, &source.url, &audio_dir).await {
                Ok(path) => {
                    println!("  [OK] {} -> {:?}", source.name, path);
                }
                Err(e) => {
                    println!("  [SKIP] {}: {}", source.name, e);
                }
            }

            // Rate limit for GitHub
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        }
        println!();
    }

    // Download Cosmos from GWOSC - stores raw strain data (.txt.gz)
    if !cosmos_sources.is_empty() {
        println!("=== COSMOS ({} sources) ===", cosmos_sources.len());
        let cosmos_dir = data_dir.join("cosmos");
        std::fs::create_dir_all(&cosmos_dir)?;

        for source in &cosmos_sources {
            match download::download_cosmos(&source.id, &source.url, &cosmos_dir).await {
                Ok(path) => {
                    println!("  [OK] {} -> {:?}", source.name, path);
                }
                Err(e) => {
                    println!("  [FAIL] {}: {}", source.name, e);
                }
            }

            // Rate limit
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
        println!();
    }

    // Report skipped
    if !skipped.is_empty() {
        println!("=== SKIPPED ({} sources - not implemented) ===", skipped.len());
        for source in &skipped {
            println!("  - {} ({})", source.name, source.converter);
        }
        println!();
    }

    println!("Download complete!");
    Ok(())
}

/// Download sources in a category
async fn download_category(config: &config::Config, category: &str, data_dir: &PathBuf) -> anyhow::Result<()> {
    let sources: Vec<_> = config.sources.iter()
        .filter(|s| s.category == category)
        .collect();

    if sources.is_empty() {
        println!("No sources in category '{}'", category);
        return Ok(());
    }

    println!("Downloading {} sources in category '{}'...", sources.len(), category);

    for source in sources {
        download_source(config, &source.id, data_dir).await?;
    }

    Ok(())
}

/// Download a single source - stores RAW data only
async fn download_source(config: &config::Config, id: &str, data_dir: &PathBuf) -> anyhow::Result<()> {
    let source = config.sources.iter()
        .find(|s| s.id == id)
        .ok_or_else(|| anyhow::anyhow!("Source not found: {}", id))?;

    println!("Downloading: {} ({})", source.name, source.converter);

    match source.converter.as_str() {
        "dna" => {
            let dna_dir = data_dir.join("dna");
            std::fs::create_dir_all(&dna_dir)?;
            let accession = source.url.split('/').last().unwrap_or(&source.id);
            let path = download::download_dna(accession, &dna_dir).await?;
            println!("  Saved raw FASTA to {:?}", path);
        }
        converter if converter.starts_with("math.") => {
            // Math is computed on-the-fly during plotting
            println!("  Math data is computed on-the-fly - no download needed");
        }
        "finance" => {
            let finance_dir = data_dir.join("finance");
            std::fs::create_dir_all(&finance_dir)?;
            let symbol = source.url.split('/').last().unwrap_or(&source.id).replace("%5E", "^");
            let path = download::download_finance(&symbol, &finance_dir).await?;
            println!("  Saved raw prices to {:?}", path);
        }
        "audio" => {
            let audio_dir = data_dir.join("audio");
            std::fs::create_dir_all(&audio_dir)?;
            match download::download_audio(&source.id, &source.url, &audio_dir).await {
                Ok(path) => println!("  Saved raw audio to {:?}", path),
                Err(e) => println!("  Skipped: {}", e),
            }
        }
        "cosmos" => {
            let cosmos_dir = data_dir.join("cosmos");
            std::fs::create_dir_all(&cosmos_dir)?;
            match download::download_cosmos(&source.id, &source.url, &cosmos_dir).await {
                Ok(path) => println!("  Saved raw strain data to {:?}", path),
                Err(e) => println!("  Failed: {}", e),
            }
        }
        other => {
            println!("  Converter '{}' not implemented yet", other);
        }
    }

    Ok(())
}

/// Default config when no file exists
fn default_config() -> config::Config {
    use std::collections::HashMap;

    let mut mappings = HashMap::new();
    mappings.insert("Identity".to_string(), vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    mappings.insert("Optimal".to_string(), vec![0, 1, 2, 3, 4, 5, 6, 7, 10, 9, 8, 11]);
    mappings.insert("Spiral".to_string(), vec![0, 2, 4, 6, 8, 10, 1, 3, 5, 7, 9, 11]);
    mappings.insert("Stock-opt".to_string(), vec![1, 0, 2, 4, 10, 5, 6, 9, 8, 7, 3, 11]);

    let mut categories = HashMap::new();
    categories.insert("math".to_string(), "Math".to_string());

    let sources = vec![
        config::Source {
            id: "pi".to_string(),
            name: "Pi".to_string(),
            category: "math".to_string(),
            subcategory: "Constants".to_string(),
            converter: "math.constant.pi".to_string(),
            mapping: "Identity".to_string(),
            url: "computed://mpmath".to_string(),
        },
        config::Source {
            id: "e".to_string(),
            name: "Euler's Number (e)".to_string(),
            category: "math".to_string(),
            subcategory: "Constants".to_string(),
            converter: "math.constant.e".to_string(),
            mapping: "Identity".to_string(),
            url: "computed://mpmath".to_string(),
        },
        config::Source {
            id: "dragon_curve".to_string(),
            name: "Dragon Curve".to_string(),
            category: "math".to_string(),
            subcategory: "Fractals".to_string(),
            converter: "math.fractal.dragon".to_string(),
            mapping: "Identity".to_string(),
            url: "computed://lsystem".to_string(),
        },
    ];

    config::Config {
        mappings,
        mappings_base6: HashMap::new(),
        categories,
        converters: HashMap::new(),
        sources,
    }
}

/// Kill any existing data_walker GUI instances (Windows)
fn kill_existing_instances() {
    use std::process::Command;

    // Get current process ID to avoid killing ourselves
    let current_pid = std::process::id();

    // On Windows, use PowerShell for reliable process killing
    #[cfg(windows)]
    {
        // Use PowerShell to get and kill data_walker processes
        let script = format!(
            "Get-Process -Name 'data_walker' -ErrorAction SilentlyContinue | Where-Object {{ $_.Id -ne {} }} | Stop-Process -Force",
            current_pid
        );

        let result = Command::new("powershell")
            .args(["-Command", &script])
            .output();

        match result {
            Ok(output) => {
                if !output.stderr.is_empty() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if !stderr.contains("Cannot find a process") {
                        tracing::debug!("PowerShell stderr: {}", stderr);
                    }
                }
                tracing::info!("Killed any existing data_walker instances");
            }
            Err(e) => {
                tracing::warn!("Failed to kill existing instances: {}", e);
            }
        }
    }

    // On Unix, kill other data_walker gui instances (skip our own PID)
    #[cfg(unix)]
    {
        let _ = Command::new("bash")
            .args(["-c", &format!(
                "pgrep -f 'data_walker.*gui' | grep -v {} | xargs -r kill",
                current_pid
            )])
            .output();
    }

    // Small delay to let old process fully exit
    std::thread::sleep(std::time::Duration::from_millis(200));
}

/// Test MIDI note extraction from an audio file
fn test_note_extraction(file: &PathBuf, count: usize) -> anyhow::Result<()> {
    use converters::load_audio_midi_notes;

    println!("Testing note extraction from: {:?}", file);
    println!();

    let notes = load_audio_midi_notes(file)?;

    println!("Extracted {} notes total, showing first {}:", notes.len(), count.min(notes.len()));
    println!();
    println!("{:>4}  {:>4}  {:>6}  {:>8}  {}", "#", "MIDI", "Note", "Freq(Hz)", "Velocity");
    println!("{}", "-".repeat(42));

    // Count note frequencies for summary
    let mut note_counts: std::collections::HashMap<u8, usize> = std::collections::HashMap::new();

    for (i, note) in notes.iter().enumerate() {
        *note_counts.entry(note.note).or_insert(0) += 1;

        if i < count {
            let freq = 440.0 * 2.0_f32.powf((note.note as f32 - 69.0) / 12.0);
            println!("{:>4}  {:>4}  {:>6}  {:>8.1}  {:.2}",
                i + 1, note.note, note.name(), freq, note.velocity);
        }
    }

    // Show note distribution
    println!();
    println!("Note distribution (top 12):");
    println!("{}", "-".repeat(30));

    let mut sorted_notes: Vec<_> = note_counts.into_iter().collect();
    sorted_notes.sort_by(|a, b| b.1.cmp(&a.1));

    for (midi, count) in sorted_notes.iter().take(12) {
        let note = converters::audio::MidiNote { note: *midi, velocity: 1.0 };
        let bar_len = (*count as f32 / notes.len() as f32 * 30.0) as usize;
        let bar = "█".repeat(bar_len);
        println!("{:>3} {:>4}: {} ({:.1}%)",
            note.name(), midi, bar, *count as f32 / notes.len() as f32 * 100.0);
    }

    // For Bach Prelude in C, we expect mostly C, E, G notes
    println!();
    println!("For Bach Prelude in C Major, expect predominantly:");
    println!("  C (MIDI 48, 60, 72, 84) - root");
    println!("  E (MIDI 52, 64, 76) - major third");
    println!("  G (MIDI 55, 67, 79) - perfect fifth");

    Ok(())
}
