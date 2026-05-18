use std::path::PathBuf;

use clap::Parser;

use arcli::cli::{
    Cli, ClaimCommands, Commands, ConfigCommands, DataCommands, DbCommands, EnvCommands, ExpCommands,
    LogCommands, ProjectCommands,
};
use arcli::commands::{claim, config, data, db, env, exp, log, project};
use arcli::error::ArcliError;
use arcli::output;
use arcli::repo::Repository;

fn main() {
    let cli = Cli::parse();

    let json_mode = match &cli.command {
        Commands::Project(cmd) => matches!(cmd, ProjectCommands::Init { json: true, .. }),
        Commands::Env(cmd) => matches!(
            cmd,
            EnvCommands::Status { json: true, .. } | EnvCommands::Check { json: true, .. }
        ),
        Commands::Data(cmd) => matches!(
            cmd,
            DataCommands::Register { json: true, .. }
                | DataCommands::List { json: true }
                | DataCommands::Info { json: true, .. }
                | DataCommands::Update { json: true, .. }
                | DataCommands::Verify { json: true, .. }
        ),
        Commands::Exp(cmd) => matches!(
            cmd,
            ExpCommands::New { json: true, .. }
                | ExpCommands::Run { json: true, .. }
                | ExpCommands::Stop { json: true, .. }
                | ExpCommands::Status { json: true, .. }
                | ExpCommands::Metric { json: true, .. }
                | ExpCommands::Param { json: true, .. }
                | ExpCommands::Finish { json: true, .. }
                | ExpCommands::Export { json: true, .. }
                | ExpCommands::List { json: true, .. }
                | ExpCommands::Import { json: true, .. }
                | ExpCommands::Diff { json: true, .. }
        ),
        Commands::Claim(cmd) => matches!(
            cmd,
            ClaimCommands::Add { json: true, .. }
                | ClaimCommands::List { json: true }
                | ClaimCommands::Show { json: true, .. }
                | ClaimCommands::Verify { json: true, .. }
                | ClaimCommands::Unverify { json: true, .. }
                | ClaimCommands::Update { json: true, .. }
                | ClaimCommands::Remove { json: true, .. }
        ),
        Commands::Db(cmd) => matches!(
            cmd,
            DbCommands::Sync { json: true, .. }
                | DbCommands::ExportAll { json: true, .. }
                | DbCommands::Import { json: true, .. }
                | DbCommands::Status { json: true }
        ),
        Commands::Log(cmd) => matches!(cmd, LogCommands::Show { json: true, .. }),
        Commands::Config(cmd) => matches!(
            cmd,
            ConfigCommands::Get { json: true, .. } | ConfigCommands::Set { json: true, .. }
        ),
    };

    let result = match &cli.command {
        Commands::Project(cmd) => handle_project(cmd, cli.repo.as_deref(), json_mode),
        Commands::Env(cmd) => handle_env(cmd, cli.repo.as_deref(), json_mode),
        Commands::Data(cmd) => handle_data(cmd, cli.repo.as_deref(), json_mode),
        Commands::Exp(cmd) => handle_exp(cmd, cli.repo.as_deref(), json_mode),
        Commands::Claim(cmd) => handle_claim(cmd, cli.repo.as_deref(), json_mode),
        Commands::Db(cmd) => handle_db(cmd, cli.repo.as_deref(), json_mode),
        Commands::Log(cmd) => handle_log(cmd, cli.repo.as_deref(), json_mode),
        Commands::Config(cmd) => handle_config(cmd, cli.repo.as_deref(), json_mode),
    };

    if let Err(e) = result {
        output::print_error(&e, json_mode);
        std::process::exit(1);
    }
}

fn get_repo(repo_override: Option<&str>) -> Result<Repository, ArcliError> {
    let start = repo_override.map(PathBuf::from);
    let start_ref = start.as_deref();
    Repository::discover(start_ref)
}

