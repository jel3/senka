use std::io::Write;
use std::path::Path;

use rusqlite::{params, Connection};
use thiserror::Error;

use crate::models::{Payload, Run, RunWithPayload};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid duration: {0}")]
    InvalidDuration(String),
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS runs (
    id            TEXT PRIMARY KEY,
    ts            INTEGER NOT NULL,
    project       TEXT NOT NULL,
    env           TEXT NOT NULL,
    request_name  TEXT NOT NULL,
    method        TEXT NOT NULL,
    url           TEXT NOT NULL,
    status        INTEGER,
    duration_ms   INTEGER NOT NULL,
    error         TEXT
);

CREATE TABLE IF NOT EXISTS payloads (
    run_id            TEXT PRIMARY KEY REFERENCES runs(id) ON DELETE CASCADE,
    request_headers   TEXT NOT NULL,
    request_body      TEXT,
    response_headers  TEXT NOT NULL,
    response_body     TEXT
);

CREATE INDEX IF NOT EXISTS idx_runs_ts ON runs(ts);
CREATE INDEX IF NOT EXISTS idx_runs_request_name ON runs(request_name);
CREATE INDEX IF NOT EXISTS idx_runs_status ON runs(status);
";

/// Open (or create) the log database at the given path and run migrations.
pub fn open(path: &Path) -> Result<Connection, StoreError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

/// Open an in-memory database for testing.
#[cfg(test)]
pub fn open_in_memory() -> Result<Connection, StoreError> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    conn.execute_batch(SCHEMA)?;
    Ok(conn)
}

/// Insert a run and its payload in a single transaction.
pub fn insert_run(conn: &Connection, run: &Run, payload: &Payload) -> Result<(), StoreError> {
    let tx = conn.unchecked_transaction()?;

    tx.execute(
        "INSERT INTO runs (id, ts, project, env, request_name, method, url, status, duration_ms, error)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            run.id,
            run.ts,
            run.project,
            run.env,
            run.request_name,
            run.method,
            run.url,
            run.status,
            run.duration_ms,
            run.error,
        ],
    )?;

    tx.execute(
        "INSERT INTO payloads (run_id, request_headers, request_body, response_headers, response_body)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            payload.run_id,
            payload.request_headers,
            payload.request_body,
            payload.response_headers,
            payload.response_body,
        ],
    )?;

    tx.commit()?;
    Ok(())
}

/// Return the last `n` runs ordered by timestamp descending.
pub fn tail(conn: &Connection, n: u32) -> Result<Vec<Run>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT id, ts, project, env, request_name, method, url, status, duration_ms, error
         FROM runs ORDER BY ts DESC LIMIT ?1",
    )?;

    let rows = stmt.query_map(params![n], row_to_run)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::Db)
}

/// Filter parameters for listing runs.
pub struct ListFilters {
    /// Only runs with `ts >= since` (epoch ms).
    pub since: Option<u64>,
    /// Exact HTTP status code match.
    pub status: Option<u16>,
    /// Substring match on request_name.
    pub request_name: Option<String>,
}

/// List runs matching optional filters, ordered by timestamp descending.
pub fn list(conn: &Connection, filters: &ListFilters) -> Result<Vec<Run>, StoreError> {
    let mut sql = String::from(
        "SELECT id, ts, project, env, request_name, method, url, status, duration_ms, error FROM runs",
    );
    let mut conditions = Vec::new();

    if filters.since.is_some() {
        conditions.push("ts >= ?".to_string());
    }
    if filters.status.is_some() {
        conditions.push("status = ?".to_string());
    }
    if filters.request_name.is_some() {
        conditions.push("request_name LIKE ?".to_string());
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    sql.push_str(" ORDER BY ts DESC");

    let mut stmt = conn.prepare(&sql)?;

    // Bind parameters in order
    let mut param_idx = 1;
    let mut bound: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(since) = filters.since {
        bound.push(Box::new(since));
        param_idx += 1;
    }
    if let Some(status) = filters.status {
        bound.push(Box::new(status));
        param_idx += 1;
    }
    if let Some(ref name) = filters.request_name {
        bound.push(Box::new(format!("%{name}%")));
        let _ = param_idx; // suppress unused warning
    }

    let params_ref: Vec<&dyn rusqlite::types::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let rows = stmt.query_map(params_ref.as_slice(), row_to_run)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::Db)
}

