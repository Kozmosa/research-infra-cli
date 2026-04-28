use rusqlite::{Connection, OpenFlags};
use std::path::Path;

use crate::error::Result;

pub struct Database {
    pub(crate) conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;"
        )?;
        Ok(Database { conn })
    }

    pub fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS seq (
                name TEXT PRIMARY KEY,
                id INTEGER NOT NULL DEFAULT 0
            );

            INSERT OR IGNORE INTO seq (name, id) VALUES ('experiment', 0);

            CREATE TABLE IF NOT EXISTS experiments (
                id TEXT PRIMARY KEY,
                short_id TEXT NOT NULL UNIQUE,
                status TEXT NOT NULL DEFAULT 'created',
                created_at TEXT NOT NULL,
                started_at TEXT,
                finished_at TEXT,
                commit_hash TEXT,
                data_used TEXT,
                command TEXT NOT NULL,
                params TEXT,
                notes TEXT,
                env TEXT,
                exit_code INTEGER,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS metrics_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                exp_id TEXT NOT NULL,
                step INTEGER NOT NULL,
                metric_key TEXT NOT NULL,
                metric_value REAL NOT NULL,
                recorded_at TEXT NOT NULL,
                UNIQUE(exp_id, step, metric_key),
                FOREIGN KEY (exp_id) REFERENCES experiments(id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS datasets (
                name TEXT PRIMARY KEY,
                path TEXT NOT NULL,
                checksum TEXT,
                description TEXT,
                registered_at TEXT NOT NULL
            );
            "
        )?;
        Ok(())
    }

    pub fn next_short_id(&self) -> Result<i64> {
        let tx = self.conn.unchecked_transaction()?;
        let id: i64 = tx.query_row(
            "UPDATE seq SET id = id + 1 WHERE name = 'experiment' RETURNING id",
            [],
            |row| row.get(0),
        )?;
        tx.commit()?;
        Ok(id)
    }

    pub fn insert_experiment(
        &self,
        id: &str,
        short_id: &str,
        status: &str,
        created_at: &str,
        commit_hash: Option<&str>,
        data_used: Option<&str>,
        command: &str,
        params: Option<&str>,
        notes: Option<&str>,
        env: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO experiments (id, short_id, status, created_at, commit_hash, data_used, command, params, notes, env, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            [
                id, short_id, status, created_at,
                commit_hash.unwrap_or(""),
                data_used.unwrap_or(""),
                command,
                params.unwrap_or(""),
                notes.unwrap_or(""),
                env.unwrap_or(""),
                created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_experiment(&self, exp_id: &str) -> Result<Option<Experiment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, short_id, status, created_at, started_at, finished_at, commit_hash, data_used, command, params, notes, env, exit_code, updated_at
             FROM experiments WHERE id = ?1"
        )?;
        let mut rows = stmt.query([exp_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Experiment {
                id: row.get(0)?,
                short_id: row.get(1)?,
                status: row.get(2)?,
                created_at: row.get(3)?,
                started_at: row.get(4)?,
                finished_at: row.get(5)?,
                commit_hash: row.get::<_, Option<String>>(6)?.filter(|s| !s.is_empty()),
                data_used: row.get::<_, Option<String>>(7)?.filter(|s| !s.is_empty()),
                command: row.get(8)?,
                params: row.get::<_, Option<String>>(9)?.filter(|s| !s.is_empty()),
                notes: row.get::<_, Option<String>>(10)?.filter(|s| !s.is_empty()),
                env: row.get::<_, Option<String>>(11)?.filter(|s| !s.is_empty()),
                exit_code: row.get(12)?,
                updated_at: row.get(13)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_experiments(&self, status_filter: Option<&str>, since: Option<&str>) -> Result<Vec<ExperimentSummary>> {
        let mut sql = String::from(
            "SELECT id, short_id, status, created_at, data_used, command FROM experiments WHERE 1=1"
        );
        if status_filter.is_some() {
            sql.push_str(" AND status = ?1");
        }
        if since.is_some() {
            sql.push_str(" AND created_at >= ?2");
        }
        sql.push_str(" ORDER BY created_at DESC");

        let mut stmt = self.conn.prepare(&sql)?;

        fn map_row(row: &rusqlite::Row) -> rusqlite::Result<ExperimentSummary> {
            Ok(ExperimentSummary {
                id: row.get(0)?,
                short_id: row.get(1)?,
                status: row.get(2)?,
                created_at: row.get(3)?,
                data_used: row.get(4)?,
                command: row.get(5)?,
            })
        }

        let rows = match (status_filter, since) {
            (Some(st), Some(si)) => stmt.query_map([st, si], map_row)?,
            (Some(st), None) => stmt.query_map([st], map_row)?,
            (None, Some(si)) => stmt.query_map([si], map_row)?,
            (None, None) => stmt.query_map([], map_row)?,
        };

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn list_active_experiments(&self) -> Result<Vec<ExperimentSummary>> {
        self.list_experiments(Some("running"), None)
    }

    pub fn insert_dataset(&self, name: &str, path: &str, checksum: Option<&str>, description: Option<&str>, registered_at: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO datasets (name, path, checksum, description, registered_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(name) DO UPDATE SET
                path = excluded.path,
                checksum = excluded.checksum,
                description = excluded.description,
                registered_at = excluded.registered_at",
            [name, path, checksum.unwrap_or(""), description.unwrap_or(""), registered_at],
        )?;
        Ok(())
    }

    pub fn get_dataset(&self, name: &str) -> Result<Option<Dataset>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, path, checksum, description, registered_at FROM datasets WHERE name = ?1"
        )?;
        let mut rows = stmt.query([name])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Dataset {
                name: row.get(0)?,
                path: row.get(1)?,
                checksum: row.get(2)?,
                description: row.get(3)?,
                registered_at: row.get(4)?,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn list_datasets(&self) -> Result<Vec<Dataset>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, path, checksum, description, registered_at FROM datasets ORDER BY registered_at DESC"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Dataset {
                name: row.get(0)?,
                path: row.get(1)?,
                checksum: row.get(2)?,
                description: row.get(3)?,
                registered_at: row.get(4)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn upsert_experiment(
        &self,
        id: &str,
        short_id: &str,
        status: &str,
        created_at: &str,
        updated_at: &str,
        commit_hash: Option<&str>,
        data_used: Option<&str>,
        command: &str,
        params: Option<&str>,
        notes: Option<&str>,
        env: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO experiments (id, short_id, status, created_at, commit_hash, data_used, command, params, notes, env, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                status = excluded.status,
                commit_hash = excluded.commit_hash,
                data_used = excluded.data_used,
                command = excluded.command,
                params = excluded.params,
                notes = excluded.notes,
                env = excluded.env,
                updated_at = excluded.updated_at",
            [
                id, short_id, status, created_at,
                commit_hash.unwrap_or(""),
                data_used.unwrap_or(""),
                command,
                params.unwrap_or(""),
                notes.unwrap_or(""),
                env.unwrap_or(""),
                updated_at,
            ],
        )?;
        Ok(())
    }

    pub fn update_experiment_status(
        &self,
        exp_id: &str,
        status: &str,
        started_at: Option<&str>,
        finished_at: Option<&str>,
        exit_code: Option<i32>,
    ) -> Result<()> {
        let mut sql = String::from("UPDATE experiments SET status = ?1");
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&status];

        if started_at.is_some() {
            sql.push_str(", started_at = ?");
            params.push(&started_at);
        }
        if finished_at.is_some() {
            sql.push_str(", finished_at = ?");
            params.push(&finished_at);
        }
        if exit_code.is_some() {
            sql.push_str(", exit_code = ?");
            params.push(&exit_code);
        }
        sql.push_str(", updated_at = ?");
        let now = chrono::Local::now().to_rfc3339();
        params.push(&now);
        sql.push_str(" WHERE id = ?");
        params.push(&exp_id);

        self.conn.execute(&sql, rusqlite::params_from_iter(params.iter()))?;
        Ok(())
    }

    pub fn insert_metric(
        &self,
        exp_id: &str,
        step: i64,
        key: &str,
        value: f64,
    ) -> Result<()> {
        let recorded_at = chrono::Local::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO metrics_history (exp_id, step, metric_key, metric_value, recorded_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(exp_id, step, metric_key) DO UPDATE SET
                metric_value = excluded.metric_value,
                recorded_at = excluded.recorded_at",
            [exp_id, &step.to_string(), key, &value.to_string(), &recorded_at],
        )?;
        Ok(())
    }

    pub fn get_metrics(&self, exp_id: &str) -> Result<Vec<MetricRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT exp_id, step, metric_key, metric_value, recorded_at
             FROM metrics_history WHERE exp_id = ?1 ORDER BY step, metric_key"
        )?;
        let rows = stmt.query_map([exp_id], |row| {
            Ok(MetricRecord {
                exp_id: row.get(0)?,
                step: row.get(1)?,
                metric_key: row.get(2)?,
                metric_value: row.get(3)?,
                recorded_at: row.get(4)?,
            })
        })?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn update_experiment_params(&self, exp_id: &str, params: &str) -> Result<()> {
        let updated_at = chrono::Local::now().to_rfc3339();
        self.conn.execute(
            "UPDATE experiments SET params = ?1, updated_at = ?2 WHERE id = ?3",
            [params, &updated_at, exp_id],
        )?;
        Ok(())
    }

    pub fn append_experiment_note(&self, exp_id: &str, note: &str) -> Result<()> {
        let updated_at = chrono::Local::now().to_rfc3339();
        self.conn.execute(
            "UPDATE experiments SET notes = COALESCE(notes, '') || '\n finish message: ' || ?1, updated_at = ?2 WHERE id = ?3",
            [note, &updated_at, exp_id],
        )?;
        Ok(())
    }
}

#[derive(Debug, serde::Serialize)]
pub struct Experiment {
    pub id: String,
    pub short_id: String,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub commit_hash: Option<String>,
    pub data_used: Option<String>,
    pub command: String,
    pub params: Option<String>,
    pub notes: Option<String>,
    pub env: Option<String>,
    pub exit_code: Option<i32>,
    pub updated_at: String,
}

#[derive(Debug, serde::Serialize)]
pub struct ExperimentSummary {
    pub id: String,
    pub short_id: String,
    pub status: String,
    pub created_at: String,
    pub data_used: Option<String>,
    pub command: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct Dataset {
    pub name: String,
    pub path: String,
    pub checksum: Option<String>,
    pub description: Option<String>,
    pub registered_at: String,
}

#[derive(Debug, serde::Serialize)]
pub struct MetricRecord {
    pub exp_id: String,
    pub step: i64,
    pub metric_key: String,
    pub metric_value: f64,
    pub recorded_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = Database::open(&path).unwrap();
        db.init_schema().unwrap();
        db
    }

    #[test]
    fn test_next_short_id_increments() {
        let db = create_test_db();
        let id1 = db.next_short_id().unwrap();
        let id2 = db.next_short_id().unwrap();
        assert_eq!(id2, id1 + 1);
    }

    #[test]
    fn test_insert_and_get_experiment() {
        let db = create_test_db();
        db.insert_experiment(
            "exp-001", "001", "created", "2026-01-01T00:00:00Z",
            None, None, "python train.py", None, None, None,
        ).unwrap();

        let exp = db.get_experiment("exp-001").unwrap().unwrap();
        assert_eq!(exp.id, "exp-001");
        assert_eq!(exp.short_id, "001");
        assert_eq!(exp.status, "created");
        assert_eq!(exp.command, "python train.py");
    }

    #[test]
    fn test_update_experiment_status() {
        let db = create_test_db();
        db.insert_experiment(
            "exp-002", "002", "created", "2026-01-01T00:00:00Z",
            None, None, "python train.py", None, None, None,
        ).unwrap();

        db.update_experiment_status("exp-002", "running", Some("2026-01-01T01:00:00Z"), None, None).unwrap();

        let exp = db.get_experiment("exp-002").unwrap().unwrap();
        assert_eq!(exp.status, "running");
        assert_eq!(exp.started_at, Some("2026-01-01T01:00:00Z".to_string()));
    }

    #[test]
    fn test_insert_and_get_metric() {
        let db = create_test_db();
        db.insert_experiment(
            "exp-003", "003", "running", "2026-01-01T00:00:00Z",
            None, None, "python train.py", None, None, None,
        ).unwrap();

        db.insert_metric("exp-003", 1, "loss", 0.5).unwrap();
        db.insert_metric("exp-003", 1, "loss", 0.3).unwrap();

        let metrics = db.get_metrics("exp-003").unwrap();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].metric_value, 0.3);
    }

    #[test]
    fn test_insert_and_get_dataset() {
        let db = create_test_db();
        db.insert_dataset("imdb-v1", "./data/imdb", Some("abc123"), Some("IMDB dataset"), "2026-01-01T00:00:00Z").unwrap();

        let ds = db.get_dataset("imdb-v1").unwrap().unwrap();
        assert_eq!(ds.name, "imdb-v1");
        assert_eq!(ds.path, "./data/imdb");
        assert_eq!(ds.checksum, Some("abc123".to_string()));
    }

    #[test]
    fn test_list_experiments_with_filter() {
        let db = create_test_db();
        db.insert_experiment(
            "exp-004", "004", "finished", "2026-01-01T00:00:00Z",
            None, None, "python train.py", None, None, None,
        ).unwrap();
        db.insert_experiment(
            "exp-005", "005", "running", "2026-01-02T00:00:00Z",
            None, None, "python eval.py", None, None, None,
        ).unwrap();

        let running = db.list_experiments(Some("running"), None).unwrap();
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].id, "exp-005");
    }

    #[test]
    fn test_concurrent_short_id_generation() {
        use std::thread;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");

        // 先初始化 schema
        let db = Database::open(&path).unwrap();
        db.init_schema().unwrap();
        drop(db);

        let mut handles = Vec::new();
        for _ in 0..10 {
            let path_clone = path.clone();
            let handle = thread::spawn(move || {
                let db = Database::open(&path_clone).unwrap();
                db.next_short_id().unwrap()
            });
            handles.push(handle);
        }

        let mut ids: Vec<i64> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        ids.sort();

        // 所有 ID 唯一且连续
        assert_eq!(ids.len(), 10);
        for i in 1..ids.len() {
            assert_eq!(ids[i], ids[i - 1] + 1, "并发 short_id 应连续无重复");
        }
    }

    #[test]
    fn test_database_opens_in_wal_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = Database::open(&path).unwrap();

        let journal_mode: String = db.conn.query_row(
            "PRAGMA journal_mode",
            [],
            |row| row.get(0),
        ).unwrap();

        assert_eq!(journal_mode.to_lowercase(), "wal", "数据库应以 WAL 模式打开");
    }
}
