use std::path::PathBuf;

use clap::Parser;

use rcli::cli::{Cli, Commands, ConfigCommands, DataCommands, DbCommands, EnvCommands, ExpCommands, LogCommands, ProjectCommands};
use rcli::commands::{config, data, db, env, exp, log, project};
use rcli::error::RcliError;
use rcli::output;
use rcli::repo::Repository;

fn main() {
    let cli = Cli::parse();

    let result = match &cli.command {
        Commands::Project(cmd) => handle_project(cmd, cli.json),
        Commands::Env(cmd) => handle_env(cmd, cli.repo.as_deref(), cli.json),
        Commands::Data(cmd) => handle_data(cmd, cli.repo.as_deref(), cli.json),
        Commands::Exp(cmd) => handle_exp(cmd, cli.repo.as_deref(), cli.json),
        Commands::Db(cmd) => handle_db(cmd, cli.repo.as_deref(), cli.json),
        Commands::Log(cmd) => handle_log(cmd, cli.repo.as_deref(), cli.json),
        Commands::Config(cmd) => handle_config(cmd, cli.repo.as_deref(), cli.json),
    };

    if let Err(e) = result {
        output::print_error(&e, cli.json);
        std::process::exit(1);
    }
}

fn get_repo(repo_override: Option<&str>) -> Result<Repository, RcliError> {
    let start = repo_override.map(PathBuf::from);
    let start_ref = start.as_deref();
    Repository::discover(start_ref)
}

