//! Configuration loader - YAML manifest + .env secrets

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Base-12 mapping (permutation of 0-11)
pub type Mapping = [u8; 12];

#[derive(Debug, Clone)]
pub struct DataPaths {
    root: PathBuf,
}

/// Main configuration loaded from sources.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub mappings: HashMap<String, Vec<u8>>,
    #[serde(default)]
    pub mappings_base6: HashMap<String, Vec<u8>>,
    pub categories: HashMap<String, String>,
    pub converters: HashMap<String, String>,
    pub sources: Vec<Source>,
}

/// A single data source definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub category: String,
    pub subcategory: String,
    pub converter: String,
    pub mapping: String,
    pub url: String,
}

/// Secrets loaded from .env
#[derive(Debug, Clone, Default)]
pub struct Secrets {
    pub data_dir: String,
}

impl Config {
    /// Load configuration from YAML file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Get a mapping by name.
    pub fn get_mapping(&self, name: &str) -> Result<Mapping> {
        let values = self
            .mappings
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Mapping '{}' not found in sources.yaml", name))?;

        if values.len() < 12 {
            bail!(
                "Mapping '{}' has {} entries, expected at least 12",
                name,
                values.len()
            );
        }

        let mut arr = [0u8; 12];
        for (i, &val) in values.iter().take(12).enumerate() {
            arr[i] = val;
        }
        Ok(arr)
    }

    /// Get a base-6 mapping by name.
    pub fn get_mapping_base6(&self, name: &str) -> Result<[u8; 6]> {
        let values = self
            .mappings_base6
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Base-6 mapping '{}' not found in sources.yaml", name))?;

        if values.len() < 6 {
            bail!(
                "Base-6 mapping '{}' has {} entries, expected at least 6",
                name,
                values.len()
            );
        }

        let mut arr = [0u8; 6];
        for (i, &val) in values.iter().take(6).enumerate() {
            arr[i] = val;
        }
        Ok(arr)
    }

}

impl DataPaths {
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        Self { root: root.into() }
    }

    pub fn audio_wav(&self, id: &str) -> PathBuf {
        self.root.join("audio").join(format!("{}.wav", id))
    }

    pub fn audio_mp3(&self, id: &str) -> PathBuf {
        self.root.join("audio").join(format!("{}.mp3", id))
    }

    pub fn audio_file(&self, id: &str) -> Option<PathBuf> {
        let wav_path = self.audio_wav(id);
        if wav_path.exists() {
            return Some(wav_path);
        }

        let mp3_path = self.audio_mp3(id);
        if mp3_path.exists() {
            return Some(mp3_path);
        }

        None
    }

    pub fn dna_file(&self, url: &str, id: &str) -> PathBuf {
        let accession = url.rsplit('/').next().unwrap_or(id);
        self.root
            .join("dna")
            .join(format!("{}.fasta", accession.replace(".", "_")))
    }

    pub fn cosmos_file(&self, id: &str) -> PathBuf {
        self.root.join("cosmos").join(format!("{}.txt.gz", id))
    }

    pub fn finance_file(&self, url: &str, id: &str) -> PathBuf {
        let symbol = url
            .split('/')
            .last()
            .unwrap_or(id)
            .replace("%5E", "^")
            .replace("^", "")
            .replace("-", "_");
        self.root.join("finance").join(format!("{}.json", symbol))
    }

    pub fn protein_file(&self, url: &str, id: &str) -> PathBuf {
        let pdb_id = url
            .rsplit('/')
            .next()
            .unwrap_or(id)
            .trim_end_matches(".pdb")
            .to_lowercase();
        self.root.join("proteins").join(format!("{}.pdb", pdb_id))
    }
}

impl Secrets {
    /// Load secrets from .env file
    pub fn load() -> Self {
        dotenvy::dotenv().ok();

        Secrets {
            data_dir: std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_mapping() {
        let mut mappings = HashMap::new();
        mappings.insert(
            "Identity".to_string(),
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        );
        let config = Config {
            mappings,
            mappings_base6: HashMap::new(),
            categories: HashMap::new(),
            converters: HashMap::new(),
            sources: vec![],
        };
        let identity = config.get_mapping("Identity").unwrap();
        assert_eq!(identity, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    }

    #[test]
    fn test_missing_mapping_is_error() {
        let config = Config {
            mappings: HashMap::new(),
            mappings_base6: HashMap::new(),
            categories: HashMap::new(),
            converters: HashMap::new(),
            sources: vec![],
        };
        assert!(config.get_mapping("NonExistent").is_err());
    }

    #[test]
    fn test_data_paths_build_expected_locations() {
        let paths = DataPaths::new("data");

        assert_eq!(paths.audio_wav("dog"), PathBuf::from("data").join("audio").join("dog.wav"));
        assert_eq!(
            paths.dna_file("https://www.ncbi.nlm.nih.gov/nuccore/NC_045512.2", "sars_cov2"),
            PathBuf::from("data").join("dna").join("NC_045512_2.fasta")
        );
        assert_eq!(
            paths.finance_file("https://finance.yahoo.com/quote/%5EGSPC", "sp500"),
            PathBuf::from("data").join("finance").join("GSPC.json")
        );
        assert_eq!(
            paths.protein_file("https://files.rcsb.org/download/1CRN.pdb", "crambin"),
            PathBuf::from("data").join("proteins").join("1crn.pdb")
        );
    }
}
