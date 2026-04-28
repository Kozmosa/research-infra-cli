use std::fs;

use crate::commands::{data, env};
use crate::config::Config;
use crate::db::Database;
use crate::error::{RcliError, Result};
use crate::repo::Repository;

pub fn new(
    repo: &Repository,
    data_name: Option<String>,
    cmd: Option<String>,
    manual: bool,
    label: Option<String>,
    params: Option<String>,
    notes: Option<String>,
    env: Option<String>,
    template: Option<String>,
) -> Result<(String, String)> {
    if !manual {
        if data_name.is_none() {
            return Err(RcliError::MissingRequiredArg("--data".to_string()));
        }
        if cmd.is_none() {
            return Err(RcliError::MissingRequiredArg("--cmd".to_string()));
        }
    }

    env::check(repo, true)?;

    let data_name = data_name.unwrap_or_default();
    let command = cmd.unwrap_or_default();

    if !manual {
        let datasets = data::load_data_index(&repo.data_index_path())?;
        if !datasets.iter().any(|d| d.name == data_name) {
            return Err(RcliError::DataNotFound(data_name.clone()));
        }
    }

    let config = Config::load(&repo.config_path())?;
    let exp_dir_name = &config.experiments_dir;

    let db = Database::open(&repo.db_path())?;
    let short_id = db.next_short_id()?;
    let short_id_str = format!("{:03}", short_id);

    let now = chrono::Local::now();
    let timestamp = now.format("%Y-%m-%d-%H%M").to_string();

    let exp_id = if let Some(lbl) = label {
        format!("run-{}-{}_{}", short_id_str, timestamp, lbl)
    } else {
        format!("run-{}-{}", short_id_str, timestamp)
    };

    let exp_dir = repo.root.join(exp_dir_name).join(&exp_id);
    fs::create_dir_all(&exp_dir)?;
    fs::create_dir_all(exp_dir.join("artifacts"))?;
    fs::create_dir_all(exp_dir.join("logs"))?;

    let commit_hash = match git2::Repository::open(&repo.root) {
        Ok(git_repo) => match git_repo.head() {
            Ok(head) => head.target().map(|oid| oid.to_string()),
            Err(_) => None,
        },
        Err(_) => None,
    };

    let created_at = now.to_rfc3339();

    let experiment_json = serde_json::json!({
        "id": &exp_id,
        "short_id": short_id_str,
        "status": "created",
        "created_at": created_at,
        "commit_hash": commit_hash,
        "data_used": if manual { serde_json::Value::Null } else { serde_json::Value::String(data_name.clone()) },
        "command": command,
        "params": params.as_ref().and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok()),
        "notes": notes,
        "env": env,
        "started_at": null,
        "finished_at": null,
        "exit_code": null,
    });

    let json_path = exp_dir.join("experiment.json");
    fs::write(&json_path, serde_json::to_string_pretty(&experiment_json)?)?;

    db.insert_experiment(
        &exp_id,
        &short_id_str,
        "created",
        &created_at,
        commit_hash.as_deref(),
        if manual { None } else { Some(&data_name) },
        &command,
        params.as_deref(),
        notes.as_deref(),
        env.as_deref(),
    )?;

    if let Some(tpl) = template {
        let template_dir = repo.research_dir().join("templates").join(&tpl);
        if template_dir.exists() {
            for entry in walkdir::WalkDir::new(&template_dir) {
                let entry = entry?;
                if entry.file_type().is_file() {
                    let relative = entry.path().strip_prefix(&template_dir).unwrap();
                    let dest = exp_dir.join(relative);
                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::copy(entry.path(), dest)?;
                }
            }
        }
    }

    Ok((exp_id, exp_dir.to_string_lossy().to_string()))
}

