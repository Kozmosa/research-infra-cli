use std::fs;
use std::path::Path;

use crate::db::Database;
use crate::error::{RcliError, Result};
use crate::repo::Repository;

pub fn sync(repo: &Repository, mode: &str) -> Result<()> {
    match mode {
        "export" => sync_export(repo),
        "import" => sync_import(repo),
        "auto" => sync_auto(repo),
        _ => Err(RcliError::InvalidStatus(format!("未知同步模式: {}", mode))),
    }
}

pub fn export_all(repo: &Repository, out_dir: Option<&str>) -> Result<()> {
    let db = Database::open(&repo.db_path())?;
    let exps = db.list_experiments(None, None)?;

    let base_dir = match out_dir {
        Some(d) => std::path::PathBuf::from(d),
        None => repo.experiments_dir(),
    };

    for exp_summary in &exps {
        if let Some(exp) = db.get_experiment(&exp_summary.id)? {
            let exp_dir = base_dir.join(&exp.id);
            fs::create_dir_all(&exp_dir)?;
            let json_path = exp_dir.join("experiment.json");
            let json_value = experiment_to_json(&exp)?;
            fs::write(&json_path, serde_json::to_string_pretty(&json_value)?)?;
        }
    }

    Ok(())
}

pub fn import_from(repo: &Repository, from: &str) -> Result<()> {
    let from_path = Path::new(from);
    let db = Database::open(&repo.db_path())?;

    if from_path.is_file() {
        import_single_file(&db, from_path)?;
    } else if from_path.is_dir() {
        for entry in fs::read_dir(from_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                import_single_file(&db, &path)?;
            } else if path.is_dir() && path.join("experiment.json").exists() {
                import_single_file(&db, &path.join("experiment.json"))?;
            }
        }
    }

    Ok(())
}

pub fn status(repo: &Repository) -> Result<SyncStatus> {
    let db = Database::open(&repo.db_path())?;
    let db_exps = db.list_experiments(None, None)?;

    let mut need_export = Vec::new();
    let mut need_import = Vec::new();
    let mut in_sync = Vec::new();
    let mut not_in_db = Vec::new();

    for exp_summary in &db_exps {
        let json_path = repo.exp_json_path(&exp_summary.id);
        if !json_path.exists() {
            need_export.push(exp_summary.id.clone());
            continue;
        }

        let json_content = fs::read_to_string(&json_path)?;
        let json_exp: serde_json::Value = serde_json::from_str(&json_content)?;

        if let Some(exp) = db.get_experiment(&exp_summary.id)? {
            let db_json = experiment_to_json(&exp)?;
            if json_exp == db_json {
                in_sync.push(exp_summary.id.clone());
            } else {
                let db_time = exp.created_at.as_str();
                let json_time = json_exp.get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if db_time > json_time {
                    need_export.push(exp_summary.id.clone());
                } else {
                    need_import.push(exp_summary.id.clone());
                }
            }
        }
    }

    let exp_dir = repo.experiments_dir();
    if exp_dir.exists() {
        for entry in fs::read_dir(&exp_dir)? {
            let entry = entry?;
            let json_path = entry.path().join("experiment.json");
            if json_path.exists() {
                let id = entry.file_name().to_string_lossy().to_string();
                if db.get_experiment(&id)?.is_none() {
                    not_in_db.push(id);
                }
            }
        }
    }

    Ok(SyncStatus {
        need_export,
        need_import,
        in_sync,
        not_in_db,
    })
}

#[derive(serde::Serialize)]
pub struct SyncStatus {
    pub need_export: Vec<String>,
    pub need_import: Vec<String>,
    pub in_sync: Vec<String>,
    pub not_in_db: Vec<String>,
}

