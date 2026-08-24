//! Configuration management (XDG + serde + TOML).
//!
//! Stores user preferences at `$XDG_CONFIG_HOME/miracast-client/config.toml`.

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub streaming: StreamingConfig,
    pub advanced: AdvancedConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub minimize_to_tray: bool,
    pub start_minimized: bool,
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamingConfig {
    pub video_quality: String,
    pub frame_rate: u32,
    pub audio_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AdvancedConfig {
    pub discovery_timeout_secs: u64,
    pub connection_timeout_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            streaming: StreamingConfig::default(),
            advanced: AdvancedConfig::default(),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            minimize_to_tray: true,
            start_minimized: false,
            log_level: "info".to_string(),
        }
    }
}

impl Default for StreamingConfig {
    fn default() -> Self {
        Self {
            video_quality: "High".to_string(),
            frame_rate: 30,
            audio_enabled: true,
        }
    }
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            discovery_timeout_secs: 10,
            connection_timeout_secs: 15,
        }
    }
}

/// Configuration manager.
pub struct ConfigManager {
    config: AppConfig,
    config_path: PathBuf,
}

impl ConfigManager {
    /// Load or create the configuration.
    pub fn load() -> Result<Self> {
        let config_path = Self::config_file_path();

        let config = if config_path.exists() {
            debug!("Loading config from {}", config_path.display());
            let content = fs::read_to_string(&config_path)?;
            toml::from_str(&content).unwrap_or_else(|e| {
                warn!("Failed to parse config: {e}, using defaults");
                AppConfig::default()
            })
        } else {
            info!("No config found, creating defaults at {}", config_path.display());
            let config = AppConfig::default();
            // Create directory and save defaults
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let content = toml::to_string_pretty(&config)?;
            fs::write(&config_path, content)?;
            config
        };

        Ok(Self { config, config_path })
    }

    /// Get reference to the current configuration.
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Get mutable reference to the configuration.
    pub fn config_mut(&mut self) -> &mut AppConfig {
        &mut self.config
    }

    /// Save the current configuration to disk.
    pub fn save(&self) -> Result<()> {
        let content = toml::to_string_pretty(&self.config)?;
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.config_path, &content)?;
        debug!("Config saved to {}", self.config_path.display());
        Ok(())
    }

    /// Get the XDG config file path.
    fn config_file_path() -> PathBuf {
        if let Some(proj_dirs) =
            ProjectDirs::from("com", "github.eddypepy", "MiracastClient")
        {
            proj_dirs.config_dir().join("config.toml")
        } else {
            // Fallback
            PathBuf::from(".config/miracast-client/config.toml")
        }
    }

    /// Get the XDG data directory path (for session history, etc.).
    pub fn data_dir() -> PathBuf {
        if let Some(proj_dirs) =
            ProjectDirs::from("com", "github.eddypepy", "MiracastClient")
        {
            proj_dirs.data_dir().to_path_buf()
        } else {
            PathBuf::from(".local/share/miracast-client")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AppConfig::default();
        assert_eq!(config.streaming.frame_rate, 30);
        assert_eq!(config.streaming.video_quality, "High");
        assert!(config.streaming.audio_enabled);
        assert_eq!(config.advanced.discovery_timeout_secs, 10);
    }

    #[test]
    fn test_config_serialize_deserialize() {
        let config = AppConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.streaming.frame_rate, config.streaming.frame_rate);
    }

    #[test]
    fn test_config_partial_deserialize() {
        let toml_str = r#"
[streaming]
frame_rate = 60
"#;
        let config: AppConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.streaming.frame_rate, 60);
        // Other fields should have defaults
        assert_eq!(config.streaming.video_quality, "High");
        assert!(config.general.minimize_to_tray);
    }
}
