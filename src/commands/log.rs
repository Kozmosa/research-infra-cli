use std::fs;
use std::io::{BufRead, Seek, SeekFrom};
use std::thread;
use std::time::Duration;

use crate::error::{RcliError, Result};
use crate::repo::Repository;

pub fn show(repo: &Repository, exp_id: &str, tail: Option<usize>, follow: bool) -> Result<()> {
    let log_path = repo.exp_log_path(exp_id);
    if !log_path.exists() {
        return Err(RcliError::Other(format!("实验 '{}' 的日志文件不存在", exp_id)));
    }

    let file = fs::File::open(&log_path)?;
    let reader = std::io::BufReader::new(file);

    if let Some(n) = tail {
        let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
        let start = lines.len().saturating_sub(n);
        for line in &lines[start..] {
            println!("{}", line);
        }
    } else {
        for line in reader.lines() {
            if let Ok(l) = line {
                println!("{}", l);
            }
        }
    }

    if follow {
        let mut pos = fs::metadata(&log_path)?.len();
        loop {
            thread::sleep(Duration::from_millis(500));
            let metadata = fs::metadata(&log_path)?;
            let len = metadata.len();
            if len > pos {
                let file = fs::File::open(&log_path)?;
                let mut new_reader = std::io::BufReader::new(file);
                new_reader.seek(SeekFrom::Start(pos))?;
                for line in new_reader.lines() {
                    if let Ok(l) = line {
                        println!("{}", l);
                    }
                }
                pos = len;
            }
        }
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
    fn test_show_reads_log_file() {
        let (repo, _dir) = create_test_repo();

        let exp_dir = repo.exp_dir("exp-001");
        fs::create_dir_all(exp_dir.join("logs")).unwrap();
        fs::write(exp_dir.join("logs/run.log"), "line1\nline2\nline3\n").unwrap();

        show(&repo, "exp-001", None, false).unwrap();
    }

    #[test]
    fn test_show_tail_limits_lines() {
        let (repo, _dir) = create_test_repo();

        let exp_dir = repo.exp_dir("exp-002");
        fs::create_dir_all(exp_dir.join("logs")).unwrap();
        fs::write(exp_dir.join("logs/run.log"), "a\nb\nc\nd\ne\n").unwrap();

        show(&repo, "exp-002", Some(2), false).unwrap();
    }

    #[test]
    fn test_show_missing_log_fails() {
        let (repo, _dir) = create_test_repo();

        let result = show(&repo, "exp-003", None, false);
        assert!(result.is_err());
    }
}
