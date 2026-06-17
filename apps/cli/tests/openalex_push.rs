//! Integration tests for the openalex push phase.
//! Boots the real server in-process, exercises mapping + StoreClient push round-trips.

use std::sync::Arc;

use braincrawl_auth::SharedSecret;
use braincrawl_cli::openalex::entity::Entity;
use braincrawl_cli::openalex::mapping::{extract_aliases, node_kind, to_edges, to_work_record};
use braincrawl_cli::store_client::StoreClient;
use braincrawl_server::{AuthConfig, make_app, make_store};

// ── server helpers ────────────────────────────────────────────────────────────

fn start_server_auth(dir: &std::path::Path, token: &str) -> (String, std::thread::JoinHandle<()>) {
    let db_path = dir.join("test.db").to_string_lossy().to_string();
    let blob_root = dir.join("blobs").to_string_lossy().to_string();
    std::fs::create_dir_all(dir.join("blobs")).unwrap();

    let token = token.to_string();
    let (tx, rx) = std::sync::mpsc::channel::<String>();

    let handle = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let store = Arc::new(make_store(&db_path, &blob_root).expect("make_store"));
            let auth = Arc::new(AuthConfig {
                disabled: false,
                allowlist: SharedSecret::new(&token, "default"),
            });
            let app = make_app(store, auth);
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

// ── unit tests: node_kind ─────────────────────────────────────────────────────

#[test]
fn node_kind_all_variants() {
    assert_eq!(node_kind(Entity::Works), Some("Work"));
    assert_eq!(node_kind(Entity::Authors), Some("Author"));
    assert_eq!(node_kind(Entity::Sources), Some("Venue"));
    assert_eq!(node_kind(Entity::Topics), Some("Topic"));
    assert_eq!(node_kind(Entity::Concepts), Some("Concept"));
    assert_eq!(node_kind(Entity::Institutions), None);
    assert_eq!(node_kind(Entity::Publishers), None);
    assert_eq!(node_kind(Entity::Funders), None);
    assert_eq!(node_kind(Entity::Keywords), None);
}

// ── unit tests: alias URL normalization ───────────────────────────────────────

#[test]
fn aliases_work_strips_urls() {
    let record = serde_json::json!({
        "id": "https://openalex.org/W2741809807",
        "ids": {
            "doi": "https://doi.org/10.7717/peerj.4375",
            "pmid": "29456894",
            "mag": 2741809807u64
        }
    });
    let aliases = extract_aliases(Entity::Works, &record);
    let map: std::collections::HashMap<&str, &str> = aliases
        .iter()
        .map(|a| (a["namespace"].as_str().unwrap(), a["value"].as_str().unwrap()))
        .collect();
    assert_eq!(map.get("openalex"), Some(&"W2741809807"));
    assert_eq!(map.get("doi"), Some(&"10.7717/peerj.4375"));
    assert_eq!(map.get("pmid"), Some(&"29456894"));
    assert_eq!(map.get("mag"), Some(&"2741809807"));
}

#[test]
fn aliases_author_strips_orcid_url() {
    let record = serde_json::json!({
        "id": "https://openalex.org/A5023888391",
        "orcid": "https://orcid.org/0000-0001-6187-6610"
    });
    let aliases = extract_aliases(Entity::Authors, &record);
    let map: std::collections::HashMap<&str, &str> = aliases
        .iter()
        .map(|a| (a["namespace"].as_str().unwrap(), a["value"].as_str().unwrap()))
        .collect();
    assert_eq!(map.get("openalex"), Some(&"A5023888391"));
    assert_eq!(map.get("orcid"), Some(&"0000-0001-6187-6610"));
}

#[test]
fn aliases_source_includes_all_issns() {
    let record = serde_json::json!({
        "id": "https://openalex.org/S1983995261",
        "issn_l": "2041-1723",
        "issn": ["2041-1723", "1234-5678"]
    });
    let aliases = extract_aliases(Entity::Sources, &record);
    let issn_vals: Vec<&str> = aliases
        .iter()
        .filter(|a| a["namespace"].as_str() == Some("issn"))
        .map(|a| a["value"].as_str().unwrap())
        .collect();
    assert_eq!(issn_vals.len(), 2);
    assert!(issn_vals.contains(&"2041-1723"));
    assert!(issn_vals.contains(&"1234-5678"));
}

#[test]
fn institution_not_pushable() {
    let record = serde_json::json!({"id": "https://openalex.org/I27837315"});
    assert!(to_work_record(Entity::Institutions, &record).is_none());
}

// ── unit tests: to_edges ──────────────────────────────────────────────────────

#[test]
fn to_edges_shape() {
    let pairs = vec![
        ("https://openalex.org/W111".to_string(), "https://openalex.org/W222".to_string()),
        ("W333".to_string(), "W444".to_string()),
    ];
    let edges = to_edges(&pairs);
    assert_eq!(edges.len(), 2);

    let e0 = &edges[0];
    assert_eq!(e0["src"]["namespace"].as_str(), Some("openalex"));
    assert_eq!(e0["src"]["value"].as_str(), Some("W111"));
    assert_eq!(e0["dst"]["namespace"].as_str(), Some("openalex"));
    assert_eq!(e0["dst"]["value"].as_str(), Some("W222"));
    assert_eq!(e0["relation"].as_str(), Some("cites"));
    assert_eq!(e0["source"].as_str(), Some("openalex"));
    assert!(e0["fetched_at"].as_str().is_some());

    // already-bare IDs pass through unchanged
    let e1 = &edges[1];
    assert_eq!(e1["src"]["value"].as_str(), Some("W333"));
    assert_eq!(e1["dst"]["value"].as_str(), Some("W444"));
}

// ── integration: push with auth ───────────────────────────────────────────────

#[test]
fn push_work_round_trip_with_auth() {
    let dir = tempfile::tempdir().unwrap();
    let token = "test-token-abc";
    let (base, _handle) = start_server_auth(dir.path(), token);

    let client = StoreClient::new(&base).with_token(Some(token.to_string()));

    let record = serde_json::json!({
        "id": "https://openalex.org/W2741809807",
        "ids": {
            "doi": "https://doi.org/10.7717/peerj.4375"
        },
        "title": "A test work"
    });

    let work_record = to_work_record(Entity::Works, &record).unwrap();
    assert_eq!(work_record["kind"].as_str(), Some("Work"));
    assert_eq!(work_record["source"].as_str(), Some("openalex"));

    // push succeeds
    client.put_work(&work_record).unwrap();

    // retrieve by openalex alias
    let result = client.get_work("openalex:W2741809807").unwrap();
    assert!(result.is_some(), "stored work should be retrievable");
    let node = result.unwrap();
    let attrs = &node["attrs"];
    assert_eq!(attrs["title"].as_str(), Some("A test work"));
}

#[test]
fn push_rejected_without_token() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _handle) = start_server_auth(dir.path(), "correct-token");

    // client without token — should get 401/403
    let client = StoreClient::new(&base);
    let record = serde_json::json!({
        "source": "openalex",
        "kind": "Work",
        "aliases": [{"namespace": "openalex", "value": "W999"}],
        "attrs": {}
    });
    let err = client.put_work(&record).unwrap_err();
    let status = match &err {
        braincrawl_cli::store_client::ClientError::Server { status, .. } => *status,
        _ => panic!("expected Server error, got: {err}"),
    };
    assert!(
        status == 401 || status == 403,
        "expected 401 or 403, got {status}"
    );
}

