use std::fs;

use crate::commands::{data, env};
use crate::config::Config;
use crate::db::Database;
use crate::error::{ArcliError, Result};
use crate::repo::Repository;

#[allow(clippy::too_many_arguments)]
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
    claims: Option<String>,
    hypothesis: Option<String>,
) -> Result<(String, String)> {
    if !manual {
        if data_name.is_none() {
            return Err(ArcliError::MissingRequiredArg("--data".to_string()));
        }
        if cmd.is_none() {
            return Err(ArcliError::MissingRequiredArg("--cmd".to_string()));
        }
    }

    env::check(repo, true)?;

    let data_name = data_name.unwrap_or_default();
    let command = cmd.unwrap_or_default();

    if !manual {
        let datasets = data::load_data_index(&repo.data_index_path())?;
        if !datasets.iter().any(|d| d.name == data_name) {
            return Err(ArcliError::DataNotFound(data_name.clone()));
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

    let diff_at_creation = match git2::Repository::open(&repo.root) {
        Ok(git_repo) => {
            let mut diff_opts = git2::DiffOptions::new();
            diff_opts.include_untracked(true);
            let diff = git_repo.diff_index_to_workdir(None, Some(&mut diff_opts)).ok();

            let mut diff_text = String::new();
            if let Some(d) = diff {
                d.print(git2::DiffFormat::NameStatus, |_delta, _hunk, line| {
                    if let Ok(s) = std::str::from_utf8(line.content()) {
                        diff_text.push_str(s);
                        if !s.ends_with('\n') {
                            diff_text.push('\n');
                        }
                    }
                    true
                }).ok();
            }

            if diff_text.trim().is_empty() {
                None
            } else {
                Some(diff_text)
            }
        }
        Err(_) => None,
    };

    let created_at = now.to_rfc3339();

    // Parse and validate claims
    let relates_to_claims: Option<Vec<String>> = claims.as_ref().map(|c| {
        c.split(',')
            .map(|s| s.trim().to_uppercase().to_string())
            .collect()
    });

    if let Some(ref claim_ids) = relates_to_claims {
        let claims_path = repo.claims_path();
        let cf = crate::commands::claim::load_claims(&claims_path)?;
        for cid in claim_ids {
            if !cf.claims.contains_key(cid) {
                return Err(ArcliError::ClaimNotFound(cid.clone()));
            }
        }
    }

    let relates_to_claims_json = relates_to_claims
        .as_ref()
        .map(|v| serde_json::to_value(v).unwrap_or(serde_json::Value::Null));

    let relates_to_claims_json_str = relates_to_claims
        .as_ref()
        .and_then(|v| serde_json::to_string(v).ok());

    let experiment_json = serde_json::json!({
        "id": &exp_id,
        "short_id": short_id_str,
        "status": "created",
        "created_at": &created_at,
        "updated_at": &created_at,
        "commit_hash": commit_hash,
        "data_used": if manual { serde_json::Value::Null } else { serde_json::Value::String(data_name.clone()) },
        "command": command,
        "params": params.as_ref().and_then(|p| serde_json::from_str::<serde_json::Value>(p).ok()),
        "notes": notes,
        "env": env,
        "started_at": null,
        "finished_at": null,
        "exit_code": null,
        "env_snapshot": null,
        "diff_at_creation": diff_at_creation,
        "artifacts_index": [],
        "relates_to_claims": relates_to_claims_json,
        "hypothesis": hypothesis,
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
        relates_to_claims_json_str.as_deref(),
        hypothesis.as_deref(),
        None,
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

pub fn run(
    repo: &Repository,
    exp_id: &str,
    extra_args: &[String],
    timeout_secs: Option<u64>,
) -> Result<i32> {
    let db = Database::open(&repo.db_path())?;

    let exp = match db.get_experiment(exp_id)? {
        Some(e) => e,
        None => return Err(ArcliError::ExperimentNotFound(exp_id.to_string())),
    };

    if exp.status != "created" && exp.status != "interrupted" {
        return Err(ArcliError::Other(format!(
            "实验 '{}' 当前状态为 '{}'，无法运行",
            exp_id, exp.status
        )));
    }

    let mut command = exp.command.clone();
    if !extra_args.is_empty() {
        command.push(' ');
        command.push_str(&extra_args.join(" "));
    }

    let exp_dir = repo.exp_dir(exp_id);
    let log_dir = exp_dir.join("logs");
    fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join("run.log");

    use std::process::{Command, Stdio};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[cfg(unix)]
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    #[cfg(unix)]
    let mut child = Command::new(&shell)
        .arg("-c")
        .arg(&command)
        .env("ARCLI_EXP_DIR", &exp_dir)
        .env("ARCLI_EXP_ID", exp_id)
        .env("ARCLI_REPO_ROOT", &repo.root)
        .env("ARCLI_LOG_DIR", &log_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    #[cfg(windows)]
    let mut child = Command::new("cmd")
        .arg("/C")
        .arg(&command)
        .env("ARCLI_EXP_DIR", &exp_dir)
        .env("ARCLI_EXP_ID", exp_id)
        .env("ARCLI_REPO_ROOT", &repo.root)
        .env("ARCLI_LOG_DIR", &log_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let child_id = child.id() as i32;
    let pid_path = repo.exp_dir(exp_id).join("pid");
    fs::write(&pid_path, child_id.to_string())?;

    let started_at = chrono::Local::now().to_rfc3339();
    db.update_experiment_status(exp_id, "running", Some(&started_at), None, None)?;
    update_exp_json_status(repo, exp_id, "running", Some(&started_at), None, None)?;

    let interrupted = Arc::new(AtomicBool::new(false));
    let interrupted_ctrlc = interrupted.clone();

    ctrlc::set_handler(move || {
        interrupted_ctrlc.store(true, Ordering::SeqCst);
        #[cfg(unix)]
        unsafe {
            libc::kill(child_id, libc::SIGTERM);
        }
        #[cfg(windows)]
        {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &child_id.to_string(), "/T", "/F"])
                .output();
        }
    })
    .ok();

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let log_file = fs::File::create(&log_path)?;
    let log_file2 = log_file.try_clone()?;

    let stdout_handle = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        let mut log = std::io::BufWriter::new(log_file);
        let reader = BufReader::new(stdout);
        for l in reader.lines().map_while(|r| r.ok()) {
            println!("{}", l);
            let _ = writeln!(log, "[stdout] {}", l);
            let _ = log.flush();
        }
    });

    let stderr_handle = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader, Write};
        let mut log = std::io::BufWriter::new(log_file2);
        let reader = BufReader::new(stderr);
        for l in reader.lines().map_while(|r| r.ok()) {
            eprintln!("{}", l);
            let _ = writeln!(log, "[stderr] {}", l);
            let _ = log.flush();
        }
    });

    let exit_status = match timeout_secs {
        Some(secs) => {
            use std::sync::mpsc;
            use std::time::Duration;

            let (tx, rx) = mpsc::channel();
            let mut child_inner = child;
            let _ = std::thread::spawn(move || {
                let result = child_inner.wait();
                let _ = tx.send(result);
            });

            match rx.recv_timeout(Duration::from_secs(secs)) {
                Ok(result) => result?,
                Err(_) => {
                    // Kill the child process using its PID
                    let _ = std::process::Command::new("kill")
                        .arg("-9")
                        .arg(child_id.to_string())
                        .output();
                    let _ = stdout_handle.join();
                    let _ = stderr_handle.join();

                    let finished_at = chrono::Local::now().to_rfc3339();
                    db.update_experiment_status(
                        exp_id,
                        "interrupted",
                        None,
                        Some(&finished_at),
                        None,
                    )?;
                    update_exp_json_status(
                        repo,
                        exp_id,
                        "interrupted",
                        None,
                        Some(&finished_at),
                        None,
                    )?;
                    let _ = fs::remove_file(&pid_path);

                    let _ = discover_artifacts(&repo.exp_dir(exp_id), &repo.root);

                    return Err(ArcliError::ExperimentTimeout(secs));
                }
            }
        }
        None => child.wait()?,
    };

    let _ = stdout_handle.join();
    let _ = stderr_handle.join();

    let finished_at = chrono::Local::now().to_rfc3339();

    let stop_intent_path = repo.exp_dir(exp_id).join(".stop");
    let stop_intent_present = stop_intent_path.exists();
    if stop_intent_present {
        let _ = fs::remove_file(&stop_intent_path);
    }

    let (new_status, exit_code) = if interrupted.load(Ordering::SeqCst) || stop_intent_present {
        ("interrupted", None)
    } else if exit_status.success() {
        ("finished", Some(0))
    } else if exit_status.code().is_none() {
        ("interrupted", None)
    } else {
        ("failed", exit_status.code())
    };

    db.update_experiment_status(exp_id, new_status, None, Some(&finished_at), exit_code)?;
    update_exp_json_status(
        repo,
        exp_id,
        new_status,
        None,
        Some(&finished_at),
        exit_code,
    )?;

    // Discover and record artifacts
    match discover_artifacts(&repo.exp_dir(exp_id), &repo.root) {
        Ok(artifacts) => {
            if let Err(e) = update_exp_json_artifacts(repo, exp_id, &artifacts) {
                eprintln!("警告: artifact 索引写入失败: {}", e);
            }
        }
        Err(e) => {
            eprintln!("警告: artifact 扫描失败: {}", e);
        }
    }

    let _ = fs::remove_file(&pid_path);

    Ok(exit_code.unwrap_or(-1))
}