fn handle_project(
    cmd: &ProjectCommands,
    _repo_override: Option<&str>,
    json_mode: bool,
) -> Result<(), ArcliError> {
    match cmd {
        ProjectCommands::Init {
            path,
            name,
            force,
            exp_dir,
            research_type,
            stack,
            zh,
            json: _,
        } => {
            let created = project::init(
                path.clone(),
                name.clone(),
                *force,
                exp_dir.clone(),
                research_type.clone(),
                stack.clone(),
                *zh,
            )?;
            if json_mode {
                println!("{}", serde_json::to_string_pretty(&created)?);
            } else {
                println!("项目初始化完成，创建以下文件/目录：");
                for item in created {
                    println!("  {}", item);
                }
            }
            Ok(())
        }
    }
}

fn handle_env(
    cmd: &EnvCommands,
    repo_override: Option<&str>,
    json_mode: bool,
) -> Result<(), ArcliError> {
    let repo = get_repo(repo_override)?;
    match cmd {
        EnvCommands::Status { json: _ } => {
            let status = env::status(&repo)?;
            if json_mode {
                output::print_json(&status);
            } else {
                println!("仓库根目录: {}", status.repo_root);
                println!("当前分支: {}", status.git.branch);
                println!("提交哈希: {}", status.git.commit_hash);
                println!("工作区干净: {}", status.git.is_clean);
                if let Some(ref time) = status.git.last_commit_time {
                    println!("最后提交时间: {}", time);
                }
                println!("\n活跃实验:");
                if status.active_experiments.is_empty() {
                    println!("  无");
                } else {
                    for exp in &status.active_experiments {
                        println!("  {} - {}", exp.id, exp.status);
                    }
                }
                println!("\n数据资产:");
                if status.data_assets.is_empty() {
                    println!("  无");
                } else {
                    for asset in &status.data_assets {
                        println!("  {}", asset);
                    }
                }
            }
            Ok(())
        }
        EnvCommands::Check { strict, json: _ } => {
            env::check(&repo, *strict)?;
            if json_mode {
                println!("{}", serde_json::json!({"status": "ok"}));
            } else {
                println!("工作区检查通过");
            }
            Ok(())
        }
    }
}

fn handle_data(
    cmd: &DataCommands,
    repo_override: Option<&str>,
    json_mode: bool,
) -> Result<(), ArcliError> {
    let repo = get_repo(repo_override)?;
    match cmd {
        DataCommands::Register {
            path,
            name,
            desc,
            checksum,
            json: _,
        } => {
            data::register(&repo, path, name, desc.clone(), checksum.clone())?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"name": name, "status": "registered"})
                );
            } else {
                println!("数据资产 '{}' 已注册", name);
            }
            Ok(())
        }
        DataCommands::List { json: _ } => {
            let datasets = data::list(&repo)?;
            if json_mode {
                output::print_json(&datasets);
            } else if datasets.is_empty() {
                println!("无已注册的数据资产");
            } else {
                for ds in &datasets {
                    println!("{}", ds.name);
                }
            }
            Ok(())
        }
        DataCommands::Info { name, json: _ } => {
            let ds = data::info(&repo, name)?;
            let status = data::dataset_status(&repo, name)?;
            if json_mode {
                let mut val = serde_json::to_value(&ds)?;
                if let Some(obj) = val.as_object_mut() {
                    obj.insert("status".to_string(), serde_json::Value::String(status));
                }
                output::print_json(&val);
            } else {
                println!("名称: {}", ds.name);
                println!("路径: {}", ds.path);
                if let Some(ref cs) = ds.checksum {
                    println!("注册校验和: {}", cs);
                }
                println!("状态: {}", status.to_uppercase());
                if let Some(ref desc) = ds.description {
                    println!("描述: {}", desc);
                }
                println!("注册时间: {}", ds.registered_at);
            }
            Ok(())
        }
        DataCommands::Update {
            name,
            path,
            recompute_checksum,
            json: _,
        } => {
            data::update(&repo, name, path.clone(), *recompute_checksum)?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"name": name, "status": "updated"})
                );
            } else {
                println!("数据资产 '{}' 已更新", name);
            }
            Ok(())
        }
        DataCommands::Verify {
            changed_only,
            json: _,
        } => {
            let results = data::verify(&repo, *changed_only)?;
            if json_mode {
                output::print_json(&results);
            } else {
                println!(
                    "{:<20} {:<15} {:<10} 注册时间",
                    "名称", "路径", "状态"
                );
                for r in &results {
                    println!(
                        "{:<20} {:<15} {:<10} {}",
                        r.name,
                        r.path,
                        r.status.to_uppercase(),
                        r.registered_at
                    );
                }
            }
            Ok(())
        }
    }
}

