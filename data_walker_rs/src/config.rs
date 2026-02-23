//! Configuration loader - YAML manifest + .env secrets

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use anyhow::Result;

/// Base-12 mapping (permutation of 0-11)
pub type Mapping = [u8; 12];

/// Main configuration loaded from sources.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub mappings: HashMap<String, Vec<u8>>,
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
    pub yahoo_api_key: Option<String>,
    pub data_dir: String,
    pub port: u16,
}

impl Config {
    /// Load configuration from YAML file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Get a mapping by name, returns Identity if not found
    pub fn get_mapping(&self, name: &str) -> Mapping {
        self.mappings
            .get(name)
            .map(|v| {
                let mut arr = [0u8; 12];
                for (i, &val) in v.iter().take(12).enumerate() {
                    arr[i] = val;
                }
                arr
            })
            .unwrap_or([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11])
    }

    /// Get source by ID
    pub fn get_source(&self, id: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }

    /// Get all sources in a category
    pub fn sources_by_category(&self, category: &str) -> Vec<&Source> {
        self.sources.iter().filter(|s| s.category == category).collect()
    }
}

impl Secrets {
    /// Load secrets from .env file
    pub fn load() -> Self {
        dotenvy::dotenv().ok();

        Secrets {
            yahoo_api_key: std::env::var("YAHOO_API_KEY").ok(),
            data_dir: std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string()),
            port: std::env::var("PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(8080),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_mapping() {
        let config = Config {
            mappings: HashMap::new(),
            categories: HashMap::new(),
            converters: HashMap::new(),
            sources: vec![],
        };
        let identity = config.get_mapping("NonExistent");
        assert_eq!(identity, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    }
}
