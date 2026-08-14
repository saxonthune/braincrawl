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
        fetched_at: None,
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
        fetched_at: None,
    };
    let b = WorkRecord {
        source: "pubmed".to_string(),
        kind: NodeKind::Work,
        aliases: vec![alias("doi", "10.1/x"), alias("pmid", "99999")],
        attrs: serde_json::json!({}),
        fetched_at: None,
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
        fetched_at: None,
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
        None,
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
        None,
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

// ── 8. list_artifacts ─────────────────────────────────────────────────────────

use braincrawl_core::types::ArtifactRole;

#[tokio::test]
async fn test_list_artifacts_versions_and_current_flag() {
    let dir = tempfile::tempdir().unwrap();
    let s = make_store(dir.path());
    s.put_work(work("s", vec![alias("doi", "10.1/w")])).await.unwrap();

    for body in [b"v1".to_vec(), b"v2".to_vec()] {
        s.put_content(
            alias("doi", "10.1/w"),
            ArtifactRole::Abstract,
            body,
            "text/plain".to_string(),
            None,
            None,
            "2024-01-01T00:00:00Z".to_string(),
            None,
        )
        .await
        .unwrap();
    }

    let current = s
        .list_artifacts(alias("doi", "10.1/w"), None, false)
        .await
        .unwrap();
    assert_eq!(current.len(), 1, "default returns only the current version");
    assert!(current[0].is_current);

    let all = s
        .list_artifacts(alias("doi", "10.1/w"), None, true)
        .await
        .unwrap();
    assert_eq!(all.len(), 2, "all_versions returns every version");
    assert_eq!(all.iter().filter(|a| a.is_current).count(), 1, "exactly one is_current");
}

#[tokio::test]
async fn test_list_artifacts_role_filter_and_empty() {
    let dir = tempfile::tempdir().unwrap();
    let s = make_store(dir.path());
    s.put_work(work("s", vec![alias("doi", "10.1/w")])).await.unwrap();

    let empty = s
        .list_artifacts(alias("doi", "10.1/w"), None, false)
        .await
        .unwrap();
    assert!(empty.is_empty(), "known work with no artifacts returns empty vector");

    s.put_content(
        alias("doi", "10.1/w"),
        ArtifactRole::Abstract,
        b"a".to_vec(),
        "text/plain".to_string(),
        None,
        None,
        "2024-01-01T00:00:00Z".to_string(),
        None,
    )
    .await
    .unwrap();
    s.put_content(
        alias("doi", "10.1/w"),
        ArtifactRole::Other("fulltext".to_string()),
        b"f".to_vec(),
        "text/plain".to_string(),
        None,
        None,
        "2024-01-01T00:00:00Z".to_string(),
        None,
    )
    .await
    .unwrap();

    let list = s
        .list_artifacts(alias("doi", "10.1/w"), Some(ArtifactRole::Abstract), false)
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].role.as_str(), "abstract");
}

#[tokio::test]
async fn test_list_artifacts_unknown_alias_errors() {
    let dir = tempfile::tempdir().unwrap();
    let s = make_store(dir.path());
    let result = s.list_artifacts(alias("doi", "10.1/nothing"), None, false).await;
    assert!(result.is_err(), "unknown alias must error, not return an empty list");
}

#[tokio::test]
async fn test_list_artifacts_ordering() {
    let dir = tempfile::tempdir().unwrap();
    let s = make_store(dir.path());
    s.put_work(work("s", vec![alias("doi", "10.1/w")])).await.unwrap();

    s.put_content(
        alias("doi", "10.1/w"),
        ArtifactRole::Other("fulltext".to_string()),
        b"f".to_vec(),
        "text/plain".to_string(),
        None,
        None,
        "2024-01-01T00:00:00Z".to_string(),
        None,
    )
    .await
    .unwrap();
    for body in [b"c1".to_vec(), b"c2".to_vec()] {
        s.put_content(
            alias("doi", "10.1/w"),
            ArtifactRole::Other("chunks".to_string()),
            body,
            "text/plain".to_string(),
            None,
            None,
            "2024-01-01T00:00:00Z".to_string(),
            None,
        )
        .await
        .unwrap();
    }

    let list = s
        .list_artifacts(alias("doi", "10.1/w"), None, true)
        .await
        .unwrap();
    let ordering: Vec<(String, u32)> = list
        .iter()
        .map(|a| (a.role.as_str().to_string(), a.version))
        .collect();
    assert_eq!(
        ordering,
        vec![
            ("chunks".to_string(), 2),
            ("chunks".to_string(), 1),
            ("fulltext".to_string(), 1),
        ],
        "role ascending, then version descending"
    );
}

