use crate::store::{CaptureRecord, Store};
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use std::time::Instant;

#[derive(Clone)]
pub struct AppState {
    pub upstream_base: String,
    pub store: Store,
}

/// Forward each request to the upstream, capture both halves, and return its response unchanged.
pub async fn forward(
    State(state): State<AppState>,
    method: axum::http::Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> Response {
    let started = Instant::now();
    let path = uri
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| uri.path().to_string());

    let req_body = match axum::body::to_bytes(body, MAX_BODY).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(error = %e, "request body too large or unreadable");
            return (StatusCode::PAYLOAD_TOO_LARGE, "body too large").into_response();
        }
    };

    // Store the request headers; the UI redacts secret values.
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
    let req_body_text = String::from_utf8_lossy(&req_body).to_string();
    let mut req_rec = CaptureRecord::request(
        method.as_str(),
        &path,
        req_headers.clone(),
        Some(req_body_text.clone()),
    );
    if has_secret {
        req_rec.secret = crate::store::SecretFlag::Secret;
    }
    if let Err(e) = state.store.insert(&req_rec).await {
        tracing::error!(error = %e, "failed to persist request capture");
    }

    // Build the upstream request.
    let upstream_url = format!("{}{}", state.upstream_base.trim_end_matches('/'), path);
    let client = match reqwest::Client::builder().build() {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("client error: {e}"),
            )
                .into_response()
        }
    };
    let upstream_method: reqwest::Method =
        reqwest::Method::from_bytes(method.as_str().as_bytes()).unwrap_or(reqwest::Method::GET);
    let mut out = client.request(upstream_method, &upstream_url);
    for (k, v) in &req_headers {
        // Reqwest manages hop-by-hop headers itself.
        let lk = k.to_ascii_lowercase();
        if matches!(
            lk.as_str(),
            "host" | "content-length" | "transfer-encoding" | "connection"
        ) {
            continue;
        }
        if let Ok(val) = v.parse::<reqwest::header::HeaderValue>() {
            out = out.header(k.as_str(), val);
        }
    }
    if !req_body.is_empty() {
        out = out.body(req_body.to_vec());
    }

    let resp = match out.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(upstream = %upstream_url, error = %e, "upstream call failed");
            let rec = CaptureRecord::response(
                &path,
                502,
                vec![],
                Some(format!("upstream error: {e}")),
                started.elapsed().as_millis() as u64,
            );
            let _ = state.store.insert(&rec).await;
            return (StatusCode::BAD_GATEWAY, format!("upstream error: {e}")).into_response();
        }
    };

    let status = resp.status().as_u16();
    let resp_headers: Vec<(String, String)> = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().to_string(),
                v.to_str().unwrap_or("<binary>").to_string(),
            )
        })
        .collect();
    let resp_bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "failed reading upstream body");
            return (StatusCode::BAD_GATEWAY, "upstream body read failed").into_response();
        }
    };
    let duration_ms = started.elapsed().as_millis() as u64;

    // Store the upstream response.
    let resp_body_text = String::from_utf8_lossy(&resp_bytes).to_string();
    let resp_rec = CaptureRecord::response(
        &path,
        status,
        resp_headers.clone(),
        Some(resp_body_text),
        duration_ms,
    );
    if let Err(e) = state.store.insert(&resp_rec).await {
        tracing::error!(error = %e, "failed to persist response capture");
    }

    // Return the response to the caller without changing its body.
    let mut out_resp =
        Response::builder().status(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY));
    for (k, v) in &resp_headers {
        let lk = k.to_ascii_lowercase();
        if matches!(
            lk.as_str(),
            "content-length" | "transfer-encoding" | "connection"
        ) {
            continue;
        }
        if let (Ok(name), Ok(val)) = (
            k.parse::<axum::http::HeaderName>(),
            v.parse::<axum::http::HeaderValue>(),
        ) {
            out_resp = out_resp.header(name, val);
        }
    }
    out_resp
        .body(Body::from(resp_bytes))
        .unwrap_or_else(|_| (StatusCode::BAD_GATEWAY, "replay failed").into_response())
}

const MAX_BODY: usize = 64 * 1024 * 1024; // 64 MiB maximum

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn test_store() -> (Store, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("t.db")).await.unwrap();
        (store, tmp) // Keep TempDir alive for the whole test.
    }

    #[tokio::test]
    async fn forwards_and_captures() {
        let upstream = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"ok":true}"#))
            .expect(1)
            .mount(&upstream)
            .await;

        let (store, _guard) = test_store().await;
        let state = AppState {
            upstream_base: upstream.uri(),
            store,
        };
        let router = crate::proxy::router::build_router(state.clone());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://{addr}/v1/chat/completions"))
            .header("authorization", "Bearer test")
            .json(&serde_json::json!({"model": "m", "messages": []}))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), r#"{"ok":true}"#);

        // The store should contain one request and one response.
        let caps = state.store.list_latest(10).await.unwrap();
        assert_eq!(caps.len(), 2);
        assert!(caps
            .iter()
            .any(|c| c.direction == crate::store::Direction::Request
                && c.secret == crate::store::SecretFlag::Secret));
        assert!(caps
            .iter()
            .any(|c| c.direction == crate::store::Direction::Response && c.status == Some(200)));
    }
}
