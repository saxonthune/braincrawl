//! Integration tests for the server auth gate.

use std::sync::Arc;

use braincrawl_auth::SharedSecret;
use braincrawl_server_lib::{make_app, make_store, AuthConfig};
use tokio::net::TcpListener;

async fn start_server_with_auth(
    dir: &std::path::Path,
    auth: Arc<AuthConfig>,
) -> (String, tokio::task::JoinHandle<()>) {
    let db_path = dir.join("test.db").to_string_lossy().to_string();
    let blob_root = dir.join("blobs").to_string_lossy().to_string();
    std::fs::create_dir_all(dir.join("blobs")).unwrap();

    let store = Arc::new(make_store(&db_path, &blob_root).expect("store"));
    let app = make_app(store, auth);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    (base, handle)
}

fn auth_enabled() -> Arc<AuthConfig> {
    Arc::new(AuthConfig {
        disabled: false,
        allowlist: SharedSecret::new("bc_test_token", "default"),
    })
}

fn auth_disabled() -> Arc<AuthConfig> {
    Arc::new(AuthConfig {
        disabled: true,
        allowlist: SharedSecret::new("", ""),
    })
}

fn put_works_body() -> serde_json::Value {
    serde_json::json!({
        "source": "test",
        "kind": "Work",
        "aliases": [{"namespace": "doi", "value": "10.0/auth-test"}],
        "attrs": {}
    })
}

#[tokio::test]
async fn test_no_auth_header_returns_401() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server_with_auth(dir.path(), auth_enabled()).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base}/works"))
        .json(&put_works_body())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 401, "missing bearer should return 401");
    handle.abort();
}

#[tokio::test]
async fn test_wrong_token_returns_403() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server_with_auth(dir.path(), auth_enabled()).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base}/works"))
        .header("Authorization", "Bearer wrong_token")
        .json(&put_works_body())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 403, "wrong bearer should return 403");
    handle.abort();
}

#[tokio::test]
async fn test_correct_token_returns_200() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server_with_auth(dir.path(), auth_enabled()).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base}/works"))
        .header("Authorization", "Bearer bc_test_token")
        .json(&put_works_body())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200, "correct bearer should return 200");
    handle.abort();
}

#[tokio::test]
async fn test_disabled_auth_bypasses_gate() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server_with_auth(dir.path(), auth_disabled()).await;

    let client = reqwest::Client::new();
    let res = client
        .put(format!("{base}/works"))
        .json(&put_works_body())
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200, "disabled auth should pass through without bearer");
    handle.abort();
}