/// Show a single run by ID, joined with its payload.
pub fn show(conn: &Connection, id: &str) -> Result<Option<RunWithPayload>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.ts, r.project, r.env, r.request_name, r.method, r.url,
                r.status, r.duration_ms, r.error,
                p.request_headers, p.request_body, p.response_headers, p.response_body
         FROM runs r
         LEFT JOIN payloads p ON p.run_id = r.id
         WHERE r.id = ?1",
    )?;

    let mut rows = stmt.query_map(params![id], |row| {
        Ok(RunWithPayload {
            run: Run {
                id: row.get(0)?,
                ts: row.get(1)?,
                project: row.get(2)?,
                env: row.get(3)?,
                request_name: row.get(4)?,
                method: row.get(5)?,
                url: row.get(6)?,
                status: row.get(7)?,
                duration_ms: row.get(8)?,
                error: row.get(9)?,
            },
            request_headers: row.get::<_, Option<String>>(10)?.unwrap_or_default(),
            request_body: row.get(11)?,
            response_headers: row.get::<_, Option<String>>(12)?.unwrap_or_default(),
            response_body: row.get(13)?,
        })
    })?;

    match rows.next() {
        Some(Ok(rwp)) => Ok(Some(rwp)),
        Some(Err(e)) => Err(StoreError::Db(e)),
        None => Ok(None),
    }
}

/// Delete all runs (and cascade payloads) with `ts < cutoff_ts`.
/// Returns the number of deleted rows.
pub fn prune(conn: &Connection, cutoff_ts: u64) -> Result<usize, StoreError> {
    let deleted = conn.execute("DELETE FROM runs WHERE ts < ?1", params![cutoff_ts])?;
    Ok(deleted)
}

/// Delete all runs and payloads. Returns the number of deleted rows.
pub fn clear(conn: &Connection) -> Result<usize, StoreError> {
    let deleted = conn.execute("DELETE FROM runs", [])?;
    Ok(deleted)
}

/// Delete a single run by ID. Returns true if a row was deleted.
pub fn delete_by_id(conn: &Connection, id: &str) -> Result<bool, StoreError> {
    let deleted = conn.execute("DELETE FROM runs WHERE id = ?1", params![id])?;
    Ok(deleted > 0)
}

/// Delete runs matching the given filters. Returns the number of deleted rows.
pub fn delete_filtered(conn: &Connection, filters: &ListFilters) -> Result<usize, StoreError> {
    let mut sql = String::from("DELETE FROM runs");
    let mut conditions = Vec::new();

    if filters.since.is_some() {
        conditions.push("ts >= ?".to_string());
    }
    if filters.status.is_some() {
        conditions.push("status = ?".to_string());
    }
    if filters.request_name.is_some() {
        conditions.push("request_name LIKE ?".to_string());
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }

    let mut bound: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    if let Some(since) = filters.since {
        bound.push(Box::new(since));
    }
    if let Some(status) = filters.status {
        bound.push(Box::new(status));
    }
    if let Some(ref name) = filters.request_name {
        bound.push(Box::new(format!("%{name}%")));
    }

    let params_ref: Vec<&dyn rusqlite::types::ToSql> = bound.iter().map(|b| b.as_ref()).collect();
    let deleted = conn.execute(&sql, params_ref.as_slice())?;
    Ok(deleted)
}

/// Export all runs+payloads as JSONL to the given writer.
/// Returns the number of exported rows.
pub fn export_jsonl<W: Write>(conn: &Connection, writer: &mut W) -> Result<usize, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.ts, r.project, r.env, r.request_name, r.method, r.url,
                r.status, r.duration_ms, r.error,
                p.request_headers, p.request_body, p.response_headers, p.response_body
         FROM runs r
         LEFT JOIN payloads p ON p.run_id = r.id
         ORDER BY r.ts ASC",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(RunWithPayload {
            run: Run {
                id: row.get(0)?,
                ts: row.get(1)?,
                project: row.get(2)?,
                env: row.get(3)?,
                request_name: row.get(4)?,
                method: row.get(5)?,
                url: row.get(6)?,
                status: row.get(7)?,
                duration_ms: row.get(8)?,
                error: row.get(9)?,
            },
            request_headers: row.get::<_, Option<String>>(10)?.unwrap_or_default(),
            request_body: row.get(11)?,
            response_headers: row.get::<_, Option<String>>(12)?.unwrap_or_default(),
            response_body: row.get(13)?,
        })
    })?;

    let mut count = 0;
    for row in rows {
        let rwp = row.map_err(StoreError::Db)?;
        let line =
            serde_json::to_string(&rwp).map_err(|e| StoreError::Io(std::io::Error::other(e)))?;
        writeln!(writer, "{line}")?;
        count += 1;
    }

    Ok(count)
}

