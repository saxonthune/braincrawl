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
async fn conformance_native() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path(), None).await;

    tokio::task::spawn_blocking(move || {
        braincrawl_conformance::run_all(&base, None).unwrap();
    })
    .await
    .unwrap();

    handle.abort();
}

#[tokio::test]
async fn conformance_l3_native() {
    let dir = tempfile::tempdir().unwrap();
    let l3_root = dir.path().join("l3-root");
    std::fs::create_dir_all(&l3_root).unwrap();
    let (base, handle) = start_server(dir.path(), Some(l3_root)).await;

    tokio::task::spawn_blocking(move || {
        braincrawl_conformance::check_l3_docs(&base, "unused").unwrap();
        braincrawl_conformance::check_l3_prev_backup(&base, "unused").unwrap();
        braincrawl_conformance::check_l3_agent_files(&base, "unused").unwrap();
    })
    .await
    .unwrap();

    handle.abort();
}
