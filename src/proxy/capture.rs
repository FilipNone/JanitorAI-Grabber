use super::handler::AppState;
use crate::store::{CaptureRecord, SecretFlag};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use std::time::Instant;

/// Maximum stored request body (8 MiB) — prompts can be large.
pub const MAX_BODY: usize = 8 * 1024 * 1024;

/// Capture-only endpoint used as a fake JanitorAI proxy preset.
///
/// Stores the incoming chat-completion request (the assembled prompt) and
/// replies with a stub OpenAI-shaped success so the chat UI reports the message
/// as delivered. Nothing is sent anywhere else.
pub async fn capture(
    State(state): State<AppState>,
    method: axum::http::Method,
    uri: Uri,
    headers: HeaderMap,
    body: axum::body::Body,
) -> Response {
    let started = Instant::now();
    let path = uri
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| uri.path().to_string());

    let body_bytes = match axum::body::to_bytes(body, MAX_BODY).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "request body too large or unreadable");
            return (StatusCode::PAYLOAD_TOO_LARGE, "body too large").into_response();
        }
    };
    let body_text = String::from_utf8_lossy(&body_bytes).to_string();

    // Reject non-JSON payloads early — they are not chat-completion traffic.
    let parsed: serde_json::Value = if body_bytes.is_empty() {
        serde_json::Value::Null
    } else {
        match serde_json::from_str(&body_text) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, path = %path, "non-JSON body on capture endpoint");
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": {"message": format!("body is not valid JSON: {e}"), "type": "invalid_request_error"}})),
                )
                    .into_response();
            }
        }
    };

    let req_headers: Vec<(String, String)> = headers
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                v.to_str().unwrap_or("<binary>").to_string(),
            )
        })
        .collect();
    let has_secret = req_headers
        .iter()
        .any(|(k, _)| CaptureRecord::is_secret_header(k));

    let rec = CaptureRecord::request(method.as_str(), &path, req_headers, Some(body_text));
    let mut rec = rec;
    if has_secret {
        rec.secret = SecretFlag::Secret;
    }
    match state.store.insert(&rec).await {
        Ok(()) => tracing::info!(path = %path, bytes = body_bytes.len(), "captured request"),
        Err(e) => {
            tracing::error!(error = %e, "failed to persist capture");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": {"message": "capture store failed", "type": "api_error"}})),
            )
                .into_response();
        }
    }

    let duration_ms = started.elapsed().as_millis() as u64;
    let resp_body = stub_response(&parsed);
    let resp_text = serde_json::to_string(&resp_body).unwrap_or_else(|_| "{}".into());
    let resp_rec = CaptureRecord::response(
        &path,
        200,
        vec![("content-type".into(), "application/json".into())],
        Some(resp_text),
        duration_ms,
    );
    let _ = state.store.insert(&resp_rec).await;

    Json(resp_body).into_response()
}

/// Build a minimal OpenAI-compatible success response. If the request carries
/// `messages`, echo the last user line back as the assistant content so the
/// JanitorAI UI shows something recognizable.
fn stub_response(req: &serde_json::Value) -> serde_json::Value {
    let last_user = req
        .get("messages")
        .and_then(|m| m.as_array())
        .and_then(|arr| {
            arr.iter()
                .rev()
                .find(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        })
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .chars()
        .take(120)
        .collect::<String>();

    json!({
        "id": format!("chatcmpl-grabber-{}", uuid::Uuid::new_v4().simple()),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": req.get("model").and_then(|m| m.as_str()).unwrap_or("grabber-capture"),
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": last_user},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_contains_last_user_message() {
        let req = serde_json::json!({
            "model": "gpt-x",
            "messages": [
                {"role": "system", "content": "sys"},
                {"role": "user", "content": "hello there"}
            ]
        });
        let stub = stub_response(&req);
        assert_eq!(stub["choices"][0]["message"]["content"], "hello there");
        assert_eq!(stub["model"], "gpt-x");
        assert_eq!(stub["object"], "chat.completion");
    }

    #[test]
    fn stub_without_messages_is_valid() {
        let stub = stub_response(&serde_json::json!({}));
        assert_eq!(stub["choices"][0]["message"]["content"], "");
    }
}
