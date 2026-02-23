//! Application State - Single Source of Truth (SSOT)
//!
//! Manages all loaded walk data and provides thread-safe access.

use crate::config::{Config, Mapping};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Walk data: base-12 sequence + metadata
#[derive(Debug, Clone)]
pub struct WalkData {
    pub id: String,
    pub name: String,
    pub category: String,
    pub subcategory: String,
    pub mapping: String,
    pub url: String,
    pub base12: Vec<u8>,
}

/// Application state shared across all requests
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub walks: Arc<RwLock<HashMap<String, WalkData>>>,
    pub data_dir: String,
}

impl AppState {
    /// Create new app state from config
    pub fn new(config: Config, data_dir: String) -> Self {
        Self {
            config: Arc::new(config),
            walks: Arc::new(RwLock::new(HashMap::new())),
            data_dir,
        }
    }

    /// Get a mapping by name
    pub fn get_mapping(&self, name: &str) -> Mapping {
        self.config.get_mapping(name)
    }

    /// Load a walk's base12 data
    pub async fn load_walk(&self, id: &str) -> Option<WalkData> {
        tracing::debug!("load_walk called for id='{}'", id);

        // Check cache first
        {
            let walks = self.walks.read().await;
            if let Some(walk) = walks.get(id) {
                tracing::debug!("Walk '{}' found in cache", id);
                return Some(walk.clone());
            }
        }
        tracing::debug!("Walk '{}' not in cache, looking up source", id);

        // Look up source in config
        let source = match self.config.get_source(id) {
            Some(s) => {
                tracing::debug!("Source '{}' found: converter={}", id, s.converter);
                s
            }
            None => {
                tracing::warn!("Source '{}' NOT found in config", id);
                return None;
            }
        };

        // Check if it's a math source (computed, no file needed)
        if source.converter.starts_with("math.") {
            tracing::info!("Generating math walk '{}' using converter '{}'", id, source.converter);
            let generator = match crate::converters::math::MathGenerator::from_converter_string(&source.converter) {
                Some(g) => g,
                None => {
                    tracing::error!("Failed to create MathGenerator for converter '{}'", source.converter);
                    return None;
                }
            };

            let base12 = generator.generate(5000);
            tracing::debug!("Generated {} base12 digits for '{}'", base12.len(), id);

            let walk = WalkData {
                id: source.id.clone(),
                name: source.name.clone(),
                category: source.category.clone(),
                subcategory: source.subcategory.clone(),
                mapping: source.mapping.clone(),
                url: source.url.clone(),
                base12,
            };

            // Cache it
            {
                let mut walks = self.walks.write().await;
                walks.insert(id.to_string(), walk.clone());
                tracing::debug!("Walk '{}' cached", id);
            }

            return Some(walk);
        }

        // Non-math sources: try to download
        let base12 = match source.converter.as_str() {
            "dna" => {
                // Extract accession from URL
                let accession = extract_ncbi_accession(&source.url);
                tracing::info!("Downloading DNA for '{}' accession: {}", id, accession);
                let output_dir = std::path::PathBuf::from(&self.data_dir).join("dna");
                match crate::download::download_dna(&accession, &output_dir).await {
                    Ok(data) => data,
                    Err(e) => {
                        tracing::error!("Failed to download DNA '{}': {}", id, e);
                        return None;
                    }
                }
            }
            "audio" => {
                tracing::error!("Audio converter not yet implemented for '{}'", id);
                return None;
            }
            "cosmos" => {
                tracing::error!("Cosmos converter not yet implemented for '{}'", id);
                return None;
            }
            "finance" => {
                tracing::error!("Finance converter not yet implemented for '{}'", id);
                return None;
            }
            other => {
                tracing::error!("Unknown converter '{}' for source '{}'", other, id);
                return None;
            }
        };

        let walk = WalkData {
            id: source.id.clone(),
            name: source.name.clone(),
            category: source.category.clone(),
            subcategory: source.subcategory.clone(),
            mapping: source.mapping.clone(),
            url: source.url.clone(),
            base12,
        };

        // Cache it
        {
            let mut walks = self.walks.write().await;
            walks.insert(id.to_string(), walk.clone());
            tracing::debug!("Walk '{}' cached", id);
        }

        Some(walk)
    }

    /// Get all available walk IDs
    pub fn walk_ids(&self) -> Vec<String> {
        self.config.sources.iter().map(|s| s.id.clone()).collect()
    }

    /// Get walks by category
    pub fn walks_by_category(&self, category: &str) -> Vec<&crate::config::Source> {
        self.config.sources_by_category(category)
    }

    /// Get all categories
    pub fn categories(&self) -> Vec<String> {
        self.config.categories.keys().cloned().collect()
    }
}

/// Walk metadata for listing (without base12 data)
#[derive(Debug, Clone, serde::Serialize)]
pub struct WalkMeta {
    pub id: String,
    pub name: String,
    pub category: String,
    pub subcategory: String,
    pub mapping: String,
    pub url: String,
}

impl From<&crate::config::Source> for WalkMeta {
    fn from(s: &crate::config::Source) -> Self {
        Self {
            id: s.id.clone(),
            name: s.name.clone(),
            category: s.category.clone(),
            subcategory: s.subcategory.clone(),
            mapping: s.mapping.clone(),
            url: s.url.clone(),
        }
    }
}

/// Extract NCBI accession number from URL
/// e.g., "https://www.ncbi.nlm.nih.gov/nuccore/NC_045512.2" -> "NC_045512.2"
fn extract_ncbi_accession(url: &str) -> String {
    url.rsplit('/')
        .next()
        .unwrap_or(url)
        .to_string()
}
