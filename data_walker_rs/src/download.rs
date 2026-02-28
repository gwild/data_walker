//! Download CLI commands - fetch RAW data from sources
//!
//! IMPORTANT: This module downloads and stores RAW DATA ONLY.
//! All base-12 conversion happens on-the-fly during plotting.
//!
//! Raw data formats:
//! - DNA: .fasta files (ACGT sequence)
//! - Audio: .wav or .mp3 files
//! - Cosmos: .txt.gz files (strain values)
//! - Finance: .json files (raw price arrays)

use anyhow::Result;
use std::path::PathBuf;

/// Download DNA sequence from NCBI GenBank - stores RAW FASTA
pub async fn download_dna(accession: &str, output_dir: &PathBuf) -> Result<PathBuf> {
    tracing::info!("Downloading DNA sequence: {}", accession);

    let url = format!(
        "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi?db=nuccore&id={}&rettype=fasta&retmode=text",
        accession
    );

    tracing::debug!("Fetching from: {}", url);

    let client = reqwest::Client::new();
    let response = client.get(&url)
        .header("User-Agent", "DataWalker/0.1 (github.com/data-walker)")
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("NCBI returned status {}", response.status());
    }

    let fasta = response.text().await?;
    tracing::debug!("Downloaded {} bytes of FASTA data", fasta.len());

    // Save RAW FASTA file
    std::fs::create_dir_all(output_dir)?;
    let path = output_dir.join(format!("{}.fasta", accession.replace(".", "_")));
    std::fs::write(&path, &fasta)?;
    tracing::info!("Saved raw FASTA to {:?}", path);

    Ok(path)
}

