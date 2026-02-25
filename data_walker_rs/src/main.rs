//! Data Walker - Rust Implementation
//!
//! CLI commands:
//! - serve: Start HTTP server
//! - generate-math: Generate math-based walks
//! - list: List available sources
//! - download: Download data from sources

mod config;
mod converters;
mod download;
mod gui;
mod logging;
mod server;
mod state;
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
    /// Start HTTP server
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,
    },

    /// Launch native GUI viewer
    Gui,

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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging first
    logging::init_logging("logs");
    tracing::info!("Data Walker starting up");

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
        Commands::Serve { port } => {
            let state = state::AppState::new(config, secrets.data_dir);
            server::serve(state, port).await?;
        }

        Commands::Gui => {
            // Kill any existing instance first
            kill_existing_instances();
            tracing::info!("Launching native GUI viewer");
            gui::run_viewer(config)?;
        }

        Commands::GenerateMath { output } => {
            generate_math(&config, &output)?;
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
    }

    Ok(())
}

/// Generate all math walks
fn generate_math(config: &config::Config, output: &PathBuf) -> anyhow::Result<()> {
    use converters::math::MathGenerator;

    std::fs::create_dir_all(output)?;

    let math_sources: Vec<_> = config
        .sources
        .iter()
        .filter(|s| s.converter.starts_with("math."))
        .collect();

    println!("Generating {} math walks...", math_sources.len());

    for source in math_sources {
        if let Some(generator) = MathGenerator::from_converter_string(&source.converter) {
            let base12 = generator.generate(5000);
            let path = output.join(format!("{}.json", source.id));

            let data = serde_json::json!({
                "id": source.id,
                "name": source.name,
                "category": source.category,
                "subcategory": source.subcategory,
                "base12": base12,
            });

            std::fs::write(&path, serde_json::to_string_pretty(&data)?)?;
            println!("  {} -> {:?} ({} digits)", source.name, path, base12.len());
        }
    }

    println!("Done!");
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

    // Generate math (local computation)
    if !math_sources.is_empty() {
        println!("=== MATH ({} sources) ===", math_sources.len());
        let math_dir = data_dir.join("math");
        std::fs::create_dir_all(&math_dir)?;

        for source in &math_sources {
            if let Some(generator) = converters::math::MathGenerator::from_converter_string(&source.converter) {
                let base12 = generator.generate(10000);
                let path = math_dir.join(format!("{}.json", source.id));
                let data = serde_json::json!({
                    "id": source.id,
                    "name": source.name,
                    "base12": base12,
                    "source": "computed"
                });
                std::fs::write(&path, serde_json::to_string(&data)?)?;
                println!("  [OK] {} ({} digits)", source.name, base12.len());
            }
        }
        println!();
    }

    // Download DNA from NCBI
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
                Ok(base12) => {
                    println!("  [OK] {} ({} digits)", source.name, base12.len());
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

    // Download Finance from Yahoo
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
                Ok(base12) => {
                    println!("  [OK] {} ({} digits)", source.name, base12.len());
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

    // Download Audio from ESC-50 and other sources
    if !audio_sources.is_empty() {
        println!("=== AUDIO ({} sources) ===", audio_sources.len());
        let audio_dir = data_dir.join("audio");
        std::fs::create_dir_all(&audio_dir)?;

        for source in &audio_sources {
            match download::download_audio(&source.id, &source.url, &audio_dir).await {
                Ok(base12) => {
                    println!("  [OK] {} ({} digits)", source.name, base12.len());
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

    // Download Cosmos from GWOSC
    if !cosmos_sources.is_empty() {
        println!("=== COSMOS ({} sources) ===", cosmos_sources.len());
        let cosmos_dir = data_dir.join("cosmos");
        std::fs::create_dir_all(&cosmos_dir)?;

        for source in &cosmos_sources {
            match download::download_cosmos(&source.id, &source.url, &cosmos_dir).await {
                Ok(raw_path) => {
                    // Load and convert to base12
                    match download::load_cosmos_raw(&raw_path) {
                        Ok(base12) => println!("  [OK] {} ({} digits)", source.name, base12.len()),
                        Err(e) => println!("  [FAIL] {} (conversion): {}", source.name, e),
                    }
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

/// Download a single source
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
            let base12 = download::download_dna(accession, &dna_dir).await?;
            println!("  Downloaded {} base12 digits", base12.len());
        }
        converter if converter.starts_with("math.") => {
            let math_dir = data_dir.join("math");
            std::fs::create_dir_all(&math_dir)?;
            if let Some(generator) = converters::math::MathGenerator::from_converter_string(converter) {
                let base12 = generator.generate(10000);
                let path = math_dir.join(format!("{}.json", source.id));
                let data = serde_json::json!({
                    "id": source.id,
                    "name": source.name,
                    "base12": base12,
                    "source": "computed"
                });
                std::fs::write(&path, serde_json::to_string(&data)?)?;
                println!("  Generated {} base12 digits", base12.len());
            }
        }
        "finance" => {
            let finance_dir = data_dir.join("finance");
            std::fs::create_dir_all(&finance_dir)?;
            let symbol = source.url.split('/').last().unwrap_or(&source.id).replace("%5E", "^");
            let base12 = download::download_finance(&symbol, &finance_dir).await?;
            println!("  Downloaded {} base12 digits", base12.len());
        }
        "audio" => {
            let audio_dir = data_dir.join("audio");
            std::fs::create_dir_all(&audio_dir)?;
            match download::download_audio(&source.id, &source.url, &audio_dir).await {
                Ok(base12) => println!("  Downloaded {} base12 digits", base12.len()),
                Err(e) => println!("  Skipped: {}", e),
            }
        }
        "cosmos" => {
            let cosmos_dir = data_dir.join("cosmos");
            std::fs::create_dir_all(&cosmos_dir)?;
            match download::download_cosmos(&source.id, &source.url, &cosmos_dir).await {
                Ok(raw_path) => {
                    match download::load_cosmos_raw(&raw_path) {
                        Ok(base12) => println!("  Downloaded {} base12 digits", base12.len()),
                        Err(e) => println!("  Conversion failed: {}", e),
                    }
                }
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

    // On Unix, use pkill (but skip our own PID)
    #[cfg(unix)]
    {
        let _ = Command::new("pkill")
            .args(["-f", "data_walker.*gui"])
            .output();
    }

    // Small delay to let old process fully exit
    std::thread::sleep(std::time::Duration::from_millis(200));
}