fn handle_exp(
    cmd: &ExpCommands,
    repo_override: Option<&str>,
    json_mode: bool,
) -> Result<(), ArcliError> {
    let repo = get_repo(repo_override)?;
    match cmd {
        ExpCommands::New {
            data,
            cmd: command,
            manual,
            label,
            params,
            notes,
            env: env_name,
            template,
            claims,
            hypothesis,
            json: _,
        } => {
            let (exp_id, exp_dir) = exp::new(
                &repo,
                data.clone(),
                command.clone(),
                *manual,
                label.clone(),
                params.clone(),
                notes.clone(),
                env_name.clone(),
                template.clone(),
                claims.clone(),
                hypothesis.clone(),
            )?;
            if json_mode {
                let result = serde_json::json!({
                    "id": exp_id,
                    "directory": exp_dir,
                });
                output::print_json(&result);
            } else {
                println!("实验已创建");
                println!("  ID: {}", exp_id);
                println!("  目录: {}", exp_dir);
            }
            Ok(())
        }
        ExpCommands::Run {
            exp_id,
            args,
            timeout,
            json: _,
        } => {
            let exit_code = exp::run(&repo, exp_id, args, *timeout)?;
            if json_mode {
                let result = serde_json::json!({ "exit_code": exit_code });
                output::print_json(&result);
            } else {
                println!("实验完成，退出码: {}", exit_code);
            }
            Ok(())
        }
        ExpCommands::Stop {
            exp_id,
            signal,
            json: _,
        } => {
            exp::stop(&repo, exp_id, signal)?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"exp_id": exp_id, "status": "stopped"})
                );
            } else {
                println!("实验 {} 已终止", exp_id);
            }
            Ok(())
        }
        ExpCommands::Status { exp_id, json: _ } => {
            let status = exp::status(&repo, exp_id.as_deref())?;
            if json_mode {
                output::print_json(&status);
            } else if let Some(obj) = status.as_object() {
                for (k, v) in obj {
                    println!("{}: {}", k, v);
                }
            }
            Ok(())
        }
        ExpCommands::List {
            status,
            since,
            json: _,
        } => {
            let exps = exp::list(&repo, status.as_deref(), since.as_deref())?;
            if json_mode {
                output::print_json(&exps);
            } else if exps.is_empty() {
                println!("无实验记录");
            } else {
                println!("{:<30} {:<12} {:<20} 命令", "ID", "状态", "创建时间");
                for e in &exps {
                    println!(
                        "{:<30} {:<12} {:<20} {}",
                        e.id, e.status, e.created_at, e.command
                    );
                }
            }
            Ok(())
        }
        ExpCommands::Export {
            exp_id,
            output,
            json: _,
        } => {
            let path = exp::export(&repo, exp_id, output.as_deref())?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"exp_id": exp_id, "path": path})
                );
            } else {
                println!("实验已导出到: {}", path);
            }
            Ok(())
        }
        ExpCommands::Metric {
            exp_id,
            step,
            metrics_json,
            keys,
            vals,
            json: _,
        } => {
            exp::metric(&repo, exp_id, *step, metrics_json.as_deref(), keys, vals)?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"exp_id": exp_id, "step": step, "status": "recorded"})
                );
            } else {
                println!("指标已记录到实验 {}", exp_id);
            }
            Ok(())
        }
        ExpCommands::Param {
            exp_id,
            params_json,
            json: _,
        } => {
            exp::param(&repo, exp_id, params_json)?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"exp_id": exp_id, "status": "updated"})
                );
            } else {
                println!("参数已更新到实验 {}", exp_id);
            }
            Ok(())
        }
        ExpCommands::Finish {
            exp_id,
            status,
            message,
            json: _,
        } => {
            exp::finish(&repo, exp_id, status, message.as_deref())?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"exp_id": exp_id, "status": status})
                );
            } else {
                println!("实验 {} 已标记为 {}", exp_id, status);
            }
            Ok(())
        }
        ExpCommands::Import {
            path,
            label,
            cmd,
            data,
            move_dir,
            yes,
            json: _,
        } => {
            let exp_id = exp::import(&repo, path, label, cmd, data.clone(), *move_dir, *yes)?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"id": exp_id, "status": "imported"})
                );
            } else {
                println!("实验已导入: {}", exp_id);
            }
            Ok(())
        }
        ExpCommands::Diff {
            exp_id_1,
            exp_id_2,
            full,
            json: _,
        } => {
            let diff_output = exp::diff(&repo, exp_id_1, exp_id_2, *full)?;
            if json_mode {
                println!("{}", serde_json::json!({"diff": diff_output}));
            } else {
                println!("{}", diff_output);
            }
            Ok(())
        }
        ExpCommands::Claim {
            exp_id,
            add,
            remove,
            json: _,
        } => {
            if let Some(claim_id) = add {
                claim::verify(&repo, claim_id, exp_id)?;
            }
            if let Some(claim_id) = remove {
                exp::remove_claim(&repo, exp_id, claim_id)?;
            }
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"exp_id": exp_id, "status": "updated"})
                );
            } else {
                println!("实验 {} claim 关联已更新", exp_id);
            }
            Ok(())
        }
        ExpCommands::Hypothesis {
            exp_id,
            set,
            json: _,
        } => {
            exp::set_hypothesis(&repo, exp_id, set)?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"exp_id": exp_id, "status": "updated"})
                );
            } else {
                println!("实验 {} hypothesis 已更新", exp_id);
            }
            Ok(())
        }
        ExpCommands::Lesson {
            exp_id,
            set,
            json: _,
        } => {
            exp::set_lesson(&repo, exp_id, set)?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"exp_id": exp_id, "status": "updated"})
                );
            } else {
                println!("实验 {} lesson 已更新", exp_id);
            }
            Ok(())
        }
    }
}

