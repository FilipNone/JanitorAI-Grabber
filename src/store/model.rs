use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Request,
    Response,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SecretFlag {
    Clean,
    /// Contains authorization/session material — UI must redact by default.
    Secret,
}

/// One captured HTTP half (request or response) of the proxy exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureRecord {
    pub id: Uuid,
    pub direction: Direction,
    pub timestamp: DateTime<Utc>,
    pub method: String,
    pub path: String,
    pub status: Option<u16>,
    /// Header names only at this level; values live in `headers`.
    pub headers: Vec<(String, String)>,
    pub secret: SecretFlag,
    pub body: Option<String>,
    /// Milliseconds spent on the upstream call.
    pub duration_ms: Option<u64>,
}

impl CaptureRecord {
    pub fn request(
        method: impl Into<String>,
        path: impl Into<String>,
        headers: Vec<(String, String)>,
        body: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            direction: Direction::Request,
            timestamp: Utc::now(),
            method: method.into(),
            path: path.into(),
            status: None,
            headers,
            secret: SecretFlag::Clean,
            body,
            duration_ms: None,
        }
    }

    pub fn response(
        path: impl Into<String>,
        status: u16,
        headers: Vec<(String, String)>,
        body: Option<String>,
        duration_ms: u64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            direction: Direction::Response,
            timestamp: Utc::now(),
            method: String::new(),
            path: path.into(),
            status: Some(status),
            headers,
            secret: SecretFlag::Clean,
            body,
            duration_ms: Some(duration_ms),
        }
    }

    /// Header names that always carry credentials.
    pub fn is_secret_header(name: &str) -> bool {
        let n = name.to_ascii_lowercase();
        n == "authorization"
            || n == "proxy-authorization"
            || n == "cookie"
            || n == "set-cookie"
            || n == "x-api-key"
            || n == "x-janitor-ai-key"
    }

    /// Header names whose values must not be shown by default in the UI.
    pub fn redacted_headers(&self) -> Vec<(String, String)> {
        self.headers
            .iter()
            .map(|(k, v)| {
                if Self::is_secret_header(k) {
                    (k.clone(), "«redacted»".to_string())
                } else {
                    (k.clone(), v.clone())
                }
            })
            .collect()
    }

    /// Pretty-print the body if it is valid JSON.
    pub fn body_pretty(&self) -> Option<String> {
        let raw = self.body.as_ref()?;
        serde_json::from_str::<serde_json::Value>(raw)
            .map(|v| serde_json::to_string_pretty(&v).unwrap_or_else(|_| raw.clone()))
            .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_flagged_and_redactable() {
        let rec = CaptureRecord::request(
            "POST",
            "/v1/chat/completions",
            vec![
                ("authorization".into(), "Bearer sk-supersecret".into()),
                ("content-type".into(), "application/json".into()),
            ],
            None,
        );
        let shown = rec.redacted_headers();
        assert_eq!(shown[0].1, "«redacted»");
        assert_eq!(shown[1].1, "application/json");
        // Raw value never in pretty output.
        let pretty = serde_json::to_string(&rec.redacted_headers()).unwrap();
        assert!(!pretty.contains("sk-supersecret"));
    }

    #[test]
    fn json_body_pretty_prints() {
        let mut rec = CaptureRecord::response(
            "/v1/chat/completions",
            200,
            vec![],
            Some(r#"{"a":1}"#.to_string()),
            12,
        );
        rec.secret = SecretFlag::Clean;
        assert_eq!(rec.body_pretty().unwrap(), "{\n  \"a\": 1\n}");
    }
}