pub fn run(repo: &Repository, exp_id: &str, extra_args: &[String]) -> Result<i32> {
    let db = Database::open(&repo.db_path())?;

    let exp = match db.get_experiment(exp_id)? {
        Some(e) => e,
        None => return Err(RcliError::DataNotFound(exp_id.to_string())),
    };

    if exp.status != "created" && exp.status != "interrupted" {
        return Err(RcliError::Other(format!(
            "实验 '{}' 当前状态为 '{}'，无法运行",
            exp_id, exp.status
        )));
    }

    let mut command = exp.command.clone();
    if !extra_args.is_empty() {
        command.push(' ');
        command.push_str(&extra_args.join(" "));
    }

    let log_dir = repo.exp_dir(exp_id).join("logs");
    fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join("run.log");

    let started_at = chrono::Local::now().to_rfc3339();
    db.update_experiment_status(exp_id, "running", Some(&started_at), None, None)?;
    update_exp_json_status(repo, exp_id, "running", Some(&started_at), None, None)?;

    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let mut child = Command::new(&shell)
        .arg("-c")
        .arg(&command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let child_id = child.id() as i32;
    let pid_path = repo.exp_dir(exp_id).join("pid");
    fs::write(&pid_path, child_id.to_string())?;

    let interrupted = Arc::new(AtomicBool::new(false));
    let interrupted_ctrlc = interrupted.clone();

    ctrlc::set_handler(move || {
        interrupted_ctrlc.store(true, Ordering::SeqCst);
        unsafe {
            libc::kill(child_id, libc::SIGTERM);
        }
    }).ok();

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let log_file = fs::File::create(&log_path)?;
    let log_file2 = log_file.try_clone()?;

    let stdout_handle = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        let mut log = std::io::BufWriter::new(log_file);
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if let Ok(l) = line {
                println!("{}", l);
                let _ = writeln!(log, "[stdout] {}", l);
                let _ = log.flush();
            }
        }
    });

    let stderr_handle = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        let mut log = std::io::BufWriter::new(log_file2);
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            if let Ok(l) = line {
                eprintln!("{}", l);
                let _ = writeln!(log, "[stderr] {}", l);
                let _ = log.flush();
            }
        }
    });

    let exit_status = child.wait()?;
    let _ = stdout_handle.join();
    let _ = stderr_handle.join();

    let finished_at = chrono::Local::now().to_rfc3339();

    let (new_status, exit_code) = if interrupted.load(Ordering::SeqCst) {
        ("interrupted", None)
    } else if exit_status.success() {
        ("finished", Some(0))
    } else if exit_status.code().is_none() {
        ("interrupted", None)
    } else {
        ("failed", exit_status.code())
    };

    db.update_experiment_status(exp_id, new_status, None, Some(&finished_at), exit_code)?;
    update_exp_json_status(repo, exp_id, new_status, None, Some(&finished_at), exit_code)?;

    let _ = fs::remove_file(&pid_path);

    Ok(exit_code.unwrap_or(-1))
}

pub fn stop(repo: &Repository, exp_id: &str, signal: &str) -> Result<()> {
    let db = Database::open(&repo.db_path())?;

    let exp = match db.get_experiment(exp_id)? {
        Some(e) => e,
        None => return Err(RcliError::DataNotFound(exp_id.to_string())),
    };

    if exp.status != "running" {
        return Err(RcliError::Other(format!(
            "实验 '{}' 当前状态为 '{}'，不在运行中",
            exp_id, exp.status
        )));
    }

    let pid_path = repo.exp_dir(exp_id).join("pid");
    if !pid_path.exists() {
        return Err(RcliError::Other(format!(
            "实验 '{}' 的 PID 文件不存在，无法终止", exp_id
        )));
    }

    let pid_str = fs::read_to_string(&pid_path)?;
    let pid = pid_str.trim().parse::<i32>()
        .map_err(|_| RcliError::Other(format!(
            "实验 '{}' 的 PID 文件内容无效: '{}'", exp_id, pid_str.trim()
        )))?;

    let sig = match signal {
        "SIGTERM" => libc::SIGTERM,
        "SIGKILL" => libc::SIGKILL,
        _ => {
            return Err(RcliError::InvalidStatus(format!(
                "无效信号 '{}', 仅支持 SIGTERM 和 SIGKILL", signal
            )));
        }
    };

    let ret = unsafe { libc::kill(pid, sig) };
    if ret != 0 {
        let err = std::io::Error::last_os_error();
        return Err(RcliError::Other(format!(
            "向实验 '{}' 的进程 {} 发送信号失败: {}", exp_id, pid, err
        )));
    }

    Ok(())
}

