//! In-process HTTP test for GET /api/l3/graph.

use std::sync::Arc;

use braincrawl_auth::SharedSecret;
use braincrawl_server_lib::{make_app, make_store, AuthConfig};
use tokio::net::TcpListener;

async fn start_server(
    dir: &std::path::Path,
    l3_root: Option<std::path::PathBuf>,
) -> (String, tokio::task::JoinHandle<()>) {
    let db_path = dir.join("test.db").to_string_lossy().to_string();
    let blob_root = dir.join("blobs").to_string_lossy().to_string();
    std::fs::create_dir_all(dir.join("blobs")).unwrap();

    let store = Arc::new(make_store(&db_path, &blob_root).expect("store"));
    let auth = Arc::new(AuthConfig {
        disabled: true,
        allowlist: SharedSecret::new("", ""),
    });
    let app = make_app(store, auth, l3_root);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    (base, handle)
}

#[tokio::test]
async fn test_l3_graph_endpoint_and_etag_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let l3_root = dir.path().join("l3");
    std::fs::create_dir_all(&l3_root).unwrap();
    std::fs::write(
        l3_root.join("fixture.l3.md"),
        "---\ndoc: fixture\nschema: freeform\n---\n\n## A node ^r-aaa1\n- tags: #landmark\n",
    )
    .unwrap();

    let (base, handle) = start_server(dir.path(), Some(l3_root)).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base}/api/l3/graph"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET /api/l3/graph should return 200");
    let etag = res
        .headers()
        .get(reqwest::header::ETAG)
        .expect("ETag header present")
        .to_str()
        .unwrap()
        .to_string();
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body.get("nodes").is_some(), "response must have nodes");
    assert!(body.get("links").is_some(), "response must have links");
    let nodes = body["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["id"], "r-aaa1");

    // If-None-Match round-trip: same ETag → 304.
    let res = client
        .get(format!("{base}/api/l3/graph"))
        .header(reqwest::header::IF_NONE_MATCH, etag)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 304, "matching If-None-Match should return 304");

    handle.abort();
}

#[tokio::test]
async fn test_l3_graph_endpoint_404_when_root_unset() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path(), None).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base}/api/l3/graph"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404, "unset L3 root should return 404");

    handle.abort();
}
