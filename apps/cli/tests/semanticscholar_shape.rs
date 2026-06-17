/// Fixture-based unit tests for the semanticscholar module.
/// No network calls — all tests use canned JSON from tests/fixtures/.
use braincrawl_cli::cli::OutputOpts;
use braincrawl_cli::semanticscholar::entity::{Entity, infer_entity};
use braincrawl_cli::semanticscholar::mapping::{extract_aliases, node_kind, to_edges, to_work_record};
use braincrawl_cli::semanticscholar::shape::{build_envelope, trim};

fn fixture(name: &str) -> serde_json::Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("fixture not found: {}", path.display()));
    serde_json::from_str(&text).expect("invalid fixture JSON")
}

fn opts() -> OutputOpts {
    OutputOpts {
        json: false,
        text: false,
        limit: None,
        all: false,
        fields: vec![],
        full: false,
        skip_push: false,
        include_abstract: false,
    }
}

// ── infer_entity ──────────────────────────────────────────────────────────────

#[test]
fn infer_doi_prefix_maps_to_papers() {
    let (e, id) = infer_entity("doi:10.7717/peerj.4375").unwrap();
    assert_eq!(e, Entity::Papers);
    assert_eq!(id, "DOI:10.7717/peerj.4375");
}

#[test]
fn infer_arxiv_prefix() {
    let (e, id) = infer_entity("arxiv:2301.07041").unwrap();
    assert_eq!(e, Entity::Papers);
    assert_eq!(id, "ARXIV:2301.07041");
}

#[test]
fn infer_corpusid_prefix() {
    let (e, id) = infer_entity("corpusid:12345").unwrap();
    assert_eq!(e, Entity::Papers);
    assert_eq!(id, "CorpusId:12345");
}

#[test]
fn infer_mag_prefix() {
    let (e, _) = infer_entity("mag:2741809807").unwrap();
    assert_eq!(e, Entity::Papers);
}

#[test]
fn infer_pmid_prefix() {
    let (e, _) = infer_entity("pmid:29456894").unwrap();
    assert_eq!(e, Entity::Papers);
}

#[test]
fn infer_pmcid_prefix() {
    let (e, _) = infer_entity("pmcid:PMC5045003").unwrap();
    assert_eq!(e, Entity::Papers);
}

#[test]
fn infer_s2_prefix_papers() {
    let (e, id) = infer_entity("s2:649def34f8be52c8b66281af98ae884c09aef38b").unwrap();
    assert_eq!(e, Entity::Papers);
    assert_eq!(id, "649def34f8be52c8b66281af98ae884c09aef38b");
}

#[test]
fn infer_s2author_prefix() {
    let (e, id) = infer_entity("s2author:1741101").unwrap();
    assert_eq!(e, Entity::Authors);
    assert_eq!(id, "1741101");
}

#[test]
fn infer_bare_40hex_maps_to_papers() {
    let (e, id) = infer_entity("649def34f8be52c8b66281af98ae884c09aef38b").unwrap();
    assert_eq!(e, Entity::Papers);
    assert_eq!(id, "649def34f8be52c8b66281af98ae884c09aef38b");
}

#[test]
fn infer_native_doi_passes_through() {
    let (e, id) = infer_entity("DOI:10.7717/peerj.4375").unwrap();
    assert_eq!(e, Entity::Papers);
    assert_eq!(id, "DOI:10.7717/peerj.4375");
}

#[test]
fn infer_ambiguous_bare_number_fails_loudly() {
    let err = infer_entity("12345").unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("12345"), "error should name the bad id: {msg}");
}

#[test]
fn infer_unknown_form_fails() {
    assert!(infer_entity("totally-unknown").is_err());
}

// ── node_kind ─────────────────────────────────────────────────────────────────

#[test]
fn node_kind_papers_is_work() {
    assert_eq!(node_kind(Entity::Papers), Some("Work"));
}

#[test]
fn node_kind_authors_is_author() {
    assert_eq!(node_kind(Entity::Authors), Some("Author"));
}

// ── extract_aliases + DOI merge guarantee ────────────────────────────────────

#[test]
fn aliases_paper_doi_is_bare_value() {
    let paper = fixture("s2_paper.json");
    let aliases = extract_aliases(Entity::Papers, &paper);
    let map: std::collections::HashMap<&str, &str> = aliases
        .iter()
        .map(|a| (a["namespace"].as_str().unwrap(), a["value"].as_str().unwrap()))
        .collect();
    // DOI merge guarantee: bare value (no https://doi.org/ prefix)
    assert_eq!(
        map.get("doi"),
        Some(&"10.7717/peerj.4375"),
        "DOI alias must be bare for merge with OpenAlex nodes"
    );
    assert!(map.contains_key("s2"), "s2 alias should be registered");
    assert!(map.contains_key("arxiv"), "arxiv alias should be registered");
    assert!(map.contains_key("corpusid"), "corpusid alias should be registered");
}

