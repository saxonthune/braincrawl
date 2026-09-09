use serde_json::Value;

use crate::cli::OutputOpts;
use crate::output::{Envelope, QueryMeta};
use super::entity::Entity;

/// Top-level field allowlist per entity for default (non-`--full`) mode.
fn curated_fields(entity: Entity) -> &'static [&'static str] {
    match entity {
        Entity::Works => &[
            "id", "doi", "title", "display_name", "publication_year", "publication_date",
            "type", "cited_by_count", "open_access", "authorships", "primary_location",
            "primary_topic", "language", "ids",
        ],
        Entity::Authors => &[
            "id", "orcid", "display_name", "display_name_alternatives", "works_count",
            "cited_by_count", "summary_stats", "last_known_institutions", "ids",
        ],
        Entity::Sources => &[
            "id", "issn_l", "issn", "display_name", "type", "is_oa", "is_in_doaj",
            "works_count", "cited_by_count", "country_code", "ids",
        ],
        Entity::Institutions => &[
            "id", "ror", "display_name", "country_code", "type", "works_count",
            "cited_by_count", "ids",
        ],
        Entity::Topics => &[
            "id", "display_name", "description", "subfield", "field", "domain",
            "works_count", "cited_by_count",
        ],
        Entity::Keywords => &["id", "display_name", "works_count", "cited_by_count"],
        Entity::Publishers => &[
            "id", "display_name", "works_count", "cited_by_count", "country_codes",
            "hierarchy_level", "ids",
        ],
        Entity::Funders => &[
            "id", "display_name", "works_count", "cited_by_count", "country_code",
            "description", "ids",
        ],
        Entity::Concepts => &[
            "id", "display_name", "level", "description", "works_count", "cited_by_count",
        ],
    }
}

/// Trim a record to the curated field set for the given entity.
/// With `full=true` in opts, returns the record unchanged.
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
        if let Some(val) = obj.get(field) {
            let trimmed = if field == "authorships" {
                trim_authorships(val)
            } else {
                val.clone()
            };
            out.insert(field.to_string(), trimmed);
        }
    }
    Value::Object(out)
}

/// Trim each authorship entry to keep only the essential author identity fields.
fn trim_authorships(v: &Value) -> Value {
    let Some(arr) = v.as_array() else {
        return v.clone();
    };
    let trimmed: Vec<Value> = arr.iter().map(|entry| {
        let Some(obj) = entry.as_object() else {
            return entry.clone();
        };
        let mut out = serde_json::Map::new();
        if let Some(pos) = obj.get("author_position") {
            out.insert("author_position".into(), pos.clone());
        }
        if let Some(author) = obj.get("author") {
            if let Some(a_obj) = author.as_object() {
                let mut a_out = serde_json::Map::new();
                for &field in &["id", "display_name", "orcid"] {
                    if let Some(v) = a_obj.get(field) {
                        a_out.insert(field.into(), v.clone());
                    }
                }
                out.insert("author".into(), Value::Object(a_out));
            }
        }
        // Keep institution country codes for a lightweight affiliation hint
        if let Some(insts) = obj.get("institutions") {
            if let Some(inst_arr) = insts.as_array() {
                let slim: Vec<Value> = inst_arr.iter().map(|i| {
                    if let Some(io) = i.as_object() {
                        let mut m = serde_json::Map::new();
                        for &f in &["id", "display_name", "country_code"] {
                            if let Some(v) = io.get(f) {
                                m.insert(f.into(), v.clone());
                            }
                        }
                        Value::Object(m)
                    } else {
                        i.clone()
                    }
                }).collect();
                out.insert("institutions".into(), Value::Array(slim));
            }
        }
        Value::Object(out)
    }).collect();
    Value::Array(trimmed)
}