/// Download audio - stores RAW audio file (WAV or MP3)
pub async fn download_audio(id: &str, url: &str, output_dir: &PathBuf) -> Result<PathBuf> {
    tracing::info!("Downloading audio: {} from {}", id, url);

    // ESC-50 audio mappings
    let esc50_files: std::collections::HashMap<&str, &str> = [
        ("dog", "1-100032-A-0.wav"),
        ("cat", "1-34094-A-5.wav"),
        ("crow", "1-103298-A-9.wav"),
        ("insects_buzzing", "1-17585-A-7.wav"),
        ("crickets", "1-57316-A-13.wav"),
        ("chirping_birds", "1-100038-A-14.wav"),
        ("tree_frog_1", "1-15689-A-4.wav"),
        ("tree_frog_2", "1-15689-B-4.wav"),
        ("tree_frog_3", "1-17970-A-4.wav"),
        ("water_frog", "1-18755-A-4.wav"),
        ("pacific_chorus", "1-18755-B-4.wav"),
        ("swamp_frog", "1-18757-A-4.wav"),
        ("tropical_frog", "1-31836-A-4.wav"),
        ("edible_frog", "1-31836-B-4.wav"),
        ("heavy_frogs", "2-32515-A-4.wav"),
        ("rain", "1-17367-A-10.wav"),
        ("sea_waves", "1-28135-A-11.wav"),
        ("fire", "1-17150-A-12.wav"),
        ("wind", "1-137296-A-16.wav"),
        ("thunder", "1-101296-A-19.wav"),
    ].into_iter().collect();

    std::fs::create_dir_all(output_dir)?;

    // ESC-50 sources - download WAV
    if url.contains("ESC-50") {
        if let Some(&filename) = esc50_files.get(id) {
            let audio_url = format!(
                "https://github.com/karolpiczak/ESC-50/raw/master/audio/{}",
                filename
            );

            tracing::debug!("Fetching ESC-50 audio: {}", audio_url);

            let client = reqwest::Client::new();
            let response = client.get(&audio_url)
                .header("User-Agent", "DataWalker/0.1")
                .send()
                .await?;

            if !response.status().is_success() {
                anyhow::bail!("Failed to download audio: {}", response.status());
            }

            let wav_data = response.bytes().await?;
            tracing::debug!("Downloaded {} bytes of WAV data", wav_data.len());

            // Save RAW WAV file
            let path = output_dir.join(format!("{}.wav", id));
            std::fs::write(&path, &wav_data)?;
            tracing::info!("Saved raw WAV to {:?}", path);

            return Ok(path);
        }
    }

    // Whale sounds from Archive.org
    let whale_sounds: std::collections::HashMap<&str, &str> = [
        ("whale_humpback", "https://archive.org/download/whale-songs-whale-sound-effects/Humpback%20Whale-SoundBible.com-93645231.mp3"),
        ("whale_blue", "https://archive.org/download/whale-songs-whale-sound-effects/lowwhalesong-33955.mp3"),
        ("whale_orca", "https://archive.org/download/whale-songs-whale-sound-effects/killer-whale.mp3"),
        ("whale_sperm", "https://archive.org/download/whale-songs-whale-sound-effects/whale_song.mp3"),
        ("whale_beluga", "https://archive.org/download/whale-songs-whale-sound-effects/beluga-whale.mp3"),
    ].into_iter().collect();

    if let Some(&mp3_url) = whale_sounds.get(id) {
        return download_raw_mp3(id, mp3_url, output_dir).await;
    }

    // Archive.org bird sounds
    let archive_birds: std::collections::HashMap<&str, &str> = [
        ("forest_birds", "https://archive.org/download/various-bird-sounds/Various%20Bird%20Sounds.mp3"),
        ("sea_birds", "https://archive.org/download/various-bird-sounds/NatureSounds.mp3"),
        ("amazon_jungle", "https://archive.org/download/various-bird-sounds/Lyrebird%20plus%20others-June%202017.mp3"),
        ("birds_forest_sunny", "https://archive.org/download/various-bird-sounds/El%20Sot%20de%20l%27infern.mp3"),
        ("dawn_chorus", "https://archive.org/download/various-bird-sounds/PajaritosUno.mp3"),
        ("florida_birds", "https://archive.org/download/various-bird-sounds/FloridaBirds.mp3"),
        ("garden_birds", "https://archive.org/download/various-bird-sounds/Back_Garden_Jan_11.mp3"),
    ].into_iter().collect();

    if let Some(&mp3_url) = archive_birds.get(id) {
        return download_raw_mp3(id, mp3_url, output_dir).await;
    }

    // Archive.org indigenous music
    let archive_indigenous: std::collections::HashMap<&str, &str> = [
        ("karaja_solo", "https://archive.org/download/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr/disc1/01.01.%20Solo%20Song%2C%20Man.mp3"),
        ("karaja_dance", "https://archive.org/download/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr/disc1/01.02.%20Jahave%20%28Sacred%20Masked%20Dance%2C%20Songs%2C%20%22Aruana%22%2C%20Two%20Masks%20Dancing%29.mp3"),
        ("karaja_choir", "https://archive.org/download/lp_anthology-of-brazilian-indian-music_various-javahe-juruna-karaja-kraho-suya-tr/disc1/01.05.%20Boys%20And%20Girls%20Choir.mp3"),
    ].into_iter().collect();

    if let Some(&mp3_url) = archive_indigenous.get(id) {
        return download_raw_mp3(id, mp3_url, output_dir).await;
    }

    // Classical music from Archive.org - UNIQUE URLs for each piece
    let classical_composers: std::collections::HashMap<&str, &str> = [
        // Bach - each piece has its own unique recording
        ("bach_prelude_c", "https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko%20Ishizaka%20-%20Bach-%20Well-Tempered%20Clavier%2C%20Book%201%20-%2001%20Prelude%20No.%201%20in%20C%20major%2C%20BWV%20846.mp3"),
        ("bach_fugue_c", "https://archive.org/download/bach-well-tempered-clavier-book-1/Kimiko%20Ishizaka%20-%20Bach-%20Well-Tempered%20Clavier%2C%20Book%201%20-%2004%20Fugue%20No.%202%20in%20C%20minor%2C%20BWV%20847.mp3"),
        ("bach_invention", "https://archive.org/download/BachInventionNo.1/Bach%20invention%20no.1.mp3"),
        ("bach_toccata", "https://archive.org/download/ToccataAndFugueInDMinorBWV565/Toccata%20and%20Fugue%20in%20D%20Minor%2C%20BWV%20565.mp3"),
        // Beethoven - each piece has its own unique recording
        ("beethoven_elise", "https://archive.org/download/beethoven-fur-elise/Beethoven%20-%20F%C3%BCr%20Elise%20.mp3"),
        ("beethoven_moonlight", "https://archive.org/download/MoonlightSonata_845/Sonata_no_14_in_c_sharp_minor_moonlight_op_27_no_2_Iii.Presto.mp3"),
        ("beethoven_ode", "https://archive.org/download/lp_beethoven-ode-to-joy_arturo-toscanini-nbc-symphony-orchestra/disc1/01.01.%20Fourth%20Movement%3A%20Presto%20%28Part%201%29.mp3"),
        ("beethoven_5th", "https://archive.org/download/LudwigVanBeethovenSymphonyNo.5Full/Ludwig%20van%20Beethoven%20-%20Symphony%20No.%205%20%5BFull%5D.mp3"),
        ("beethoven_pathetique", "https://archive.org/download/BeethovenPathetiqueSonata/Beethoven__Piano_Sonata_Pathetique__Arthur_Rubenstein.mp3"),
        // Schoenberg
        ("schoenberg_suite", "https://archive.org/download/lp_piano-music_arnold-schoenberg-jurg-von-vintschger/disc1/02.01.%20Side%202%3A%205%20Piano%20Pieces%2C%20Op.%2023%3A%20No.%201%3B%20No.%202%3B%20No.%203%3B%20No.%204%3B%20No.%205%3B%20Suite%20For%20Piano%2C%20Op.%2025%3A%20Praeludium%3B%20Gavotte%20-%20Musette%20-%20Gavotte%3B%20Intermezzo%3B%20Menuett%3B%20Gigue.mp3"),
        ("schoenberg_variations", "https://archive.org/download/musicofarnoldsch00scho/03_Three_little_orchestra_pieces__1910.mp3"),
        ("schoenberg_quartet", "https://archive.org/download/lp_quintet-for-wind-instruments-op-26_arnold-schoenberg-philadelphia-woodwind-qu/disc1/01.01.%20Quintet%20For%20Wind%20Instruments%2C%20Op.%2026%3A%20I%20-%20Schwungvoll.mp3"),
        ("schoenberg_verklarte", "https://archive.org/download/lp_schoenberg-transfigured-night-verklarte-na_arnold-schoenberg-charles-martin-loeffler/disc1/01.01.%20Transfigured%20Night%20%28Verklarte%20Nacht%2C%20Op.%204%29.mp3"),
        ("schoenberg_pierrot", "https://archive.org/download/musicofarnoldsch00scho/01_Pelleas_and_Melisande.mp3"),
    ].into_iter().collect();

    if let Some(&mp3_url) = classical_composers.get(id) {
        return download_raw_mp3(id, mp3_url, output_dir).await;
    }

    anyhow::bail!("Audio source '{}' requires manual download from: {}", id, url)
}

