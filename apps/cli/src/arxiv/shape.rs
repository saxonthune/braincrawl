use serde_json::Value;

use crate::cli::OutputOpts;
use crate::output::{Envelope, QueryMeta};

const CURATED_FIELDS: &[&str] = &[
    "arxiv_id", "title", "abstract", "authors", "published",
    "doi", "primary_category", "categories", "pdf_url",
];

/// Trim a record to the curated field set.
/// `abstract` is dropped unless `opts.include_abstract` (same rule as S2).
pub fn trim(record: &Value, opts: &OutputOpts) -> Value {
    if opts.full {
        return record.clone();
    }
    let Some(obj) = record.as_object() else {
        return record.clone();
    };
    let mut out = serde_json::Map::new();
    for &field in CURATED_FIELDS {
        if field == "abstract" && !opts.include_abstract {
            continue;
        }
        if let Some(v) = obj.get(field) {
            out.insert(field.to_string(), v.clone());
        }
    }
    Value::Object(out)
}

/// Build an Envelope from a page of raw records with optional limit truncation.
pub fn build_envelope(
    records_raw: Vec<Value>,
    count: u64,
    resolved_filter: Option<String>,
    url: Option<String>,
    opts: &OutputOpts,
) -> Envelope {
    let limit = if opts.all { None } else { opts.limit };
    let mut results: Vec<Value> = records_raw.iter().map(|v| trim(v, opts)).collect();

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
            entity: Some("arxiv".to_string()),
            resolved_filter,
            url,
        },
        count,
        returned,
        truncated,
        next_cursor: None,
        results,
    }
}