/// Reconstruct an abstract from the inverted index stored in `v`.
/// Only called when `--abstract` is set. Returns `None` if the field is absent or empty.
pub fn reconstruct_abstract(v: &Value) -> Option<String> {
    let index = v.get("abstract_inverted_index")?.as_object()?;
    if index.is_empty() {
        return None;
    }
    let mut positions: Vec<(usize, &str)> = Vec::new();
    for (word, pos_arr) in index {
        if let Some(arr) = pos_arr.as_array() {
            for pos in arr {
                if let Some(p) = pos.as_u64() {
                    positions.push((p as usize, word.as_str()));
                }
            }
        }
    }
    if positions.is_empty() {
        return None;
    }
    positions.sort_by_key(|(p, _)| *p);
    let max_pos = positions.iter().map(|(p, _)| *p).max().unwrap_or(0);
    let mut words = vec![""; max_pos + 1];
    for (pos, word) in &positions {
        words[*pos] = word;
    }
    let text = words.join(" ");
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

/// Build a foundation Envelope from a page of results, with optional abstract injection.
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
        .map(|v| {
            let mut trimmed = trim(entity, v, opts);
            if opts.include_abstract {
                if let Some(text) = reconstruct_abstract(v) {
                    if let Some(obj) = trimmed.as_object_mut() {
                        obj.insert("abstract".into(), Value::String(text));
                    }
                }
            }
            trimmed
        })
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
            emission: false,
        }
    }

    fn full_opts() -> OutputOpts {
        OutputOpts { full: true, ..default_opts() }
    }

    fn abstract_opts() -> OutputOpts {
        OutputOpts { include_abstract: true, ..default_opts() }
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
    fn trim_works_keeps_curated_fields() {
        let work = load_fixture("sample_work.json");
        let opts = default_opts();
        let trimmed = trim(Entity::Works, &work, &opts);
        let obj = trimmed.as_object().unwrap();
        assert!(obj.contains_key("id"), "id should be present");
        assert!(obj.contains_key("doi"), "doi should be present");
        assert!(obj.contains_key("title"), "title should be present");
        assert!(obj.contains_key("cited_by_count"), "cited_by_count should be present");
        assert!(obj.contains_key("open_access"), "open_access should be present");
        assert!(!obj.contains_key("abstract_inverted_index"),
            "abstract_inverted_index should be trimmed");
        assert!(!obj.contains_key("referenced_works"),
            "referenced_works should be trimmed");
    }

    #[test]
    fn trim_full_bypasses_trimming() {
        let work = load_fixture("sample_work.json");
        let opts = full_opts();
        let result = trim(Entity::Works, &work, &opts);
        let obj = result.as_object().unwrap();
        // Full mode should include everything
        assert!(obj.contains_key("abstract_inverted_index"),
            "full mode should keep abstract_inverted_index");
    }

    #[test]
    fn reconstruct_abstract_correct_order() {
        let work = load_fixture("sample_work.json");
        let text = reconstruct_abstract(&work).expect("should reconstruct");
        assert_eq!(text, "Despite growing interest");
    }

    #[test]
    fn abstract_injected_when_flag_set() {
        let work = load_fixture("sample_work.json");
        let opts = abstract_opts();
        // abstract field is injected by build_envelope, not trim — check via build_envelope
        let envelope = build_envelope(
            Entity::Works,
            vec![work],
            1,
            None,
            Some("publication_year:2018".into()),
            None,
            &opts,
        );
        let result = &envelope.results[0];
        assert!(result.get("abstract").is_some(), "abstract should be injected");
        let text = result["abstract"].as_str().unwrap();
        assert_eq!(text, "Despite growing interest");
    }

    #[test]
    fn envelope_carries_count_and_filter() {
        let work = load_fixture("sample_work.json");
        let opts = default_opts();
        let envelope = build_envelope(
            Entity::Works,
            vec![work],
            42,
            Some("cursor-abc".into()),
            Some("publication_year:2020".into()),
            Some("https://api.openalex.org/works?filter=publication_year:2020".into()),
            &opts,
        );
        assert_eq!(envelope.count, 42);
        assert_eq!(envelope.query.resolved_filter.as_deref(), Some("publication_year:2020"));
        assert_eq!(envelope.next_cursor.as_deref(), Some("cursor-abc"));
    }
}