/// Download raw MP3 file (no conversion)
async fn download_raw_mp3(id: &str, url: &str, output_dir: &PathBuf) -> Result<PathBuf> {
    tracing::info!("Downloading MP3: {} from {}", id, url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()?;

    let response = client.get(url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) DataWalker/0.1")
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("Failed to download {}: {} from {}", id, response.status(), url);
    }

    let mp3_data = response.bytes().await?;
    tracing::debug!("Downloaded {} bytes of MP3", mp3_data.len());

    // Save RAW MP3 file
    let path = output_dir.join(format!("{}.mp3", id));
    std::fs::write(&path, &mp3_data)?;
    tracing::info!("Saved raw MP3 to {:?}", path);

    Ok(path)
}

/// Download gravitational wave data from GWOSC - stores RAW strain data
pub async fn download_cosmos(id: &str, url: &str, output_dir: &PathBuf) -> Result<PathBuf> {
    tracing::info!("Downloading LIGO data: {} from {}", id, url);

    std::fs::create_dir_all(output_dir)?;
    let raw_path = output_dir.join(format!("{}.txt.gz", id));

    // Get actual data URL from GWOSC event API
    let data_url = get_gwosc_data_url(id, url).await?;
    tracing::debug!("Fetching from: {}", data_url);

    let client = reqwest::Client::new();
    let response = client.get(&data_url)
        .header("User-Agent", "DataWalker/0.1 (github.com/data-walker)")
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("GWOSC returned status {} for {}", response.status(), id);
    }

    let bytes = response.bytes().await?;
    tracing::debug!("Downloaded {} bytes", bytes.len());

    // Save RAW gzipped strain data
    std::fs::write(&raw_path, &bytes)?;
    tracing::info!("Saved raw strain data to {:?}", raw_path);

    Ok(raw_path)
}

