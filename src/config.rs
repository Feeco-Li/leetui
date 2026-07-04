use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub workspace_dir: String,
    pub language: String,
    pub editor: String,
    #[serde(default)]
    pub leetcode_session: Option<String>,
    #[serde(default)]
    pub csrf_token: Option<String>,
    #[serde(default)]
    pub cf_clearance: Option<String>,
    #[serde(default)]
    pub user_agent: Option<String>,
    #[serde(default)]
    pub solutions_repo_url: Option<String>,
}

impl Config {
    pub fn is_authenticated(&self) -> bool {
        self.leetcode_session.as_ref().is_some_and(|s| !s.is_empty())
            && self.csrf_token.as_ref().is_some_and(|s| !s.is_empty())
    }

    pub fn config_dir() -> PathBuf {
        dirs::home_dir()
            .expect("Could not find home directory")
            .join(".leetcode-cli")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn cache_path() -> PathBuf {
        Self::config_dir().join("problems.json")
    }

    pub fn load() -> Result<Option<Config>> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(None);
        }
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        let config: Config =
            toml::from_str(&contents).with_context(|| "Failed to parse config.toml")?;
        Ok(Some(config))
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create config dir {}", dir.display()))?;
        let path = Self::config_path();
        let contents =
            toml::to_string_pretty(self).with_context(|| "Failed to serialize config")?;
        std::fs::write(&path, contents)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }

    pub fn expanded_workspace(&self) -> PathBuf {
        let expanded = if self.workspace_dir.starts_with('~') {
            let home = dirs::home_dir().expect("Could not find home directory");
            home.join(self.workspace_dir.strip_prefix("~/").unwrap_or(""))
        } else {
            PathBuf::from(&self.workspace_dir)
        };
        expanded
    }
}
