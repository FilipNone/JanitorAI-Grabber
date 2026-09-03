//! Integration tests:
//! 1. capture mode: the fake endpoint stores the chat-completion request and
//!    replies with a stub OpenAI success (JanitorAI proxy-preset flow),
//! 2. forward mode: a canned chat-completion exchange is forwarded upstream
//!    and stored on both sides, byte-identical response returned.

use janitorai_grabber::config::Mode;
use janitorai_grabber::store::{Direction, SecretFlag, Store};
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_endpoint_stores_and_replies() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();

    let tmp = tempdir().unwrap();
    let store = Store::open(&tmp.path().join("t.db")).await.unwrap();

    let handle = janitorai_grabber::proxy::server::spawn(
        "127.0.0.1:0",
        Mode::Capture,
        String::new(),
        store.clone(),
    )
    .await
    .unwrap();
    let url = format!("http://{}/v1/chat/completions", handle.addr);

    let client = reqwest::Client::new();
    let mut resp = None;
    for _ in 0..20 {
        match client
            .post(&url)
            .header("authorization", "Bearer fake-key")
            .json(&serde_json::json!({
                "model": "gpt-x",
                "messages": [
                    {"role": "system", "content": "<Char's Persona> card </Char's Persona>"},
                    {"role": "user", "content": "hello from janitor"}
                ]
            }))
            .send()
            .await
        {
            Ok(r) => {
                resp = Some(r);
                break;
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
        }
    }
    let resp = resp.expect("capture endpoint never became reachable");

    // Success stub returned so the chat UI sees the message as delivered.
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["object"], "chat.completion");
    assert_eq!(body["choices"][0]["message"]["role"], "assistant");
    assert_eq!(
        body["choices"][0]["message"]["content"],
        "hello from janitor"
    );

    // Stored request contains the full prompt; secrets flagged.
    let caps = store.list_latest(10).await.unwrap();
    let req = caps
        .iter()
        .find(|c| c.direction == Direction::Request)
        .expect("no request capture stored");
    assert!(req.body.as_ref().unwrap().contains("hello from janitor"));
    assert!(req.body.as_ref().unwrap().contains("Char's Persona"));
    assert_eq!(req.secret, SecretFlag::Secret);

    // A stub response record was stored too.
    let resp_rec = caps
        .iter()
        .find(|c| c.direction == Direction::Response)
        .expect("no response capture stored");
    assert_eq!(resp_rec.status, Some(200));
    assert!(resp_rec.body.as_ref().unwrap().contains("chat.completion"));

    handle.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_rejects_non_json_body() {
    let tmp = tempdir().unwrap();
    let store = Store::open(&tmp.path().join("t.db")).await.unwrap();
    let handle =
        janitorai_grabber::proxy::server::spawn("127.0.0.1:0", Mode::Capture, String::new(), store)
            .await
            .unwrap();

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{}/v1/chat/completions", handle.addr))
        .body("this is not json")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    handle.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn end_to_end_forward_and_capture() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();

    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(
                r#"{"choices":[{"message":{"role":"assistant","content":"hi"}}]}"#,
            ),
        )
        .expect(1)
        .mount(&upstream)
        .await;

    let tmp = tempdir().unwrap();
    let db_path = tmp.path().join("t.db");
    let store = Store::open(&db_path).await.unwrap();
    eprintln!("db at {}", db_path.display());

    let handle = janitorai_grabber::proxy::server::spawn(
        "127.0.0.1:0",
        Mode::Forward,
        upstream.uri(),
        store.clone(),
    )
    .await
    .unwrap();
    eprintln!("proxy at {}", handle.addr);
    let url = format!("http://{}/v1/chat/completions", handle.addr);

    // Retry a few times in case the accept loop isn't live yet.
    let client = reqwest::Client::new();
    let mut resp = None;
    for _ in 0..20 {
        match client
            .post(&url)
            .header("authorization", "Bearer integration-test")
            .json(&serde_json::json!({"model": "gpt-x", "messages": [{"role": "user", "content": "hello"}]}))
            .send()
            .await
        {
            Ok(r) => {
                resp = Some(r);
                break;
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
        }
    }
    let resp = resp.expect("proxy never became reachable");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["choices"][0]["message"]["content"], "hi");

    let caps = store.list_latest(10).await.unwrap();
    assert_eq!(caps.len(), 2);

    let req = caps
        .iter()
        .find(|c| c.direction == janitorai_grabber::store::Direction::Request)
        .unwrap();
    assert!(req.body.as_ref().unwrap().contains("hello"));
    assert_eq!(req.secret, janitorai_grabber::store::SecretFlag::Secret);

    let resp_rec = caps
        .iter()
        .find(|c| c.direction == janitorai_grabber::store::Direction::Response)
        .unwrap();
    assert_eq!(resp_rec.status, Some(200));
    assert!(resp_rec.body.as_ref().unwrap().contains("assistant"));

    handle.stop();
}