fn handle_claim(
    cmd: &ClaimCommands,
    repo_override: Option<&str>,
    json_mode: bool,
) -> Result<(), ArcliError> {
    let repo = get_repo(repo_override)?;
    match cmd {
        ClaimCommands::Add {
            id,
            statement,
            falsification,
            json: _,
        } => {
            claim::add(&repo, id, statement, falsification)?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"id": id.to_uppercase(), "status": "created"})
                );
            } else {
                println!("claim '{}' 已创建", id.to_uppercase());
            }
            Ok(())
        }
        ClaimCommands::List { json: _ } => {
            let claims = claim::list(&repo)?;
            if json_mode {
                output::print_json(&claims);
            } else if claims.is_empty() {
                println!("无已定义的 claim");
            } else {
                for c in &claims {
                    println!(
                        "  {} — {} ({} 实验验证)",
                        c.id, c.statement, c.verified_by_count
                    );
                }
            }
            Ok(())
        }
        ClaimCommands::Show { id, json: _ } => {
            let detail = claim::show(&repo, id)?;
            if json_mode {
                output::print_json(&detail);
            } else {
                println!("Claim: {}", detail.id);
                println!("Statement: {}", detail.statement);
                println!("Falsification: {}", detail.falsification);
                println!("Verified by:");
                if detail.verified_by.is_empty() {
                    println!("  无");
                } else {
                    for ve in &detail.verified_by {
                        println!(
                            "  {} — status={} commit={}",
                            ve.exp_id,
                            ve.status,
                            ve.commit_hash.as_deref().unwrap_or("N/A")
                        );
                    }
                }
            }
            Ok(())
        }
        ClaimCommands::Verify { id, exp, json: _ } => {
            claim::verify(&repo, id, exp)?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"claim": id, "exp": exp, "status": "verified"})
                );
            } else {
                println!("claim '{}' 已绑定实验 '{}'", id, exp);
            }
            Ok(())
        }
        ClaimCommands::Unverify { id, exp, json: _ } => {
            claim::unverify(&repo, id, exp)?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"claim": id, "exp": exp, "status": "unverified"})
                );
            } else {
                println!("已解除 claim '{}' 与实验 '{}' 的绑定", id, exp);
            }
            Ok(())
        }
        ClaimCommands::Update {
            id,
            statement,
            falsification,
            json: _,
        } => {
            claim::update(&repo, id, statement.as_deref(), falsification.as_deref())?;
            if json_mode {
                println!("{}", serde_json::json!({"id": id, "status": "updated"}));
            } else {
                println!("claim '{}' 已更新", id);
            }
            Ok(())
        }
        ClaimCommands::Remove { id, force, json: _ } => {
            claim::remove(&repo, id, *force)?;
            if json_mode {
                println!("{}", serde_json::json!({"id": id, "status": "removed"}));
            } else {
                println!("claim '{}' 已删除", id);
            }
            Ok(())
        }
    }
}

