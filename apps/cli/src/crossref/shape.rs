use crate::cli::OutputOpts;
use crate::output::{Envelope, QueryMeta};

/// Build an Envelope for a Crossref refs response.
pub fn build_envelope(citing_doi: &str, cited_dois: &[String], _opts: &OutputOpts) -> Envelope {
    let results: Vec<serde_json::Value> = cited_dois
        .iter()
        .map(|d| serde_json::json!({"id": format!("doi:{d}")}))
        .collect();
    let count = cited_dois.len() as u64;
    let returned = results.len();
    Envelope {
        query: QueryMeta {
            entity: Some("crossref:refs".to_string()),
            resolved_filter: Some(format!("doi:{citing_doi}")),
            url: None,
        },
        count,
        returned,
        truncated: false,
        next_cursor: None,
        results,
    }
}
