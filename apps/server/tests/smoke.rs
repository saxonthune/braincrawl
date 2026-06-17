//! In-process HTTP smoke test: boots the axum server, exercises key routes.

use std::sync::Arc;

use braincrawl_auth::SharedSecret;
use braincrawl_server_lib::{make_app, make_store, AuthConfig};
use tokio::net::TcpListener;

async fn start_server(dir: &std::path::Path) -> (String, tokio::task::JoinHandle<()>) {
    let db_path = dir.join("test.db").to_string_lossy().to_string();
    let blob_root = dir.join("blobs").to_string_lossy().to_string();
    std::fs::create_dir_all(dir.join("blobs")).unwrap();

    let store = Arc::new(make_store(&db_path, &blob_root).expect("store"));
    let auth = Arc::new(AuthConfig {
        disabled: true,
        allowlist: SharedSecret::new("", ""),
    });
    let app = make_app(store, auth);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    // Brief yield so the server task is scheduled before we fire requests.
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    (base, handle)
}

#[tokio::test]
async fn test_put_and_get_work() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    // PUT /works
    let body = serde_json::json!({
        "source": "openalex",
        "kind": "Work",
        "aliases": [{"namespace": "doi", "value": "10.99/smoke"}],
        "attrs": {"title": "Smoke Test Paper"}
    });
    let res = client
        .put(format!("{base}/works"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "PUT /works should return 200");
    let json: serde_json::Value = res.json().await.unwrap();
    assert!(
        json["id"].as_str().unwrap_or("").starts_with("guid:"),
        "id should be a guid"
    );

    // GET /works/{id}
    let res = client
        .get(format!("{base}/works/doi:10.99/smoke"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET /works/doi:10.99/smoke should return 200");

    // GET /works/unknown → 404
    let res = client
        .get(format!("{base}/works/doi:10.99/does-not-exist"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);

    handle.abort();
}

#[tokio::test]
async fn test_have() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    // POST /works/have — none present yet
    let res = client
        .post(format!("{base}/works/have"))
        .json(&serde_json::json!({"ids": ["doi:10.0/none"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let present: Vec<String> = res.json().await.unwrap();
    assert!(present.is_empty(), "no aliases present yet");

    // Put a work, then check again
    client
        .put(format!("{base}/works"))
        .json(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"namespace": "doi", "value": "10.0/known"}],
            "attrs": {}
        }))
        .send()
        .await
        .unwrap();

    let res = client
        .post(format!("{base}/works/have"))
        .json(&serde_json::json!({"ids": ["doi:10.0/known", "doi:10.0/none"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let present: Vec<String> = res.json().await.unwrap();
    assert_eq!(present, vec!["doi:10.0/known"]);

    handle.abort();
}

#[tokio::test]
async fn test_content_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    // Create a work
    client
        .put(format!("{base}/works"))
        .json(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"namespace": "doi", "value": "10.1"}],
            "attrs": {}
        }))
        .send()
        .await
        .unwrap();

    // PUT content
    let res = client
        .put(format!(
            "{base}/works/doi:10.1/content/abstract?mime=text/plain&rights=open&fetched_at=2024-01-01T00:00:00Z"
        ))
        .body("hello braincrawl")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "PUT content should succeed");

    // GET content → 200 with bytes
    let res = client
        .get(format!("{base}/works/doi:10.1/content/abstract"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET content should return 200");
    let body = res.text().await.unwrap();
    assert_eq!(body, "hello braincrawl");

    handle.abort();
}