pub fn stop(repo: &Repository, exp_id: &str, signal: &str) -> Result<()> {
    let db = Database::open(&repo.db_path())?;

    let exp = match db.get_experiment(exp_id)? {
        Some(e) => e,
        None => return Err(ArcliError::DataNotFound(exp_id.to_string())),
    };

    if exp.status != "running" {
        return Err(ArcliError::Other(format!(
            "实验 '{}' 当前状态为 '{}'，不在运行中",
            exp_id, exp.status
        )));
    }

    let pid_path = repo.exp_dir(exp_id).join("pid");
    if !pid_path.exists() {
        return Err(ArcliError::Other(format!(
            "实验 '{}' 的 PID 文件不存在，无法终止",
            exp_id
        )));
    }

    let pid_str = fs::read_to_string(&pid_path)?;
    let pid = pid_str.trim().parse::<i32>().map_err(|_| {
        ArcliError::Other(format!(
            "实验 '{}' 的 PID 文件内容无效: '{}'",
            exp_id,
            pid_str.trim()
        ))
    })?;

    #[cfg(unix)]
    {
        let sig = match signal {
            "SIGTERM" => libc::SIGTERM,
            "SIGKILL" => libc::SIGKILL,
            _ => {
                return Err(ArcliError::InvalidStatus(format!(
                    "无效信号 '{}', 仅支持 SIGTERM 和 SIGKILL",
                    signal
                )));
            }
        };

        let ret = unsafe { libc::kill(pid, sig) };
        if ret != 0 {
            let err = std::io::Error::last_os_error();
            return Err(ArcliError::Other(format!(
                "向实验 '{}' 的进程 {} 发送信号失败: {}",
                exp_id, pid, err
            )));
        }
    }
    #[cfg(windows)]
    {
        let output = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output()
            .map_err(|e| ArcliError::Other(format!("终止进程失败: {}", e)))?;
        if !output.status.success() {
            let msg = String::from_utf8_lossy(&output.stderr);
            return Err(ArcliError::Other(format!(
                "向实验 '{}' 的进程 {} 发送终止信号失败: {}",
                exp_id, pid, msg
            )));
        }
    }

    // Write stop-intent so run() converges to "interrupted" regardless of child exit code
    let stop_intent_path = repo.exp_dir(exp_id).join(".stop");
    fs::write(&stop_intent_path, "")?;

    Ok(())
}

