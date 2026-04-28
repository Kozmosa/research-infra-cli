use chrono::DateTime;
use serde::Serialize;

use crate::config::Config;
use crate::db::Database;
use crate::error::{RcliError, Result};
use crate::repo::Repository;

#[derive(Serialize)]
pub struct EnvStatus {
    pub repo_root: String,
    pub git: GitInfo,
    pub active_experiments: Vec<ActiveExp>,
    pub data_assets: Vec<String>,
    pub config: Config,
}

#[derive(Serialize)]
pub struct GitInfo {
    pub branch: String,
    pub commit_hash: String,
    pub is_clean: bool,
    pub last_commit_time: Option<String>,
}

#[derive(Serialize)]
pub struct ActiveExp {
    pub id: String,
    pub status: String,
    pub start_time: Option<String>,
}

pub fn status(repo: &Repository) -> Result<EnvStatus> {
    let git_info = get_git_info(repo)?;
    let db = Database::open(&repo.db_path())?;

    let active = db.list_active_experiments()?;
    let active_experiments: Vec<ActiveExp> = active.into_iter().map(|e| ActiveExp {
        id: e.id,
        status: e.status,
        start_time: None,
    }).collect();

    let data_index_path = repo.data_index_path();
    let data_assets = if data_index_path.exists() {
        let content = std::fs::read_to_string(&data_index_path)?;
        let datasets: Vec<crate::db::Dataset> = serde_yaml::from_str(&content).unwrap_or_default();
        datasets.into_iter().map(|d| d.name).collect()
    } else {
        Vec::new()
    };

    let config = Config::load(&repo.config_path())?;

    Ok(EnvStatus {
        repo_root: repo.root.to_string_lossy().to_string(),
        git: git_info,
        active_experiments,
        data_assets,
        config,
    })
}

pub fn check(repo: &Repository, strict: bool) -> Result<()> {
    if strict {
        let git_info = get_git_info(repo)?;
        if !git_info.is_clean {
            return Err(RcliError::WorkspaceNotClean);
        }
    }
    Ok(())
}

fn get_git_info(repo: &Repository) -> Result<GitInfo> {
    let git_repo = git2::Repository::open(&repo.root)?;

    let (branch, commit_hash, last_commit_time) = match git_repo.head() {
        Ok(head) => {
            let branch = head.shorthand().unwrap_or("unknown").to_string();
            match head.peel_to_commit() {
                Ok(commit) => {
                    let hash = commit.id().to_string();
                    let time = commit.time().seconds();
                    let time_str = if time > 0 {
                        DateTime::from_timestamp(time, 0).map(|d| d.to_rfc3339())
                    } else {
                        None
                    };
                    (branch, hash, time_str)
                }
                Err(_) => (branch, String::new(), None),
            }
        }
        Err(_) => ("main".to_string(), String::new(), None),
    };

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true);
    let statuses = git_repo.statuses(Some(&mut opts))?;
    let is_clean = statuses.is_empty();

    Ok(GitInfo {
        branch,
        commit_hash,
        is_clean,
        last_commit_time,
    })
}
