//! Integration tests for the semanticscholar push phase.
//! Boots the real server in-process, exercises mapping + StoreClient push round-trips.

use std::sync::Arc;

use braincrawl_auth::SharedSecret;
use braincrawl_cli::semanticscholar::entity::Entity;
use braincrawl_cli::semanticscholar::mapping::{extract_aliases, node_kind, to_edges, to_work_record};
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

// ── unit tests: node_kind ─────────────────────────────────────────────────────

#[test]
fn node_kind_papers_is_work() {
    assert_eq!(node_kind(Entity::Papers), Some("Work"));
}

#[test]
fn node_kind_authors_is_author() {
    assert_eq!(node_kind(Entity::Authors), Some("Author"));
}

// ── unit tests: DOI merge guarantee ──────────────────────────────────────────

#[test]
fn doi_alias_is_bare_for_openalex_merge() {
    let record = serde_json::json!({
        "paperId": "649def34f8be52c8b66281af98ae884c09aef38b",
        "externalIds": {
            "DOI": "10.7717/peerj.4375",
            "ArXiv": "2301.07041",
            "CorpusId": 4375000u64
        }
    });
    let aliases = extract_aliases(Entity::Papers, &record);
    let map: std::collections::HashMap<&str, &str> = aliases
        .iter()
        .map(|a| (a["scheme"].as_str().unwrap(), a["value"].as_str().unwrap()))
        .collect();
    // The merge guarantee: this exact value must match what OpenAlex registers
    assert_eq!(
        map.get("doi"),
        Some(&"10.7717/peerj.4375"),
        "doi alias must be bare (not https://doi.org/-prefixed) to merge with OpenAlex"
    );
    assert_eq!(map.get("s2"), Some(&"649def34f8be52c8b66281af98ae884c09aef38b"));
    assert_eq!(map.get("arxiv"), Some(&"2301.07041"));
    assert_eq!(map.get("corpusid"), Some(&"4375000"));
}

#[test]
fn doi_url_prefix_stripped() {
    let record = serde_json::json!({
        "paperId": "abc",
        "externalIds": {"DOI": "https://doi.org/10.1000/test"}
    });
    let aliases = extract_aliases(Entity::Papers, &record);
    let doi_alias = aliases
        .iter()
        .find(|a| a["scheme"].as_str() == Some("doi"))
        .expect("doi alias should be present");
    assert_eq!(
        doi_alias["value"].as_str(),
        Some("10.1000/test"),
        "https://doi.org/ prefix must be stripped"
    );
}

// ── unit tests: alias extraction ──────────────────────────────────────────────

#[test]
fn aliases_paper_all_schemes() {
    let record = serde_json::json!({
        "paperId": "649def34f8be52c8b66281af98ae884c09aef38b",
        "externalIds": {
            "DOI": "10.1000/test",
            "ArXiv": "2301.07041",
            "MAG": "2741809807",
            "PubMed": "29456894",
            "PubMedCentral": "PMC5045003",
            "CorpusId": 999u64
        }
    });
    let aliases = extract_aliases(Entity::Papers, &record);
    let ns_set: std::collections::HashSet<&str> = aliases
        .iter()
        .map(|a| a["scheme"].as_str().unwrap())
        .collect();
    assert!(ns_set.contains("s2"));
    assert!(ns_set.contains("doi"));
    assert!(ns_set.contains("arxiv"));
    assert!(ns_set.contains("mag"));
    assert!(ns_set.contains("pmid"));
    assert!(ns_set.contains("pmcid"));
    assert!(ns_set.contains("corpusid"));
}

#[test]
fn aliases_author_s2author_and_orcid() {
    let record = serde_json::json!({
        "authorId": "1741101",
        "externalIds": {"ORCID": "0000-0001-6187-6610"}
    });
    let aliases = extract_aliases(Entity::Authors, &record);
    let map: std::collections::HashMap<&str, &str> = aliases
        .iter()
        .map(|a| (a["scheme"].as_str().unwrap(), a["value"].as_str().unwrap()))
        .collect();
    assert_eq!(map.get("s2author"), Some(&"1741101"));
    assert_eq!(map.get("orcid"), Some(&"0000-0001-6187-6610"));
}

// ── unit tests: to_edges ──────────────────────────────────────────────────────

