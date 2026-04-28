use std::fs;
use std::path::PathBuf;

use crate::config::Config;
use crate::db::Database;
use crate::error::{RcliError, Result};

pub fn init(
    path: Option<String>,
    name: Option<String>,
    force: bool,
    exp_dir: String,
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
            return Err(RcliError::Other(
                "目标目录非空，请使用 --force 强制初始化".to_string(),
            ));
        }
    }

    let research_dir = target.join(".research");
    if research_dir.join("config.yaml").exists() {
        return Err(RcliError::Other(
            "该目录已是研究仓库".to_string(),
        ));
    }

    let mut created = Vec::new();

    let dirs = vec![
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

    for dir in &dirs {
        if !dir.exists() {
            fs::create_dir_all(dir)?;
            created.push(dir.to_string_lossy().to_string());
        }
    }

    let gitignore = target.join(".gitignore");
    if !gitignore.exists() {
        let content = "# rcli 内部数据\n\
             .research/*.db*\n\
             .research/*.db-journal\n\
             .research/*.db-wal\n\
             .research/*.db-shm\n\
             \n\
             # Python\n\
             __pycache__/\n\
             *.pyc\n\
             .venv/\n\
             \n\
             # Rust\n\
             /target\n\
             Cargo.lock\n\
             \n\
             # 大数据建议使用 DVC 管理\n\
             # data/raw/large-datasets/\n".to_string();
        fs::write(&gitignore, content)?;
        created.push(gitignore.to_string_lossy().to_string());
    }

    let readme = target.join("README.md");
    if !readme.exists() {
        let project_name = name.clone().unwrap_or_else(|| {
            target.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "research-project".to_string())
        });
        let content = format!("# {}\n\n研究项目仓库。\n", project_name);
        fs::write(&readme, content)?;
        created.push(readme.to_string_lossy().to_string());
    }

    let config = Config {
        project_name: name.unwrap_or_else(|| {
            target.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "research-project".to_string())
        }),
        experiments_dir: exp_dir,
        ..Config::default()
    };
    let config_path = research_dir.join("config.yaml");
    config.save(&config_path)?;
    created.push(config_path.to_string_lossy().to_string());

    let db_path = research_dir.join("research.db");
    let db = Database::open(&db_path)?;
    db.init_schema()?;
    created.push(db_path.to_string_lossy().to_string());

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
        ).unwrap();

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

        assert!(target.join(".gitignore").exists());
        assert!(target.join("README.md").exists());
        assert!(target.join(".research/config.yaml").exists());
        assert!(target.join(".research/research.db").exists());
        assert!(target.join(".git").exists());

        let config = Config::load(&target.join(".research/config.yaml")).unwrap();
        assert_eq!(config.project_name, "my-project");
        assert_eq!(config.experiments_dir, "experiments");
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
        ).unwrap();

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
        ).unwrap();

        let result = init(
            Some(target.to_string_lossy().to_string()),
            None,
            false,
            "experiments".to_string(),
        );
        assert!(result.is_err());
    }
}