fn sync_export(repo: &Repository) -> Result<()> {
    let db = Database::open(&repo.db_path())?;
    let exps = db.list_experiments(None, None)?;

    for exp_summary in &exps {
        if let Some(exp) = db.get_experiment(&exp_summary.id)? {
            let json_path = repo.exp_json_path(&exp.id);
            if let Some(parent) = json_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let json_value = experiment_to_json(&exp)?;
            fs::write(&json_path, serde_json::to_string_pretty(&json_value)?)?;
        }
    }

    Ok(())
}

fn sync_import(repo: &Repository) -> Result<()> {
    let db = Database::open(&repo.db_path())?;
    let exp_dir = repo.experiments_dir();

    if !exp_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&exp_dir)? {
        let entry = entry?;
        let json_path = entry.path().join("experiment.json");
        if json_path.exists() {
            import_single_file(&db, &json_path)?;
        }
    }

    Ok(())
}

fn sync_auto(repo: &Repository) -> Result<()> {
    let db = Database::open(&repo.db_path())?;
    let db_exps = db.list_experiments(None, None)?;
    let mut conflicts = Vec::new();

    for exp_summary in &db_exps {
        let json_path = repo.exp_json_path(&exp_summary.id);
        if !json_path.exists() {
            if let Some(exp) = db.get_experiment(&exp_summary.id)? {
                let json_value = experiment_to_json(&exp)?;
                fs::write(&json_path, serde_json::to_string_pretty(&json_value)?)?;
            }
            continue;
        }

        let json_content = fs::read_to_string(&json_path)?;
        let json_exp: serde_json::Value = serde_json::from_str(&json_content)?;

        if let Some(exp) = db.get_experiment(&exp_summary.id)? {
            let db_json = experiment_to_json(&exp)?;
            if json_exp == db_json {
                continue;
            }

            let db_time = exp.created_at.as_str();
            let json_time = json_exp.get("created_at")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if db_time > json_time {
                fs::write(&json_path, serde_json::to_string_pretty(&db_json)?)?;
            } else if json_time > db_time {
                import_single_file(&db, &json_path)?;
            } else {
                let db_str = serde_json::to_string(&db_json)?;
                let json_str = serde_json::to_string(&json_exp)?;
                if db_str != json_str {
                    conflicts.push(exp.id.clone());
                }
            }
        }
    }

    if !conflicts.is_empty() {
        return Err(RcliError::SyncConflict(conflicts));
    }

    Ok(())
}

