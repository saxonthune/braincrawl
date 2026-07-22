/// Fixture-based unit tests for the openalex module.
/// No network calls — all tests use canned JSON from tests/fixtures/.
use braincrawl_cli::cli::OutputOpts;
use braincrawl_cli::openalex::entity::{Entity, infer_entity};
use braincrawl_cli::openalex::filters::{KV, validate_filters};
use braincrawl_cli::openalex::shape::{build_envelope, reconstruct_abstract, trim};

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
        emission: false,
    }
}

// ── infer_entity ──────────────────────────────────────────────────────────────

#[test]
fn infer_works_openalex_id() {
    let (e, id) = infer_entity("W2741809807").unwrap();
    assert_eq!(e, Entity::Works);
    assert_eq!(id, "W2741809807");
}

#[test]
fn infer_works_doi_prefix() {
    let (e, _) = infer_entity("doi:10.7717/peerj.4375").unwrap();
    assert_eq!(e, Entity::Works);
}

#[test]
fn infer_works_doi_url() {
    let (e, _) = infer_entity("https://doi.org/10.7717/peerj.4375").unwrap();
    assert_eq!(e, Entity::Works);
}

#[test]
fn infer_works_pmid() {
    let (e, _) = infer_entity("pmid:29456894").unwrap();
    assert_eq!(e, Entity::Works);
}

#[test]
fn infer_works_pmcid() {
    let (e, _) = infer_entity("pmcid:PMC5045003").unwrap();
    assert_eq!(e, Entity::Works);
}

#[test]
fn infer_works_mag() {
    let (e, _) = infer_entity("mag:2741809807").unwrap();
    assert_eq!(e, Entity::Works);
}

#[test]
fn infer_authors_openalex_id() {
    let (e, _) = infer_entity("A5023888391").unwrap();
    assert_eq!(e, Entity::Authors);
}

#[test]
fn infer_authors_orcid_prefix() {
    let (e, _) = infer_entity("orcid:0000-0001-6187-6610").unwrap();
    assert_eq!(e, Entity::Authors);
}

#[test]
fn infer_authors_orcid_url() {
    let (e, _) = infer_entity("https://orcid.org/0000-0001-6187-6610").unwrap();
    assert_eq!(e, Entity::Authors);
}

#[test]
fn infer_sources_openalex_id() {
    let (e, _) = infer_entity("S1983995261").unwrap();
    assert_eq!(e, Entity::Sources);
}

#[test]
fn infer_sources_issn() {
    let (e, _) = infer_entity("issn:2041-1723").unwrap();
    assert_eq!(e, Entity::Sources);
}

#[test]
fn infer_institutions_openalex_id() {
    let (e, _) = infer_entity("I27837315").unwrap();
    assert_eq!(e, Entity::Institutions);
}

#[test]
fn infer_institutions_ror() {
    let (e, _) = infer_entity("ror:https://ror.org/00cvxb145").unwrap();
    assert_eq!(e, Entity::Institutions);
}

#[test]
fn infer_topics_openalex_id() {
    let (e, _) = infer_entity("T12419").unwrap();
    assert_eq!(e, Entity::Topics);
}

#[test]
fn infer_publishers_openalex_id() {
    let (e, _) = infer_entity("P4310319965").unwrap();
    assert_eq!(e, Entity::Publishers);
}

#[test]
fn infer_funders_openalex_id() {
    let (e, _) = infer_entity("F4320306076").unwrap();
    assert_eq!(e, Entity::Funders);
}

#[test]
fn infer_concepts_openalex_id() {
    let (e, _) = infer_entity("C41008148").unwrap();
    assert_eq!(e, Entity::Concepts);
}

#[test]
fn infer_openalex_url_strips_prefix() {
    let (e, id) = infer_entity("https://openalex.org/W2741809807").unwrap();
    assert_eq!(e, Entity::Works);
    assert_eq!(id, "W2741809807");
}

#[test]
fn infer_keyword_slug() {
    let (e, id) = infer_entity("keywords/computer-science").unwrap();
    assert_eq!(e, Entity::Keywords);
    assert_eq!(id, "computer-science");
}

#[test]
fn infer_unknown_returns_err() {
    assert!(infer_entity("totally-ambiguous").is_err());
}

// ── validate_filters ──────────────────────────────────────────────────────────

#[test]
fn filter_known_works_keys_accepted() {
    let kvs = vec![
        KV { key: "publication_year".into(), value: "2020".into() },
        KV { key: "is_oa".into(), value: "true".into() },
    ];
    let result = validate_filters(Entity::Works, &kvs).unwrap();
    assert_eq!(result, "publication_year:2020,is_oa:true");
}

#[test]
fn filter_unknown_key_is_loud_error() {
    let kvs = vec![KV { key: "bogus_field_xyz".into(), value: "foo".into() }];
    let err = validate_filters(Entity::Works, &kvs).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("bogus_field_xyz"), "error should name the bad key: {msg}");
}

