use serde_json::Value;

use crate::provider::{Alias, EdgeInput, Emission, WorkRecord};
use super::entity::Entity;

/// Map a Semantic Scholar entity type to the server's NodeKind string.
pub fn node_kind(entity: Entity) -> Option<&'static str> {
    match entity {
        Entity::Papers => Some("Work"),
        Entity::Authors => Some("Author"),
    }
}

/// Extract structured aliases from a Semantic Scholar record.
/// Values are bare (URL prefixes stripped) to match the server's `ns:value` form.
pub fn extract_aliases(entity: Entity, record: &Value) -> Vec<Value> {
    let mut aliases: Vec<Value> = Vec::new();

    match entity {
        Entity::Papers => {
            if let Some(paper_id) = record.get("paperId").and_then(|v| v.as_str()) {
                aliases.push(alias("s2", paper_id));
            }
            if let Some(ext) = record.get("externalIds").and_then(|v| v.as_object()) {
                if let Some(doi) = ext.get("DOI").and_then(|v| v.as_str()) {
                    aliases.push(alias("doi", strip_doi_url(doi)));
                }
                if let Some(arxiv) = ext.get("ArXiv").and_then(|v| v.as_str()) {
                    aliases.push(alias("arxiv", arxiv));
                }
                if let Some(mag) = ext.get("MAG") {
                    let mag_str = if let Some(s) = mag.as_str() {
                        Some(s.to_string())
                    } else {
                        mag.as_u64().map(|n| n.to_string())
                    };
                    if let Some(s) = mag_str {
                        aliases.push(alias("mag", &s));
                    }
                }
                if let Some(pmid) = ext.get("PubMed").and_then(|v| v.as_str()) {
                    aliases.push(alias("pmid", pmid));
                }
                if let Some(pmcid) = ext.get("PubMedCentral").and_then(|v| v.as_str()) {
                    aliases.push(alias("pmcid", pmcid));
                }
                // CorpusId may be numeric
                if let Some(cid) = ext.get("CorpusId") {
                    let cid_str = if let Some(s) = cid.as_str() {
                        Some(s.to_string())
                    } else {
                        cid.as_u64().map(|n| n.to_string())
                    };
                    if let Some(s) = cid_str {
                        aliases.push(alias("corpusid", &s));
                    }
                }
            }
        }
        Entity::Authors => {
            if let Some(author_id) = record.get("authorId").and_then(|v| v.as_str()) {
                aliases.push(alias("s2author", author_id));
            }
            if let Some(ext) = record.get("externalIds").and_then(|v| v.as_object()) {
                if let Some(orcid) = ext.get("ORCID").and_then(|v| v.as_str()) {
                    aliases.push(alias("orcid", strip_orcid_url(orcid)));
                }
            }
        }
    }

    aliases
}

/// Build a WorkRecord JSON value for the given entity and record.
/// Returns `None` if the entity has no push target (never for S2 — both map to a kind).
pub fn to_work_record(entity: Entity, record: &Value) -> Option<Value> {
    let kind = node_kind(entity)?;
    let aliases = extract_aliases(entity, record);
    Some(serde_json::json!({
        "source": "semanticscholar",
        "kind": kind,
        "aliases": aliases,
        "attrs": record
    }))
}

/// Build EdgeInput JSON values for a slice of (citing_alias, cited_alias) pairs.
/// Each alias is already in `ns:value` form (e.g. `doi:10.x/y` or `s2:<paperId>`).
pub fn to_edges(pairs: &[(String, String)]) -> Vec<Value> {
    let fetched_at = crate::provider::rfc3339_now();
    pairs
        .iter()
        .map(|(citing, cited)| {
            let (src_ns, src_val) = split_alias(citing);
            let (dst_ns, dst_val) = split_alias(cited);
            serde_json::json!({
                "src": {"scheme": src_ns, "value": src_val},
                "dst": {"scheme": dst_ns, "value": dst_val},
                "relation": "cites",
                "source": "semanticscholar",
                "attrs": null,
                "fetched_at": fetched_at
            })
        })
        .collect()
}

