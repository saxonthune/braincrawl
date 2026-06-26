//! Phase-2 guarantee suite run against the SQLite + filesystem stack.

use braincrawl_blob_fs::FsBlobStore;
use braincrawl_coord_local::{LocalCoordinator, SystemClock, UuidGen};
use braincrawl_core::{
    types::{Alias, ContentOutcome, EdgeDir, EdgeInput, NodeKind, WorkRecord},
    usecases::Store,
};
use braincrawl_resolver_mem::MemResolver;
use braincrawl_store_sqlite::SqliteStore;

fn alias(ns: &str, val: &str) -> Alias {
    Alias {
        namespace: ns.to_string(),
        value: val.to_string(),
    }
}

fn work(source: &str, aliases: Vec<Alias>) -> WorkRecord {
    WorkRecord {
        source: source.to_string(),
        kind: NodeKind::Work,
        aliases,
        attrs: serde_json::json!({ "title": format!("record from {}", source) }),
    }
}

type TestStore = Store<
    SqliteStore,
    FsBlobStore,
    SqliteStore,
    MemResolver,
    LocalCoordinator,
    SystemClock,
    UuidGen,
>;

fn make_store(dir: &std::path::Path) -> TestStore {
    let db_path = dir.join("test.db").to_string_lossy().to_string();
    let blob_root = dir.join("blobs");
    std::fs::create_dir_all(&blob_root).unwrap();
    Store {
        meta: SqliteStore::open(&db_path).expect("open meta"),
        blob: FsBlobStore::new(blob_root),
        artifacts: SqliteStore::open(&db_path).expect("open artifacts"),
        resolver: MemResolver::new(),
        coord: LocalCoordinator,
        clock: SystemClock,
        id_gen: UuidGen,
    }
}

// ── 1. Idempotent put_work ────────────────────────────────────────────────────

#[tokio::test]
async fn test_idempotent_put_work() {
    let dir = tempfile::tempdir().unwrap();
    let s = make_store(dir.path());
    let rec = work("openalex", vec![alias("doi", "10.1/x")]);

    let id1 = s.put_work(rec.clone()).await.unwrap();
    let id2 = s.put_work(rec).await.unwrap();
    assert_eq!(id1.0, id2.0, "same record twice must resolve to same GUID");

    let view = s.get_work(alias("doi", "10.1/x")).await.unwrap().unwrap();
    assert_eq!(view.canonical_id.0, id1.0);
}

// ── 2. Bundle convergence ─────────────────────────────────────────────────────

#[tokio::test]
async fn test_bundle_convergence() {
    let dir = tempfile::tempdir().unwrap();
    let s = make_store(dir.path());

    let a = WorkRecord {
        source: "openalex".to_string(),
        kind: NodeKind::Work,
        aliases: vec![alias("doi", "10.1/x"), alias("arxiv", "2301.00001")],
        attrs: serde_json::json!({}),
    };
    let b = WorkRecord {
        source: "pubmed".to_string(),
        kind: NodeKind::Work,
        aliases: vec![alias("doi", "10.1/x"), alias("pmid", "99999")],
        attrs: serde_json::json!({}),
    };

    let id_a = s.put_work(a).await.unwrap();
    let id_b = s.put_work(b).await.unwrap();
    assert_eq!(id_a.0, id_b.0, "overlapping bundle must converge to one node");

    for a in [
        alias("doi", "10.1/x"),
        alias("arxiv", "2301.00001"),
        alias("pmid", "99999"),
    ] {
        let v = s.get_work(a.clone()).await.unwrap().unwrap();
        assert_eq!(
            v.canonical_id.0, id_a.0,
            "alias {:?} should resolve to the same node",
            a
        );
    }
}

