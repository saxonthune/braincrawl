use serde_json::Value;

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
    let fetched_at = rfc3339_now();
    pairs
        .iter()
        .map(|(citing, cited)| {
            let (src_ns, src_val) = split_alias(citing);
            let (dst_ns, dst_val) = split_alias(cited);
            serde_json::json!({
                "src": {"namespace": src_ns, "value": src_val},
                "dst": {"namespace": dst_ns, "value": dst_val},
                "relation": "cites",
                "source": "semanticscholar",
                "attrs": null,
                "fetched_at": fetched_at
            })
        })
        .collect()
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn alias(namespace: &str, value: &str) -> Value {
    serde_json::json!({"namespace": namespace, "value": value})
}

fn strip_doi_url(s: &str) -> &str {
    s.strip_prefix("https://doi.org/").unwrap_or(s)
}

fn strip_orcid_url(s: &str) -> &str {
    s.strip_prefix("https://orcid.org/").unwrap_or(s)
}

/// Split an `ns:value` alias on the first `:`.
/// Falls back to namespace "s2" if no colon is present.
fn split_alias(alias: &str) -> (&str, &str) {
    if let Some(pos) = alias.find(':') {
        (&alias[..pos], &alias[pos + 1..])
    } else {
        ("s2", alias)
    }
}

/// Format the current time as an RFC3339 UTC timestamp without external deps.
fn rfc3339_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    let (year, month, day) = days_to_ymd(days);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let diy = if is_leap(year) { 366 } else { 365 };
        if days < diy {
            break;
        }
        days -= diy;
        year += 1;
    }
    let dims: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u64;
    for &dim in &dims {
        if days < dim {
            break;
        }
        days -= dim;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
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
            .map(|a| (a["namespace"].as_str().unwrap(), a["value"].as_str().unwrap()))
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
            .map(|a| (a["namespace"].as_str().unwrap(), a["value"].as_str().unwrap()))
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
            .map(|a| (a["namespace"].as_str().unwrap(), a["value"].as_str().unwrap()))
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
            .map(|a| (a["namespace"].as_str().unwrap(), a["value"].as_str().unwrap()))
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
        assert_eq!(e["src"]["namespace"].as_str(), Some("doi"));
        assert_eq!(e["src"]["value"].as_str(), Some("10.1000/citing"));
        assert_eq!(e["dst"]["namespace"].as_str(), Some("s2"));
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
            .map(|a| (a["namespace"].as_str().unwrap(), a["value"].as_str().unwrap()))
            .collect();
        assert_eq!(map.get("corpusid"), Some(&"99999"));
    }
}
