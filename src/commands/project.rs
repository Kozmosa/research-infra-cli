use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use crate::cli::{ResearchType, TechStack};
use crate::config::Config;
use crate::db::Database;
use crate::error::{ArcliError, Result};

const README_TEMPLATE: &str = include_str!("../templates/readme/research.md");
const README_ZH_TEMPLATE: &str = include_str!("../templates/readme/research.zh.md");
const GITIGNORE_BASE: &str = include_str!("../templates/gitignore/base.txt");
const GITIGNORE_PYTHON: &str = include_str!("../templates/gitignore/python.txt");
const GITIGNORE_RUST: &str = include_str!("../templates/gitignore/rust.txt");
const GITIGNORE_JULIA: &str = include_str!("../templates/gitignore/julia.txt");
const GITIGNORE_GO: &str = include_str!("../templates/gitignore/go.txt");
// CONFIG_TEMPLATE reserved for future use
// const CONFIG_TEMPLATE: &str = include_str!("../templates/config/default.yaml");
const PROJECT_BASIS_TEMPLATE: &str = include_str!("../templates/docs/project_basis.md");
const HOOKS_README_TEMPLATE: &str = include_str!("../templates/hooks/README.md");

fn render_template(template: &str, vars: &HashMap<&str, &str>) -> String {
    let mut result = template.to_string();
    for (key, val) in vars {
        result = result.replace(&format!("{{{}}}", key), val);
    }
    result
}

fn detect_stack(target: &Path) -> TechStack {
    if target.join("requirements.txt").exists()
        || target.join("pyproject.toml").exists()
        || target.join("setup.py").exists()
    {
        return TechStack::Python;
    }
    if target.join("Cargo.toml").exists() {
        return TechStack::Rust;
    }
    if target.join("Project.toml").exists() {
        return TechStack::Julia;
    }
    if target.join("go.mod").exists() {
        return TechStack::Go;
    }
    TechStack::Python
}

fn stack_gitignore(stack: &TechStack) -> &'static str {
    match stack {
        TechStack::Python => GITIGNORE_PYTHON,
        TechStack::Rust => GITIGNORE_RUST,
        TechStack::Julia => GITIGNORE_JULIA,
        TechStack::Go => GITIGNORE_GO,
    }
}

fn stack_name(stack: &TechStack) -> &'static str {
    match stack {
        TechStack::Python => "Python",
        TechStack::Rust => "Rust",
        TechStack::Julia => "Julia",
        TechStack::Go => "Go",
    }
}

fn stack_install_command(stack: &TechStack) -> &'static str {
    match stack {
        TechStack::Python => "pip install -r requirements.txt",
        TechStack::Rust => "cargo build",
        TechStack::Julia => "julia --project=. -e 'using Pkg; Pkg.instantiate()'",
        TechStack::Go => "go mod download",
    }
}

fn stack_example_command(stack: &TechStack) -> &'static str {
    match stack {
        TechStack::Python => "python train.py --epochs 10",
        TechStack::Rust => "cargo run --release",
        TechStack::Julia => "julia src/main.jl",
        TechStack::Go => "go run ./cmd/main.go",
    }
}

fn research_type_name(rt: &ResearchType) -> &'static str {
    match rt {
        ResearchType::Ml => "机器学习 / 深度学习",
        ResearchType::Data => "数据分析",
        ResearchType::Math => "数学建模",
        ResearchType::Generic => "通用研究",
    }
}