/// Get the actual data URL from GWOSC event API
async fn get_gwosc_data_url(id: &str, url: &str) -> Result<String> {
    if url.ends_with(".txt.gz") || url.ends_with(".hdf5") {
        return Ok(url.to_string());
    }

    let event = url.trim_end_matches('/')
        .rsplit('/')
        .nth(1)
        .unwrap_or(id);

    let detector = if id.ends_with("_h1") || id.contains("H1") {
        "H1"
    } else if id.ends_with("_l1") || id.contains("L1") {
        "L1"
    } else {
        "H1"
    };

    let api_url = format!(
        "https://gwosc.org/eventapi/json/GWTC-1-confident/{}/v3/",
        event.to_uppercase()
    );

    tracing::debug!("Querying GWOSC API: {}", api_url);

    let client = reqwest::Client::new();
    let response = client.get(&api_url)
        .header("User-Agent", "DataWalker/0.1")
        .send()
        .await;

    if let Ok(resp) = response {
        if resp.status().is_success() {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(strain) = json.get("strain") {
                    for (key, value) in strain.as_object().unwrap_or(&serde_json::Map::new()) {
                        if key.contains(detector) {
                            if let Some(url) = value.get("url").and_then(|u| u.as_str()) {
                                if url.ends_with(".txt.gz") {
                                    return Ok(url.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback URLs
    let fallback = match (event.to_uppercase().as_str(), detector) {
        ("GW150914", "H1") => "https://gwosc.org/eventapi/json/GWTC-1-confident/GW150914/v3/H-H1_GWOSC_4KHZ_R1-1126259447-32.txt.gz",
        ("GW150914", "L1") => "https://gwosc.org/eventapi/json/GWTC-1-confident/GW150914/v3/L-L1_GWOSC_4KHZ_R1-1126259447-32.txt.gz",
        _ => anyhow::bail!("No data URL found for event {} detector {}", event, detector),
    };

    Ok(fallback.to_string())
}

/// Download stock data from Yahoo Finance - stores RAW price data
pub async fn download_finance(symbol: &str, output_dir: &PathBuf) -> Result<PathBuf> {
    tracing::info!("Downloading stock data: {}", symbol);

    let url = format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=5y",
        symbol
    );

    tracing::debug!("Fetching from: {}", url);

    let client = reqwest::Client::new();
    let response = client.get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("Yahoo Finance returned status {}", response.status());
    }

    let json: serde_json::Value = response.json().await?;

    // Extract raw price data
    let prices: Vec<f64> = json["chart"]["result"][0]["indicators"]["quote"][0]["close"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Failed to parse price data"))?
        .iter()
        .filter_map(|v| v.as_f64())
        .collect();

    let timestamps: Vec<i64> = json["chart"]["result"][0]["timestamp"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
        .unwrap_or_default();

    tracing::debug!("Got {} price points", prices.len());

    if prices.len() < 2 {
        anyhow::bail!("Not enough price data");
    }

    // Save RAW price data (not base12)
    std::fs::create_dir_all(output_dir)?;
    let path = output_dir.join(format!("{}.json", symbol.replace("^", "").replace("-", "_")));
    let data = serde_json::json!({
        "symbol": symbol,
        "prices": prices,
        "timestamps": timestamps,
        "source": url,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&data)?)?;
    tracing::info!("Saved raw prices to {:?}", path);

    Ok(path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_placeholder() {
        // Tests moved to converters module
        assert!(true);
    }
}