fn handle_project(cmd: &ProjectCommands, json_mode: bool) -> Result<(), RcliError> {
    match cmd {
        ProjectCommands::Init { path, name, force, exp_dir } => {
            let created = project::init(path.clone(), name.clone(), *force, exp_dir.clone())?;
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

fn handle_env(cmd: &EnvCommands, repo_override: Option<&str>, json_mode: bool) -> Result<(), RcliError> {
    let repo = get_repo(repo_override)?;
    match cmd {
        EnvCommands::Status => {
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
        EnvCommands::Check { strict } => {
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

fn handle_data(cmd: &DataCommands, repo_override: Option<&str>, json_mode: bool) -> Result<(), RcliError> {
    let repo = get_repo(repo_override)?;
    match cmd {
        DataCommands::Register { path, name, desc, checksum } => {
            data::register(&repo, path, name, desc.clone(), checksum.clone())?;
            if json_mode {
                println!("{}", serde_json::json!({"name": name, "status": "registered"}));
            } else {
                println!("数据资产 '{}' 已注册", name);
            }
            Ok(())
        }
        DataCommands::List => {
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
        DataCommands::Info { name } => {
            let ds = data::info(&repo, name)?;
            if json_mode {
                output::print_json(&ds);
            } else {
                println!("名称: {}", ds.name);
                println!("路径: {}", ds.path);
                if let Some(ref cs) = ds.checksum {
                    println!("校验和: {}", cs);
                }
                if let Some(ref desc) = ds.description {
                    println!("描述: {}", desc);
                }
                println!("注册时间: {}", ds.registered_at);
            }
            Ok(())
        }
        DataCommands::Update { name, path, recompute_checksum } => {
            data::update(&repo, name, path.clone(), *recompute_checksum
            )?;
            if json_mode {
                println!("{}", serde_json::json!({"name": name, "status": "updated"}));
            } else {
                println!("数据资产 '{}' 已更新", name);
            }
            Ok(())
        }
    }
}

fn handle_exp(cmd: &ExpCommands, repo_override: Option<&str>, json_mode: bool) -> Result<(), RcliError> {
    let repo = get_repo(repo_override)?;
    match cmd {
        ExpCommands::New { data, cmd: command, manual, label, params, notes, env: env_name, template } => {
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
        ExpCommands::Run { exp_id, args } => {
            let exit_code = exp::run(&repo, exp_id, args)?;
            if json_mode {
                let result = serde_json::json!({ "exit_code": exit_code });
                output::print_json(&result);
            } else {
                println!("实验完成，退出码: {}", exit_code);
            }
            Ok(())
        }
        ExpCommands::Stop { exp_id, signal } => {
            exp::stop(&repo, exp_id, signal)?;
            if json_mode {
                println!("{}", serde_json::json!({"exp_id": exp_id, "status": "stopped"}));
            } else {
                println!("实验 {} 已终止", exp_id);
            }
            Ok(())
        }
        ExpCommands::Status { exp_id } => {
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
        ExpCommands::List { status, since } => {
            let exps = exp::list(&repo, status.as_deref(), since.as_deref())?;
            if json_mode {
                output::print_json(&exps);
            } else if exps.is_empty() {
                println!("无实验记录");
            } else {
                println!("{:<30} {:<12} {:<20} 命令", "ID", "状态", "创建时间");
                for e in &exps {
                    println!("{:<30} {:<12} {:<20} {}", e.id, e.status, e.created_at, e.command);
                }
            }
            Ok(())
        }
        ExpCommands::Export { exp_id, output } => {
            let path = exp::export(&repo, exp_id, output.as_deref())?;
            if json_mode {
                println!("{}", serde_json::json!({"exp_id": exp_id, "path": path}));
            } else {
                println!("实验已导出到: {}", path);
            }
            Ok(())
        }
        ExpCommands::Metric { exp_id, step, metrics_json, keys, vals } => {
            exp::metric(&repo, exp_id, *step, metrics_json.as_deref(), keys, vals)?;
            if json_mode {
                println!("{}", serde_json::json!({"exp_id": exp_id, "step": step, "status": "recorded"}));
            } else {
                println!("指标已记录到实验 {}", exp_id);
            }
            Ok(())
        }
        ExpCommands::Param { exp_id, params_json } => {
            exp::param(&repo, exp_id, params_json)?;
            if json_mode {
                println!("{}", serde_json::json!({"exp_id": exp_id, "status": "updated"}));
            } else {
                println!("参数已更新到实验 {}", exp_id);
            }
            Ok(())
        }
        ExpCommands::Finish { exp_id, status, message } => {
            exp::finish(&repo, exp_id, status, message.as_deref())?;
            if json_mode {
                println!("{}", serde_json::json!({"exp_id": exp_id, "status": status}));
            } else {
                println!("实验 {} 已标记为 {}", exp_id, status);
            }
            Ok(())
        }
    }
}

fn handle_db(cmd: &DbCommands, repo_override: Option<&str>, json_mode: bool) -> Result<(), RcliError> {
    let repo = get_repo(repo_override)?;
    match cmd {
        DbCommands::Sync { mode } => {
            db::sync(&repo, mode)?;
            if json_mode {
                println!("{}", serde_json::json!({"mode": mode, "status": "synced"}));
            } else {
                println!("数据库同步完成 (模式: {})", mode);
            }
            Ok(())
        }
        DbCommands::ExportAll { out_dir } => {
            db::export_all(&repo, out_dir.as_deref())?;
            if json_mode {
                println!("{}", serde_json::json!({"status": "exported"}));
            } else {
                println!("全量导出完成");
            }
            Ok(())
        }
        DbCommands::Import { from } => {
            db::import_from(&repo, from)?;
            if json_mode {
                println!("{}", serde_json::json!({"from": from, "status": "imported"}));
            } else {
                println!("导入完成: {}", from);
            }
            Ok(())
        }
        DbCommands::Status => {
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

fn handle_log(cmd: &LogCommands, repo_override: Option<&str>, json_mode: bool) -> Result<(), RcliError> {
    let repo = get_repo(repo_override)?;
    match cmd {
        LogCommands::Show { exp_id, tail, follow } => {
            if json_mode && !follow {
                let log_path = repo.exp_log_path(exp_id);
                if !log_path.exists() {
                    return Err(RcliError::Other(format!("实验 '{}' 的日志文件不存在", exp_id)));
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
                println!("{}", serde_json::json!({"exp_id": exp_id, "lines": output_lines}));
            } else {
                log::show(&repo, exp_id, *tail, *follow)?;
            }
            Ok(())
        }
    }
}

fn handle_config(cmd: &ConfigCommands, repo_override: Option<&str>, json_mode: bool) -> Result<(), RcliError> {
    let repo = get_repo(repo_override)?;
    match cmd {
        ConfigCommands::Get { key } => {
            let value = config::get(&repo, key)?;
            if json_mode {
                let result = serde_json::json!({ "key": key, "value": value });
                output::print_json(&result);
            } else {
                println!("{}", value);
            }
            Ok(())
        }
        ConfigCommands::Set { key, value } => {
            config::set(&repo, key, value)?;
            if json_mode {
                println!("{}", serde_json::json!({"key": key, "value": value, "status": "updated"}));
            } else {
                println!("配置已更新: {} = {}", key, value);
            }
            Ok(())
        }
    }
}