#[test]
fn aliases_author_s2author_and_orcid() {
    let author = fixture("s2_author.json");
    let aliases = extract_aliases(Entity::Authors, &author);
    let map: std::collections::HashMap<&str, &str> = aliases
        .iter()
        .map(|a| (a["namespace"].as_str().unwrap(), a["value"].as_str().unwrap()))
        .collect();
    assert_eq!(map.get("s2author"), Some(&"1741101"));
    assert_eq!(map.get("orcid"), Some(&"0000-0001-6187-6610"));
}

// ── to_work_record ────────────────────────────────────────────────────────────

#[test]
fn work_record_paper_has_semanticscholar_source() {
    let paper = fixture("s2_paper.json");
    let wr = to_work_record(Entity::Papers, &paper).unwrap();
    assert_eq!(wr["source"].as_str(), Some("semanticscholar"));
    assert_eq!(wr["kind"].as_str(), Some("Work"));
    let aliases = wr["aliases"].as_array().unwrap();
    assert!(!aliases.is_empty());
}

#[test]
fn work_record_author_has_semanticscholar_source() {
    let author = fixture("s2_author.json");
    let wr = to_work_record(Entity::Authors, &author).unwrap();
    assert_eq!(wr["source"].as_str(), Some("semanticscholar"));
    assert_eq!(wr["kind"].as_str(), Some("Author"));
}

// ── to_edges ──────────────────────────────────────────────────────────────────

#[test]
fn edges_have_cites_relation_and_semanticscholar_source() {
    let pairs = vec![
        ("doi:10.1000/citing".to_string(), "doi:10.7717/peerj.4375".to_string()),
    ];
    let edges = to_edges(&pairs);
    assert_eq!(edges.len(), 1);
    let e = &edges[0];
    assert_eq!(e["relation"].as_str(), Some("cites"));
    assert_eq!(e["source"].as_str(), Some("semanticscholar"));
    assert_eq!(e["src"]["namespace"].as_str(), Some("doi"));
    assert_eq!(e["src"]["value"].as_str(), Some("10.1000/citing"));
    assert_eq!(e["dst"]["namespace"].as_str(), Some("doi"));
    assert_eq!(e["dst"]["value"].as_str(), Some("10.7717/peerj.4375"));
    assert!(e["fetched_at"].as_str().is_some());
}

#[test]
fn edges_split_s2_prefix_correctly() {
    let pairs = vec![
        (
            "s2:649def34f8be52c8b66281af98ae884c09aef38b".to_string(),
            "doi:10.7717/peerj.4375".to_string(),
        ),
    ];
    let edges = to_edges(&pairs);
    let e = &edges[0];
    assert_eq!(e["src"]["namespace"].as_str(), Some("s2"));
    assert_eq!(e["src"]["value"].as_str(), Some("649def34f8be52c8b66281af98ae884c09aef38b"));
}

// ── trim ─────────────────────────────────────────────────────────────────────

#[test]
fn trim_paper_keeps_curated_drops_abstract_by_default() {
    let paper = fixture("s2_paper.json");
    let trimmed = trim(Entity::Papers, &paper, &opts());
    let obj = trimmed.as_object().unwrap();
    assert!(obj.contains_key("paperId"));
    assert!(obj.contains_key("externalIds"));
    assert!(obj.contains_key("title"));
    assert!(obj.contains_key("year"));
    assert!(!obj.contains_key("abstract"),
        "abstract must be omitted without --abstract");
}

#[test]
fn trim_paper_includes_abstract_with_flag() {
    let paper = fixture("s2_paper.json");
    let abs_opts = OutputOpts { include_abstract: true, ..opts() };
    let trimmed = trim(Entity::Papers, &paper, &abs_opts);
    let obj = trimmed.as_object().unwrap();
    assert!(obj.contains_key("abstract"), "abstract must be present with --abstract");
    let text = obj["abstract"].as_str().unwrap();
    assert!(!text.is_empty());
}

#[test]
fn trim_full_bypasses_trimming() {
    let paper = fixture("s2_paper.json");
    let full_opts = OutputOpts { full: true, ..opts() };
    let result = trim(Entity::Papers, &paper, &full_opts);
    assert_eq!(&result, &paper);
}

// ── build_envelope ────────────────────────────────────────────────────────────

#[test]
fn envelope_carries_count_and_resolved_filter() {
    let paper = fixture("s2_paper.json");
    let env = build_envelope(
        Entity::Papers,
        vec![paper],
        99,
        Some("next-cursor".into()),
        Some("query:mesopotamia".into()),
        None,
        &opts(),
    );
    assert_eq!(env.count, 99);
    assert_eq!(env.query.resolved_filter.as_deref(), Some("query:mesopotamia"));
    assert_eq!(env.next_cursor.as_deref(), Some("next-cursor"));
    assert_eq!(env.query.entity.as_deref(), Some("paper"));
}

#[test]
fn envelope_limit_truncates() {
    let paper = fixture("s2_paper.json");
    let lim_opts = OutputOpts { limit: Some(0), ..opts() };
    let env = build_envelope(Entity::Papers, vec![paper], 1, None, None, None, &lim_opts);
    assert_eq!(env.returned, 0);
    assert!(env.truncated);
}
