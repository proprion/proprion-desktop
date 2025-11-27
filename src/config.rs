//! Configuration management for Proprion CLI.
//!
//! Config file location: ~/.config/proprion/config.toml

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

/// Provider configuration - different fields for different provider types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProviderConfig {
    #[serde(rename = "scaleway")]
    Scaleway(ScalewayProviderConfig),

    #[serde(rename = "exoscale")]
    Exoscale(ExoscaleProviderConfig),
}

/// Scaleway-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalewayProviderConfig {
    pub access_key: String,
    pub secret_key: String,
    pub organization_id: String,
    pub project_id: String,
    pub region: String,
    pub bucket: String,
}

/// Exoscale-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExoscaleProviderConfig {
    /// API key for Exoscale API
    pub api_key: String,
    /// API secret for Exoscale API
    pub api_secret: String,
    /// Zone (e.g., ch-gva-2, de-fra-1, ch-dk-2)
    pub zone: String,
    /// Bucket name
    pub bucket: String,
}

impl Config {
    /// Get the default config file path (OS-specific)
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = directories::ProjectDirs::from("org", "proprion", "proprion")
            .context("Could not determine config directory")?
            .config_dir()
            .to_path_buf();

        Ok(config_dir.join("config.toml"))
    }

    /// Get config file path, using custom path if provided
    pub fn path(custom_path: Option<&PathBuf>) -> Result<PathBuf> {
        match custom_path {
            Some(p) => Ok(p.clone()),
            None => Self::default_path(),
        }
    }

    /// Load config from file, or return empty config if file doesn't exist
    pub fn load(custom_path: Option<&PathBuf>) -> Result<Self> {
        let path = Self::path(custom_path)?;

        if !path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;

        Ok(config)
    }

    /// Save config to file
    pub fn save(&self, custom_path: Option<&PathBuf>) -> Result<()> {
        let path = Self::path(custom_path)?;

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;

        fs::write(&path, content)
            .with_context(|| format!("Failed to write config file: {}", path.display()))?;

        Ok(())
    }

    /// Get a provider by name
    pub fn get_provider(&self, name: &str) -> Option<&ProviderConfig> {
        self.providers.get(name)
    }

    /// Add or update a provider
    pub fn set_provider(&mut self, name: String, config: ProviderConfig) {
        self.providers.insert(name, config);
    }

    /// Remove a provider
    pub fn remove_provider(&mut self, name: &str) -> Option<ProviderConfig> {
        self.providers.remove(name)
    }

    /// List all provider names
    pub fn list_providers(&self) -> Vec<&String> {
        self.providers.keys().collect()
    }
}

impl ScalewayProviderConfig {
    /// Get the S3 endpoint URL
    pub fn endpoint(&self) -> String {
        format!("https://s3.{}.scw.cloud", self.region)
    }
}

impl ExoscaleProviderConfig {
    /// Get the S3 endpoint URL
    pub fn endpoint(&self) -> String {
        format!("https://sos-{}.exo.io", self.zone)
    }

    /// Get the API base URL for the zone
    pub fn api_base(&self) -> String {
        format!("https://api-{}.exoscale.com/v2", self.zone)
    }
}