pub fn status(repo: &Repository, exp_id: Option<&str>) -> Result<serde_json::Value> {
    let db = Database::open(&repo.db_path())?;

    if let Some(id) = exp_id {
        let exp = db.get_experiment(id)?;
        match exp {
            Some(e) => Ok(serde_json::to_value(e)?),
            None => Err(ArcliError::DataNotFound(id.to_string())),
        }
    } else {
        let exps = db.list_experiments(None, None)?;
        Ok(serde_json::to_value(exps)?)
    }
}

pub fn list(
    repo: &Repository,
    status_filter: Option<&str>,
    since: Option<&str>,
) -> Result<Vec<crate::db::ExperimentSummary>> {
    let db = Database::open(&repo.db_path())?;
    db.list_experiments(status_filter, since)
}

pub fn export(repo: &Repository, exp_id: &str, output: Option<&str>) -> Result<String> {
    let db = Database::open(&repo.db_path())?;

    let exp = match db.get_experiment(exp_id)? {
        Some(e) => e,
        None => return Err(ArcliError::DataNotFound(exp_id.to_string())),
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
        None => return Err(ArcliError::DataNotFound(exp_id.to_string())),
    };

    if exp.status != "running"
        && exp.status != "created"
        && exp.status != "finished"
        && exp.status != "failed"
    {
        return Err(ArcliError::Other(format!(
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
            if let Some(val_str) = vals.get(i)
                && let Ok(val) = val_str.parse::<f64>()
            {
                metrics.push((key.clone(), val));
            }
        }
    }

    if metrics.is_empty() {
        return Err(ArcliError::Other("未提供任何指标".to_string()));
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
        None => return Err(ArcliError::DataNotFound(exp_id.to_string())),
    };

    let new_params: serde_json::Value = serde_json::from_str(json_params)?;
    let new_obj = match new_params.as_object() {
        Some(o) => o.clone(),
        None => return Err(ArcliError::Other("参数必须是 JSON 对象".to_string())),
    };

    let mut merged = match exp.params {
        Some(ref p) if !p.is_empty() && p != "null" => {
            serde_json::from_str::<serde_json::Value>(p)?
                .as_object()
                .cloned()
                .unwrap_or_default()
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
            let now = chrono::Local::now().to_rfc3339();
            obj.insert("updated_at".to_string(), serde_json::Value::String(now));
        }
        fs::write(&json_path, serde_json::to_string_pretty(&json)?)?;
    }

    Ok(())
}

pub fn finish(repo: &Repository, exp_id: &str, status: &str, message: Option<&str>) -> Result<()> {
    if status != "finished" && status != "failed" {
        return Err(ArcliError::InvalidStatus(format!(
            "finish 命令只接受 'finished' 或 'failed' 状态，收到 '{}'",
            status
        )));
    }

    let db = Database::open(&repo.db_path())?;

    let exp = match db.get_experiment(exp_id)? {
        Some(e) => e,
        None => return Err(ArcliError::DataNotFound(exp_id.to_string())),
    };

    if exp.status == "finished" || exp.status == "failed" {
        return Err(ArcliError::Other(format!(
            "实验 '{}' 已处于 '{}' 状态，无法再次标记",
            exp_id, exp.status
        )));
    }

    let finished_at = chrono::Local::now().to_rfc3339();
    db.update_experiment_status(exp_id, status, None, Some(&finished_at), None)?;
    update_exp_json_status(repo, exp_id, status, None, Some(&finished_at), None)?;

    if let Some(msg) = message {
        db.append_experiment_note(exp_id, msg)?;

        // Sync note to JSON
        let json_path = repo.exp_json_path(exp_id);
        if json_path.exists() {
            let content = fs::read_to_string(&json_path)?;
            let mut json: serde_json::Value = serde_json::from_str(&content)?;
            if let Some(obj) = json.as_object_mut() {
                let existing = obj.get("notes").and_then(|v| v.as_str()).unwrap_or("");
                let new_note = format!("{}\n finish message: {}", existing, msg);
                obj.insert("notes".to_string(), serde_json::Value::String(new_note));
            }
            fs::write(&json_path, serde_json::to_string_pretty(&json)?)?;
        }
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

    let now = chrono::Local::now().to_rfc3339();

    if let Some(obj) = json.as_object_mut() {
        obj.insert(
            "status".to_string(),
            serde_json::Value::String(status.to_string()),
        );
        obj.insert("updated_at".to_string(), serde_json::Value::String(now));
        if let Some(sa) = started_at {
            obj.insert(
                "started_at".to_string(),
                serde_json::Value::String(sa.to_string()),
            );
        }
        if let Some(fa) = finished_at {
            obj.insert(
                "finished_at".to_string(),
                serde_json::Value::String(fa.to_string()),
            );
        }
        if let Some(ec) = exit_code {
            obj.insert(
                "exit_code".to_string(),
                serde_json::Value::Number(ec.into()),
            );
        }
    }

    fs::write(&json_path, serde_json::to_string_pretty(&json)?)?;
    Ok(())
}

#[derive(Debug, serde::Serialize)]
struct ArtifactEntry {
    path: String,
    size: u64,
}

fn discover_artifacts(
    exp_dir: &std::path::Path,
    repo_root: &std::path::Path,
) -> Result<Vec<ArtifactEntry>> {
    use walkdir::WalkDir;

    let mut artifacts = Vec::new();

    let git_repo = git2::Repository::open(repo_root).ok();

    for entry in WalkDir::new(exp_dir) {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let relative = path.strip_prefix(exp_dir).unwrap_or(path);
        let relative_str = relative.to_string_lossy().to_string();

        // Skip experiment.json itself
        if relative_str == "experiment.json" {
            continue;
        }

        // Skip logs/ directory
        if relative_str.starts_with("logs/") || relative_str == "logs" {
            continue;
        }

        // Skip hidden files
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }

        // Skip gitignored files
        if let Some(ref repo) = git_repo {
            let abs_path = path.to_path_buf();
            if let Ok(ignored) = repo.status_should_ignore(&abs_path)
                && ignored
            {
                continue;
            }
        }

        let size = entry.metadata()?.len();
        artifacts.push(ArtifactEntry {
            path: relative_str,
            size,
        });
    }

    artifacts.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(artifacts)
}

fn update_exp_json_artifacts(
    repo: &Repository,
    exp_id: &str,
    artifacts: &[ArtifactEntry],
) -> Result<()> {
    let json_path = repo.exp_json_path(exp_id);
    if !json_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&json_path)?;
    let mut json: serde_json::Value = serde_json::from_str(&content)?;

    if let Some(obj) = json.as_object_mut() {
        let artifacts_json = serde_json::to_value(artifacts)?;
        obj.insert("artifacts_index".to_string(), artifacts_json);
    }

    fs::write(&json_path, serde_json::to_string_pretty(&json)?)?;
    Ok(())
}

pub fn diff(repo: &Repository, exp_id_1: &str, exp_id_2: &str, full: bool) -> Result<String> {
    let db = Database::open(&repo.db_path())?;

    let exp1 = db
        .get_experiment(exp_id_1)?
        .ok_or_else(|| ArcliError::ExperimentNotFound(exp_id_1.to_string()))?;
    let exp2 = db
        .get_experiment(exp_id_2)?
        .ok_or_else(|| ArcliError::ExperimentNotFound(exp_id_2.to_string()))?;

    // Read experiment.json for diff_at_creation
    let json1_path = repo.exp_json_path(exp_id_1);
    let json2_path = repo.exp_json_path(exp_id_2);

    let diff1 = if json1_path.exists() {
        let content = fs::read_to_string(&json1_path)?;
        let json: serde_json::Value = serde_json::from_str(&content)?;
        json.get("diff_at_creation")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    } else {
        None
    };

    let diff2 = if json2_path.exists() {
        let content = fs::read_to_string(&json2_path)?;
        let json: serde_json::Value = serde_json::from_str(&content)?;
        json.get("diff_at_creation")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    } else {
        None
    };

    let mut output = String::new();

    // Print experiment 1 info
    output.push_str(&format!("实验 {} ({})", exp_id_1, exp1.command));
    if let Some(ref hash) = exp1.commit_hash {
        output.push_str(&format!("\n  commit: {}", hash));
    } else {
        output.push_str("\n  commit: (无)");
    }
    output.push_str(&format!(
        "\n  diff_at_creation: {}",
        diff1.as_deref().unwrap_or("(无)")
    ));
    output.push('\n');

    // Print experiment 2 info
    output.push_str(&format!("\n实验 {} ({})", exp_id_2, exp2.command));
    if let Some(ref hash) = exp2.commit_hash {
        output.push_str(&format!("\n  commit: {}", hash));
    } else {
        output.push_str("\n  commit: (无)");
    }
    output.push_str(&format!(
        "\n  diff_at_creation: {}",
        diff2.as_deref().unwrap_or("(无)")
    ));
    output.push('\n');

    // Git diff between commits
    let hash1 = exp1.commit_hash.as_deref();
    let hash2 = exp2.commit_hash.as_deref();

    match (hash1, hash2) {
        (Some(h1), Some(h2)) => {
            if h1 == h2 {
                return Err(ArcliError::Other(format!(
                    "两个实验的 commit 相同 ('{}')",
                    h1
                )));
            }

            output.push_str(&format!("\ngit diff {}..{}:\n", h1, h2));
            output.push_str("----------------------------------------\n");

            let git_repo = git2::Repository::open(&repo.root)?;

            // Verify commits exist
            let oid1 = git2::Oid::from_str(h1)
                .map_err(|_| ArcliError::Other(format!("无效的 commit hash: {}", h1)))?;
            let oid2 = git2::Oid::from_str(h2)
                .map_err(|_| ArcliError::Other(format!("无效的 commit hash: {}", h2)))?;

            if git_repo.find_commit(oid1).is_err() {
                return Err(ArcliError::CommitNotReachable(h1.to_string()));
            }
            if git_repo.find_commit(oid2).is_err() {
                return Err(ArcliError::CommitNotReachable(h2.to_string()));
            }

            // Use git command for full diff output
            let mut cmd = std::process::Command::new("git");
            cmd.arg("-C").arg(&repo.root);
            if full {
                cmd.arg("diff").arg(format!("{}..{}", h1, h2));
            } else {
                cmd.arg("diff").arg("--stat").arg(format!("{}..{}", h1, h2));
            }
            let result = cmd.output()?;

            if !result.status.success() {
                let stderr = String::from_utf8_lossy(&result.stderr);
                return Err(ArcliError::Other(format!("git diff 失败: {}", stderr)));
            }

            let diff_output = String::from_utf8_lossy(&result.stdout);
            output.push_str(&diff_output);
        }
        (None, _) => {
            return Err(ArcliError::Other(format!(
                "实验 '{}' 未记录 commit hash（非 Git 仓库）",
                exp_id_1
            )));
        }
        (_, None) => {
            return Err(ArcliError::Other(format!(
                "实验 '{}' 未记录 commit hash（非 Git 仓库）",
                exp_id_2
            )));
        }
    }

    Ok(output)
}

pub fn import(
    repo: &Repository,
    path: &str,
    label: &str,
    cmd: &str,
    data_name: Option<String>,
    move_dir: bool,
    yes: bool,
) -> Result<String> {
    let source_path = std::path::PathBuf::from(path);

    // Validate source is a directory
    if !source_path.exists() || !source_path.is_dir() {
        return Err(ArcliError::ImportPathInvalid(format!(
            "'{}' 不是有效目录",
            path
        )));
    }

    // Check if already an experiment directory
    if source_path.join("experiment.json").exists() {
        return Err(ArcliError::ImportPathInvalid(
            "目录已包含 experiment.json，拒绝导入已管理的实验".to_string(),
        ));
    }

    // Confirm if not --yes
    if !yes {
        println!("即将导入目录 '{}' 为实验", source_path.display());
        println!("标签: {}", label);
        println!("命令: {}", cmd);
        if move_dir {
            println!("模式: 移动（原目录将被删除）");
        } else {
            println!("模式: 复制");
        }
        println!("确认? [y/N]");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            return Err(ArcliError::Other("导入已取消".to_string()));
        }
    }

    let db = Database::open(&repo.db_path())?;
    let short_id = db.next_short_id()?;
    let short_id_str = format!("{:03}", short_id);

    let now = chrono::Local::now();
    let timestamp = now.format("%Y-%m-%d-%H%M").to_string();
    let exp_id = format!("run-{}-{}_{}", short_id_str, timestamp, label);

    let exp_dir = repo.experiments_dir().join(&exp_id);
    fs::create_dir_all(&exp_dir)?;

    // Copy or move contents
    if move_dir {
        for entry in fs::read_dir(&source_path)? {
            let entry = entry?;
            let dest = exp_dir.join(entry.file_name());
            fs::rename(entry.path(), dest)?;
        }
    } else {
        fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
            fs::create_dir_all(dst)?;
            for entry in fs::read_dir(src)? {
                let entry = entry?;
                let dest = dst.join(entry.file_name());
                if entry.file_type()?.is_dir() {
                    copy_dir_all(&entry.path(), &dest)?;
                } else {
                    fs::copy(entry.path(), dest)?;
                }
            }
            Ok(())
        }
        copy_dir_all(&source_path, &exp_dir)?;
    }

    // Ensure logs dir exists
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
        "short_id": &short_id_str,
        "status": "finished",
        "created_at": &created_at,
        "updated_at": &created_at,
        "commit_hash": commit_hash,
        "data_used": data_name.as_ref().map(|s| serde_json::Value::String(s.clone())).unwrap_or(serde_json::Value::Null),
        "command": cmd,
        "params": null,
        "notes": format!("Imported from {}", source_path.display()),
        "env": null,
        "started_at": null,
        "finished_at": &created_at,
        "exit_code": null,
        "env_snapshot": null,
        "diff_at_creation": null,
        "artifacts_index": [],
    });

    fs::write(
        exp_dir.join("experiment.json"),
        serde_json::to_string_pretty(&experiment_json)?,
    )?;

    db.insert_experiment(
        &exp_id,
        &short_id_str,
        "finished",
        &created_at,
        commit_hash.as_deref(),
        data_name.as_deref(),
        cmd,
        None,
        Some(&format!("Imported from {}", source_path.display())),
        None,
        None, None, None,
    )?;

    Ok(exp_id)
}