#[test]
fn to_edges_cites_relation_and_source() {
    let pairs = vec![
        ("doi:10.1000/citing".to_string(), "doi:10.7717/peerj.4375".to_string()),
        ("s2:abc".to_string(), "s2:def".to_string()),
    ];
    let edges = to_edges(&pairs);
    assert_eq!(edges.len(), 2);

    let e0 = &edges[0];
    assert_eq!(e0["src"]["scheme"].as_str(), Some("doi"));
    assert_eq!(e0["src"]["value"].as_str(), Some("10.1000/citing"));
    assert_eq!(e0["dst"]["scheme"].as_str(), Some("doi"));
    assert_eq!(e0["dst"]["value"].as_str(), Some("10.7717/peerj.4375"));
    assert_eq!(e0["relation"].as_str(), Some("cites"));
    assert_eq!(e0["source"].as_str(), Some("semanticscholar"));
    assert!(e0["fetched_at"].as_str().is_some());

    let e1 = &edges[1];
    assert_eq!(e1["src"]["scheme"].as_str(), Some("s2"));
    assert_eq!(e1["src"]["value"].as_str(), Some("abc"));
}

// ── integration: push paper round-trip with auth ──────────────────────────────

#[test]
fn push_paper_round_trip_doi_merge() {
    let dir = tempfile::tempdir().unwrap();
    let token = "s2-test-token-abc";
    let (base, _handle) = start_server_auth(dir.path(), token);

    let client = StoreClient::new(&base).with_token(Some(token.to_string()));

    let record = serde_json::json!({
        "paperId": "649def34f8be52c8b66281af98ae884c09aef38b",
        "externalIds": {
            "DOI": "10.7717/peerj.4375",
            "CorpusId": 4375000u64
        },
        "title": "A Semantic Scholar test work"
    });

    let work_record = to_work_record(Entity::Papers, &record).unwrap();
    assert_eq!(work_record["source"].as_str(), Some("semanticscholar"));
    assert_eq!(work_record["kind"].as_str(), Some("Work"));

    client.put_work(&work_record).unwrap();

    // Retrieve by s2 alias
    let result = client.get_work("s2:649def34f8be52c8b66281af98ae884c09aef38b").unwrap();
    assert!(result.is_some(), "should be retrievable by s2 alias");

    // Retrieve by doi alias — confirms the merge alias is registered
    let result = client.get_work("doi:10.7717/peerj.4375").unwrap();
    assert!(result.is_some(), "should be retrievable by doi alias (merge guarantee)");
    let node = result.unwrap();
    assert_eq!(node["attrs"]["title"].as_str(), Some("A Semantic Scholar test work"));
}

#[test]
fn push_author_round_trip_orcid() {
    let dir = tempfile::tempdir().unwrap();
    let token = "s2-author-token";
    let (base, _handle) = start_server_auth(dir.path(), token);

    let client = StoreClient::new(&base).with_token(Some(token.to_string()));

    let record = serde_json::json!({
        "authorId": "1741101",
        "externalIds": {"ORCID": "0000-0001-6187-6610"},
        "name": "Test Author"
    });

    let work_record = to_work_record(Entity::Authors, &record).unwrap();
    assert_eq!(work_record["source"].as_str(), Some("semanticscholar"));
    assert_eq!(work_record["kind"].as_str(), Some("Author"));

    client.put_work(&work_record).unwrap();

    let result = client.get_work("s2author:1741101").unwrap();
    assert!(result.is_some(), "should be retrievable by s2author alias");

    let result = client.get_work("orcid:0000-0001-6187-6610").unwrap();
    assert!(result.is_some(), "should be retrievable by orcid alias");
}

#[test]
fn push_edges_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let token = "s2-edge-token";
    let (base, _handle) = start_server_auth(dir.path(), token);

    let client = StoreClient::new(&base).with_token(Some(token.to_string()));

    // Store two works so edges can reference them
    for (s2id, doi) in [
        ("649def34f8be52c8b66281af98ae884c09aef38b", "10.0/seed"),
        ("abcdef1234567890abcdef1234567890abcdef12", "10.0/citing"),
    ] {
        let wr = to_work_record(Entity::Papers, &serde_json::json!({
            "paperId": s2id,
            "externalIds": {"DOI": doi}
        })).unwrap();
        client.put_work(&wr).unwrap();
    }

    let pairs = vec![
        (
            "doi:10.0/citing".to_string(),
            "doi:10.0/seed".to_string(),
        ),
    ];
    let edges = to_edges(&pairs);
    let count = client.put_edges(&edges).unwrap();
    assert_eq!(count, 1, "expected 1 edge stored");
}
