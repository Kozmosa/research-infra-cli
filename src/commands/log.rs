use std::fs;
use std::io::{BufRead, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::error::{RcliError, Result};
use crate::repo::Repository;

pub fn show(repo: &Repository, exp_id: &str, tail: Option<usize>, follow: bool) -> Result<()> {
    show_inner(repo, exp_id, tail, follow, &mut std::io::stdout(), None)
}

fn show_inner(
    repo: &Repository,
    exp_id: &str,
    tail: Option<usize>,
    follow: bool,
    writer: &mut dyn Write,
    stop_signal: Option<&AtomicBool>,
) -> Result<()> {
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
            writeln!(writer, "{}", line)?;
        }
    } else {
        for l in reader.lines().flatten() {
            writeln!(writer, "{}", l)?;
        }
    }

    if follow {
        let mut pos = fs::metadata(&log_path)?.len();
        loop {
            thread::sleep(Duration::from_millis(500));
            if let Some(stop) = stop_signal
                && stop.load(Ordering::Relaxed) {
                    break;
                }
            let metadata = fs::metadata(&log_path)?;
            let len = metadata.len();
            if len > pos {
                let file = fs::File::open(&log_path)?;
                let mut new_reader = std::io::BufReader::new(file);
                new_reader.seek(SeekFrom::Start(pos))?;
                for l in new_reader.lines().flatten() {
                    writeln!(writer, "{}", l)?;
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
    use std::io::Write;
    use std::sync::Arc;

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

    #[test]
    fn test_show_follow_detects_new_content() {
        let (repo, _dir) = create_test_repo();

        let exp_dir = repo.exp_dir("exp-004");
        fs::create_dir_all(exp_dir.join("logs")).unwrap();
        fs::write(exp_dir.join("logs/run.log"), "initial\n").unwrap();

        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);

        // follow 模式会阻塞，在线程中运行
        let repo_clone = Repository { root: repo.root.clone() };
        let handle = thread::spawn(move || {
            let mut buf = Vec::new();
            show_inner(&repo_clone, "exp-004", None, true, &mut buf, Some(&stop_clone)).unwrap();
            buf
        });

        // 等待 follow 进入循环
        thread::sleep(Duration::from_millis(100));

        // 追加新内容
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(exp_dir.join("logs/run.log"))
            .unwrap();
        file.write_all(b"new line\n").unwrap();
        drop(file);

        // 等待 follow 检测到新内容（follow 每 500ms 检查一次）
        thread::sleep(Duration::from_millis(700));

        // 发送停止信号
        stop.store(true, Ordering::Relaxed);

        // 等待线程结束
        let output = handle.join().unwrap();
        let output_str = String::from_utf8(output).unwrap();

        // 验证初始内容和追加内容都被捕获
        assert!(output_str.contains("initial"), "输出应包含初始内容");
        assert!(output_str.contains("new line"), "follow 模式应检测到并输出追加的新内容");
    }
}
