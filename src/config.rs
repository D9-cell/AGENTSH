use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub llm: LlmConfig,
    pub safety: SafetyConfig,
    pub agent: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct LlmConfig {
    pub base_url: String,
    pub model: String,
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct SafetyConfig {
    pub require_confirm: bool,
    pub auto_approve_safe: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AgentConfig {
    pub max_commands_per_turn: usize,
    pub context_lines: usize,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434/v1".to_string(),
            model: "llama3.1:8b".to_string(),
            timeout_secs: 120,
        }
    }
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            require_confirm: true,
            auto_approve_safe: false,
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_commands_per_turn: 8,
            context_lines: 40,
        }
    }
}

impl Config {
    pub fn agentsh_dir() -> Result<PathBuf> {
        let home = dirs::home_dir().context("failed to resolve home directory")?;
        Ok(home.join(".agentsh"))
    }

    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::agentsh_dir()?.join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if !path.exists() {
            Self::write_default_file(&path)?;
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config file at {}", path.display()))?;
        let config = toml::from_str::<Self>(&contents)
            .with_context(|| format!("failed to parse config file at {}", path.display()))?;
        Ok(config)
    }

    pub fn write_default_file(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create config directory {}", parent.display())
            })?;
        }

        let contents = toml::to_string_pretty(&Self::default())?;
        fs::write(path, contents)
            .with_context(|| format!("failed to write default config to {}", path.display()))
    }

    pub fn apply_overrides(&mut self, model: Option<String>, base_url: Option<String>) {
        if let Some(model) = model {
            self.llm.model = model;
        }

        if let Some(base_url) = base_url {
            self.llm.base_url = base_url;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let parsed: Config = toml::from_str(
            r#"
                [llm]
                model = "qwen2.5:14b"
            "#,
        )
        .unwrap();

        assert_eq!(parsed.llm.model, "qwen2.5:14b");
        assert_eq!(parsed.llm.base_url, Config::default().llm.base_url);
        assert_eq!(parsed.agent.context_lines, 40);
        assert!(parsed.safety.require_confirm);
    }
}