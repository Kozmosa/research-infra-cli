use std::env;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::{ArcliError, Result};

pub struct Repository {
    pub root: PathBuf,
}

impl Repository {
    pub fn discover(start_dir: Option<&Path>) -> Result<Self> {
        let start = match start_dir {
            Some(p) => p.to_path_buf(),
            None => env::current_dir().map_err(ArcliError::Io)?,
        };

        let mut current = start.as_path();
        loop {
            let research_dir = current.join(".research");
            if research_dir.is_dir() && research_dir.join("config.yaml").is_file() {
                return Ok(Repository {
                    root: current.to_path_buf(),
                });
            }

            match current.parent() {
                Some(parent) => current = parent,
                None => break,
            }
        }

        Err(ArcliError::RepoNotFound)
    }

    pub fn discover_or_current(start_dir: Option<&Path>) -> Self {
        match Self::discover(start_dir) {
            Ok(repo) => repo,
            Err(_) => {
                let root = match start_dir {
                    Some(p) => p.to_path_buf(),
                    None => env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                };
                Repository { root }
            }
        }
    }

    pub fn research_dir(&self) -> PathBuf {
        self.root.join(".research")
    }

    pub fn config_path(&self) -> PathBuf {
        self.research_dir().join("config.yaml")
    }

    pub fn db_path(&self) -> PathBuf {
        self.research_dir().join("research.db")
    }

    pub fn data_index_path(&self) -> PathBuf {
        self.research_dir().join("data_index.yaml")
    }

    pub fn claims_path(&self) -> PathBuf {
        self.research_dir().join("claims.yaml")
    }

    pub fn experiments_dir(&self) -> PathBuf {
        let config = Config::load(&self.config_path()).unwrap_or_default();
        self.root.join(&config.experiments_dir)
    }

    pub fn exp_dir(&self, exp_id: &str) -> PathBuf {
        self.experiments_dir().join(exp_id)
    }

    pub fn exp_json_path(&self, exp_id: &str) -> PathBuf {
        self.exp_dir(exp_id).join("experiment.json")
    }

    pub fn exp_log_path(&self, exp_id: &str) -> PathBuf {
        self.exp_dir(exp_id).join("logs").join("run.log")
    }
}