fn import_single_file(db: &Database, path: &Path) -> Result<()> {
    let content = fs::read_to_string(path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;

    let id = json.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let short_id = json.get("short_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("created").to_string();
    let created_at = json.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let commit_hash = json.get("commit_hash").and_then(|v| v.as_str());
    let data_used = json.get("data_used").and_then(|v| v.as_str());
    let command = json.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let params = json.get("params").map(|v| v.to_string());
    let notes = json.get("notes").and_then(|v| v.as_str());
    let env = json.get("env").and_then(|v| v.as_str());

    if db.get_experiment(&id)?.is_some() {
        db.upsert_experiment(
            &id, &short_id, &status, &created_at,
            commit_hash, data_used, &command,
            params.as_deref(), notes, env,
        )?;
    } else {
        db.insert_experiment(
            &id, &short_id, &status, &created_at, commit_hash, data_used, &command,
            params.as_deref(), notes, env,
        )?;
    }

    Ok(())
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

        fs::create_dir_all(root.join(".research")).unwrap();
        fs::create_dir_all(root.join("experiments")).unwrap();

        let config = crate::config::Config::default();
        config.save(&root.join(".research/config.yaml")).unwrap();

        let db = Database::open(&root.join(".research/research.db")).unwrap();
        db.init_schema().unwrap();

        (Repository { root: root.to_path_buf() }, dir)
    }

    #[test]
    fn test_sync_export_creates_json() {
        let (repo, _dir) = create_test_repo();

        let db = Database::open(&repo.db_path()).unwrap();
        db.insert_experiment(
            "exp-001", "001", "created", "2026-01-01T00:00:00Z",
            None, None, "python train.py", None, None, None,
        ).unwrap();

        sync_export(&repo).unwrap();

        let json_path = repo.exp_json_path("exp-001");
        assert!(json_path.exists());

        let content = fs::read_to_string(&json_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json.get("id").unwrap().as_str().unwrap(), "exp-001");
    }

    #[test]
    fn test_sync_import_from_json() {
        let (repo, _dir) = create_test_repo();

        let exp_dir = repo.exp_dir("exp-002");
        fs::create_dir_all(&exp_dir).unwrap();

        let json = serde_json::json!({
            "id": "exp-002",
            "short_id": "002",
            "status": "finished",
            "created_at": "2026-01-02T00:00:00Z",
            "command": "python eval.py",
        });
        fs::write(exp_dir.join("experiment.json"), serde_json::to_string_pretty(&json).unwrap()).unwrap();

        sync_import(&repo).unwrap();

        let db = Database::open(&repo.db_path()).unwrap();
        let exp = db.get_experiment("exp-002").unwrap().unwrap();
        assert_eq!(exp.status, "finished");
        assert_eq!(exp.command, "python eval.py");
    }

    #[test]
    fn test_sync_status_detects_need_export() {
        let (repo, _dir) = create_test_repo();

        let db = Database::open(&repo.db_path()).unwrap();
        db.insert_experiment(
            "exp-003", "003", "created", "2026-01-01T00:00:00Z",
            None, None, "python train.py", None, None, None,
        ).unwrap();

        let sync_status = status(&repo).unwrap();
        assert_eq!(sync_status.need_export.len(), 1);
        assert_eq!(sync_status.need_export[0], "exp-003");
        assert_eq!(sync_status.in_sync.len(), 0);
    }

    #[test]
    fn test_sync_status_detects_in_sync() {
        let (repo, _dir) = create_test_repo();

        let db = Database::open(&repo.db_path()).unwrap();
        db.insert_experiment(
            "exp-004", "004", "created", "2026-01-01T00:00:00Z",
            None, None, "python train.py", None, None, None,
        ).unwrap();

        sync_export(&repo).unwrap();

        let sync_status = status(&repo).unwrap();
        assert_eq!(sync_status.in_sync.len(), 1);
        assert_eq!(sync_status.in_sync[0], "exp-004");
    }
}

pub fn experiment_to_json(exp: &crate::db::Experiment) -> Result<serde_json::Value> {
    let mut map = serde_json::Map::new();
    map.insert("id".to_string(), serde_json::Value::String(exp.id.clone()));
    map.insert("short_id".to_string(), serde_json::Value::String(exp.short_id.clone()));
    map.insert("status".to_string(), serde_json::Value::String(exp.status.clone()));
    map.insert("created_at".to_string(), serde_json::Value::String(exp.created_at.clone()));
    if let Some(ref sa) = exp.started_at {
        map.insert("started_at".to_string(), serde_json::Value::String(sa.clone()));
    }
    if let Some(ref fa) = exp.finished_at {
        map.insert("finished_at".to_string(), serde_json::Value::String(fa.clone()));
    }
    if let Some(ref ch) = exp.commit_hash {
        map.insert("commit_hash".to_string(), serde_json::Value::String(ch.clone()));
    }
    if let Some(ref du) = exp.data_used {
        map.insert("data_used".to_string(), serde_json::Value::String(du.clone()));
    }
    map.insert("command".to_string(), serde_json::Value::String(exp.command.clone()));
    if let Some(ref p) = exp.params {
        map.insert("params".to_string(), serde_json::json!(p));
    }
    if let Some(ref n) = exp.notes {
        map.insert("notes".to_string(), serde_json::Value::String(n.clone()));
    }
    if let Some(ref e) = exp.env {
        map.insert("env".to_string(), serde_json::Value::String(e.clone()));
    }
    if let Some(ref ec) = exp.exit_code {
        map.insert("exit_code".to_string(), serde_json::Value::Number((*ec).into()));
    }
    Ok(serde_json::Value::Object(map))
}
