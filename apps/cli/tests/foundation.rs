//! Round-trip integration test: boots the real server in a background thread,
//! exercises the StoreClient's have/get_work methods over HTTP.

use std::sync::Arc;

use braincrawl_auth::SharedSecret;
use braincrawl_cli::store_client::StoreClient;
use braincrawl_server::{make_app, make_store, AuthConfig};

fn start_server(dir: &std::path::Path) -> (String, std::thread::JoinHandle<()>) {
    let db_path = dir.join("test.db").to_string_lossy().to_string();
    let blob_root = dir.join("blobs").to_string_lossy().to_string();
    std::fs::create_dir_all(dir.join("blobs")).unwrap();

    let (tx, rx) = std::sync::mpsc::channel::<String>();

    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let store = Arc::new(make_store(&db_path, &blob_root).expect("make_store"));
            let auth = Arc::new(AuthConfig {
                disabled: true,
                allowlist: SharedSecret::new("", ""),
            });
            let app = make_app(store, auth, None);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tx.send(format!("http://{addr}")).unwrap();
            axum::serve(listener, app).await.ok();
        });
    });

    let base = rx.recv().expect("server did not send address");
    // Brief pause to let the server task be scheduled.
    std::thread::sleep(std::time::Duration::from_millis(20));
    (base, handle)
}

#[test]
fn test_have_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _handle) = start_server(dir.path());
    let client = StoreClient::new(&base);

    // Nothing present yet.
    let present = client.have(&["doi:10.0/none".to_string()]).unwrap();
    assert!(present.is_empty(), "expected empty have result");

    // Store a work.
    client
        .put_work(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"namespace": "doi", "value": "10.0/known"}],
            "attrs": {}
        }))
        .unwrap();

    // The stored alias is present; the unknown one is not.
    let present = client
        .have(&["doi:10.0/known".to_string(), "doi:10.0/none".to_string()])
        .unwrap();
    assert_eq!(present, vec!["doi:10.0/known"]);
}

#[test]
fn test_get_work_missing_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _handle) = start_server(dir.path());
    let client = StoreClient::new(&base);

    let result = client.get_work("doi:10.0/does-not-exist").unwrap();
    assert!(result.is_none(), "expected None for unknown alias");
}

#[test]
fn test_get_work_present_returns_some() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _handle) = start_server(dir.path());
    let client = StoreClient::new(&base);

    client
        .put_work(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"namespace": "doi", "value": "10.1/present"}],
            "attrs": {"title": "Hello"}
        }))
        .unwrap();

    let result = client.get_work("doi:10.1/present").unwrap();
    assert!(result.is_some(), "expected Some for stored alias");
}