#[test]
fn filter_known_authors_key_accepted() {
    let kvs = vec![KV { key: "has_orcid".into(), value: "true".into() }];
    validate_filters(Entity::Authors, &kvs).unwrap();
}

#[test]
fn filter_empty_kvs_returns_empty_string() {
    let result = validate_filters(Entity::Works, &[]).unwrap();
    assert_eq!(result, "");
}

#[test]
fn filter_range_syntax_accepted() {
    // Range values like >100 are passed through as-is
    let kvs = vec![KV { key: "cited_by_count".into(), value: ">100".into() }];
    let result = validate_filters(Entity::Works, &kvs).unwrap();
    assert_eq!(result, "cited_by_count:>100");
}

// ── trim ─────────────────────────────────────────────────────────────────────

#[test]
fn trim_works_keeps_curated_and_drops_bulk_fields() {
    let work = fixture("sample_work.json");
    let trimmed = trim(Entity::Works, &work, &opts());
    let obj = trimmed.as_object().unwrap();

    // Curated fields present
    assert!(obj.contains_key("id"));
    assert!(obj.contains_key("doi"));
    assert!(obj.contains_key("title"));
    assert!(obj.contains_key("cited_by_count"));
    assert!(obj.contains_key("open_access"));
    assert!(obj.contains_key("authorships"));

    // Bulk / unwanted fields absent
    assert!(!obj.contains_key("abstract_inverted_index"),
        "abstract_inverted_index should be trimmed by default");
    assert!(!obj.contains_key("referenced_works"),
        "referenced_works should be trimmed by default");
    assert!(!obj.contains_key("related_works"),
        "related_works should be trimmed by default");
    assert!(!obj.contains_key("counts_by_year"),
        "counts_by_year should be trimmed by default");
}

#[test]
fn trim_works_full_flag_bypasses_trimming() {
    let work = fixture("sample_work.json");
    let full_opts = OutputOpts { full: true, ..opts() };
    let result = trim(Entity::Works, &work, &full_opts);
    let obj = result.as_object().unwrap();
    assert!(obj.contains_key("abstract_inverted_index"),
        "--full should keep abstract_inverted_index");
    assert!(obj.contains_key("referenced_works"),
        "--full should keep referenced_works");
}

#[test]
fn trim_works_authorships_simplified() {
    let work = fixture("sample_work.json");
    let trimmed = trim(Entity::Works, &work, &opts());
    let authorships = trimmed["authorships"].as_array().unwrap();
    assert!(!authorships.is_empty());
    let first = authorships[0].as_object().unwrap();
    // Should have author sub-object with display_name
    assert!(first.contains_key("author"));
    assert!(first["author"].get("display_name").is_some());
}

// ── reconstruct_abstract ──────────────────────────────────────────────────────

#[test]
fn abstract_reconstruction_correct_word_order() {
    let work = fixture("sample_work.json");
    let text = reconstruct_abstract(&work).expect("should produce abstract text");
    assert_eq!(text, "Despite growing interest");
}

#[test]
fn abstract_reconstruction_returns_none_when_absent() {
    let v = serde_json::json!({"id": "W1"});
    assert!(reconstruct_abstract(&v).is_none());
}

#[test]
fn abstract_reconstruction_returns_none_when_empty_index() {
    let v = serde_json::json!({"abstract_inverted_index": {}});
    assert!(reconstruct_abstract(&v).is_none());
}

// ── build_envelope ────────────────────────────────────────────────────────────

#[test]
fn envelope_carries_count_and_resolved_filter() {
    let work = fixture("sample_work.json");
    let env = build_envelope(
        Entity::Works,
        vec![work],
        99,
        Some("cursor-xyz".into()),
        Some("publication_year:2020".into()),
        Some("https://api.openalex.org/works?filter=publication_year:2020".into()),
        &opts(),
    );
    assert_eq!(env.count, 99);
    assert_eq!(env.query.resolved_filter.as_deref(), Some("publication_year:2020"));
    assert_eq!(env.next_cursor.as_deref(), Some("cursor-xyz"));
    assert_eq!(env.query.entity.as_deref(), Some("works"));
}

#[test]
fn envelope_abstract_injected_when_flag_set() {
    let work = fixture("sample_work.json");
    let abs_opts = OutputOpts { include_abstract: true, ..opts() };
    let env = build_envelope(Entity::Works, vec![work], 1, None, None, None, &abs_opts);
    let result = &env.results[0];
    let text = result["abstract"].as_str().expect("abstract should be injected");
    assert_eq!(text, "Despite growing interest");
}

#[test]
fn envelope_limit_truncates_results() {
    let list = fixture("list_page.json");
    let results: Vec<serde_json::Value> = list["results"]
        .as_array()
        .unwrap()
        .to_vec();
    let lim_opts = OutputOpts { limit: Some(0), ..opts() };
    let env = build_envelope(Entity::Works, results, 1, None, None, None, &lim_opts);
    assert_eq!(env.returned, 0);
    assert!(env.truncated);
}