#[test]
fn push_rejected_with_wrong_token() {
    let dir = tempfile::tempdir().unwrap();
    let (base, _handle) = start_server_auth(dir.path(), "correct-token");

    let client = StoreClient::new(&base).with_token(Some("wrong-token".to_string()));
    let record = serde_json::json!({
        "source": "openalex",
        "kind": "Work",
        "aliases": [{"namespace": "openalex", "value": "W998"}],
        "attrs": {}
    });
    let err = client.put_work(&record).unwrap_err();
    let status = match &err {
        braincrawl_cli::store_client::ClientError::Server { status, .. } => *status,
        _ => panic!("expected Server error, got: {err}"),
    };
    assert!(
        status == 401 || status == 403,
        "expected 401 or 403, got {status}"
    );
}

#[test]
fn push_edges_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let token = "edge-test-token";
    let (base, _handle) = start_server_auth(dir.path(), token);

    let client = StoreClient::new(&base).with_token(Some(token.to_string()));

    // First store the two works so edges can reference them
    for (id, doi) in [("W100", "10.0/citing"), ("W200", "10.0/cited")] {
        let wr = serde_json::json!({
            "source": "openalex",
            "kind": "Work",
            "aliases": [
                {"namespace": "openalex", "value": id},
                {"namespace": "doi", "value": doi}
            ],
            "attrs": {}
        });
        client.put_work(&wr).unwrap();
    }

    let pairs = vec![("W100".to_string(), "W200".to_string())];
    let edges = to_edges(&pairs);
    let count = client.put_edges(&edges).unwrap();
    assert_eq!(count, 1, "expected 1 edge stored");
}