// ── 3. Merge confluence ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_merge_confluence() {
    let dir = tempfile::tempdir().unwrap();
    let s = make_store(dir.path());

    let id_x = s
        .put_work(work("src_a", vec![alias("doi", "10.1/x")]))
        .await
        .unwrap();
    let id_y = s
        .put_work(work("src_b", vec![alias("pmid", "12345")]))
        .await
        .unwrap();
    assert_ne!(id_x.0, id_y.0, "initially two separate nodes");

    let bridging = WorkRecord {
        source: "crossref".to_string(),
        kind: NodeKind::Work,
        aliases: vec![alias("doi", "10.1/x"), alias("pmid", "12345")],
        attrs: serde_json::json!({}),
    };
    let survivor = s.put_work(bridging).await.unwrap();

    let expected = if id_x.0 < id_y.0 { &id_x.0 } else { &id_y.0 };
    assert_eq!(&survivor.0, expected, "survivor must be lex-smallest GUID");

    let vx = s.get_work(alias("doi", "10.1/x")).await.unwrap().unwrap();
    let vy = s
        .get_work(alias("pmid", "12345"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(vx.canonical_id.0, survivor.0);
    assert_eq!(vy.canonical_id.0, survivor.0);
}

// ── 4. Stub creation ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_stub_creation() {
    let dir = tempfile::tempdir().unwrap();
    let s = make_store(dir.path());

    s.put_work(work("openalex", vec![alias("doi", "10.1/src")]))
        .await
        .unwrap();

    let edges = vec![EdgeInput {
        src: alias("doi", "10.1/src"),
        dst: alias("doi", "10.1/dst-unknown"),
        relation: "cites".to_string(),
        source: "openalex".to_string(),
        attrs: None,
        fetched_at: "2024-01-01T00:00:00Z".to_string(),
    }];
    let count = s.put_edges(edges).await.unwrap();
    assert_eq!(count, 1);

    let present = s
        .have(vec![alias("doi", "10.1/dst-unknown")])
        .await
        .unwrap();
    assert_eq!(present.len(), 1, "stub node alias must be present after put_edges");

    let view = s
        .get_work(alias("doi", "10.1/dst-unknown"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view.attrs, serde_json::Value::Object(serde_json::Map::new()));
}

// ── 5. Edge dedup + multi-source assertion retention ─────────────────────────

#[tokio::test]
async fn test_edge_dedup_multi_source() {
    let dir = tempfile::tempdir().unwrap();
    let s = make_store(dir.path());

    s.put_work(work("s", vec![alias("doi", "10.1/a")])).await.unwrap();
    s.put_work(work("s", vec![alias("doi", "10.1/b")])).await.unwrap();

    let edges = vec![
        EdgeInput {
            src: alias("doi", "10.1/a"),
            dst: alias("doi", "10.1/b"),
            relation: "cites".to_string(),
            source: "openalex".to_string(),
            attrs: Some(serde_json::json!({"score": 1})),
            fetched_at: "2024-01-01T00:00:00Z".to_string(),
        },
        EdgeInput {
            src: alias("doi", "10.1/a"),
            dst: alias("doi", "10.1/b"),
            relation: "cites".to_string(),
            source: "semantic_scholar".to_string(),
            attrs: Some(serde_json::json!({"score": 2})),
            fetched_at: "2024-01-02T00:00:00Z".to_string(),
        },
    ];
    let count = s.put_edges(edges).await.unwrap();
    assert_eq!(count, 2);

    let (views, _next) = s
        .get_edges(alias("doi", "10.1/a"), EdgeDir::Forward, None, 10)
        .await
        .unwrap();
    assert_eq!(views.len(), 1, "deduped to one edge");
    assert_eq!(views[0].assertions.len(), 2, "both source assertions retained");
}

// ── 6. have — returns only present aliases ────────────────────────────────────

#[tokio::test]
async fn test_have() {
    let dir = tempfile::tempdir().unwrap();
    let s = make_store(dir.path());

    s.put_work(work("s", vec![alias("doi", "10.1/known")])).await.unwrap();

    let result = s
        .have(vec![alias("doi", "10.1/known"), alias("doi", "10.1/unknown")])
        .await
        .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].value, "10.1/known");
}

// ── 7. put_content / get_content roundtrip ───────────────────────────────────

#[tokio::test]
async fn test_content_roundtrip_custom_role() {
    let dir = tempfile::tempdir().unwrap();
    let s = make_store(dir.path());
    s.put_work(work("s", vec![alias("doi", "10.1/w")])).await.unwrap();

    let role = braincrawl_core::types::ArtifactRole::Other("map".to_string());
    s.put_content(
        alias("doi", "10.1/w"),
        role.clone(),
        b"map bytes".to_vec(),
        "image/png".to_string(),
        None,
        None,
        "2024-01-01T00:00:00Z".to_string(),
    )
    .await
    .unwrap();

    let outcome = s
        .get_content(alias("doi", "10.1/w"), role)
        .await
        .unwrap();
    match outcome {
        ContentOutcome::Bytes { bytes, mime, .. } => {
            assert_eq!(bytes, b"map bytes");
            assert_eq!(mime, "image/png");
        }
        other => panic!("expected Bytes, got {:?}", std::mem::discriminant(&other)),
    }

    assert!(braincrawl_core::types::ArtifactRole::parse("map").is_some());
    assert!(braincrawl_core::types::ArtifactRole::parse("Map").is_none());
    assert!(braincrawl_core::types::ArtifactRole::parse("a/b").is_none());
    assert!(braincrawl_core::types::ArtifactRole::parse("").is_none());
}

#[tokio::test]
async fn test_content_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let s = make_store(dir.path());
    s.put_work(work("s", vec![alias("doi", "10.1/w")])).await.unwrap();

    s.put_content(
        alias("doi", "10.1/w"),
        braincrawl_core::types::ArtifactRole::Abstract,
        b"hello world".to_vec(),
        "text/plain".to_string(),
        Some("s".to_string()),
        None,
        "2024-01-01T00:00:00Z".to_string(),
    )
    .await
    .unwrap();

    let outcome = s
        .get_content(
            alias("doi", "10.1/w"),
            braincrawl_core::types::ArtifactRole::Abstract,
        )
        .await
        .unwrap();
    assert!(
        matches!(outcome, ContentOutcome::Bytes { .. }),
        "stored content must return Bytes"
    );

    // get_content on a never-stored alias → Absent.
    let absent = s
        .get_content(
            alias("doi", "10.1/nothing"),
            braincrawl_core::types::ArtifactRole::Abstract,
        )
        .await
        .unwrap();
    assert!(matches!(absent, ContentOutcome::Absent));
}