pub fn status(repo: &Repository, exp_id: Option<&str>) -> Result<serde_json::Value> {
    let db = Database::open(&repo.db_path())?;

    if let Some(id) = exp_id {
        let exp = db.get_experiment(id)?;
        match exp {
            Some(e) => Ok(serde_json::to_value(e)?),
            None => Err(RcliError::DataNotFound(id.to_string())),
        }
    } else {
        let exps = db.list_experiments(None, None)?;
        Ok(serde_json::to_value(exps)?)
    }
}

pub fn list(repo: &Repository, status_filter: Option<&str>, since: Option<&str>) -> Result<Vec<crate::db::ExperimentSummary>> {
    let db = Database::open(&repo.db_path())?;
    db.list_experiments(status_filter, since)
}

pub fn export(repo: &Repository, exp_id: &str, output: Option<&str>) -> Result<String> {
    let db = Database::open(&repo.db_path())?;

    let exp = match db.get_experiment(exp_id)? {
        Some(e) => e,
        None => return Err(RcliError::DataNotFound(exp_id.to_string())),
    };

    let json_value = serde_json::to_value(&exp)?;
    let json_str = serde_json::to_string_pretty(&json_value)?;

    let out_path = match output {
        Some(p) => std::path::PathBuf::from(p),
        None => repo.exp_json_path(exp_id),
    };

    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&out_path, &json_str)?;

    Ok(out_path.to_string_lossy().to_string())
}

pub fn metric(
    repo: &Repository,
    exp_id: &str,
    step: i64,
    json_metrics: Option<&str>,
    keys: &[String],
    vals: &[String],
) -> Result<()> {
    let db = Database::open(&repo.db_path())?;

    let exp = match db.get_experiment(exp_id)? {
        Some(e) => e,
        None => return Err(RcliError::DataNotFound(exp_id.to_string())),
    };

    if exp.status != "running" && exp.status != "created" && exp.status != "finished" && exp.status != "failed" {
        return Err(RcliError::Other(format!(
            "实验 '{}' 当前状态为 '{}'，无法记录指标",
            exp_id, exp.status
        )));
    }

    let mut metrics: Vec<(String, f64)> = Vec::new();

    if let Some(json_str) = json_metrics {
        let parsed: serde_json::Value = serde_json::from_str(json_str)?;
        if let Some(obj) = parsed.as_object() {
            for (k, v) in obj {
                if let Some(num) = v.as_f64() {
                    metrics.push((k.clone(), num));
                } else if let Some(i) = v.as_i64() {
                    metrics.push((k.clone(), i as f64));
                }
            }
        }
    }

    if !keys.is_empty() {
        for (i, key) in keys.iter().enumerate() {
            if let Some(val_str) = vals.get(i) {
                if let Ok(val) = val_str.parse::<f64>() {
                    metrics.push((key.clone(), val));
                }
            }
        }
    }

    if metrics.is_empty() {
        return Err(RcliError::Other("未提供任何指标".to_string()));
    }

    for (key, value) in metrics {
        db.insert_metric(exp_id, step, &key, value)?;
    }

    Ok(())
}

pub fn param(repo: &Repository, exp_id: &str, json_params: &str) -> Result<()> {
    let db = Database::open(&repo.db_path())?;

    let exp = match db.get_experiment(exp_id)? {
        Some(e) => e,
        None => return Err(RcliError::DataNotFound(exp_id.to_string())),
    };

    let new_params: serde_json::Value = serde_json::from_str(json_params)?;
    let new_obj = match new_params.as_object() {
        Some(o) => o.clone(),
        None => return Err(RcliError::Other("参数必须是 JSON 对象".to_string())),
    };

    let mut merged = match exp.params {
        Some(ref p) if !p.is_empty() && p != "null" => {
            serde_json::from_str::<serde_json::Value>(p)?.as_object().cloned().unwrap_or_default()
        }
        _ => serde_json::Map::new(),
    };

    for (k, v) in new_obj {
        merged.insert(k, v);
    }

    let merged_json = serde_json::to_string(&merged)?;

    db.update_experiment_params(exp_id, &merged_json)?;

    let json_path = repo.exp_json_path(exp_id);
    if json_path.exists() {
        let content = fs::read_to_string(&json_path)?;
        let mut json: serde_json::Value = serde_json::from_str(&content)?;
        if let Some(obj) = json.as_object_mut() {
            obj.insert("params".to_string(), serde_json::Value::Object(merged));
        }
        fs::write(&json_path, serde_json::to_string_pretty(&json)?)?;
    }

    Ok(())
}