/// Parse a human-readable duration string ("30m", "1h", "7d") into milliseconds.
pub fn parse_duration_str(s: &str) -> Result<u64, StoreError> {
    let s = s.trim();
    if s.is_empty() {
        return Err(StoreError::InvalidDuration(
            "empty duration string".to_string(),
        ));
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str
        .parse()
        .map_err(|_| StoreError::InvalidDuration(format!("invalid number in '{s}'")))?;

    let ms = match unit {
        "m" => num * 60 * 1000,
        "h" => num * 60 * 60 * 1000,
        "d" => num * 24 * 60 * 60 * 1000,
        _ => {
            return Err(StoreError::InvalidDuration(format!(
                "unknown unit '{unit}' in '{s}' (expected m/h/d)"
            )))
        }
    };

    Ok(ms)
}

fn row_to_run(row: &rusqlite::Row) -> rusqlite::Result<Run> {
    Ok(Run {
        id: row.get(0)?,
        ts: row.get(1)?,
        project: row.get(2)?,
        env: row.get(3)?,
        request_name: row.get(4)?,
        method: row.get(5)?,
        url: row.get(6)?,
        status: row.get(7)?,
        duration_ms: row.get(8)?,
        error: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_run(id: &str, ts: u64, name: &str, status: Option<u16>) -> Run {
        Run {
            id: id.to_string(),
            ts,
            project: "test-project".to_string(),
            env: "dev".to_string(),
            request_name: name.to_string(),
            method: "GET".to_string(),
            url: "https://example.com".to_string(),
            status,
            duration_ms: 42,
            error: None,
        }
    }

    fn make_payload(run_id: &str) -> Payload {
        Payload {
            run_id: run_id.to_string(),
            request_headers: r#"{"content-type":"application/json"}"#.to_string(),
            request_body: Some(r#"{"key":"value"}"#.to_string()),
            response_headers: r#"{"content-type":"application/json"}"#.to_string(),
            response_body: Some(r#"{"result":"ok"}"#.to_string()),
        }
    }

    #[test]
    fn insert_and_tail() {
        let conn = open_in_memory().unwrap();
        let run = make_run("r1", 1000, "users.get", Some(200));
        let payload = make_payload("r1");
        insert_run(&conn, &run, &payload).unwrap();

        let runs = tail(&conn, 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, "r1");
        assert_eq!(runs[0].status, Some(200));
    }

    #[test]
    fn tail_ordering_and_limit() {
        let conn = open_in_memory().unwrap();
        for i in 0..5 {
            let id = format!("r{i}");
            let run = make_run(&id, 1000 + i, "req", Some(200));
            let payload = make_payload(&id);
            insert_run(&conn, &run, &payload).unwrap();
        }

        let runs = tail(&conn, 3).unwrap();
        assert_eq!(runs.len(), 3);
        // Most recent first
        assert_eq!(runs[0].ts, 1004);
        assert_eq!(runs[1].ts, 1003);
        assert_eq!(runs[2].ts, 1002);
    }

    #[test]
    fn filter_by_status() {
        let conn = open_in_memory().unwrap();
        insert_run(
            &conn,
            &make_run("r1", 1000, "a", Some(200)),
            &make_payload("r1"),
        )
        .unwrap();
        insert_run(
            &conn,
            &make_run("r2", 1001, "b", Some(404)),
            &make_payload("r2"),
        )
        .unwrap();
        insert_run(
            &conn,
            &make_run("r3", 1002, "c", Some(200)),
            &make_payload("r3"),
        )
        .unwrap();

        let filters = ListFilters {
            since: None,
            status: Some(404),
            request_name: None,
        };
        let runs = list(&conn, &filters).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, "r2");
    }

    #[test]
    fn filter_by_since() {
        let conn = open_in_memory().unwrap();
        insert_run(
            &conn,
            &make_run("r1", 1000, "a", Some(200)),
            &make_payload("r1"),
        )
        .unwrap();
        insert_run(
            &conn,
            &make_run("r2", 2000, "b", Some(200)),
            &make_payload("r2"),
        )
        .unwrap();
        insert_run(
            &conn,
            &make_run("r3", 3000, "c", Some(200)),
            &make_payload("r3"),
        )
        .unwrap();

        let filters = ListFilters {
            since: Some(2000),
            status: None,
            request_name: None,
        };
        let runs = list(&conn, &filters).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].id, "r3");
        assert_eq!(runs[1].id, "r2");
    }

    #[test]
    fn filter_by_request_name() {
        let conn = open_in_memory().unwrap();
        insert_run(
            &conn,
            &make_run("r1", 1000, "users.get", Some(200)),
            &make_payload("r1"),
        )
        .unwrap();
        insert_run(
            &conn,
            &make_run("r2", 1001, "users.create", Some(201)),
            &make_payload("r2"),
        )
        .unwrap();
        insert_run(
            &conn,
            &make_run("r3", 1002, "orders.list", Some(200)),
            &make_payload("r3"),
        )
        .unwrap();

        let filters = ListFilters {
            since: None,
            status: None,
            request_name: Some("users".to_string()),
        };
        let runs = list(&conn, &filters).unwrap();
        assert_eq!(runs.len(), 2);
    }

    #[test]
    fn show_with_payload() {
        let conn = open_in_memory().unwrap();
        let run = make_run("r1", 1000, "users.get", Some(200));
        let payload = make_payload("r1");
        insert_run(&conn, &run, &payload).unwrap();

        let result = show(&conn, "r1").unwrap().unwrap();
        assert_eq!(result.run.id, "r1");
        assert_eq!(result.request_body.as_deref(), Some(r#"{"key":"value"}"#));
        assert_eq!(result.response_body.as_deref(), Some(r#"{"result":"ok"}"#));
    }

    #[test]
    fn show_missing() {
        let conn = open_in_memory().unwrap();
        let result = show(&conn, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn prune_cascade() {
        let conn = open_in_memory().unwrap();
        insert_run(
            &conn,
            &make_run("r1", 1000, "a", Some(200)),
            &make_payload("r1"),
        )
        .unwrap();
        insert_run(
            &conn,
            &make_run("r2", 2000, "b", Some(200)),
            &make_payload("r2"),
        )
        .unwrap();
        insert_run(
            &conn,
            &make_run("r3", 3000, "c", Some(200)),
            &make_payload("r3"),
        )
        .unwrap();

        let deleted = prune(&conn, 2500).unwrap();
        assert_eq!(deleted, 2);

        // Only r3 should remain
        let runs = tail(&conn, 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].id, "r3");

        // Payloads for r1 and r2 should be cascade-deleted
        let p = show(&conn, "r1").unwrap();
        assert!(p.is_none());
    }

    #[test]
    fn export_jsonl_format() {
        let conn = open_in_memory().unwrap();
        insert_run(
            &conn,
            &make_run("r1", 1000, "a", Some(200)),
            &make_payload("r1"),
        )
        .unwrap();
        insert_run(
            &conn,
            &make_run("r2", 2000, "b", Some(404)),
            &make_payload("r2"),
        )
        .unwrap();

        let mut buf = Vec::new();
        let count = export_jsonl(&conn, &mut buf).unwrap();
        assert_eq!(count, 2);

        let output = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = output.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);

        // Each line should be valid JSON
        let v1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v1["id"], "r1");
        let v2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(v2["id"], "r2");
    }

    #[test]
    fn parse_duration_valid() {
        assert_eq!(parse_duration_str("30m").unwrap(), 30 * 60 * 1000);
        assert_eq!(parse_duration_str("1h").unwrap(), 60 * 60 * 1000);
        assert_eq!(parse_duration_str("7d").unwrap(), 7 * 24 * 60 * 60 * 1000);
    }

    #[test]
    fn parse_duration_invalid() {
        assert!(parse_duration_str("").is_err());
        assert!(parse_duration_str("abc").is_err());
        assert!(parse_duration_str("10x").is_err());
        assert!(parse_duration_str("d").is_err());
    }
}
