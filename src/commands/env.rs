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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::repo::Repository;
    use std::fs;

    fn create_test_repo() -> (Repository, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let _ = git2::Repository::init(&root);

        fs::create_dir_all(root.join(".research")).unwrap();
        fs::create_dir_all(root.join("data/raw")).unwrap();
        fs::create_dir_all(root.join("experiments")).unwrap();

        let config = crate::config::Config::default();
        config.save(&root.join(".research/config.yaml")).unwrap();

        let db = Database::open(&root.join(".research/research.db")).unwrap();
        db.init_schema().unwrap();

        // Create .gitignore to ignore SQLite temp files
        fs::write(root.join(".gitignore"), ".research/*.db*\n.research/*.db-wal\n.research/*.db-shm\n").unwrap();

        // Commit all files so workspace is clean
        let git_repo = git2::Repository::open(&root).unwrap();
        let sig = git2::Signature::new("test", "test@test.com", &git2::Time::new(0, 0)).unwrap();
        let mut index = git_repo.index().unwrap();
        index.add_all(["."], git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = git_repo.find_tree(tree_id).unwrap();
        git_repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        let repo = Repository { root: root.to_path_buf() };
        (repo, dir)
    }

    #[test]
    fn test_check_strict_requires_clean_workspace() {
        let (repo, _dir) = create_test_repo();

        // Clean workspace should pass
        check(&repo, true).unwrap();

        // Dirty workspace should fail
        fs::write(repo.root.join("dirty.txt"), "dirty").unwrap();
        let result = check(&repo, true);
        assert!(matches!(result, Err(RcliError::WorkspaceNotClean)));
    }

    #[test]
    fn test_check_non_strict_allows_dirty() {
        let (repo, _dir) = create_test_repo();

        fs::write(repo.root.join("dirty.txt"), "dirty").unwrap();
        check(&repo, false).unwrap();
    }

    #[test]
    fn test_status_returns_repo_info() {
        let (repo, _dir) = create_test_repo();

        let status = status(&repo).unwrap();
        assert!(status.repo_root.contains(".tmp"));
        assert_eq!(status.active_experiments.len(), 0);
        assert_eq!(status.data_assets.len(), 0);
        assert_eq!(status.config.project_name, "research-project");
    }
}