fn generate_project_structure(exp_dir: &str, rt: &ResearchType) -> String {
    let mut lines = vec![
        "├── data/".to_string(),
        "│   ├── raw/                 # 原始数据，Agent 只读".to_string(),
        "│   └── processed/           # 处理后数据".to_string(),
        format!("├── {}/             # 实验容器", exp_dir),
        "│   └── run-001-.../         # 单个实验目录".to_string(),
        "│       ├── experiment.json  # 实验记录（版本控制）".to_string(),
        "│       ├── artifacts/       # 模型、图表等产出".to_string(),
        "│       └── logs/            # 标准输出/错误日志".to_string(),
        "├── src/                     # 源代码".to_string(),
        "├── tests/                   # 测试代码".to_string(),
        "├── artifacts/               # 项目级共享制品".to_string(),
        "├── docs/                    # 文档".to_string(),
        "└── .research/               # arcli 内部数据".to_string(),
        "    ├── config.yaml          # 项目配置".to_string(),
        "    ├── research.db          # SQLite 运行时缓存（gitignore）".to_string(),
        "    ├── templates/           # 实验模板".to_string(),
        "    └── hooks/               # 可选 hooks".to_string(),
    ];

    match rt {
        ResearchType::Ml => {
            lines.insert(8, "├── models/                  # 模型定义".to_string());
            lines.insert(9, "├── checkpoints/             # 训练检查点".to_string());
        }
        ResearchType::Data => {
            lines.insert(
                8,
                "├── notebooks/               # Jupyter notebooks".to_string(),
            );
            lines.insert(9, "├── reports/                 # 分析报告".to_string());
        }
        ResearchType::Math => {
            lines.insert(8, "├── proofs/                  # 证明草稿".to_string());
            lines.insert(9, "├── formulations/            # 数学公式".to_string());
        }
        ResearchType::Generic => {}
    }

    lines.join("\n")
}

