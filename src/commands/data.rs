use std::fs;
use std::path::Path;

use sha2::{Sha256, Digest};
use walkdir::WalkDir;

use crate::db::Database;
use crate::error::{RcliError, Result};
use crate::repo::Repository;

pub fn register(repo: &Repository, path: &str, name: &str, desc: Option<String>, checksum: Option<String>) -> Result<()> {
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

    let relative_path = abs_path.strip_prefix(&repo.root)
        .unwrap_or(&abs_path)
        .to_string_lossy()
        .to_string();

    let registered_at = chrono::Utc::now().to_rfc3339();

    let db = Database::open(&repo.db_path())?;
    db.insert_dataset(name, &relative_path, Some(&checksum), desc.as_deref(), &registered_at)?;

    let dataset = crate::db::Dataset {
        name: name.to_string(),
        path: relative_path,
        checksum: Some(checksum),
        description: desc,
        registered_at,
    };

    let index_path = repo.data_index_path();
    let mut datasets = load_data_index(&index_path)?;

    if datasets.iter().any(|d| d.name == name) {
        return Err(RcliError::DataAlreadyExists(name.to_string()));
    }
    datasets.push(dataset);

    save_data_index(&index_path, &datasets)?;

    Ok(())
}

pub fn list(repo: &Repository) -> Result<Vec<crate::db::Dataset>> {
    load_data_index(&repo.data_index_path())
}

pub fn info(repo: &Repository, name: &str) -> Result<crate::db::Dataset> {
    let datasets = load_data_index(&repo.data_index_path())?;
    datasets.into_iter()
        .find(|d| d.name == name)
        .ok_or_else(|| RcliError::DataNotFound(name.to_string()))
}

pub fn update(repo: &Repository, name: &str, new_path: Option<String>, recompute_checksum: bool) -> Result<()> {
    let mut datasets = load_data_index(&repo.data_index_path())?;
    let idx = datasets.iter()
        .position(|d| d.name == name)
        .ok_or_else(|| RcliError::DataNotFound(name.to_string()))?;

    if let Some(path) = new_path {
        let abs_path = if Path::new(&path).is_relative() {
            repo.root.join(&path)
        } else {
            Path::new(&path).to_path_buf()
        };
        let relative_path = abs_path.strip_prefix(&repo.root)
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
