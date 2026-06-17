use serde_json::Value;

use crate::cli::OutputOpts;
use crate::output::{Envelope, QueryMeta};
use super::entity::Entity;

/// Top-level field allowlist per entity for default (non-`--full`) mode.
fn curated_fields(entity: Entity) -> &'static [&'static str] {
    match entity {
        Entity::Papers => &[
            "paperId", "externalIds", "title", "abstract", "year",
            "publicationDate", "venue", "citationCount", "referenceCount",
            "authors", "openAccessPdf",
        ],
        Entity::Authors => &[
            "authorId", "externalIds", "name", "affiliations",
            "paperCount", "citationCount", "hIndex",
        ],
    }
}

/// Trim a record to the curated field set for the given entity.
/// With `full=true` in opts, returns the record unchanged.
/// The `abstract` field is dropped unless `opts.include_abstract` is set,
/// since S2 returns full-text abstracts which are bulky.
pub fn trim(entity: Entity, v: &Value, opts: &OutputOpts) -> Value {
    if opts.full {
        return v.clone();
    }
    let Some(obj) = v.as_object() else {
        return v.clone();
    };
    let fields = curated_fields(entity);
    let mut out = serde_json::Map::new();
    for &field in fields {
        if field == "abstract" && !opts.include_abstract {
            continue;
        }
        if let Some(val) = obj.get(field) {
            out.insert(field.to_string(), val.clone());
        }
    }
    Value::Object(out)
}

/// Build an Envelope from a page of results with optional limit truncation.
pub fn build_envelope(
    entity: Entity,
    results_raw: Vec<Value>,
    count: u64,
    next_cursor: Option<String>,
    resolved_filter: Option<String>,
    url: Option<String>,
    opts: &OutputOpts,
) -> Envelope {
    let limit = if opts.all { None } else { opts.limit };
    let mut results: Vec<Value> = results_raw
        .iter()
        .map(|v| trim(entity, v, opts))
        .collect();

    let truncated = if let Some(lim) = limit {
        if results.len() > lim as usize {
            results.truncate(lim as usize);
            true
        } else {
            false
        }
    } else {
        false
    };

    let returned = results.len();
    Envelope {
        query: QueryMeta {
            entity: Some(entity.to_string()),
            resolved_filter,
            url,
        },
        count,
        returned,
        truncated,
        next_cursor,
        results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::OutputOpts;

    fn default_opts() -> OutputOpts {
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

    fn load_fixture(name: &str) -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("fixture not found: {}", path.display()));
        serde_json::from_str(&text).expect("invalid fixture JSON")
    }

    #[test]
    fn trim_paper_keeps_curated_fields() {
        let paper = load_fixture("s2_paper.json");
        let opts = default_opts();
        let trimmed = trim(Entity::Papers, &paper, &opts);
        let obj = trimmed.as_object().unwrap();
        assert!(obj.contains_key("paperId"));
        assert!(obj.contains_key("externalIds"));
        assert!(obj.contains_key("title"));
        assert!(obj.contains_key("year"));
        assert!(obj.contains_key("citationCount"));
        // abstract should be omitted by default
        assert!(!obj.contains_key("abstract"),
            "abstract should be omitted without --abstract");
    }

    #[test]
    fn trim_paper_includes_abstract_with_flag() {
        let paper = load_fixture("s2_paper.json");
        let opts = OutputOpts { include_abstract: true, ..default_opts() };
        let trimmed = trim(Entity::Papers, &paper, &opts);
        let obj = trimmed.as_object().unwrap();
        assert!(obj.contains_key("abstract"), "abstract should be present with --abstract");
        assert!(obj["abstract"].as_str().is_some());
    }

    #[test]
    fn trim_full_bypasses_trimming() {
        let paper = load_fixture("s2_paper.json");
        let opts = OutputOpts { full: true, ..default_opts() };
        let result = trim(Entity::Papers, &paper, &opts);
        // full mode returns everything unchanged
        assert_eq!(&result, &paper);
    }

    #[test]
    fn trim_author_keeps_curated_fields() {
        let author = load_fixture("s2_author.json");
        let opts = default_opts();
        let trimmed = trim(Entity::Authors, &author, &opts);
        let obj = trimmed.as_object().unwrap();
        assert!(obj.contains_key("authorId"));
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("hIndex"));
    }

    #[test]
    fn envelope_carries_count_and_filter() {
        let paper = load_fixture("s2_paper.json");
        let opts = default_opts();
        let env = build_envelope(
            Entity::Papers,
            vec![paper],
            42,
            Some("next-token".into()),
            Some("query:test".into()),
            Some("https://api.semanticscholar.org/graph/v1/paper/search?query=test".into()),
            &opts,
        );
        assert_eq!(env.count, 42);
        assert_eq!(env.query.resolved_filter.as_deref(), Some("query:test"));
        assert_eq!(env.next_cursor.as_deref(), Some("next-token"));
        assert_eq!(env.query.entity.as_deref(), Some("paper"));
    }

    #[test]
    fn envelope_limit_truncates_results() {
        let paper = load_fixture("s2_paper.json");
        let opts = OutputOpts { limit: Some(0), ..default_opts() };
        let env = build_envelope(Entity::Papers, vec![paper], 1, None, None, None, &opts);
        assert_eq!(env.returned, 0);
        assert!(env.truncated);
    }
}