// ── 12. search_works ─────────────────────────────────────────────────────────

#[tokio::test]
async fn test_search_works_by_author_title_year_artifact() {
    use braincrawl_core::types::{ArtifactRole, WorkSearchFilter};

    let dir = tempfile::tempdir().unwrap();
    let s = make_store(dir.path());

    // OpenAlex-shaped authorships.
    let openalex_rec = WorkRecord {
        source: "openalex".to_string(),
        kind: NodeKind::Work,
        aliases: vec![alias("openalex", "W1")],
        attrs: serde_json::json!({
            "title": "Salt and Silt in Ancient Mesopotamian Agriculture",
            "publication_year": 1958,
            "authorships": [
                {"author": {"display_name": "Thorkild Jacobsen"}, "raw_author_name": "Thorkild Jacobsen"},
                {"author": {"display_name": "Robert M. Adams"}, "raw_author_name": "Robert M. Adams"}
            ]
        }),
        fetched_at: None,
    };
    // Manual-shaped bare-string authors.
    let manual_rec = WorkRecord {
        source: "manual".to_string(),
        kind: NodeKind::Work,
        aliases: vec![alias("isbn", "9780826481702")],
        attrs: serde_json::json!({
            "title": "A New Philosophy of Society",
            "publication_year": 2006,
            "authors": ["Manuel DeLanda"]
        }),
        fetched_at: None,
    };
    let id_openalex = s.put_work(openalex_rec).await.unwrap();
    let id_manual = s.put_work(manual_rec).await.unwrap();

    // Author substring, case-insensitive, both shapes.
    let by = |f: WorkSearchFilter| (f, None::<ArtifactRole>);
    let (f, w) = by(WorkSearchFilter { author: Some("delanda".into()), ..Default::default() });
    let hits = s.search_works(&f, w, 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].canonical_id.0, id_manual.0);

    let (f, w) = by(WorkSearchFilter { author: Some("jacobsen".into()), ..Default::default() });
    let hits = s.search_works(&f, w, 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].canonical_id.0, id_openalex.0);

    // An author needle must not match a title.
    let (f, w) = by(WorkSearchFilter { author: Some("mesopotamian".into()), ..Default::default() });
    assert!(s.search_works(&f, w, 10).await.unwrap().is_empty());

    // Title + year AND together.
    let (f, w) = by(WorkSearchFilter {
        title: Some("philosophy".into()),
        year: Some(2006),
        ..Default::default()
    });
    let hits = s.search_works(&f, w, 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].canonical_id.0, id_manual.0);

    let (f, w) = by(WorkSearchFilter {
        title: Some("philosophy".into()),
        year: Some(1958),
        ..Default::default()
    });
    assert!(s.search_works(&f, w, 10).await.unwrap().is_empty());

    // Empty filter with no artifact restriction is rejected.
    assert!(s.search_works(&WorkSearchFilter::default(), None, 10).await.is_err());

    // with_artifact keeps only the work holding a current fulltext.
    s.put_content(
        alias("openalex", "W1"),
        ArtifactRole::Fulltext,
        b"%PDF-1.4 fake".to_vec(),
        "application/pdf".to_string(),
        None,
        None,
        "2024-01-01T00:00:00Z".to_string(),
        None,
    )
    .await
    .unwrap();
    let f = WorkSearchFilter { year: Some(1958), ..Default::default() };
    let hits = s.search_works(&f, Some(ArtifactRole::Fulltext), 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].canonical_id.0, id_openalex.0);

    let f = WorkSearchFilter { author: Some("delanda".into()), ..Default::default() };
    assert!(s.search_works(&f, Some(ArtifactRole::Fulltext), 10).await.unwrap().is_empty());
}
