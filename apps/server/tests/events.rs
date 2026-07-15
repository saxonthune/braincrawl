//! In-process HTTP test for GET /api/events (SSE change notifications).

use std::sync::Arc;

use braincrawl_auth::SharedSecret;
use braincrawl_server_lib::{make_app, make_store, AuthConfig};
use futures_util::StreamExt;
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
    let app = make_app(store, auth, l3_root, None);

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
async fn test_events_endpoint_fires_on_file_change() {
    let dir = tempfile::tempdir().unwrap();
    let l3_root = dir.path().join("l3");
    std::fs::create_dir_all(&l3_root).unwrap();
    std::fs::write(
        l3_root.join("fixture.l3.md"),
        "---\ndoc: fixture\nschema: freeform\n---\n\n## A node ^r-aaa1\n- tags: #landmark\n",
    )
    .unwrap();

    let (base, handle) = start_server(dir.path(), Some(l3_root.clone())).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base}/api/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET /api/events should return 200");
    assert_eq!(
        res.headers().get(reqwest::header::CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );

    let mut stream = res.bytes_stream();

    // Give the watcher a moment to register before writing.
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    std::fs::write(
        l3_root.join("fixture.l3.md"),
        "---\ndoc: fixture\nschema: freeform\n---\n\n## A node, edited ^r-aaa1\n- tags: #landmark\n",
    )
    .unwrap();

    let event = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let chunk = stream.next().await.expect("stream ended").unwrap();
            let text = String::from_utf8_lossy(&chunk).to_string();
            if text.contains("changed") {
                return text;
            }
        }
    })
    .await
    .expect("timed out waiting for a changed event");

    assert!(event.contains("data: changed"));

    handle.abort();
}

#[tokio::test]
async fn test_events_endpoint_available_when_l3_root_unset() {
    // No write path exists without an l3_root, but the SSE route itself must
    // still connect (it just never emits) rather than 404ing.
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path(), None).await;
    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base}/api/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);

    handle.abort();
}
