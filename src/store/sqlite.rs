//! SQLite-backed capture store using sqlx and the Tokio runtime.

use super::model::{CaptureRecord, Direction, SecretFlag};
use std::path::Path;
use std::str::FromStr;

#[derive(Clone)]
pub struct Store {
    pool: sqlx::SqlitePool,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS captures (
    id TEXT PRIMARY KEY,
    direction TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    method TEXT NOT NULL,
    path TEXT NOT NULL,
    status INTEGER,
    headers TEXT NOT NULL,   -- JSON array of [name, value]
    secret TEXT NOT NULL,    -- 'clean' | 'secret'
    body TEXT,
    duration_ms INTEGER
);
CREATE INDEX IF NOT EXISTS idx_captures_ts ON captures(timestamp);
"#;

impl Store {
    pub async fn open(db_path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let url = format!("sqlite:{}?mode=rwc", db_path.display());
        let pool = sqlx::SqlitePool::connect(&url).await?;
        sqlx::query(SCHEMA).execute(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn insert(&self, rec: &CaptureRecord) -> anyhow::Result<()> {
        let headers = serde_json::to_string(&rec.headers)?;
        let secret = match rec.secret {
            SecretFlag::Clean => "clean",
            SecretFlag::Secret => "secret",
        };
        sqlx::query(
            "INSERT INTO captures (id, direction, timestamp, method, path, status, headers, secret, body, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(rec.id.to_string())
        .bind(match rec.direction {
            Direction::Request => "request",
            Direction::Response => "response",
        })
        .bind(rec.timestamp.to_rfc3339())
        .bind(&rec.method)
        .bind(&rec.path)
        .bind(rec.status.map(i64::from))
        .bind(headers)
        .bind(secret)
        .bind(&rec.body)
        .bind(rec.duration_ms.map(|d| i64::try_from(d).unwrap_or(i64::MAX)))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_latest(&self, limit: i64) -> anyhow::Result<Vec<CaptureRecord>> {
        let rows = sqlx::query_as::<_, (String, String, String, String, String, Option<i64>, String, String, Option<String>, Option<i64>)>(
            "SELECT id, direction, timestamp, method, path, status, headers, secret, body, duration_ms
             FROM captures ORDER BY timestamp DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(
                |(id, direction, ts, method, path, status, headers, secret, body, duration)| {
                    Ok(CaptureRecord {
                        id: uuid::Uuid::from_str(&id)?,
                        direction: if direction == "request" {
                            Direction::Request
                        } else {
                            Direction::Response
                        },
                        timestamp: chrono::DateTime::parse_from_rfc3339(&ts)?
                            .with_timezone(&chrono::Utc),
                        method,
                        path,
                        status: status.map(|s| s as u16),
                        headers: serde_json::from_str(&headers)?,
                        secret: if secret == "secret" {
                            SecretFlag::Secret
                        } else {
                            SecretFlag::Clean
                        },
                        body,
                        duration_ms: duration.map(|d| d as u64),
                    })
                },
            )
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_and_list_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("test.db")).await.unwrap();

        let rec = CaptureRecord::request(
            "POST",
            "/v1/chat/completions",
            vec![("authorization".into(), "Bearer x".into())],
            Some(r#"{"model":"gpt-test"}"#.into()),
        );
        store.insert(&rec).await.unwrap();

        let listed = store.list_latest(10).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, rec.id);
        assert_eq!(listed[0].headers[0].1, "Bearer x");
    }
}
