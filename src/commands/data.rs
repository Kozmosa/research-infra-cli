use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::db::Database;
use crate::error::{RcliError, Result};
use crate::repo::Repository;

pub fn register(
    repo: &Repository,
    path: &str,
    name: &str,
    desc: Option<String>,
    checksum: Option<String>,
) -> Result<()> {
    let abs_path = if Path::new(path).is_relative() {
        repo.root.join(path)
    } else {
        Path::new(path).to_path_buf()
    };

    if !abs_path.exists() {
        return Err(RcliError::DataNotFound(path.to_string()));
    }

    let checksum = match checksum {
        Some(c) => c,
        None => compute_dir_checksum(&abs_path)?,
    };

    let relative_path = abs_path
        .strip_prefix(&repo.root)
        .unwrap_or(&abs_path)
        .to_string_lossy()
        .to_string();

    let registered_at = chrono::Utc::now().to_rfc3339();

    let index_path = repo.data_index_path();
    let mut datasets = load_data_index(&index_path)?;

    if datasets.iter().any(|d| d.name == name) {
        return Err(RcliError::DataAlreadyExists(name.to_string()));
    }

    let dataset = crate::db::Dataset {
        name: name.to_string(),
        path: relative_path,
        checksum: Some(checksum),
        description: desc,
        registered_at,
    };
    datasets.push(dataset.clone());

    save_data_index(&index_path, &datasets)?;

    let db = Database::open(&repo.db_path())?;
    db.insert_dataset(
        name,
        &dataset.path,
        dataset.checksum.as_deref(),
        dataset.description.as_deref(),
        &dataset.registered_at,
    )?;

    Ok(())
}

pub fn list(repo: &Repository) -> Result<Vec<crate::db::Dataset>> {
    load_data_index(&repo.data_index_path())
}

pub fn info(repo: &Repository, name: &str) -> Result<crate::db::Dataset> {
    let datasets = load_data_index(&repo.data_index_path())?;
    datasets
        .into_iter()
        .find(|d| d.name == name)
        .ok_or_else(|| RcliError::DataNotFound(name.to_string()))
}

pub fn update(
    repo: &Repository,
    name: &str,
    new_path: Option<String>,
    recompute_checksum: bool,
) -> Result<()> {
    let mut datasets = load_data_index(&repo.data_index_path())?;
    let idx = datasets
        .iter()
        .position(|d| d.name == name)
        .ok_or_else(|| RcliError::DataNotFound(name.to_string()))?;

    if let Some(path) = new_path {
        let abs_path = if Path::new(&path).is_relative() {
            repo.root.join(&path)
        } else {
            Path::new(&path).to_path_buf()
        };
        let relative_path = abs_path
            .strip_prefix(&repo.root)
            .unwrap_or(&abs_path)
            .to_string_lossy()
            .to_string();
        datasets[idx].path = relative_path;
    }

    if recompute_checksum {
        let abs_path = if Path::new(&datasets[idx].path).is_relative() {
            repo.root.join(&datasets[idx].path)
        } else {
            Path::new(&datasets[idx].path).to_path_buf()
        };
        datasets[idx].checksum = Some(compute_dir_checksum(&abs_path)?);
    }

    datasets[idx].registered_at = chrono::Utc::now().to_rfc3339();

    save_data_index(&repo.data_index_path(), &datasets)?;

    let db = Database::open(&repo.db_path())?;
    db.insert_dataset(
        name,
        &datasets[idx].path,
        datasets[idx].checksum.as_deref(),
        datasets[idx].description.as_deref(),
        &datasets[idx].registered_at,
    )?;

    Ok(())
}

pub fn load_data_index(path: &Path) -> Result<Vec<crate::db::Dataset>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    let datasets: Vec<crate::db::Dataset> = serde_yaml::from_str(&content).unwrap_or_default();
    Ok(datasets)
}

pub fn save_data_index(path: &Path, datasets: &[crate::db::Dataset]) -> Result<()> {
    let content = serde_yaml::to_string(datasets)?;
    fs::write(path, content)?;
    Ok(())
}

