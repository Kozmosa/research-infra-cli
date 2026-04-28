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