fn handle_db(
    cmd: &DbCommands,
    repo_override: Option<&str>,
    json_mode: bool,
) -> Result<(), ArcliError> {
    let repo = get_repo(repo_override)?;
    match cmd {
        DbCommands::Sync { mode, json: _ } => {
            db::sync(&repo, mode)?;
            if json_mode {
                println!("{}", serde_json::json!({"mode": mode, "status": "synced"}));
            } else {
                println!("数据库同步完成 (模式: {})", mode);
            }
            Ok(())
        }
        DbCommands::ExportAll { out_dir, json: _ } => {
            db::export_all(&repo, out_dir.as_deref())?;
            if json_mode {
                println!("{}", serde_json::json!({"status": "exported"}));
            } else {
                println!("全量导出完成");
            }
            Ok(())
        }
        DbCommands::Import { from, json: _ } => {
            db::import_from(&repo, from)?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"from": from, "status": "imported"})
                );
            } else {
                println!("导入完成: {}", from);
            }
            Ok(())
        }
        DbCommands::Status { json: _ } => {
            let status = db::status(&repo)?;
            if json_mode {
                output::print_json(&status);
            } else {
                println!("需要导出: {:?}", status.need_export);
                println!("需要导入: {:?}", status.need_import);
                println!("已同步: {:?}", status.in_sync);
                println!("不在数据库中: {:?}", status.not_in_db);
                if !status.conflicts.is_empty() {
                    println!("冲突: {:?}", status.conflicts);
                }
            }
            Ok(())
        }
    }
}

fn handle_log(
    cmd: &LogCommands,
    repo_override: Option<&str>,
    json_mode: bool,
) -> Result<(), ArcliError> {
    let repo = get_repo(repo_override)?;
    match cmd {
        LogCommands::Show {
            exp_id,
            tail,
            follow,
            json: _,
        } => {
            if json_mode && !follow {
                let log_path = repo.exp_log_path(exp_id);
                if !log_path.exists() {
                    return Err(ArcliError::Other(format!(
                        "实验 '{}' 的日志文件不存在",
                        exp_id
                    )));
                }
                let content = std::fs::read_to_string(&log_path)?;
                let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
                let output_lines = match tail {
                    Some(n) => {
                        let start = lines.len().saturating_sub(*n);
                        lines[start..].to_vec()
                    }
                    None => lines,
                };
                println!(
                    "{}",
                    serde_json::json!({"exp_id": exp_id, "lines": output_lines})
                );
            } else {
                log::show(&repo, exp_id, *tail, *follow)?;
            }
            Ok(())
        }
    }
}

fn handle_config(
    cmd: &ConfigCommands,
    repo_override: Option<&str>,
    json_mode: bool,
) -> Result<(), ArcliError> {
    let repo = get_repo(repo_override)?;
    match cmd {
        ConfigCommands::Get { key, json: _ } => {
            let value = config::get(&repo, key)?;
            if json_mode {
                let result = serde_json::json!({ "key": key, "value": value });
                output::print_json(&result);
            } else {
                println!("{}", value);
            }
            Ok(())
        }
        ConfigCommands::Set { key, value, json: _ } => {
            config::set(&repo, key, value)?;
            if json_mode {
                println!(
                    "{}",
                    serde_json::json!({"key": key, "value": value, "status": "updated"})
                );
            } else {
                println!("配置已更新: {} = {}", key, value);
            }
            Ok(())
        }
    }
}