fn compute_dir_checksum(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();

    let mut entries: Vec<_> = WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().to_path_buf())
        .collect();

    entries.sort();

    for entry in entries {
        let relative = entry.strip_prefix(path).unwrap_or(&entry);
        hasher.update(relative.to_string_lossy().as_bytes());
        let content = fs::read(&entry)?;
        hasher.update(&content);
    }

    let result = format!("{:x}", hasher.finalize());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::db::Database;
    use crate::repo::Repository;

    fn create_test_repo() -> (Repository, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        fs::create_dir_all(root.join(".research")).unwrap();
        fs::create_dir_all(root.join("data/raw")).unwrap();

        let config = Config::default();
        config.save(&root.join(".research/config.yaml")).unwrap();

        let db = Database::open(&root.join(".research/research.db")).unwrap();
        db.init_schema().unwrap();

        (
            Repository {
                root: root.to_path_buf(),
            },
            dir,
        )
    }

    #[test]
    fn test_register_new_dataset() {
        let (repo, _dir) = create_test_repo();
        let data_path = repo.root.join("data/raw");
        fs::write(data_path.join("test.txt"), "hello").unwrap();

        register(
            &repo,
            "data/raw",
            "test-data",
            Some("desc".to_string()),
            None,
        )
        .unwrap();

        let datasets = list(&repo).unwrap();
        assert_eq!(datasets.len(), 1);
        assert_eq!(datasets[0].name, "test-data");
        assert_eq!(datasets[0].path, "data/raw");
        assert_eq!(datasets[0].description, Some("desc".to_string()));
    }

    #[test]
    fn test_register_duplicate_fails() {
        let (repo, _dir) = create_test_repo();
        let data_path = repo.root.join("data/raw");
        fs::write(data_path.join("test.txt"), "hello").unwrap();

        register(&repo, "data/raw", "test-data", None, None).unwrap();

        let result = register(
            &repo,
            "data/raw",
            "test-data",
            Some("new desc".to_string()),
            None,
        );
        assert!(matches!(result, Err(RcliError::DataAlreadyExists(_))));

        let datasets = list(&repo).unwrap();
        assert_eq!(datasets.len(), 1);
        assert_eq!(datasets[0].description, None);
    }

    #[test]
    fn test_register_duplicate_does_not_modify_sqlite() {
        let (repo, _dir) = create_test_repo();
        let data_path = repo.root.join("data/raw");
        fs::write(data_path.join("test.txt"), "hello").unwrap();

        register(
            &repo,
            "data/raw",
            "test-data",
            Some("original".to_string()),
            None,
        )
        .unwrap();

        let result = register(
            &repo,
            "data/raw",
            "test-data",
            Some("new desc".to_string()),
            None,
        );
        assert!(matches!(result, Err(RcliError::DataAlreadyExists(_))));

        // Verify SQLite cache matches YAML source (not the failed attempt)
        let db = Database::open(&repo.db_path()).unwrap();
        let ds = db.get_dataset("test-data").unwrap().unwrap();
        assert_eq!(ds.description, Some("original".to_string()));
    }

    #[test]
    fn test_info_and_update() {
        let (repo, _dir) = create_test_repo();
        let data_path = repo.root.join("data/raw");
        fs::write(data_path.join("test.txt"), "hello").unwrap();

        register(
            &repo,
            "data/raw",
            "test-data",
            Some("original".to_string()),
            None,
        )
        .unwrap();

        let ds = info(&repo, "test-data").unwrap();
        assert_eq!(ds.name, "test-data");

        update(&repo, "test-data", Some("data/new".to_string()), false).unwrap();

        let ds = info(&repo, "test-data").unwrap();
        assert_eq!(ds.path, "data/new");
    }

    #[test]
    fn test_update_nonexistent_fails() {
        let (repo, _dir) = create_test_repo();
        let result = update(&repo, "missing", Some("data/new".to_string()), false);
        assert!(matches!(result, Err(RcliError::DataNotFound(_))));
    }

    #[test]
    fn test_list_returns_all_datasets() {
        let (repo, _dir) = create_test_repo();
        let data_path = repo.root.join("data/raw");
        fs::write(data_path.join("a.txt"), "a").unwrap();
        fs::write(data_path.join("b.txt"), "b").unwrap();

        register(
            &repo,
            "data/raw",
            "dataset-a",
            Some("desc-a".to_string()),
            None,
        )
        .unwrap();
        register(
            &repo,
            "data/raw",
            "dataset-b",
            Some("desc-b".to_string()),
            None,
        )
        .unwrap();

        let datasets = list(&repo).unwrap();
        assert_eq!(datasets.len(), 2);
        assert!(datasets.iter().any(|d| d.name == "dataset-a"));
        assert!(datasets.iter().any(|d| d.name == "dataset-b"));
    }
}
