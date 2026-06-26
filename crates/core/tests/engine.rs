//! End-to-end tests covering the doc02.01.02 guarantees against the in-memory stack.

use braincrawl_blob_mem::MemBlobStore;
use braincrawl_coord_local::{LocalCoordinator, SystemClock, UuidGen};
use braincrawl_core::{
    types::{Alias, ContentOutcome, EdgeDir, EdgeInput, NodeKind, WorkRecord},
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
        artifacts: MemStore::new(),
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
// 8. neighborhood — BFS traversal
// ---------------------------------------------------------------------------

/// Build a graph: A→H, B→H, C→H, A→B
/// (H is a hub cited by A, B, C)
async fn build_hub_graph(
    s: &Store<
        braincrawl_store_mem::MemStore,
        braincrawl_blob_mem::MemBlobStore,
        braincrawl_store_mem::MemStore,
        braincrawl_resolver_mem::MemResolver,
        braincrawl_coord_local::LocalCoordinator,
        braincrawl_coord_local::SystemClock,
        braincrawl_coord_local::UuidGen,
    >,
) {
    for id in ["a", "b", "c", "h"] {
        s.put_work(work("s", vec![alias("test", id)])).await.unwrap();
    }
    let edges = vec![
        EdgeInput {
            src: alias("test", "a"),
            dst: alias("test", "h"),
            relation: "cites".to_string(),
            source: "s".to_string(),
            attrs: None,
            fetched_at: "2024-01-01T00:00:00Z".to_string(),
        },
        EdgeInput {
            src: alias("test", "b"),
            dst: alias("test", "h"),
            relation: "cites".to_string(),
            source: "s".to_string(),
            attrs: None,
            fetched_at: "2024-01-01T00:00:00Z".to_string(),
        },
        EdgeInput {
            src: alias("test", "c"),
            dst: alias("test", "h"),
            relation: "cites".to_string(),
            source: "s".to_string(),
            attrs: None,
            fetched_at: "2024-01-01T00:00:00Z".to_string(),
        },
        EdgeInput {
            src: alias("test", "a"),
            dst: alias("test", "b"),
            relation: "cites".to_string(),
            source: "s".to_string(),
            attrs: None,
            fetched_at: "2024-01-01T00:00:00Z".to_string(),
        },
    ];
    s.put_edges(edges).await.unwrap();
}

#[tokio::test]
async fn test_neighborhood_forward_bfs() {
    let s = make_store();
    build_hub_graph(&s).await;

    // From seed A, forward, depth 2: should reach A, B (A→B), H (A→H and B→H)
    let result = s
        .neighborhood(vec![alias("test", "a")], EdgeDir::Forward, 2, 50)
        .await
        .unwrap();

    let node_ids: Vec<&str> = result
        .nodes
        .iter()
        .map(|n| n.canonical_id.0.as_str())
        .collect();

    // Resolve actual canonical ids for a, b, h
    let id_a = s.get_work(alias("test", "a")).await.unwrap().unwrap().canonical_id.0;
    let id_b = s.get_work(alias("test", "b")).await.unwrap().unwrap().canonical_id.0;
    let id_h = s.get_work(alias("test", "h")).await.unwrap().unwrap().canonical_id.0;

    assert!(node_ids.contains(&id_a.as_str()), "A must be in subgraph");
    assert!(node_ids.contains(&id_b.as_str()), "B must be in subgraph");
    assert!(node_ids.contains(&id_h.as_str()), "H must be in subgraph");

    // H has in-degree 2 (A→H, B→H), so it should be first (highest in_degree).
    let h_node = result.nodes.iter().find(|n| n.canonical_id.0 == id_h).unwrap();
    assert_eq!(h_node.in_degree, 2, "H has in_degree 2 within subgraph");
    assert_eq!(result.nodes[0].canonical_id.0, id_h, "H should be first (highest in_degree)");

    // Edges should be the closed subset.
    let edge_pairs: Vec<(&str, &str)> = result
        .edges
        .iter()
        .map(|e| (e.src.0.as_str(), e.dst.0.as_str()))
        .collect();
    assert!(
        edge_pairs.contains(&(id_a.as_str(), id_h.as_str())),
        "A→H must be in edges"
    );
    assert!(
        edge_pairs.contains(&(id_a.as_str(), id_b.as_str())),
        "A→B must be in edges"
    );
    assert!(
        edge_pairs.contains(&(id_b.as_str(), id_h.as_str())),
        "B→H must be in edges"
    );

    assert!(!result.truncated);
}

#[tokio::test]
async fn test_neighborhood_max_nodes_truncation() {
    let s = make_store();
    build_hub_graph(&s).await;

    // max_nodes = 2 is smaller than the reachable set → truncated must be true.
    let result = s
        .neighborhood(vec![alias("test", "a")], EdgeDir::Forward, 2, 2)
        .await
        .unwrap();

    assert!(result.truncated, "truncated must be true when cap is hit");
    assert!(result.nodes.len() <= 2, "no more than max_nodes nodes returned");
}

#[tokio::test]
async fn test_neighborhood_depth_zero() {
    let s = make_store();
    build_hub_graph(&s).await;

    // depth = 0 → only the seed, no edges.
    let result = s
        .neighborhood(vec![alias("test", "a")], EdgeDir::Forward, 0, 50)
        .await
        .unwrap();

    assert_eq!(result.nodes.len(), 1, "depth=0 returns only the seed");
    assert!(result.edges.is_empty(), "depth=0 returns no edges");
    assert!(!result.truncated);
}

#[tokio::test]
async fn test_neighborhood_unknown_seed_skipped() {
    let s = make_store();
    build_hub_graph(&s).await;

    // Unknown seed alias → skipped, no error.
    let result = s
        .neighborhood(
            vec![alias("test", "does-not-exist"), alias("test", "a")],
            EdgeDir::Forward,
            1,
            50,
        )
        .await
        .unwrap();

    // Should still succeed and include A + its neighbors.
    let id_a = s.get_work(alias("test", "a")).await.unwrap().unwrap().canonical_id.0;
    assert!(
        result.nodes.iter().any(|n| n.canonical_id.0 == id_a),
        "known seed A should be present"
    );
}

// ---------------------------------------------------------------------------
// 7. put_content / get_content roundtrip
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_content_roundtrip_custom_role() {
    let s = make_store();
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

    // parse validation
    assert!(braincrawl_core::types::ArtifactRole::parse("map").is_some());
    assert!(braincrawl_core::types::ArtifactRole::parse("Map").is_none());
    assert!(braincrawl_core::types::ArtifactRole::parse("a/b").is_none());
    assert!(braincrawl_core::types::ArtifactRole::parse("").is_none());
}

#[tokio::test]
async fn test_content_roundtrip() {
    let s = make_store();
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