pub fn init(
    path: Option<String>,
    name: Option<String>,
    force: bool,
    exp_dir: String,
    research_type: Option<ResearchType>,
    stack: Option<TechStack>,
    zh: bool,
) -> Result<Vec<String>> {
    let target = match path {
        Some(p) => PathBuf::from(p),
        None => std::env::current_dir()?,
    };

    if !target.exists() {
        fs::create_dir_all(&target)?;
    }

    let target = target.canonicalize().unwrap_or(target);

    if !force {
        let entries: Vec<_> = fs::read_dir(&target)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                name != ".git" && name != ".humanize"
            })
            .collect();
        if !entries.is_empty() {
            return Err(ArcliError::Other(
                "目标目录非空，请使用 --force 强制初始化".to_string(),
            ));
        }
    }

    let research_dir = target.join(".research");
    if research_dir.join("config.yaml").exists() {
        return Err(ArcliError::Other("该目录已是研究仓库".to_string()));
    }

    let mut created = Vec::new();

    // Determine stack and research type
    let detected_stack = stack.unwrap_or_else(|| detect_stack(&target));
    let rt = research_type.unwrap_or(ResearchType::Generic);

    // Create directories
    let mut dirs = vec![
        target.join("data").join("raw"),
        target.join("data").join("processed"),
        target.join(&exp_dir),
        target.join("src"),
        target.join("tests"),
        target.join("artifacts"),
        target.join("docs"),
        research_dir.clone(),
        research_dir.join("templates"),
        research_dir.join("hooks"),
    ];

    match rt {
        ResearchType::Ml => {
            dirs.push(target.join("models"));
            dirs.push(target.join("checkpoints"));
        }
        ResearchType::Data => {
            dirs.push(target.join("notebooks"));
            dirs.push(target.join("reports"));
        }
        ResearchType::Math => {
            dirs.push(target.join("proofs"));
            dirs.push(target.join("formulations"));
        }
        ResearchType::Generic => {}
    }

    for dir in &dirs {
        if !dir.exists() {
            fs::create_dir_all(dir)?;
            created.push(dir.to_string_lossy().to_string());
        }
    }

    // Generate .gitignore
    let gitignore = target.join(".gitignore");
    if !gitignore.exists() {
        let content = format!("{}\n{}", GITIGNORE_BASE, stack_gitignore(&detected_stack));
        fs::write(&gitignore, content)?;
        created.push(gitignore.to_string_lossy().to_string());
    }

    // Generate README
    let project_name = name.clone().unwrap_or_else(|| {
        target
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "research-project".to_string())
    });

    let readme = target.join("README.md");
    if !readme.exists() {
        let project_structure = generate_project_structure(&exp_dir, &rt);
        let stack_n = stack_name(&detected_stack);
        let install_cmd = stack_install_command(&detected_stack);
        let example_cmd = stack_example_command(&detected_stack);
        let mut vars = HashMap::new();
        vars.insert("PROJECT_NAME", project_name.as_str());
        vars.insert("PROJECT_DESCRIPTION", "研究项目仓库。");
        vars.insert("PROJECT_STRUCTURE", &project_structure);
        vars.insert("STACK", stack_n);
        vars.insert("INSTALL_COMMAND", install_cmd);
        vars.insert("EXAMPLE_COMMAND", example_cmd);
        vars.insert("LICENSE", "MIT License");
        let content = render_template(README_TEMPLATE, &vars);
        fs::write(&readme, content)?;
        created.push(readme.to_string_lossy().to_string());
    }

    // Generate Chinese README if requested
    if zh {
        let readme_zh = target.join("README.zh.md");
        if !readme_zh.exists() {
            let project_structure = generate_project_structure(&exp_dir, &rt);
            let stack_n = stack_name(&detected_stack);
            let install_cmd = stack_install_command(&detected_stack);
            let example_cmd = stack_example_command(&detected_stack);
            let mut vars = HashMap::new();
            vars.insert("PROJECT_NAME", project_name.as_str());
            vars.insert("PROJECT_DESCRIPTION", "研究项目仓库。");
            vars.insert("PROJECT_STRUCTURE", &project_structure);
            vars.insert("STACK", stack_n);
            vars.insert("INSTALL_COMMAND", install_cmd);
            vars.insert("EXAMPLE_COMMAND", example_cmd);
            vars.insert("LICENSE", "MIT License");
            let content = render_template(README_ZH_TEMPLATE, &vars);
            fs::write(&readme_zh, content)?;
            created.push(readme_zh.to_string_lossy().to_string());
        }
    }

    // Generate config.yaml
    let config = Config {
        project_name: project_name.clone(),
        experiments_dir: exp_dir.clone(),
        ..Config::default()
    };
    let config_path = research_dir.join("config.yaml");
    config.save(&config_path)?;
    created.push(config_path.to_string_lossy().to_string());

    // Generate default hooks README
    let hooks_readme = research_dir.join("hooks").join("README.md");
    if !hooks_readme.exists() {
        fs::write(&hooks_readme, HOOKS_README_TEMPLATE)?;
        created.push(hooks_readme.to_string_lossy().to_string());
    }

    // Generate research.db
    let db_path = research_dir.join("research.db");
    let db = Database::open(&db_path)?;
    db.init_schema()?;
    created.push(db_path.to_string_lossy().to_string());

    // Generate PROJECT_BASIS_v010.md
    let basis_md = target.join("docs").join("PROJECT_BASIS_v010.md");
    if !basis_md.exists() {
        let now = chrono::Local::now().format("%Y-%m-%d").to_string();
        let project_structure = generate_project_structure(&exp_dir, &rt);
        let stack_n = stack_name(&detected_stack);
        let rt_name = research_type_name(&rt);
        let project_desc = format!("基于 {} 技术栈的 {} 项目。", stack_n, rt_name);
        let mut vars = HashMap::new();
        vars.insert("PROJECT_NAME", project_name.as_str());
        vars.insert("PROJECT_DESCRIPTION", project_desc.as_str());
        vars.insert("STACK", stack_n);
        vars.insert("RESEARCH_TYPE", rt_name);
        vars.insert("INIT_DATE", now.as_str());
        vars.insert("PROJECT_STRUCTURE", &project_structure);
        vars.insert("EXPERIMENTS_DIR", exp_dir.as_str());
        vars.insert("TIMESTAMP", "YYYYMMDD-HHMM");
        let content = render_template(PROJECT_BASIS_TEMPLATE, &vars);
        fs::write(&basis_md, content)?;
        created.push(basis_md.to_string_lossy().to_string());
    }

    // Initialize git repo
    let git_dir = target.join(".git");
    if !git_dir.exists() {
        git2::Repository::init(&target)?;
        created.push(git_dir.to_string_lossy().to_string());
    }

    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_creates_scaffold() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("new-project");

        let _created = init(
            Some(target.to_string_lossy().to_string()),
            Some("my-project".to_string()),
            false,
            "experiments".to_string(),
            None,
            None,
            false,
        )
        .unwrap();

        assert!(target.join("data/raw").exists());
        assert!(target.join("data/processed").exists());
        assert!(target.join("experiments").exists());
        assert!(target.join("src").exists());
        assert!(target.join("tests").exists());
        assert!(target.join("artifacts").exists());
        assert!(target.join("docs").exists());
        assert!(target.join(".research").exists());
        assert!(target.join(".research/templates").exists());
        assert!(target.join(".research/hooks").exists());
        assert!(target.join(".research/hooks/README.md").exists());

        assert!(target.join(".gitignore").exists());
        assert!(target.join("README.md").exists());
        assert!(target.join(".research/config.yaml").exists());
        assert!(target.join(".research/research.db").exists());
        assert!(target.join(".git").exists());

        // Check PROJECT_BASIS doc
        assert!(target.join("docs/PROJECT_BASIS_v010.md").exists());

        let config = Config::load(&target.join(".research/config.yaml")).unwrap();
        assert_eq!(config.project_name, "my-project");
        assert_eq!(config.experiments_dir, "experiments");
    }

    #[test]
    fn test_init_detects_python_stack() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("python-project");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("requirements.txt"), "numpy\n").unwrap();

        let _created = init(
            Some(target.to_string_lossy().to_string()),
            None,
            true,
            "experiments".to_string(),
            None,
            None,
            false,
        )
        .unwrap();

        let gitignore = fs::read_to_string(target.join(".gitignore")).unwrap();
        assert!(gitignore.contains("__pycache__/"));
        assert!(gitignore.contains(".research/*.db"));

        let readme = fs::read_to_string(target.join("README.md")).unwrap();
        assert!(readme.contains("pip install"));
    }

    #[test]
    fn test_init_detects_rust_stack() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("rust-project");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("Cargo.toml"), "[package]\n").unwrap();

        let _created = init(
            Some(target.to_string_lossy().to_string()),
            None,
            true,
            "experiments".to_string(),
            None,
            None,
            false,
        )
        .unwrap();

        let gitignore = fs::read_to_string(target.join(".gitignore")).unwrap();
        assert!(gitignore.contains("/target/"));

        let readme = fs::read_to_string(target.join("README.md")).unwrap();
        assert!(readme.contains("cargo build"));
    }

    #[test]
    fn test_init_generates_zh_readme() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("zh-project");

        let _created = init(
            Some(target.to_string_lossy().to_string()),
            None,
            false,
            "experiments".to_string(),
            None,
            None,
            true,
        )
        .unwrap();

        assert!(target.join("README.zh.md").exists());
    }

    #[test]
    fn test_init_research_type_ml() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("ml-project");

        let _created = init(
            Some(target.to_string_lossy().to_string()),
            None,
            false,
            "experiments".to_string(),
            Some(ResearchType::Ml),
            None,
            false,
        )
        .unwrap();

        assert!(target.join("models").exists());
        assert!(target.join("checkpoints").exists());
    }

    #[test]
    fn test_init_research_type_data() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("data-project");

        let _created = init(
            Some(target.to_string_lossy().to_string()),
            None,
            false,
            "experiments".to_string(),
            Some(ResearchType::Data),
            None,
            false,
        )
        .unwrap();

        assert!(target.join("notebooks").exists());
        assert!(target.join("reports").exists());
    }

    #[test]
    fn test_init_rejects_nonempty_without_force() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("existing");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("somefile.txt"), "hello").unwrap();

        let result = init(
            Some(target.to_string_lossy().to_string()),
            None,
            false,
            "experiments".to_string(),
            None,
            None,
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_init_force_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("existing");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("somefile.txt"), "hello").unwrap();

        let _created = init(
            Some(target.to_string_lossy().to_string()),
            None,
            true,
            "experiments".to_string(),
            None,
            None,
            false,
        )
        .unwrap();

        assert!(target.join(".research/config.yaml").exists());
    }

    #[test]
    fn test_init_rejects_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("dup");

        init(
            Some(target.to_string_lossy().to_string()),
            None,
            false,
            "experiments".to_string(),
            None,
            None,
            false,
        )
        .unwrap();

        let result = init(
            Some(target.to_string_lossy().to_string()),
            None,
            false,
            "experiments".to_string(),
            None,
            None,
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_render_template() {
        let mut vars = HashMap::new();
        vars.insert("NAME", "Test");
        vars.insert("DESC", "A test project");
        let template = "# {NAME}\n\n{DESC}";
        let result = render_template(template, &vars);
        assert_eq!(result, "# Test\n\nA test project");
    }
}