pub fn set_hypothesis(repo: &Repository, exp_id: &str, hypothesis: &str) -> Result<()> {
    let db = Database::open(&repo.db_path())?;
    db.set_hypothesis(exp_id, hypothesis)?;
    update_json_field(repo, exp_id, "hypothesis", serde_json::Value::String(hypothesis.to_string()))?;
    Ok(())
}

pub fn set_lesson(repo: &Repository, exp_id: &str, lesson: &str) -> Result<()> {
    let db = Database::open(&repo.db_path())?;
    db.set_lesson(exp_id, lesson)?;
    update_json_field(repo, exp_id, "lesson", serde_json::Value::String(lesson.to_string()))?;
    Ok(())
}

pub fn add_claim(repo: &Repository, exp_id: &str, claim_id: &str) -> Result<()> {
    let db = Database::open(&repo.db_path())?;
    db.add_claim_to_experiment(exp_id, &claim_id.to_uppercase())?;
    if let Some(exp) = db.get_experiment(exp_id)? {
        let json = crate::commands::db::experiment_to_json(&exp)?;
        let json_path = repo.exp_json_path(exp_id);
        fs::write(&json_path, serde_json::to_string_pretty(&json)?)?;
    }
    Ok(())
}

pub fn remove_claim(repo: &Repository, exp_id: &str, claim_id: &str) -> Result<()> {
    let db = Database::open(&repo.db_path())?;
    db.remove_claim_from_experiment(exp_id, &claim_id.to_uppercase())?;
    if let Some(exp) = db.get_experiment(exp_id)? {
        let json = crate::commands::db::experiment_to_json(&exp)?;
        let json_path = repo.exp_json_path(exp_id);
        fs::write(&json_path, serde_json::to_string_pretty(&json)?)?;
    }
    Ok(())
}

