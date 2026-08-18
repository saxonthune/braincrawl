//! `library extract-text --from` and `library chunk --from` select the source
//! role instead of the hardcoded "fulltext" — boots the real server and
//! drives the compiled `braincrawl` binary against it.

use std::process::Command;
use std::sync::Arc;

use braincrawl_auth::SharedSecret;
use braincrawl_cli::store_client::StoreClient;
use braincrawl_server::{make_app, make_store, AuthConfig};

fn start_server(dir: &std::path::Path) -> String {
    let db_path = dir.join("test.db").to_string_lossy().to_string();
    let blob_root = dir.join("blobs").to_string_lossy().to_string();
    std::fs::create_dir_all(dir.join("blobs")).unwrap();

    let (tx, rx) = std::sync::mpsc::channel::<String>();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let store = Arc::new(make_store(&db_path, &blob_root).expect("make_store"));
            let auth = Arc::new(AuthConfig {
                disabled: true,
                allowlist: SharedSecret::new("", ""),
            });
            let app = make_app(store, auth, None, None);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tx.send(format!("http://{addr}")).unwrap();
            axum::serve(listener, app).await.ok();
        });
    });

    let base = rx.recv().expect("server did not send address");
    std::thread::sleep(std::time::Duration::from_millis(20));
    base
}

fn run_cli(server_url: &str, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_braincrawl"))
        .env("BRAINCRAWL_SERVER_URL", server_url)
        .env_remove("BRAINCRAWL_AUTH_TOKEN")
        .args(args)
        .output()
        .expect("failed to run braincrawl binary")
}

#[test]
fn extract_text_from_missing_role_names_that_role_in_error() {
    let dir = tempfile::tempdir().unwrap();
    let base = start_server(dir.path());
    let client = StoreClient::new(&base);

    client
        .put_work(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.0/from-role"}],
            "attrs": {}
        }))
        .unwrap();

    let output = run_cli(
        &base,
        &[
            "library",
            "extract-text",
            "doi:10.0/from-role",
            "--from",
            "fulltext-1964-scan",
        ],
    );

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("fulltext-1964-scan"),
        "expected error to name the requested role, got: {stderr}"
    );
    assert!(
        !stderr.contains("no fulltext artifact"),
        "error should not fall back to the literal word fulltext, got: {stderr}"
    );
}

#[test]
fn chunk_from_missing_role_names_that_role_in_error() {
    let dir = tempfile::tempdir().unwrap();
    let base = start_server(dir.path());
    let client = StoreClient::new(&base);

    client
        .put_work(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.0/chunk-from-role"}],
            "attrs": {}
        }))
        .unwrap();

    let output = run_cli(
        &base,
        &[
            "library",
            "chunk",
            "doi:10.0/chunk-from-role",
            "--from",
            "fulltext-1964-scan",
        ],
    );

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("fulltext-1964-scan"),
        "expected error to name the requested role, got: {stderr}"
    );
}

#[test]
fn extract_text_from_reaches_the_read() {
    let dir = tempfile::tempdir().unwrap();
    let base = start_server(dir.path());
    let client = StoreClient::new(&base);

    client
        .put_work(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.0/scan-role"}],
            "attrs": {}
        }))
        .unwrap();

    // Store a non-PDF blob under a non-default role; --from must select it,
    // and the not-a-PDF message must name that role rather than "fulltext".
    client
        .put_content(
            "doi:10.0/scan-role",
            "fulltext-1964-scan",
            b"not a pdf".to_vec(),
            "text/plain",
            Some("test"),
            None,
        )
        .unwrap();

    let output = run_cli(
        &base,
        &[
            "library",
            "extract-text",
            "doi:10.0/scan-role",
            "--from",
            "fulltext-1964-scan",
        ],
    );

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("fulltext-1964-scan artifact") && stderr.contains("is not a PDF"),
        "expected the not-a-PDF error to name the --from role, got: {stderr}"
    );
}
