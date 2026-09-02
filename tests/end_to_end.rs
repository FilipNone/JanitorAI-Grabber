//! Integration test that forwards a canned chat-completion exchange and stores both halves.

use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// The crate needs a lib target for integration tests to import internals.
// See src/lib.rs.
use janitorai_grabber::store::Store;

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

    let handle =
        janitorai_grabber::proxy::server::spawn("127.0.0.1:0", upstream.uri(), store.clone())
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