fn update_json_field(repo: &Repository, exp_id: &str, key: &str, value: serde_json::Value) -> Result<()> {
    let json_path = repo.exp_json_path(exp_id);
    if json_path.exists() {
        let content = fs::read_to_string(&json_path)?;
        let mut json: serde_json::Value = serde_json::from_str(&content)?;
        if let Some(obj) = json.as_object_mut() {
            obj.insert(key.to_string(), value);
        }
        fs::write(&json_path, serde_json::to_string_pretty(&json)?)?;
    }
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

        let _ = git2::Repository::init(root);

        fs::create_dir_all(root.join(".research")).unwrap();
        fs::create_dir_all(root.join(".research/hooks")).unwrap();
        fs::write(root.join(".research/hooks/pre-experiment"), "#!/bin/sh\n").unwrap();
        fs::create_dir_all(root.join("experiments")).unwrap();

        let config = crate::config::Config::default();
        config.save(&root.join(".research/config.yaml")).unwrap();

        let db = Database::open(&root.join(".research/research.db")).unwrap();
        db.init_schema().unwrap();

        // Create .gitignore to ignore SQLite temp files
        fs::write(
            root.join(".gitignore"),
            ".research/*.db*\n.research/*.db-wal\n.research/*.db-shm\n",
        )
        .unwrap();

        // Commit all files so workspace is clean for env::check
        let git_repo = git2::Repository::open(root).unwrap();
        let sig = git2::Signature::new("test", "test@test.com", &git2::Time::new(0, 0)).unwrap();
        let mut index = git_repo.index().unwrap();
        index
            .add_all(["."], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = git_repo.find_tree(tree_id).unwrap();
        git_repo
            .commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();

        (
            Repository {
                root: root.to_path_buf(),
            },
            dir,
        )
    }

    fn create_test_experiment(repo: &Repository, exp_id: &str, command: &str) {
        let db = Database::open(&repo.db_path()).unwrap();
        db.insert_experiment(
            exp_id,
            "001",
            "created",
            "2026-01-01T00:00:00Z",
            None,
            None,
            command,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();

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
        )
        .unwrap();
    }

    #[test]
    fn test_new_creates_experiment() {
        let (repo, _dir) = create_test_repo();

        let (exp_id, exp_dir) = new(
            &repo,
            None,
            Some("echo hello".to_string()),
            true,
            None,
            None,
            None,
            None,
            None,
            None, None,
        )
        .unwrap();

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
    fn test_new_uses_custom_experiments_dir() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        let _ = git2::Repository::init(root);

        fs::create_dir_all(root.join(".research")).unwrap();
        fs::create_dir_all(root.join(".research/hooks")).unwrap();
        fs::write(root.join(".research/hooks/pre-experiment"), "#!/bin/sh\n").unwrap();
        fs::create_dir_all(root.join("exps")).unwrap();

        let config = crate::config::Config {
            experiments_dir: "exps".to_string(),
            ..Default::default()
        };
        config.save(&root.join(".research/config.yaml")).unwrap();

        let db = Database::open(&root.join(".research/research.db")).unwrap();
        db.init_schema().unwrap();

        fs::write(
            root.join(".gitignore"),
            ".research/*.db*\n.research/hooks/\n",
        )
        .unwrap();
        let git_repo = git2::Repository::open(root).unwrap();
        let sig = git2::Signature::new("test", "test@test.com", &git2::Time::new(0, 0)).unwrap();
        let mut index = git_repo.index().unwrap();
        index
            .add_all(["."], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = git_repo.find_tree(tree_id).unwrap();
        git_repo
            .commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();

        let repo = Repository {
            root: root.to_path_buf(),
        };

        let (exp_id, _exp_dir) = new(
            &repo,
            None,
            Some("echo hello".to_string()),
            true,
            None,
            None,
            None,
            None,
            None,
            None, None,
        )
        .unwrap();

        // Experiment should be created under exps/ not experiments/
        assert!(root.join("exps").join(&exp_id).exists());
        assert!(!root.join("experiments").join(&exp_id).exists());
        assert!(repo.exp_json_path(&exp_id).exists());
    }

    #[test]
    fn test_new_requires_data_and_cmd() {
        let (repo, _dir) = create_test_repo();

        let result = new(&repo, None, None, false, None, None, None, None, None, None, None);
        assert!(matches!(result, Err(ArcliError::MissingRequiredArg(_))));
    }

    #[test]
    fn test_new_validates_data_asset() {
        let (repo, _dir) = create_test_repo();

        let result = new(
            &repo,
            Some("nonexistent".to_string()),
            Some("echo hello".to_string()),
            false,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(matches!(result, Err(ArcliError::DataNotFound(_))));
    }

    #[test]
    fn test_new_manual_mode_bypasses_data_requirement() {
        let (repo, _dir) = create_test_repo();

        let (exp_id, _) = new(&repo, None, None, true, None, None, None, None, None, None, None).unwrap();

        let db = Database::open(&repo.db_path()).unwrap();
        let exp = db.get_experiment(&exp_id).unwrap().unwrap();
        assert_eq!(exp.status, "created");
        assert_eq!(exp.data_used, None);
    }

    #[test]
    fn test_new_fails_on_dirty_workspace() {
        let (repo, _dir) = create_test_repo();

        // 创建工作区脏文件
        fs::write(repo.root.join("dirty.txt"), "dirty").unwrap();

        let result = new(
            &repo,
            None,
            Some("echo hello".to_string()),
            true,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(matches!(result, Err(ArcliError::WorkspaceNotClean)));
    }

    #[test]
    fn test_new_generates_unique_ids() {
        let (repo, _dir) = create_test_repo();

        let (id1, _) = new(
            &repo,
            None,
            Some("echo 1".to_string()),
            true,
            None,
            None,
            None,
            None,
            None,
            None, None,
        )
        .unwrap();

        // 提交实验文件以保持工作区干净
        let git_repo = git2::Repository::open(&repo.root).unwrap();
        let sig = git2::Signature::new("test", "test@test.com", &git2::Time::new(0, 0)).unwrap();
        let mut index = git_repo.index().unwrap();
        index
            .add_all(["."], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = git_repo.find_tree(tree_id).unwrap();
        let parent = git_repo.head().unwrap().peel_to_commit().unwrap();
        git_repo
            .commit(Some("HEAD"), &sig, &sig, "exp1", &tree, &[&parent])
            .unwrap();

        let (id2, _) = new(
            &repo,
            None,
            Some("echo 2".to_string()),
            true,
            None,
            None,
            None,
            None,
            None,
            None, None,
        )
        .unwrap();

        assert_ne!(id1, id2, "两次 new 调用应生成不同的实验 ID");

        let db = Database::open(&repo.db_path()).unwrap();
        let exp1 = db.get_experiment(&id1).unwrap().unwrap();
        let exp2 = db.get_experiment(&id2).unwrap().unwrap();
        assert_ne!(exp1.short_id, exp2.short_id, "short_id 应唯一");
    }

    #[test]
    fn test_run_creates_and_cleans_pid_file() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-001", "echo hello");

        let pid_path = repo.exp_dir("exp-001").join("pid");

        assert!(!pid_path.exists());

        run(&repo, "exp-001", &[], None).unwrap();

        assert!(!pid_path.exists());

        let db = Database::open(&repo.db_path()).unwrap();
        let exp = db.get_experiment("exp-001").unwrap().unwrap();
        assert_eq!(exp.status, "finished");
        assert_eq!(exp.exit_code, Some(0));
    }

    #[test]
    fn test_run_failure_cleans_pid_file() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-002", "exit 42");

        let pid_path = repo.exp_dir("exp-002").join("pid");

        let code = run(&repo, "exp-002", &[], None).unwrap();
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
        create_test_experiment(&repo, "exp-003", "sleep 2");

        let pid_path = repo.exp_dir("exp-003").join("pid");
        let repo_clone = Repository {
            root: repo.root.clone(),
        };

        let run_handle = thread::spawn(move || {
            run(&repo_clone, "exp-003", &[], None).unwrap();
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
        create_test_experiment(&repo, "exp-008", "sleep 10");

        let pid_path = repo.exp_dir("exp-008").join("pid");
        let repo_run = Repository {
            root: repo.root.clone(),
        };
        let repo_stop = Repository {
            root: repo.root.clone(),
        };

        let run_handle = thread::spawn(move || {
            run(&repo_run, "exp-008", &[], None).unwrap();
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

        stop(&repo_stop, "exp-008", "SIGTERM").unwrap();

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
        create_test_experiment(&repo, "exp-004", "echo hello");

        let result = stop(&repo, "exp-004", "SIGTERM");
        assert!(matches!(result, Err(ArcliError::Other(_))));
    }

    #[test]
    fn test_stop_sends_signal() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-005", "sleep 10");

        let db = Database::open(&repo.db_path()).unwrap();
        db.update_experiment_status(
            "exp-005",
            "running",
            Some("2026-01-01T00:00:00Z"),
            None,
            None,
        )
        .unwrap();

        let mut child = StdCommand::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as i32;

        fs::write(repo.exp_dir("exp-005").join("pid"), pid.to_string()).unwrap();

        stop(&repo, "exp-005", "SIGTERM").unwrap();

        thread::sleep(Duration::from_millis(200));
        let status = child.try_wait().unwrap();
        assert!(status.is_some(), "子进程应已被信号终止");

        // stop() writes stop-intent so run() will converge to "interrupted"
        assert!(
            repo.exp_dir("exp-005").join(".stop").exists(),
            "stop() 应写入 .stop 意图文件"
        );

        // stop() 不再修改 DB 状态或删除 PID 文件，这些由 run() 负责
        let exp = db.get_experiment("exp-005").unwrap().unwrap();
        assert_eq!(exp.status, "running");
        assert!(repo.exp_dir("exp-005").join("pid").exists());

        // cleanup
        let _ = fs::remove_file(repo.exp_dir("exp-005").join(".stop"));
    }

    #[test]
    fn test_stop_invalid_signal_fails() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-009", "sleep 10");

        let db = Database::open(&repo.db_path()).unwrap();
        db.update_experiment_status(
            "exp-009",
            "running",
            Some("2026-01-01T00:00:00Z"),
            None,
            None,
        )
        .unwrap();

        let mut child = StdCommand::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as i32;
        fs::write(repo.exp_dir("exp-009").join("pid"), pid.to_string()).unwrap();

        let result = stop(&repo, "exp-009", "SIGINT");
        assert!(matches!(result, Err(ArcliError::InvalidStatus(_))));

        let result = stop(&repo, "exp-009", "INVALID");
        assert!(matches!(result, Err(ArcliError::InvalidStatus(_))));

        // 清理子进程
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
        let _ = child.wait();
    }

    #[test]
    fn test_stop_does_not_prematurely_finalize_state() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-010", "sleep 10");

        let db = Database::open(&repo.db_path()).unwrap();
        db.update_experiment_status(
            "exp-010",
            "running",
            Some("2026-01-01T00:00:00Z"),
            None,
            None,
        )
        .unwrap();

        let mut child = StdCommand::new("sleep")
            .arg("10")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id() as i32;

        fs::write(repo.exp_dir("exp-010").join("pid"), pid.to_string()).unwrap();

        // 发送 SIGTERM，stop() 应返回成功（信号已投递）
        stop(&repo, "exp-010", "SIGTERM").unwrap();

        // stop() 应写入 .stop 意图文件，但不修改 DB 状态或删除 PID 文件
        assert!(
            repo.exp_dir("exp-010").join(".stop").exists(),
            "stop() 应写入 .stop 意图文件"
        );
        let exp = db.get_experiment("exp-010").unwrap().unwrap();
        assert_eq!(exp.status, "running", "stop() 不应修改实验状态");
        assert!(
            repo.exp_dir("exp-010").join("pid").exists(),
            "stop() 不应删除 PID 文件"
        );

        // 清理：等待子进程被信号终止后清理
        thread::sleep(Duration::from_millis(300));
        let _ = child.try_wait();
        if child.try_wait().unwrap().is_none() {
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
            let _ = child.wait();
        }
        let _ = fs::remove_file(repo.exp_dir("exp-010").join(".stop"));
    }

    #[test]
    fn test_status_and_list() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-006", "echo hello");

        let val = status(&repo, Some("exp-006")).unwrap();
        assert_eq!(val.get("id").unwrap().as_str().unwrap(), "exp-006");

        let exps = list(&repo, None, None).unwrap();
        assert_eq!(exps.len(), 1);
        assert_eq!(exps[0].id, "exp-006");
    }

    #[test]
    fn test_metric_and_param_and_finish() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-007", "echo hello");

        metric(&repo, "exp-007", 1, Some("{\"loss\":0.5}"), &[], &[]).unwrap();

        let db = Database::open(&repo.db_path()).unwrap();
        let metrics = db.get_metrics("exp-007").unwrap();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].metric_value, 0.5);

        param(&repo, "exp-007", "{\"lr\":0.001}").unwrap();
        let exp = db.get_experiment("exp-007").unwrap().unwrap();
        assert!(exp.params.as_ref().unwrap().contains("lr"));

        finish(&repo, "exp-007", "finished", None).unwrap();
        let exp = db.get_experiment("exp-007").unwrap().unwrap();
        assert_eq!(exp.status, "finished");
        assert!(exp.finished_at.is_some());
    }

    #[test]
    fn test_run_timeout_kills_process() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-timeout", "sleep 10");

        let start = std::time::Instant::now();
        let result = run(&repo, "exp-timeout", &[], Some(1));
        let elapsed = start.elapsed();

        assert!(result.is_err(), "超时应返回错误");
        assert!(
            elapsed < std::time::Duration::from_secs(3),
            "应在 1 秒后终止"
        );

        let db = Database::open(&repo.db_path()).unwrap();
        let exp = db.get_experiment("exp-timeout").unwrap().unwrap();
        assert_eq!(exp.status, "interrupted");
    }

    #[test]
    fn test_run_timeout_clean_exit() {
        let (repo, _dir) = create_test_repo();
        create_test_experiment(&repo, "exp-clean", "echo done");

        let code = run(&repo, "exp-clean", &[], Some(10)).unwrap();
        assert_eq!(code, 0);

        let db = Database::open(&repo.db_path()).unwrap();
        let exp = db.get_experiment("exp-clean").unwrap().unwrap();
        assert_eq!(exp.status, "finished");
    }

    #[test]
    fn test_run_injects_env_vars() {
        let (repo, _dir) = create_test_repo();
        let script = repo.root.join("print_env.sh");
        fs::write(
            &script,
            "#!/bin/sh\necho \"EXP_DIR=$ARCLI_EXP_DIR\"\necho \"EXP_ID=$ARCLI_EXP_ID\"\necho \"REPO_ROOT=$ARCLI_REPO_ROOT\"\necho \"LOG_DIR=$ARCLI_LOG_DIR\"\n",
        )
        .unwrap();
        let script_path = script.to_string_lossy().to_string();
        create_test_experiment(&repo, "exp-env", &format!("sh '{}'", script_path));

        run(&repo, "exp-env", &[], None).unwrap();

        let log_path = repo.exp_log_path("exp-env");
        let log_content = fs::read_to_string(&log_path).unwrap();
        assert!(log_content.contains("EXP_DIR="));
        assert!(log_content.contains("EXP_ID=exp-env"));
        assert!(log_content.contains("REPO_ROOT="));
        assert!(log_content.contains("LOG_DIR="));
    }

    #[test]
    fn test_discover_artifacts_skips_logs() {
        let (repo, _dir) = create_test_repo();
        let exp_dir = repo.exp_dir("exp-art");
        fs::create_dir_all(&exp_dir).unwrap();
        fs::create_dir_all(exp_dir.join("logs")).unwrap();
        fs::create_dir_all(exp_dir.join("artifacts")).unwrap();
        fs::write(exp_dir.join("logs/run.log"), "log content").unwrap();
        fs::write(exp_dir.join("artifacts/results.csv"), "a,b\n1,2\n").unwrap();
        fs::write(exp_dir.join("experiment.json"), "{}").unwrap();

        let artifacts = discover_artifacts(&exp_dir, &repo.root).unwrap();
        let paths: Vec<&str> = artifacts.iter().map(|a| a.path.as_str()).collect();
        assert!(paths.contains(&"artifacts/results.csv"));
        assert!(!paths.contains(&"logs/run.log"));
        assert!(!paths.contains(&"experiment.json"));
    }

    #[test]
    fn test_discover_artifacts_respects_gitignore() {
        let (repo, _dir) = create_test_repo();
        let exp_dir = repo.exp_dir("exp-git");
        fs::create_dir_all(&exp_dir).unwrap();
        fs::write(exp_dir.join("tracked.txt"), "tracked").unwrap();
        fs::write(exp_dir.join("ignored.pyc"), "ignored").unwrap();
        fs::write(exp_dir.join("experiment.json"), "{}").unwrap();

        let artifacts = discover_artifacts(&exp_dir, &repo.root).unwrap();
        let paths: Vec<&str> = artifacts.iter().map(|a| a.path.as_str()).collect();
        assert!(paths.contains(&"tracked.txt"));
    }

    #[test]
    fn test_diff_at_creation_field_exists() {
        let (repo, _dir) = create_test_repo();

        let (exp_id, _) = new(
            &repo,
            None,
            Some("echo hello".to_string()),
            true,
            None,
            None,
            None,
            None,
            None,
            None, None,
        )
        .unwrap();

        let json_path = repo.exp_json_path(&exp_id);
        let content = fs::read_to_string(&json_path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        // diff_at_creation field should exist (null when workspace is clean)
        assert!(
            json.get("diff_at_creation").is_some(),
            "diff_at_creation 字段应存在"
        );
    }

    #[test]
    fn test_import_creates_experiment() {
        let (repo, _dir) = create_test_repo();
        let source = repo.root.join("legacy-exp");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("results.csv"), "a,b\n1,2\n").unwrap();

        let exp_id = import(
            &repo,
            source.to_str().unwrap(),
            "legacy",
            "python run.py",
            None,
            false,
            true,
        )
        .unwrap();

        let exp_dir = repo.exp_dir(&exp_id);
        assert!(exp_dir.exists());
        assert!(exp_dir.join("results.csv").exists());
        assert!(exp_dir.join("experiment.json").exists());

        let db = Database::open(&repo.db_path()).unwrap();
        let exp = db.get_experiment(&exp_id).unwrap().unwrap();
        assert_eq!(exp.status, "finished");
        assert_eq!(exp.command, "python run.py");
    }

    #[test]
    fn test_import_rejects_existing_experiment() {
        let (repo, _dir) = create_test_repo();
        let source = repo.root.join("existing-exp");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("experiment.json"), "{}").unwrap();

        let result = import(&repo, source.to_str().unwrap(), "existing", "cmd", None, false, true);
        assert!(matches!(result, Err(ArcliError::ImportPathInvalid(_))));
    }
}
