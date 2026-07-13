use braincrawl_sql::{alias, edge, edge_assertion, migrations, node, node_assertion, artifact, stats};
use braincrawl_sql::{alias_pair_list, in_list};
use rusqlite::{params, Connection};

fn setup_db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    for (_name, sql) in migrations() {
        conn.execute_batch(sql).unwrap();
    }
    conn
}

// ── prepare checks ────────────────────────────────────────────────────────

#[test]
fn all_queries_prepare() {
    let conn = setup_db();
    let mut stmts: Vec<(&str, &str)> = vec![
        // alias
        ("alias::INSERT_IGNORE", alias::INSERT_IGNORE),
        ("alias::GET", alias::GET),
        ("alias::LIST_BY_NODE", alias::LIST_BY_NODE),
        ("alias::MERGE_DELETE_CONFLICTS", alias::MERGE_DELETE_CONFLICTS),
        ("alias::MERGE_REPOINT", alias::MERGE_REPOINT),
        // node
        ("node::INSERT_IGNORE", node::INSERT_IGNORE),
        ("node::TOMBSTONE", node::TOMBSTONE),
        ("node::SELECT_MERGED_INTO", node::SELECT_MERGED_INTO),
        ("node::SELECT", node::SELECT),
        // node_assertion
        ("node_assertion::UPSERT", node_assertion::UPSERT),
        ("node_assertion::SELECT_BY_NODE", node_assertion::SELECT_BY_NODE),
        ("node_assertion::MERGE_UPSERT", node_assertion::MERGE_UPSERT),
        ("node_assertion::MERGE_DELETE_LOSER", node_assertion::MERGE_DELETE_LOSER),
        // edge
        ("edge::INSERT_IGNORE", edge::INSERT_IGNORE),
        ("edge::SELECT_FORWARD_FIRST", edge::SELECT_FORWARD_FIRST),
        ("edge::SELECT_FORWARD_PAGE", edge::SELECT_FORWARD_PAGE),
        ("edge::SELECT_BACKWARD_FIRST", edge::SELECT_BACKWARD_FIRST),
        ("edge::SELECT_BACKWARD_PAGE", edge::SELECT_BACKWARD_PAGE),
        ("edge::MERGE_EA_SRC_UPSERT", edge::MERGE_EA_SRC_UPSERT),
        ("edge::MERGE_EA_SRC_DELETE_LOSER", edge::MERGE_EA_SRC_DELETE_LOSER),
        ("edge::MERGE_EDGE_SRC_DELETE_CONFLICTS", edge::MERGE_EDGE_SRC_DELETE_CONFLICTS),
        ("edge::MERGE_EDGE_SRC_REPOINT", edge::MERGE_EDGE_SRC_REPOINT),
        ("edge::MERGE_EA_DST_UPSERT", edge::MERGE_EA_DST_UPSERT),
        ("edge::MERGE_EA_DST_DELETE_LOSER", edge::MERGE_EA_DST_DELETE_LOSER),
        ("edge::MERGE_EDGE_DST_DELETE_CONFLICTS", edge::MERGE_EDGE_DST_DELETE_CONFLICTS),
        ("edge::MERGE_EDGE_DST_REPOINT", edge::MERGE_EDGE_DST_REPOINT),
        // edge_assertion
        ("edge_assertion::UPSERT", edge_assertion::UPSERT),
        ("edge_assertion::SELECT_BY_EDGE", edge_assertion::SELECT_BY_EDGE),
        // artifact
        ("artifact::SELECT_CURRENT", artifact::SELECT_CURRENT),
        ("artifact::NEXT_VERSION", artifact::NEXT_VERSION),
        ("artifact::FLIP_CURRENT_OFF", artifact::FLIP_CURRENT_OFF),
        ("artifact::INSERT", artifact::INSERT),
        ("artifact::MERGE_DEMOTE_LOSER_CURRENT", artifact::MERGE_DEMOTE_LOSER_CURRENT),
        ("artifact::MERGE_REPOINT", artifact::MERGE_REPOINT),
        ("artifact::MERGE_DELETE_LOSER", artifact::MERGE_DELETE_LOSER),
        // stats
        ("stats::WORKS", stats::WORKS),
        ("stats::WORKS_DESCRIBED", stats::WORKS_DESCRIBED),
        ("stats::NODES_TOTAL", stats::NODES_TOTAL),
        ("stats::TOMBSTONES", stats::TOMBSTONES),
        ("stats::EDGES_TOTAL", stats::EDGES_TOTAL),
        ("stats::NODES_BY_KIND", stats::NODES_BY_KIND),
        ("stats::EDGES_BY_RELATION", stats::EDGES_BY_RELATION),
        ("stats::ASSERTIONS_BY_SOURCE", stats::ASSERTIONS_BY_SOURCE),
    ];

    // Variable-arity builders — test with n = 1 and n = 3.
    let in_1 = format!("SELECT canonical_id FROM alias WHERE canonical_id IN {}", in_list(1));
    let in_3 = format!("SELECT canonical_id FROM alias WHERE canonical_id IN {}", in_list(3));
    let pair_1 = format!(
        "SELECT namespace, value FROM alias WHERE (namespace, value) IN {}",
        alias_pair_list(1)
    );
    let pair_3 = format!(
        "SELECT namespace, value FROM alias WHERE (namespace, value) IN {}",
        alias_pair_list(3)
    );
    stmts.push(("in_list(1)", &in_1));
    stmts.push(("in_list(3)", &in_3));
    stmts.push(("alias_pair_list(1)", &pair_1));
    stmts.push(("alias_pair_list(3)", &pair_3));

    for (name, sql) in stmts {
        conn.prepare(sql)
            .unwrap_or_else(|e| panic!("prepare failed for {name}: {e}\nSQL: {sql}"));
    }
}

