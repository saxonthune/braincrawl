//! End-to-end tests covering the doc02.01.02 guarantees against the in-memory stack.

use braincrawl_blob_mem::MemBlobStore;
use braincrawl_coord_local::{LocalCoordinator, SystemClock, UuidGen};
use braincrawl_core::{
    types::{Alias, ContentOutcome, EdgeDir, EdgeInput, NodeKind, Rights, WorkRecord},
    usecases::Store,
};
use braincrawl_resolver_mem::MemResolver;
use braincrawl_store_mem::MemStore;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn make_store() -> Store<MemStore, MemBlobStore, MemStore, MemResolver, LocalCoordinator, SystemClock, UuidGen>
{
    Store {
        meta: MemStore::new(),
        blob: MemBlobStore::new(),
        payloads: MemStore::new(),
        resolver: MemResolver::new(),
        coord: LocalCoordinator,
        clock: SystemClock,
        id_gen: UuidGen,
    }
}

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

// ---------------------------------------------------------------------------
// 1. Idempotent put_work — same record twice → one node
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_idempotent_put_work() {
    let s = make_store();
    let rec = work("openalex", vec![alias("doi", "10.1/x")]);

    let id1 = s.put_work(rec.clone()).await.unwrap();
    let id2 = s.put_work(rec).await.unwrap();

    assert_eq!(id1.0, id2.0, "same record twice must resolve to same GUID");

    let view = s.get_work(alias("doi", "10.1/x")).await.unwrap().unwrap();
    assert_eq!(view.canonical_id.0, id1.0);
}

// ---------------------------------------------------------------------------
// 2. Bundle convergence — two records sharing one id each add a new alias
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_bundle_convergence() {
    let s = make_store();

    // A: doi + arxiv
    let a = WorkRecord {
        source: "openalex".to_string(),
        kind: NodeKind::Work,
        aliases: vec![alias("doi", "10.1/x"), alias("arxiv", "2301.00001")],
        attrs: serde_json::json!({}),
    };
    // B: doi + pmid  (overlaps on doi)
    let b = WorkRecord {
        source: "pubmed".to_string(),
        kind: NodeKind::Work,
        aliases: vec![alias("doi", "10.1/x"), alias("pmid", "99999")],
        attrs: serde_json::json!({}),
    };

    let id_a = s.put_work(a).await.unwrap();
    let id_b = s.put_work(b).await.unwrap();
    assert_eq!(id_a.0, id_b.0, "overlapping bundle must converge to one node");

    // All three aliases must resolve to the same node.
    for a in [alias("doi", "10.1/x"), alias("arxiv", "2301.00001"), alias("pmid", "99999")] {
        let v = s.get_work(a.clone()).await.unwrap().unwrap();
        assert_eq!(v.canonical_id.0, id_a.0, "alias {:?} should resolve to the same node", a);
    }
}

// ---------------------------------------------------------------------------
// 3. Merge confluence — bridging record merges two previously-separate nodes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_merge_confluence() {
    let s = make_store();

    // Put two separate records with no overlap.
    let id_x = s
        .put_work(work("src_a", vec![alias("doi", "10.1/x")]))
        .await
        .unwrap();
    let id_y = s
        .put_work(work("src_b", vec![alias("pmid", "12345")]))
        .await
        .unwrap();
    assert_ne!(id_x.0, id_y.0, "initially two separate nodes");

    // Bridging record carries both ids.
    let bridging = WorkRecord {
        source: "crossref".to_string(),
        kind: NodeKind::Work,
        aliases: vec![alias("doi", "10.1/x"), alias("pmid", "12345")],
        attrs: serde_json::json!({}),
    };
    let survivor = s.put_work(bridging).await.unwrap();

    // Survivor is lex-smallest of the two GUIDs.
    let expected = if id_x.0 < id_y.0 { &id_x.0 } else { &id_y.0 };
    assert_eq!(&survivor.0, expected, "survivor must be lex-smallest GUID");

    // Both original aliases must resolve to the survivor.
    let vx = s.get_work(alias("doi", "10.1/x")).await.unwrap().unwrap();
    let vy = s.get_work(alias("pmid", "12345")).await.unwrap().unwrap();
    assert_eq!(vx.canonical_id.0, survivor.0);
    assert_eq!(vy.canonical_id.0, survivor.0);
}

// ---------------------------------------------------------------------------
// 4. Stub creation — put_edges with unknown dst creates a stub node
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_stub_creation() {
    let s = make_store();

    // src is known (put_work first), dst is unknown.
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

    // The unknown dst alias now exists in the store (as a stub).
    let present = s
        .have(vec![alias("doi", "10.1/dst-unknown")])
        .await
        .unwrap();
    assert_eq!(present.len(), 1, "stub node alias must be present after put_edges");

    // Stub has no attrs (get_work returns a view with empty attrs).
    let view = s
        .get_work(alias("doi", "10.1/dst-unknown"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view.attrs, serde_json::Value::Object(serde_json::Map::new()));
}

// ---------------------------------------------------------------------------
// 5. Edge dedup + multi-source assertion retention
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_edge_dedup_multi_source() {
    let s = make_store();

    s.put_work(work("s", vec![alias("doi", "10.1/a")]))
        .await
        .unwrap();
    s.put_work(work("s", vec![alias("doi", "10.1/b")]))
        .await
        .unwrap();

    // Same (src, dst, relation) from two different sources.
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
    assert_eq!(
        views[0].assertions.len(),
        2,
        "both source assertions retained"
    );
}

// ---------------------------------------------------------------------------
// 6. have — returns only present aliases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_have() {
    let s = make_store();

    s.put_work(work("s", vec![alias("doi", "10.1/known")]))
        .await
        .unwrap();

    let result = s
        .have(vec![alias("doi", "10.1/known"), alias("doi", "10.1/unknown")])
        .await
        .unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].value, "10.1/known");
}

// ---------------------------------------------------------------------------
// 7. put_content / get_content rights gating
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_content_rights_gating() {
    let s = make_store();
    s.put_work(work("s", vec![alias("doi", "10.1/w")])).await.unwrap();

    // Open content — bytes must be stored and returned.
    s.put_content(
        alias("doi", "10.1/w"),
        braincrawl_core::types::PayloadKind::Abstract,
        b"hello world".to_vec(),
        Rights::Open,
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
            braincrawl_core::types::PayloadKind::Abstract,
        )
        .await
        .unwrap();
    assert!(
        matches!(outcome, ContentOutcome::Bytes { .. }),
        "open content must return Bytes"
    );

    // Restricted content — put_content must error.
    s.put_work(work("s", vec![alias("doi", "10.1/restricted")]))
        .await
        .unwrap();
    let err = s
        .put_content(
            alias("doi", "10.1/restricted"),
            braincrawl_core::types::PayloadKind::Fulltext,
            b"secret".to_vec(),
            Rights::Restricted,
            "text/plain".to_string(),
            None,
            None,
            "2024-01-01T00:00:00Z".to_string(),
        )
        .await;
    assert!(
        matches!(err, Err(braincrawl_core::types::DomainError::RightsViolation(_))),
        "restricted content must return RightsViolation"
    );

    // get_content on a never-stored alias → Absent.
    let absent = s
        .get_content(
            alias("doi", "10.1/nothing"),
            braincrawl_core::types::PayloadKind::Abstract,
        )
        .await
        .unwrap();
    assert!(matches!(absent, ContentOutcome::Absent));
}