/// Lower raw Semantic Scholar records and edge pairs into a fully-typed `Emission`.
/// S2 entities always have a NodeKind, so `skipped_unmappable` stays 0.
pub fn to_emission(records: &[(Entity, Value)], edge_pairs: &[(String, String)]) -> Emission {
    let mut work_records: Vec<WorkRecord> = Vec::new();
    let mut skipped = 0usize;

    for (entity, record) in records {
        match node_kind(*entity) {
            None => skipped += 1,
            Some(kind) => {
                let aliases_val = extract_aliases(*entity, record);
                let aliases: Vec<Alias> = aliases_val
                    .iter()
                    .map(|v| Alias {
                        scheme: v["scheme"].as_str().unwrap_or("").to_string(),
                        value: v["value"].as_str().unwrap_or("").to_string(),
                    })
                    .collect();
                work_records.push(WorkRecord {
                    source: "semanticscholar".to_string(),
                    kind: kind.to_string(),
                    aliases,
                    attrs: record.clone(),
                });
            }
        }
    }

    let fetched_at = crate::provider::rfc3339_now();
    let edges: Vec<EdgeInput> = edge_pairs
        .iter()
        .map(|(citing, cited)| {
            let (src_ns, src_val) = split_alias(citing);
            let (dst_ns, dst_val) = split_alias(cited);
            EdgeInput {
                src: Alias { scheme: src_ns.to_string(), value: src_val.to_string() },
                dst: Alias { scheme: dst_ns.to_string(), value: dst_val.to_string() },
                relation: "cites".to_string(),
                source: "semanticscholar".to_string(),
                attrs: serde_json::Value::Null,
                fetched_at: fetched_at.clone(),
            }
        })
        .collect();

    Emission { records: work_records, edges, skipped_unmappable: skipped }
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn alias(scheme: &str, value: &str) -> Value {
    serde_json::json!({"scheme": scheme, "value": value})
}

fn strip_doi_url(s: &str) -> &str {
    s.strip_prefix("https://doi.org/").unwrap_or(s)
}

fn strip_orcid_url(s: &str) -> &str {
    s.strip_prefix("https://orcid.org/").unwrap_or(s)
}

/// Split an `ns:value` alias on the first `:`.
/// Falls back to scheme "s2" if no colon is present.
fn split_alias(alias: &str) -> (&str, &str) {
    if let Some(pos) = alias.find(':') {
        (&alias[..pos], &alias[pos + 1..])
    } else {
        ("s2", alias)
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_kind_papers_to_work() {
        assert_eq!(node_kind(Entity::Papers), Some("Work"));
    }

    #[test]
    fn node_kind_authors_to_author() {
        assert_eq!(node_kind(Entity::Authors), Some("Author"));
    }

    #[test]
    fn aliases_paper_doi_bare_merge_guarantee() {
        let record = serde_json::json!({
            "paperId": "649def34f8be52c8b66281af98ae884c09aef38b",
            "externalIds": {
                "DOI": "10.7717/peerj.4375",
                "ArXiv": "2301.07041",
                "CorpusId": 4375000
            }
        });
        let aliases = extract_aliases(Entity::Papers, &record);
        let map: std::collections::HashMap<&str, &str> = aliases
            .iter()
            .map(|a| (a["scheme"].as_str().unwrap(), a["value"].as_str().unwrap()))
            .collect();
        // DOI merge guarantee: bare value, not URL-prefixed
        assert_eq!(map.get("doi"), Some(&"10.7717/peerj.4375"),
            "DOI must be bare (no https://doi.org/ prefix) for merge with OpenAlex");
        assert_eq!(map.get("s2"), Some(&"649def34f8be52c8b66281af98ae884c09aef38b"));
        assert_eq!(map.get("arxiv"), Some(&"2301.07041"));
        assert_eq!(map.get("corpusid"), Some(&"4375000"));
    }

    #[test]
    fn aliases_paper_doi_url_stripped() {
        let record = serde_json::json!({
            "paperId": "abc123",
            "externalIds": {
                "DOI": "https://doi.org/10.1000/test"
            }
        });
        let aliases = extract_aliases(Entity::Papers, &record);
        let map: std::collections::HashMap<&str, &str> = aliases
            .iter()
            .map(|a| (a["scheme"].as_str().unwrap(), a["value"].as_str().unwrap()))
            .collect();
        assert_eq!(map.get("doi"), Some(&"10.1000/test"),
            "https://doi.org/ prefix must be stripped from DOI");
    }

    #[test]
    fn aliases_author_with_orcid() {
        let record = serde_json::json!({
            "authorId": "1741101",
            "externalIds": {
                "ORCID": "0000-0001-6187-6610"
            }
        });
        let aliases = extract_aliases(Entity::Authors, &record);
        let map: std::collections::HashMap<&str, &str> = aliases
            .iter()
            .map(|a| (a["scheme"].as_str().unwrap(), a["value"].as_str().unwrap()))
            .collect();
        assert_eq!(map.get("s2author"), Some(&"1741101"));
        assert_eq!(map.get("orcid"), Some(&"0000-0001-6187-6610"));
    }

    #[test]
    fn aliases_author_orcid_url_stripped() {
        let record = serde_json::json!({
            "authorId": "1741101",
            "externalIds": {
                "ORCID": "https://orcid.org/0000-0001-6187-6610"
            }
        });
        let aliases = extract_aliases(Entity::Authors, &record);
        let map: std::collections::HashMap<&str, &str> = aliases
            .iter()
            .map(|a| (a["scheme"].as_str().unwrap(), a["value"].as_str().unwrap()))
            .collect();
        assert_eq!(map.get("orcid"), Some(&"0000-0001-6187-6610"),
            "https://orcid.org/ prefix must be stripped");
    }

    #[test]
    fn to_work_record_paper_shape() {
        let record = serde_json::json!({
            "paperId": "649def34f8be52c8b66281af98ae884c09aef38b",
            "externalIds": {"DOI": "10.7717/peerj.4375"},
            "title": "Test"
        });
        let wr = to_work_record(Entity::Papers, &record).unwrap();
        assert_eq!(wr["source"].as_str(), Some("semanticscholar"));
        assert_eq!(wr["kind"].as_str(), Some("Work"));
        let aliases = wr["aliases"].as_array().unwrap();
        assert!(!aliases.is_empty());
    }

    #[test]
    fn to_work_record_author_shape() {
        let record = serde_json::json!({
            "authorId": "1741101",
            "name": "Test Author"
        });
        let wr = to_work_record(Entity::Authors, &record).unwrap();
        assert_eq!(wr["source"].as_str(), Some("semanticscholar"));
        assert_eq!(wr["kind"].as_str(), Some("Author"));
    }

    #[test]
    fn to_edges_shape_and_source() {
        let pairs = vec![
            ("doi:10.1000/citing".to_string(), "s2:649def34f8be52c8b66281af98ae884c09aef38b".to_string()),
        ];
        let edges = to_edges(&pairs);
        assert_eq!(edges.len(), 1);
        let e = &edges[0];
        assert_eq!(e["src"]["scheme"].as_str(), Some("doi"));
        assert_eq!(e["src"]["value"].as_str(), Some("10.1000/citing"));
        assert_eq!(e["dst"]["scheme"].as_str(), Some("s2"));
        assert_eq!(e["dst"]["value"].as_str(), Some("649def34f8be52c8b66281af98ae884c09aef38b"));
        assert_eq!(e["relation"].as_str(), Some("cites"));
        assert_eq!(e["source"].as_str(), Some("semanticscholar"));
        assert!(e["fetched_at"].as_str().is_some());
    }

    #[test]
    fn aliases_paper_corpusid_numeric() {
        let record = serde_json::json!({
            "paperId": "abc",
            "externalIds": {"CorpusId": 99999u64}
        });
        let aliases = extract_aliases(Entity::Papers, &record);
        let map: std::collections::HashMap<&str, &str> = aliases
            .iter()
            .map(|a| (a["scheme"].as_str().unwrap(), a["value"].as_str().unwrap()))
            .collect();
        assert_eq!(map.get("corpusid"), Some(&"99999"));
    }

    #[test]
    fn to_emission_edge_shape() {
        let pairs = vec![
            ("doi:10.1000/a".to_string(), "s2:abc123".to_string()),
        ];
        let em = to_emission(&[], &pairs);
        assert_eq!(em.edges.len(), 1);
        assert_eq!(em.edges[0].src.scheme, "doi");
        assert_eq!(em.edges[0].src.value, "10.1000/a");
        assert_eq!(em.edges[0].dst.scheme, "s2");
        assert_eq!(em.edges[0].relation, "cites");
        assert_eq!(em.edges[0].source, "semanticscholar");
        assert!(!em.edges[0].fetched_at.is_empty());
    }
}