// ── happy-path execution ──────────────────────────────────────────────────

#[test]
fn happy_path() {
    let conn = setup_db();

    let ts = "2024-01-01T00:00:00Z";

    // Create two nodes.
    conn.execute(node::INSERT_IGNORE, params!["n1", "work", ts]).unwrap();
    conn.execute(node::INSERT_IGNORE, params!["n2", "work", ts]).unwrap();

    // Idempotent: creating the same node twice is a no-op.
    conn.execute(node::INSERT_IGNORE, params!["n1", "work", ts]).unwrap();

    // Insert alias and resolve it.
    conn.execute(alias::INSERT_IGNORE, params!["doi", "10.1/test", "n1"]).unwrap();
    let cid: String =
        conn.query_row(alias::GET, params!["doi", "10.1/test"], |r| r.get(0)).unwrap();
    assert_eq!(cid, "n1");

    // get_or_create: second insert is a no-op; SELECT returns the winner.
    conn.execute(alias::INSERT_IGNORE, params!["doi", "10.1/test", "n2"]).unwrap();
    let cid2: String =
        conn.query_row(alias::GET, params!["doi", "10.1/test"], |r| r.get(0)).unwrap();
    assert_eq!(cid2, "n1", "ON CONFLICT DO NOTHING: first writer wins");

    // Upsert a node assertion.
    conn.execute(
        node_assertion::UPSERT,
        params!["n1", "crossref", r#"{"title":"Test"}"#, ts],
    )
    .unwrap();
    let (src, _attrs, fat): (String, String, String) = conn
        .query_row(node_assertion::SELECT_BY_NODE, params!["n1"], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(src, "crossref");
    assert_eq!(fat, ts);

    // Insert an edge and its assertion.
    conn.execute(edge::INSERT_IGNORE, params!["n1", "n2", "cites"]).unwrap();
    conn.execute(
        edge_assertion::UPSERT,
        params!["n1", "n2", "cites", "openalex", Option::<String>::None, ts],
    )
    .unwrap();

    // Forward edge pagination.
    let mut stmt = conn.prepare(edge::SELECT_FORWARD_FIRST).unwrap();
    let edges: Vec<(String, String, String)> = stmt
        .query_map(params!["n1", 10i64], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0], ("n1".to_string(), "n2".to_string(), "cites".to_string()));

    // Edge assertion lookup.
    let (ea_src,): (String,) = conn
        .query_row(
            edge_assertion::SELECT_BY_EDGE,
            params!["n1", "n2", "cites"],
            |r| Ok((r.get(0)?,)),
        )
        .unwrap();
    assert_eq!(ea_src, "openalex");

    // Artifact: next_version → flip_current_off → insert → select_current.
    let next: i64 =
        conn.query_row(artifact::NEXT_VERSION, params!["n1", "abstract"], |r| r.get(0))
            .unwrap();
    assert_eq!(next, 1);

    conn.execute(artifact::FLIP_CURRENT_OFF, params!["n1", "abstract"]).unwrap();
    conn.execute(
        artifact::INSERT,
        params!["n1", "abstract", 1i64, "n1/abstract/v1", "sha256:abc", 100i64, "text/plain", Option::<String>::None, Option::<String>::None, ts, 1i64],
    )
    .unwrap();

    let (role, ver, is_curr): (String, i64, i64) = conn
        .query_row(artifact::SELECT_CURRENT, params!["n1", "abstract"], |r| {
            Ok((r.get(1)?, r.get(2)?, r.get(10)?))
        })
        .unwrap();
    assert_eq!(role, "abstract");
    assert_eq!(ver, 1);
    assert_eq!(is_curr, 1);

    // Resolve live (no tombstone yet).
    let merged_into: Option<String> =
        conn.query_row(node::SELECT_MERGED_INTO, params!["n1"], |r| r.get(0)).unwrap();
    assert!(merged_into.is_none());

    // present_aliases / have.
    conn.execute(alias::INSERT_IGNORE, params!["isbn", "978-0-00-000000-0", "n2"]).unwrap();
    let have_sql = format!(
        "SELECT namespace, value FROM alias WHERE (namespace, value) IN {}",
        alias_pair_list(2)
    );
    let mut stmt = conn.prepare(&have_sql).unwrap();
    let found: Vec<(String, String)> = stmt
        .query_map(
            params!["doi", "10.1/test", "isbn", "978-0-00-000000-0"],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(found.len(), 2);
}

// ── merge smoke test ──────────────────────────────────────────────────────

#[test]
fn merge_alias_and_node_assertion() {
    let conn = setup_db();
    let ts1 = "2024-01-01T00:00:00Z";
    let ts2 = "2024-06-01T00:00:00Z";

    // Create survivor and loser.
    conn.execute(node::INSERT_IGNORE, params!["survivor", "work", ts1]).unwrap();
    conn.execute(node::INSERT_IGNORE, params!["loser", "work", ts1]).unwrap();

    // Aliases: loser has two, survivor already owns one.
    conn.execute(alias::INSERT_IGNORE, params!["doi", "10.1/a", "survivor"]).unwrap();
    conn.execute(alias::INSERT_IGNORE, params!["doi", "10.1/b", "loser"]).unwrap();
    conn.execute(alias::INSERT_IGNORE, params!["isbn", "0-00", "loser"]).unwrap();

    // Node assertions: both have crossref but loser is newer; survivor has openalex.
    conn.execute(node_assertion::UPSERT, params!["survivor", "crossref", r#"{"v":1}"#, ts1]).unwrap();
    conn.execute(node_assertion::UPSERT, params!["loser", "crossref", r#"{"v":2}"#, ts2]).unwrap();
    conn.execute(node_assertion::UPSERT, params!["survivor", "openalex", r#"{"x":1}"#, ts1]).unwrap();

    // --- Merge steps ---
    // Tombstone.
    conn.execute(node::TOMBSTONE, params!["survivor", "loser"]).unwrap();
    // Alias merge: delete loser's doi/10.1/a conflict (survivor already owns it).
    conn.execute(alias::MERGE_DELETE_CONFLICTS, params!["loser", "survivor"]).unwrap();
    conn.execute(alias::MERGE_REPOINT, params!["survivor", "loser"]).unwrap();
    // node_assertion merge: loser's crossref (ts2) replaces survivor's (ts1).
    conn.execute(node_assertion::MERGE_UPSERT, params!["survivor", "loser"]).unwrap();
    conn.execute(node_assertion::MERGE_DELETE_LOSER, params!["loser"]).unwrap();

    // Verify: loser is tombstoned.
    let merged: Option<String> =
        conn.query_row(node::SELECT_MERGED_INTO, params!["loser"], |r| r.get(0)).unwrap();
    assert_eq!(merged.as_deref(), Some("survivor"));

    // Verify: survivor now has 3 aliases (doi/10.1/a + doi/10.1/b + isbn/0-00).
    let mut stmt = conn.prepare(alias::LIST_BY_NODE).unwrap();
    let aliases: Vec<(String, String)> = stmt
        .query_map(params!["survivor"], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(aliases.len(), 3, "survivor should have 3 aliases");

    // Loser's doi/10.1/b alias conflict was NOT deleted (no conflict with survivor).
    assert!(aliases.iter().any(|(ns, v)| ns == "doi" && v == "10.1/b"));

    // Verify: crossref assertion kept the newer one (ts2, from loser).
    let (fetched,): (String,) = conn
        .query_row(
            "SELECT fetched_at FROM node_assertion WHERE canonical_id = 'survivor' AND source = 'crossref'",
            [],
            |r| Ok((r.get(0)?,)),
        )
        .unwrap();
    assert_eq!(fetched, ts2, "loser's newer crossref should win");

    // openalex still present.
    let cnt: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM node_assertion WHERE canonical_id = 'survivor' AND source = 'openalex'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cnt, 1);
}
