//! In-process HTTP smoke test: boots the axum server, exercises key routes.

use std::sync::Arc;

use braincrawl_auth::SharedSecret;
use braincrawl_server_lib::{make_app, make_store, AuthConfig};
use tokio::net::TcpListener;

async fn start_server(dir: &std::path::Path) -> (String, tokio::task::JoinHandle<()>) {
    let db_path = dir.join("test.db").to_string_lossy().to_string();
    let blob_root = dir.join("blobs").to_string_lossy().to_string();
    std::fs::create_dir_all(dir.join("blobs")).unwrap();

    let store = Arc::new(make_store(&db_path, &blob_root).expect("store"));
    let auth = Arc::new(AuthConfig {
        disabled: true,
        allowlist: SharedSecret::new("", ""),
    });
    let app = make_app(store, auth, None, None);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.ok();
    });
    // Brief yield so the server task is scheduled before we fire requests.
    tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
    (base, handle)
}

#[tokio::test]
async fn test_put_and_get_work() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    // PUT /works
    let body = serde_json::json!({
        "source": "openalex",
        "kind": "Work",
        "aliases": [{"scheme": "doi", "value": "10.99/smoke"}],
        "attrs": {"title": "Smoke Test Paper"}
    });
    let res = client
        .put(format!("{base}/works"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "PUT /works should return 200");
    let json: serde_json::Value = res.json().await.unwrap();
    assert!(
        json["id"].as_str().unwrap_or("").starts_with("guid:"),
        "id should be a guid"
    );

    // GET /works/{id}
    let res = client
        .get(format!("{base}/works/doi:10.99/smoke"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET /works/doi:10.99/smoke should return 200");

    // GET /works/unknown → 404
    let res = client
        .get(format!("{base}/works/doi:10.99/does-not-exist"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);

    handle.abort();
}

#[tokio::test]
async fn test_have() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    // POST /works/have — none present yet
    let res = client
        .post(format!("{base}/works/have"))
        .json(&serde_json::json!({"ids": ["doi:10.0/none"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let present: Vec<String> = res.json().await.unwrap();
    assert!(present.is_empty(), "no aliases present yet");

    // Put a work, then check again
    client
        .put(format!("{base}/works"))
        .json(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.0/known"}],
            "attrs": {}
        }))
        .send()
        .await
        .unwrap();

    let res = client
        .post(format!("{base}/works/have"))
        .json(&serde_json::json!({"ids": ["doi:10.0/known", "doi:10.0/none"]}))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let present: Vec<String> = res.json().await.unwrap();
    assert_eq!(present, vec!["doi:10.0/known"]);

    handle.abort();
}

#[tokio::test]
async fn test_content_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    // Create a work
    client
        .put(format!("{base}/works"))
        .json(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.1"}],
            "attrs": {}
        }))
        .send()
        .await
        .unwrap();

    // PUT content
    let res = client
        .put(format!(
            "{base}/works/doi:10.1/content/abstract?mime=text/plain&fetched_at=2024-01-01T00:00:00Z"
        ))
        .body("hello braincrawl")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "PUT content should succeed");

    // GET content → 200 with bytes
    let res = client
        .get(format!("{base}/works/doi:10.1/content/abstract"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET content should return 200");
    let body = res.text().await.unwrap();
    assert_eq!(body, "hello braincrawl");

    handle.abort();
}

#[tokio::test]
async fn test_stats_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    // Empty store: everything zero.
    let res = client.get(format!("{base}/stats")).send().await.unwrap();
    assert_eq!(res.status(), 200, "GET /stats should return 200");
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["works"], 0);
    assert_eq!(body["edges_total"], 0);

    // One described work (carries an assertion from source "openalex").
    client
        .put(format!("{base}/works"))
        .json(&serde_json::json!({
            "source": "openalex",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.1/described"}],
            "attrs": {"title": "Described"}
        }))
        .send()
        .await
        .unwrap();

    // An edge to an unknown dst creates a stub work (no assertion).
    client
        .put(format!("{base}/edges"))
        .json(&serde_json::json!([{
            "src": {"scheme": "doi", "value": "10.1/described"},
            "dst": {"scheme": "doi", "value": "10.1/stub"},
            "relation": "cites",
            "source": "openalex",
            "attrs": null,
            "fetched_at": "2024-01-01T00:00:00Z"
        }]))
        .send()
        .await
        .unwrap();

    let res = client.get(format!("{base}/stats")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();

    assert_eq!(body["works"], 2, "described work + stub");
    assert_eq!(body["works_described"], 1);
    assert_eq!(body["works_stub"], 1);
    assert_eq!(body["nodes_total"], 2);
    assert_eq!(body["tombstones"], 0);
    assert_eq!(body["edges_total"], 1);

    // Grouped breakdowns are arrays of {key, count}.
    let kinds = body["nodes_by_kind"].as_array().unwrap();
    assert_eq!(kinds[0]["key"], "work");
    assert_eq!(kinds[0]["count"], 2);

    let rels = body["edges_by_relation"].as_array().unwrap();
    assert_eq!(rels[0]["key"], "cites");
    assert_eq!(rels[0]["count"], 1);

    let sources = body["assertions_by_source"].as_array().unwrap();
    assert_eq!(sources[0]["key"], "openalex");
    assert_eq!(sources[0]["count"], 1);

    handle.abort();
}

#[tokio::test]
async fn test_large_content_put() {
    // Regression test: PUT /works/*path must accept bodies larger than axum's
    // default 2 MB limit. Without DefaultBodyLimit::disable() on that route
    // this returns 413.
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    // Create a work.
    client
        .put(format!("{base}/works"))
        .json(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.2/large"}],
            "attrs": {}
        }))
        .send()
        .await
        .unwrap();

    // Build a ~5 MB payload (well above the 2 MB default cap).
    let large_body: Vec<u8> = (0u8..=255).cycle().take(5 * 1024 * 1024).collect();

    let res = client
        .put(format!(
            "{base}/works/doi:10.2/large/content/fulltext?mime=application/pdf&fetched_at=2024-01-01T00:00:00Z"
        ))
        .body(large_body.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "PUT large content must not 413");

    // Round-trip: GET back and verify byte identity.
    let res = client
        .get(format!("{base}/works/doi:10.2/large/content/fulltext"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "GET large content should return 200");
    let returned = res.bytes().await.unwrap();
    assert_eq!(returned.as_ref(), large_body.as_slice(), "bytes must round-trip");

    handle.abort();
}

