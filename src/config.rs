use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{RcliError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_project_name")]
    pub project_name: String,
    #[serde(default = "default_exp_dir")]
    pub experiments_dir: String,
    #[serde(default)]
    pub templates: TemplatesConfig,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplatesConfig {
    #[serde(default = "default_templates_dir")]
    pub dir: String,
}

impl Default for TemplatesConfig {
    fn default() -> Self {
        TemplatesConfig {
            dir: default_templates_dir(),
        }
    }
}

fn default_project_name() -> String {
    "research-project".to_string()
}

fn default_exp_dir() -> String {
    "experiments".to_string()
}

fn default_templates_dir() -> String {
    "templates".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            project_name: default_project_name(),
            experiments_dir: default_exp_dir(),
            templates: TemplatesConfig::default(),
            extra: HashMap::new(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_yaml::to_string(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<String> {
        let parts: Vec<&str> = key.split('.').collect();
        match parts.as_slice() {
            ["project_name"] => Ok(self.project_name.clone()),
            ["experiments", "dir"] | ["experiments_dir"] => Ok(self.experiments_dir.clone()),
            ["templates", "dir"] => Ok(self.templates.dir.clone()),
            _ => {
                if parts.len() == 1 {
                    self.extra
                        .get(parts[0])
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .ok_or_else(|| RcliError::ConfigKeyNotFound(key.to_string()))
                } else {
                    Err(RcliError::ConfigKeyNotFound(key.to_string()))
                }
            }
        }
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        let parts: Vec<&str> = key.split('.').collect();
        match parts.as_slice() {
            ["project_name"] => self.project_name = value.to_string(),
            ["experiments", "dir"] | ["experiments_dir"] => self.experiments_dir = value.to_string(),
            ["templates", "dir"] => self.templates.dir = value.to_string(),
            _ => {
                if parts.len() == 1 {
                    self.extra.insert(
                        parts[0].to_string(),
                        serde_yaml::Value::String(value.to_string()),
                    );
                } else {
                    return Err(RcliError::ConfigKeyNotFound(key.to_string()));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.project_name, "research-project");
        assert_eq!(config.experiments_dir, "experiments");
        assert_eq!(config.templates.dir, "templates");
    }

    #[test]
    fn test_config_get_set_nested() {
        let mut config = Config::default();
        config.set("templates.dir", "custom-templates").unwrap();
        assert_eq!(config.get("templates.dir").unwrap(), "custom-templates");
    }

    #[test]
    fn test_config_get_missing_key() {
        let config = Config::default();
        let result = config.get("nonexistent.key");
        assert!(matches!(result, Err(RcliError::ConfigKeyNotFound(_))));
    }

    #[test]
    fn test_config_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");

        let mut config = Config {
            project_name: "test-project".to_string(),
            ..Default::default()
        };
        config.set("custom_key", "custom_value").unwrap();
        config.save(&path).unwrap();

        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.project_name, "test-project");
        assert_eq!(loaded.get("custom_key").unwrap(), "custom_value");
    }

    #[test]
    fn test_config_load_nonexistent() {
        let path = Path::new("/nonexistent/path/config.yaml");
        let config = Config::load(path).unwrap();
        assert_eq!(config.project_name, "research-project");
    }
}
