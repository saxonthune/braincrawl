//! `library paginate` — boots the real server and drives the compiled
//! `braincrawl` binary against it, over the `hello.pdf` fixture (a single
//! page with no folio; this only verifies plumbing and storage, not
//! detection — see `pages::tests` for folio-detection coverage).

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

fn put_work_with_pdf(client: &StoreClient, id: &str) {
    client
        .put_work(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": id}],
            "attrs": {}
        }))
        .unwrap();

    let bytes = include_bytes!("fixtures/hello.pdf").to_vec();
    let alias = format!("doi:{id}");
    client
        .put_content(&alias, "fulltext", bytes, "application/pdf", Some("test"), None)
        .unwrap();
}

#[test]
fn paginate_stores_a_pages_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let base = start_server(dir.path());
    let client = StoreClient::new(&base);
    put_work_with_pdf(&client, "10.0/paginate-store");

    let output = run_cli(&base, &["library", "paginate", "doi:10.0/paginate-store", "--allow-no-folios"]);
    assert!(
        output.status.success(),
        "paginate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output = run_cli(&base, &["library", "get", "doi:10.0/paginate-store", "--role", "pages"]);
    assert!(output.status.success());
    let pages: serde_json::Value = serde_json::from_slice(&output.stdout).expect("pages artifact should be JSON");
    assert_eq!(pages["page_count"], 1);
    assert_eq!(pages["source_role"], "fulltext");
    assert_eq!(pages["pages"][0]["pdf_page"], 1);
    assert!(pages["pages"][0]["text"].as_str().unwrap().contains("Hello braincrawl"));
}

#[test]
fn paginate_refuses_to_store_a_folioless_detection() {
    let dir = tempfile::tempdir().unwrap();
    let base = start_server(dir.path());
    let client = StoreClient::new(&base);
    put_work_with_pdf(&client, "10.0/paginate-no-folio");

    let output = run_cli(&base, &["library", "paginate", "doi:10.0/paginate-no-folio"]);
    assert!(!output.status.success(), "a folio-less detection should not be stored silently");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--anchor"), "the refusal should name the remedy, got: {stderr}");

    let output = run_cli(&base, &["library", "get", "doi:10.0/paginate-no-folio", "--role", "pages"]);
    assert!(!output.status.success(), "the refused run should have stored nothing");
}

#[test]
fn paginate_takes_folios_from_anchors() {
    let dir = tempfile::tempdir().unwrap();
    let base = start_server(dir.path());
    let client = StoreClient::new(&base);
    put_work_with_pdf(&client, "10.0/paginate-anchored");

    let output = run_cli(
        &base,
        &["library", "paginate", "doi:10.0/paginate-anchored", "--anchor", "1=42"],
    );
    assert!(
        output.status.success(),
        "anchored paginate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("pdf 1 = folio 42"), "expected the mapping to be printed, got: {stderr}");

    let output = run_cli(&base, &["library", "get", "doi:10.0/paginate-anchored", "--role", "pages"]);
    assert!(output.status.success());
    let pages: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(pages["folio_method"], "Anchored");
    assert_eq!(pages["pages"][0]["folio"], "42");
}

#[test]
fn paginate_rejects_a_malformed_anchor() {
    let dir = tempfile::tempdir().unwrap();
    let base = start_server(dir.path());
    let client = StoreClient::new(&base);
    put_work_with_pdf(&client, "10.0/paginate-bad-anchor");

    let output = run_cli(
        &base,
        &["library", "paginate", "doi:10.0/paginate-bad-anchor", "--anchor", "page-one"],
    );
    assert!(!output.status.success());
}

#[test]
fn paginate_refuses_a_non_pdf_source() {
    let dir = tempfile::tempdir().unwrap();
    let base = start_server(dir.path());
    let client = StoreClient::new(&base);

    client
        .put_work(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.0/paginate-not-pdf"}],
            "attrs": {}
        }))
        .unwrap();
    client
        .put_content("doi:10.0/paginate-not-pdf", "fulltext", b"not a pdf".to_vec(), "text/plain", Some("test"), None)
        .unwrap();

    let output = run_cli(&base, &["library", "paginate", "doi:10.0/paginate-not-pdf"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("is not a PDF"), "expected not-a-PDF error, got: {stderr}");
}

#[test]
fn paginate_already_present_requires_force() {
    let dir = tempfile::tempdir().unwrap();
    let base = start_server(dir.path());
    let client = StoreClient::new(&base);
    put_work_with_pdf(&client, "10.0/paginate-force");

    let first = run_cli(&base, &["library", "paginate", "doi:10.0/paginate-force", "--allow-no-folios"]);
    assert!(first.status.success());

    let second = run_cli(&base, &["library", "paginate", "doi:10.0/paginate-force", "--allow-no-folios"]);
    assert!(second.status.success());
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(stderr.contains("already-present"), "expected already-present notice, got: {stderr}");

    let forced = run_cli(&base, &["library", "paginate", "doi:10.0/paginate-force", "--allow-no-folios", "--force"]);
    assert!(forced.status.success());
    let stderr = String::from_utf8_lossy(&forced.stderr);
    assert!(stderr.contains("paginated:"), "expected re-paginate to succeed, got: {stderr}");
}

#[test]
fn read_prints_page_markers_for_pdf_range() {
    let dir = tempfile::tempdir().unwrap();
    let base = start_server(dir.path());
    let client = StoreClient::new(&base);
    put_work_with_pdf(&client, "10.0/read-pdf-range");

    let paginate = run_cli(&base, &["library", "paginate", "doi:10.0/read-pdf-range", "--allow-no-folios"]);
    assert!(paginate.status.success());

    let output = run_cli(&base, &["library", "read", "doi:10.0/read-pdf-range", "--pdf", "1-1"]);
    assert!(output.status.success(), "read failed: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("=== pdf 1 (no folio) ==="), "got: {stdout}");
    assert!(stdout.contains("Hello braincrawl"), "got: {stdout}");
}