#[tokio::test]
async fn test_neighborhood_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    // Create two works.
    client
        .put(format!("{base}/works"))
        .json(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.1/src"}],
            "attrs": {"title": "Source"}
        }))
        .send()
        .await
        .unwrap();

    client
        .put(format!("{base}/works"))
        .json(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.1/dst"}],
            "attrs": {"title": "Dest"}
        }))
        .send()
        .await
        .unwrap();

    // Create an edge src → dst.
    client
        .put(format!("{base}/edges"))
        .json(&serde_json::json!([{
            "src": {"scheme": "doi", "value": "10.1/src"},
            "dst": {"scheme": "doi", "value": "10.1/dst"},
            "relation": "cites",
            "source": "test",
            "attrs": null,
            "fetched_at": "2024-01-01T00:00:00Z"
        }]))
        .send()
        .await
        .unwrap();

    // POST /graph/neighborhood from seed src, depth 1.
    let res = client
        .post(format!("{base}/graph/neighborhood"))
        .json(&serde_json::json!({
            "seeds": ["doi:10.1/src"],
            "dir": "forward",
            "depth": 1,
            "max_nodes": 50
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "POST /graph/neighborhood should return 200");

    let body: serde_json::Value = res.json().await.unwrap();

    // Shape check.
    assert!(body.get("nodes").is_some(), "response must have nodes");
    assert!(body.get("edges").is_some(), "response must have edges");
    assert!(body.get("truncated").is_some(), "response must have truncated");

    // Both src and dst should appear in nodes.
    let nodes = body["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 2, "both src and dst nodes expected");

    let edges = body["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1, "one edge expected");

    assert!(!body["truncated"].as_bool().unwrap());

    // Bad dir → 400.
    let res = client
        .post(format!("{base}/graph/neighborhood"))
        .json(&serde_json::json!({
            "seeds": ["doi:10.1/src"],
            "dir": "sideways",
            "depth": 1,
            "max_nodes": 50
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400, "bad dir must return 400");

    handle.abort();
}

#[tokio::test]
async fn test_work_get_carries_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    client
        .put(format!("{base}/works"))
        .json(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.3/artifacted"}],
            "attrs": {}
        }))
        .send()
        .await
        .unwrap();

    client
        .put(format!(
            "{base}/works/doi:10.3/artifacted/content/fulltext?mime=text/plain&fetched_at=2024-01-01T00:00:00Z"
        ))
        .body("full text body")
        .send()
        .await
        .unwrap();

    let res = client
        .get(format!("{base}/works/doi:10.3/artifacted"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let work_body: serde_json::Value = res.json().await.unwrap();
    let artifacts = work_body["artifacts"].as_array().unwrap();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0]["role"], "fulltext");

    let res = client
        .get(format!("{base}/works/doi:10.3/artifacted/artifacts"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let artifacts_body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(artifacts_body["artifacts"], work_body["artifacts"]);

    handle.abort();
}

#[tokio::test]
async fn test_work_get_no_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    client
        .put(format!("{base}/works"))
        .json(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.3/bare"}],
            "attrs": {}
        }))
        .send()
        .await
        .unwrap();

    let res = client
        .get(format!("{base}/works/doi:10.3/bare"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    let artifacts = body["artifacts"].as_array().unwrap();
    assert!(artifacts.is_empty());

    handle.abort();
}

/// "Known work holding nothing" and "never heard of this alias" must not look the
/// same on the wire — a caller decides whether to ingest based on which it got.
#[tokio::test]
async fn test_list_artifacts_distinguishes_empty_from_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    client
        .put(format!("{base}/works"))
        .json(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.3/known-but-bare"}],
            "attrs": {}
        }))
        .send()
        .await
        .unwrap();

    let res = client
        .get(format!("{base}/works/doi:10.3/known-but-bare/artifacts"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "a known work holding nothing is not an error");
    let body: serde_json::Value = res.json().await.unwrap();
    assert!(body["artifacts"].as_array().unwrap().is_empty());

    let res = client
        .get(format!("{base}/works/doi:10.3/never-ingested/artifacts"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404, "an alias the Catalog does not hold is 404");

    handle.abort();
}

/// Roles are open-ended slugs (`fulltext-ch01` and friends), so only a string that
/// would break a blob-key path is invalid. The route must answer that with 400
/// rather than fall through to the catch-all 404 — a 404 here reads as "no such
/// work" and sends callers off to re-ingest something they already have.
#[tokio::test]
async fn test_list_artifacts_rejects_unparseable_role() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    client
        .put(format!("{base}/works"))
        .json(&serde_json::json!({
            "source": "test",
            "kind": "Work",
            "aliases": [{"scheme": "doi", "value": "10.3/roled"}],
            "attrs": {}
        }))
        .send()
        .await
        .unwrap();

    let res = client
        .get(format!("{base}/works/doi:10.3/roled/artifacts?role=fulltext-ch01"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "an unused but well-formed role slug is not an error");

    let res = client
        .get(format!("{base}/works/doi:10.3/roled/artifacts?role=NOT.A.ROLE"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);

    handle.abort();
}

#[tokio::test]
async fn test_work_get_404_names_alias() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    let res = client
        .get(format!("{base}/works/doi:10.99/does-not-exist"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
    let body = res.text().await.unwrap();
    assert!(
        body.contains("doi:10.99/does-not-exist"),
        "404 body should name the alias: {body}"
    );

    handle.abort();
}

#[tokio::test]
async fn test_health_reports_version() {
    let dir = tempfile::tempdir().unwrap();
    let (base, handle) = start_server(dir.path()).await;

    let client = reqwest::Client::new();

    let res = client.get(format!("{base}/health")).send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["status"], "ok");
    assert!(
        body["version"].as_str().is_some_and(|v| !v.is_empty()),
        "version should be a non-empty string"
    );

    handle.abort();
}
