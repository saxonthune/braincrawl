//! Tests for the graph neighborhood command: clap parse + round-trip HTTP.

use std::sync::Arc;

use braincrawl_auth::SharedSecret;
use braincrawl_cli::cli::{CatalogCmd, Cli, Namespace};
use braincrawl_cli::store_client::StoreClient;
use braincrawl_server::{make_app, make_store, AuthConfig};
use clap::Parser;

// ── clap parse test ──────────────────────────────────────────────────────────

#[test]
fn parse_graph_neighborhood_defaults() {
    let cli = Cli::parse_from(["braincrawl", "catalog", "neighborhood", "openalex:W1"]);
    match cli.namespace {
        Namespace::Catalog(c) => match c.cmd {
            CatalogCmd::Neighborhood { seeds, dir, depth, max_nodes } => {
                assert_eq!(seeds, vec!["openalex:W1"]);
                assert_eq!(dir, "forward");
                assert_eq!(depth, 1);
                assert_eq!(max_nodes, 200);
            }
            _ => panic!("expected Neighborhood command"),
        },
        _ => panic!("expected Catalog namespace"),
    }
}

#[test]
fn parse_graph_neighborhood_explicit_flags() {
    let cli = Cli::parse_from([
        "braincrawl",
        "catalog",
        "neighborhood",
        "openalex:W1",
        "doi:10.x/y",
        "--dir",
        "backward",
        "--depth",
        "2",
        "--max-nodes",
        "10",
    ]);
    match cli.namespace {
        Namespace::Catalog(c) => match c.cmd {
            CatalogCmd::Neighborhood { seeds, dir, depth, max_nodes } => {
                assert_eq!(seeds, vec!["openalex:W1", "doi:10.x/y"]);
                assert_eq!(dir, "backward");
                assert_eq!(depth, 2);
                assert_eq!(max_nodes, 10);
            }
            _ => panic!("expected Neighborhood command"),
        },
        _ => panic!("expected Catalog namespace"),
    }
}

// ── round-trip test against in-process server ────────────────────────────────

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
            let app = make_app(store, auth, None, None);
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            tx.send(format!("http://{addr}")).unwrap();
            axum::serve(listener, app).await.ok();
        });
    });

    let base = rx.recv().expect("server did not send address");
    std::thread::sleep(std::time::Duration::from_millis(20));
    (base, handle)
}

#[test]
fn neighborhood_empty_store_returns_nodes_edges_truncated() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _handle) = start_server(dir.path());
    let client = StoreClient::new(&base);

    let result = client
        .neighborhood(&["doi:10.0/none".to_string()], "forward", 1, 200)
        .unwrap();

    // Response must have all three keys regardless of whether the seed exists.
    assert!(result.get("nodes").is_some(), "missing 'nodes' key");
    assert!(result.get("edges").is_some(), "missing 'edges' key");
    assert!(result.get("truncated").is_some(), "missing 'truncated' key");
}

#[test]
fn neighborhood_bad_dir_returns_server_error() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _handle) = start_server(dir.path());
    let client = StoreClient::new(&base);

    let err = client
        .neighborhood(&["doi:10.0/x".to_string()], "sideways", 1, 200)
        .unwrap_err();

    match err {
        braincrawl_cli::store_client::ClientError::Server { status, .. } => {
            assert_eq!(status, 400, "expected 400 for bad dir");
        }
        other => panic!("expected Server error, got: {other}"),
    }
}