pub fn finish(repo: &Repository, exp_id: &str, status: &str, message: Option<&str>) -> Result<()> {
    if status != "finished" && status != "failed" {
        return Err(RcliError::InvalidStatus(format!(
            "finish 命令只接受 'finished' 或 'failed' 状态，收到 '{}'",
            status
        )));
    }

    let db = Database::open(&repo.db_path())?;

    let exp = match db.get_experiment(exp_id)? {
        Some(e) => e,
        None => return Err(RcliError::DataNotFound(exp_id.to_string())),
    };

    if exp.status == "finished" || exp.status == "failed" {
        return Err(RcliError::Other(format!(
            "实验 '{}' 已处于 '{}' 状态，无法再次标记",
            exp_id, exp.status
        )));
    }

    let finished_at = chrono::Local::now().to_rfc3339();
    db.update_experiment_status(exp_id, status, None, Some(&finished_at), None)?;
    update_exp_json_status(repo, exp_id, status, None, Some(&finished_at), None)?;

    if let Some(msg) = message {
        db.append_experiment_note(exp_id, msg)?;
    }

    Ok(())
}

fn update_exp_json_status(
    repo: &Repository,
    exp_id: &str,
    status: &str,
    started_at: Option<&str>,
    finished_at: Option<&str>,
    exit_code: Option<i32>,
) -> Result<()> {
    let json_path = repo.exp_json_path(exp_id);
    if !json_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&json_path)?;
    let mut json: serde_json::Value = serde_json::from_str(&content)?;

    if let Some(obj) = json.as_object_mut() {
        obj.insert("status".to_string(), serde_json::Value::String(status.to_string()));
        if let Some(sa) = started_at {
            obj.insert("started_at".to_string(), serde_json::Value::String(sa.to_string()));
        }
        if let Some(fa) = finished_at {
            obj.insert("finished_at".to_string(), serde_json::Value::String(fa.to_string()));
        }
        if let Some(ec) = exit_code {
            obj.insert("exit_code".to_string(), serde_json::Value::Number(ec.into()));
        }
    }

    fs::write(&json_path, serde_json::to_string_pretty(&json)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command as StdCommand, Stdio};
    use std::thread;
    use std::time::Duration;

    fn create_test_repo() -> (Repository, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let _ = git2::Repository::init(&root);

        fs::create_dir_all(root.join(".research")).unwrap();
        fs::create_dir_all(root.join("experiments")).unwrap();

        let config = crate::config::Config::default();
        config.save(&root.join(".research/config.yaml")).unwrap();

        let db = Database::open(&root.join(".research/research.db")).unwrap();
        db.init_schema().unwrap();

        // Create .gitignore to ignore SQLite temp files
        fs::write(root.join(".gitignore"), ".research/*.db*\n.research/*.db-wal\n.research/*.db-shm\n").unwrap();

        // Commit all files so workspace is clean for env::check
        let git_repo = git2::Repository::open(&root).unwrap();
        let sig = git2::Signature::new("test", "test@test.com", &git2::Time::new(0, 0)).unwrap();
        let mut index = git_repo.index().unwrap();
        index.add_all(["."], git2::IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = git_repo.find_tree(tree_id).unwrap();
        git_repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();

        (Repository { root: root.to_path_buf() }, dir)
    }

    fn create_test_experiment(repo: &Repository, exp_id: &str, command: &str) {
        let db = Database::open(&repo.db_path()).unwrap();
        db.insert_experiment(
            exp_id, "001", "created", "2026-01-01T00:00:00Z",
            None, None, command, None, None, None,
        ).unwrap();

        let exp_dir = repo.exp_dir(exp_id);
        fs::create_dir_all(&exp_dir).unwrap();
        fs::create_dir_all(exp_dir.join("logs")).unwrap();
        fs::create_dir_all(exp_dir.join("artifacts")).unwrap();

        let json = serde_json::json!({
            "id": exp_id,
            "short_id": "001",
            "status": "created",
            "created_at": "2026-01-01T00:00:00Z",
            "command": command,
        });
        fs::write(
            exp_dir.join("experiment.json"),
            serde_json::to_string_pretty(&json).unwrap(),
        ).unwrap();
    }

    #[test]
    fn test_new_creates_experiment() {
        let (repo, _dir) = create_test_repo();

        let (exp_id, exp_dir) = new(
            &repo, None, Some("echo hello".to_string()), true,
            None, None, None, None, None,
        ).unwrap();

        assert!(exp_dir.contains(&repo.root.to_string_lossy().to_string()));
        assert!(repo.exp_dir(&exp_id).exists());
        assert!(repo.exp_dir(&exp_id).join("artifacts").exists());
        assert!(repo.exp_dir(&exp_id).join("logs").exists());
        assert!(repo.exp_json_path(&exp_id).exists());

        let db = Database::open(&repo.db_path()).unwrap();
        let exp = db.get_experiment(&exp_id).unwrap().unwrap();
        assert_eq!(exp.status, "created");
        assert_eq!(exp.command, "echo hello");
    }

    #[test]
    fn test_new_requires_data_and_cmd() {
        let (repo, _dir) = create_test_repo();

        let result = new(
            &repo, None, None, false,
            None, None, None, None, None,
        );
        assert!(matches!(result, Err(RcliError::MissingRequiredArg(_))));
    }

    #[test]
    fn test_new_validates_data_asset() {
        let (repo, _dir) = create_test_repo();

        let result = new(
            &repo, Some("nonexistent".to_string()), Some("echo hello".to_string()), false,
            None, None, None, None, None,
        );
        assert!(matches!(result, Err(RcliError::DataNotFound(_))));
    }

    #[test]
    fn test_run_creates_and_cleans_pid_file() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-001", "echo hello"
        );

        let pid_path = repo.exp_dir("exp-001").join("pid");

        assert!(!pid_path.exists());

        run(&repo, "exp-001", &[]
        ).unwrap();

        assert!(!pid_path.exists());

        let db = Database::open(&repo.db_path()).unwrap();
        let exp = db.get_experiment("exp-001").unwrap().unwrap();
        assert_eq!(exp.status, "finished");
        assert_eq!(exp.exit_code, Some(0));
    }

    #[test]
    fn test_run_failure_cleans_pid_file() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-002", "exit 42"
        );

        let pid_path = repo.exp_dir("exp-002").join("pid");

        let code = run(&repo, "exp-002", &[]
        ).unwrap();
        assert_eq!(code, 42);

        assert!(!pid_path.exists());

        let db = Database::open(&repo.db_path()).unwrap();
        let exp = db.get_experiment("exp-002").unwrap().unwrap();
        assert_eq!(exp.status, "failed");
        assert_eq!(exp.exit_code, Some(42));
    }

    #[test]
    fn test_run_pid_file_exists_during_execution() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-003", "sleep 2"
        );

        let pid_path = repo.exp_dir("exp-003").join("pid");
        let repo_clone = Repository { root: repo.root.clone() };

        let run_handle = thread::spawn(move || {
            run(&repo_clone, "exp-003", &[]
            ).unwrap();
        });

        let mut found = false;
        for _ in 0..50 {
            if pid_path.exists() {
                found = true;
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        assert!(found, "PID 文件应在实验运行期间出现");

        run_handle.join().unwrap();
        assert!(!pid_path.exists());
    }

    #[test]
    fn test_run_stop_end_to_end() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-008", "sleep 10"
        );

        let pid_path = repo.exp_dir("exp-008").join("pid");
        let repo_run = Repository { root: repo.root.clone() };
        let repo_stop = Repository { root: repo.root.clone() };

        let run_handle = thread::spawn(move || {
            run(&repo_run, "exp-008", &[]
            ).unwrap();
        });

        let mut found = false;
        for _ in 0..50 {
            if pid_path.exists() {
                found = true;
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        assert!(found, "PID 文件应在 run 启动后出现");

        stop(&repo_stop, "exp-008", "SIGTERM"
        ).unwrap();

        run_handle.join().unwrap();

        assert!(!pid_path.exists());

        let db = Database::open(&repo.db_path()).unwrap();
        let exp = db.get_experiment("exp-008").unwrap().unwrap();
        assert_eq!(exp.status, "interrupted");
        assert!(exp.finished_at.is_some());
    }

    #[test]
    fn test_stop_non_running_fails() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-004", "echo hello"
        );

        let result = stop(&repo, "exp-004", "SIGTERM"
        );
        assert!(matches!(result, Err(RcliError::Other(_))));
    }

    #[test]
    fn test_stop_sends_signal() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-005", "sleep 10"
        );

        let db = Database::open(&repo.db_path()).unwrap();
        db.update_experiment_status("exp-005", "running", Some("2026-01-01T00:00:00Z"), None, None).unwrap();

        let mut child = StdCommand::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as i32;

        fs::write(repo.exp_dir("exp-005").join("pid"), pid.to_string()).unwrap();

        stop(&repo, "exp-005", "SIGTERM"
        ).unwrap();

        thread::sleep(Duration::from_millis(200));
        let status = child.try_wait().unwrap();
        assert!(status.is_some(), "子进程应已被信号终止");

        // stop() 不再修改 DB 状态或删除 PID 文件，这些由 run() 负责
        let exp = db.get_experiment("exp-005").unwrap().unwrap();
        assert_eq!(exp.status, "running");
        assert!(repo.exp_dir("exp-005").join("pid").exists());
    }

    #[test]
    fn test_stop_invalid_signal_fails() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-009", "sleep 10"
        );

        let db = Database::open(&repo.db_path()).unwrap();
        db.update_experiment_status("exp-009", "running", Some("2026-01-01T00:00:00Z"), None, None).unwrap();

        let mut child = StdCommand::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        fs::write(repo.exp_dir("exp-009").join("pid"), pid.to_string()).unwrap();

        let result = stop(&repo, "exp-009", "SIGINT"
        );
        assert!(matches!(result, Err(RcliError::InvalidStatus(_))));

        let result = stop(&repo, "exp-009", "INVALID"
        );
        assert!(matches!(result, Err(RcliError::InvalidStatus(_))));

        // 清理子进程
        unsafe { libc::kill(pid, libc::SIGKILL); }
        let _ = child.wait();
    }

    #[test]
    fn test_stop_does_not_prematurely_finalize_state() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-010", "sleep 10"
        );

        let db = Database::open(&repo.db_path()).unwrap();
        db.update_experiment_status("exp-010", "running", Some("2026-01-01T00:00:00Z"), None, None).unwrap();

        let mut child = StdCommand::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as i32;

        fs::write(repo.exp_dir("exp-010").join("pid"), pid.to_string()).unwrap();

        // 发送 SIGTERM，stop() 应返回成功（信号已投递）
        stop(&repo, "exp-010", "SIGTERM"
        ).unwrap();

        // stop() 不应修改 DB 状态或删除 PID 文件——这些由 run() 在子进程实际退出后处理
        let exp = db.get_experiment("exp-010").unwrap().unwrap();
        assert_eq!(exp.status, "running", "stop() 不应修改实验状态");
        assert!(repo.exp_dir("exp-010").join("pid").exists(), "stop() 不应删除 PID 文件");

        // 清理：等待子进程被信号终止后清理
        thread::sleep(Duration::from_millis(300));
        let _ = child.try_wait();
        if child.try_wait().unwrap().is_none() {
            unsafe { libc::kill(pid, libc::SIGKILL); }
            let _ = child.wait();
        }
    }

    #[test]
    fn test_status_and_list() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-006", "echo hello"
        );

        let val = status(&repo, Some("exp-006")
        ).unwrap();
        assert_eq!(val.get("id").unwrap().as_str().unwrap(), "exp-006");

        let exps = list(&repo, None, None
        ).unwrap();
        assert_eq!(exps.len(), 1);
        assert_eq!(exps[0].id, "exp-006");
    }

    #[test]
    fn test_metric_and_param_and_finish() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-007", "echo hello"
        );

        metric(&repo, "exp-007", 1, Some("{\"loss\":0.5}"), &[], &[]
        ).unwrap();

        let db = Database::open(&repo.db_path()).unwrap();
        let metrics = db.get_metrics("exp-007").unwrap();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].metric_value, 0.5);

        param(&repo, "exp-007", "{\"lr\":0.001}"
        ).unwrap();
        let exp = db.get_experiment("exp-007").unwrap().unwrap();
        assert!(exp.params.as_ref().unwrap().contains("lr"));

        finish(&repo, "exp-007", "finished", None
        ).unwrap();
        let exp = db.get_experiment("exp-007").unwrap().unwrap();
        assert_eq!(exp.status, "finished");
        assert!(exp.finished_at.is_some());
    }
}
