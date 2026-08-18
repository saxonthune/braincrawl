use serde_json::Value;

use crate::provider::{Alias, EdgeInput, Emission, WorkRecord};
use super::entity::Entity;

/// Map an OpenAlex entity type to the server's NodeKind string.
/// Returns `None` for entity types without a push target (institutions, publishers,
/// funders, keywords).
pub fn node_kind(entity: Entity) -> Option<&'static str> {
    match entity {
        Entity::Works => Some("Work"),
        Entity::Authors => Some("Author"),
        Entity::Sources => Some("Venue"),
        Entity::Topics => Some("Topic"),
        Entity::Concepts => Some("Concept"),
        Entity::Institutions | Entity::Publishers | Entity::Funders | Entity::Keywords => None,
    }
}

/// Extract structured aliases from an OpenAlex record.
/// Values are bare (URL prefixes stripped) to match how the server's `have`/`get` use `ns:value`.
pub fn extract_aliases(entity: Entity, record: &Value) -> Vec<Value> {
    let mut aliases: Vec<Value> = Vec::new();

    match entity {
        Entity::Works => {
            if let Some(id) = record.get("id").and_then(|v| v.as_str()) {
                aliases.push(alias("openalex", strip_openalex_url(id)));
            }
            if let Some(ids) = record.get("ids").and_then(|v| v.as_object()) {
                if let Some(doi) = ids.get("doi").and_then(|v| v.as_str()) {
                    aliases.push(alias("doi", strip_doi_url(doi)));
                }
                if let Some(pmid) = ids.get("pmid").and_then(|v| v.as_str()) {
                    aliases.push(alias("pmid", pmid));
                }
                if let Some(pmcid) = ids.get("pmcid").and_then(|v| v.as_str()) {
                    aliases.push(alias("pmcid", pmcid));
                }
                // mag is sometimes a number, sometimes a string
                if let Some(mag) = ids.get("mag") {
                    let mag_str = if let Some(s) = mag.as_str() {
                        Some(s.to_string())
                    } else {
                        mag.as_u64().map(|n| n.to_string())
                    };
                    if let Some(s) = mag_str {
                        aliases.push(alias("mag", &s));
                    }
                }
            }
        }
        Entity::Authors => {
            if let Some(id) = record.get("id").and_then(|v| v.as_str()) {
                aliases.push(alias("openalex", strip_openalex_url(id)));
            }
            if let Some(orcid) = record.get("orcid").and_then(|v| v.as_str()) {
                aliases.push(alias("orcid", strip_orcid_url(orcid)));
            }
        }
        Entity::Sources => {
            if let Some(id) = record.get("id").and_then(|v| v.as_str()) {
                aliases.push(alias("openalex", strip_openalex_url(id)));
            }
            if let Some(issn_l) = record.get("issn_l").and_then(|v| v.as_str()) {
                aliases.push(alias("issn_l", issn_l));
            }
            if let Some(issns) = record.get("issn").and_then(|v| v.as_array()) {
                for issn in issns {
                    if let Some(s) = issn.as_str() {
                        aliases.push(alias("issn", s));
                    }
                }
            }
        }
        Entity::Topics => {
            if let Some(id) = record.get("id").and_then(|v| v.as_str()) {
                aliases.push(alias("openalex", strip_openalex_url(id)));
            }
        }
        Entity::Concepts => {
            if let Some(id) = record.get("id").and_then(|v| v.as_str()) {
                aliases.push(alias("openalex", strip_openalex_url(id)));
            }
            if let Some(wikidata) = record.get("wikidata").and_then(|v| v.as_str()) {
                aliases.push(alias("wikidata", strip_wikidata_url(wikidata)));
            }
        }
        _ => {}
    }

    aliases
}

/// Build a WorkRecord JSON value for the given entity and (optionally trimmed) record.
/// Returns `None` if the entity has no push target.
pub fn to_work_record(entity: Entity, record: &Value) -> Option<Value> {
    let kind = node_kind(entity)?;
    let aliases = extract_aliases(entity, record);
    Some(serde_json::json!({
        "source": "openalex",
        "kind": kind,
        "aliases": aliases,
        "attrs": record
    }))
}

/// Build EdgeInput JSON values for a slice of (citing_id, cited_id) pairs.
/// IDs may be full OpenAlex URLs or bare IDs — URL prefixes are stripped.
pub fn to_edges(pairs: &[(String, String)]) -> Vec<Value> {
    let fetched_at = crate::provider::rfc3339_now();
    pairs
        .iter()
        .map(|(citing, cited)| {
            let citing_bare = strip_openalex_url(citing).to_string();
            let cited_bare = strip_openalex_url(cited).to_string();
            serde_json::json!({
                "src": {"scheme": "openalex", "value": citing_bare},
                "dst": {"scheme": "openalex", "value": cited_bare},
                "relation": "cites",
                "source": "openalex",
                "attrs": null,
                "fetched_at": fetched_at
            })
        })
        .collect()
}

/// Lower raw OpenAlex records and edge pairs into a fully-typed `Emission`.
/// Drops records whose entity type has no NodeKind and counts them as `skipped_unmappable`.
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
                    source: "openalex".to_string(),
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
            let citing_bare = strip_openalex_url(citing).to_string();
            let cited_bare = strip_openalex_url(cited).to_string();
            EdgeInput {
                src: Alias { scheme: "openalex".to_string(), value: citing_bare },
                dst: Alias { scheme: "openalex".to_string(), value: cited_bare },
                relation: "cites".to_string(),
                source: "openalex".to_string(),
                attrs: serde_json::Value::Null,
                fetched_at: fetched_at.clone(),
            }
        })
        .collect();

    Emission { records: work_records, edges, skipped_unmappable: skipped }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn alias(scheme: &str, value: &str) -> Value {
    serde_json::json!({"scheme": scheme, "value": value})
}

fn strip_openalex_url(s: &str) -> &str {
    s.strip_prefix("https://openalex.org/").unwrap_or(s)
}

fn strip_doi_url(s: &str) -> &str {
    s.strip_prefix("https://doi.org/").unwrap_or(s)
}

fn strip_orcid_url(s: &str) -> &str {
    s.strip_prefix("https://orcid.org/").unwrap_or(s)
}

fn strip_wikidata_url(s: &str) -> &str {
    s.strip_prefix("https://www.wikidata.org/wiki/")
        .or_else(|| s.strip_prefix("https://www.wikidata.org/entity/"))
        .unwrap_or(s)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_kind_mapping() {
        assert_eq!(node_kind(Entity::Works), Some("Work"));
        assert_eq!(node_kind(Entity::Authors), Some("Author"));
        assert_eq!(node_kind(Entity::Sources), Some("Venue"));
        assert_eq!(node_kind(Entity::Topics), Some("Topic"));
        assert_eq!(node_kind(Entity::Concepts), Some("Concept"));
        assert_eq!(node_kind(Entity::Institutions), None);
        assert_eq!(node_kind(Entity::Publishers), None);
        assert_eq!(node_kind(Entity::Funders), None);
        assert_eq!(node_kind(Entity::Keywords), None);
    }

    #[test]
    fn alias_url_normalization_work() {
        let record = serde_json::json!({
            "id": "https://openalex.org/W2741809807",
            "ids": {
                "doi": "https://doi.org/10.7717/peerj.4375",
                "pmid": "29456894",
                "mag": 2741809807u64
            }
        });
        let aliases = extract_aliases(Entity::Works, &record);
        let ns_vals: Vec<(&str, &str)> = aliases
            .iter()
            .map(|a| (a["scheme"].as_str().unwrap(), a["value"].as_str().unwrap()))
            .collect();
        assert!(ns_vals.contains(&("openalex", "W2741809807")));
        assert!(ns_vals.contains(&("doi", "10.7717/peerj.4375")));
        assert!(ns_vals.contains(&("pmid", "29456894")));
        assert!(ns_vals.contains(&("mag", "2741809807")));
    }

    #[test]
    fn alias_url_normalization_author() {
        let record = serde_json::json!({
            "id": "https://openalex.org/A5023888391",
            "orcid": "https://orcid.org/0000-0001-6187-6610"
        });
        let aliases = extract_aliases(Entity::Authors, &record);
        let ns_vals: Vec<(&str, &str)> = aliases
            .iter()
            .map(|a| (a["scheme"].as_str().unwrap(), a["value"].as_str().unwrap()))
            .collect();
        assert!(ns_vals.contains(&("openalex", "A5023888391")));
        assert!(ns_vals.contains(&("orcid", "0000-0001-6187-6610")));
    }

    #[test]
    fn institution_maps_to_none() {
        let record = serde_json::json!({"id": "https://openalex.org/I27837315"});
        assert!(to_work_record(Entity::Institutions, &record).is_none());
    }

    #[test]
    fn to_work_record_shape() {
        let record = serde_json::json!({
            "id": "https://openalex.org/W2741809807",
            "ids": {"doi": "https://doi.org/10.7717/peerj.4375"}
        });
        let wr = to_work_record(Entity::Works, &record).unwrap();
        assert_eq!(wr["source"].as_str(), Some("openalex"));
        assert_eq!(wr["kind"].as_str(), Some("Work"));
        let aliases = wr["aliases"].as_array().unwrap();
        assert!(!aliases.is_empty());
    }

    #[test]
    fn to_edges_strips_urls() {
        let pairs = vec![
            (
                "https://openalex.org/W111".to_string(),
                "https://openalex.org/W222".to_string(),
            ),
        ];
        let edges = to_edges(&pairs);
        assert_eq!(edges.len(), 1);
        let e = &edges[0];
        assert_eq!(e["src"]["scheme"].as_str(), Some("openalex"));
        assert_eq!(e["src"]["value"].as_str(), Some("W111"));
        assert_eq!(e["dst"]["value"].as_str(), Some("W222"));
        assert_eq!(e["relation"].as_str(), Some("cites"));
        assert_eq!(e["source"].as_str(), Some("openalex"));
        assert!(e["fetched_at"].as_str().is_some());
    }

    #[test]
    fn to_emission_skips_unmappable() {
        let record_work = serde_json::json!({"id": "https://openalex.org/W1"});
        let record_inst = serde_json::json!({"id": "https://openalex.org/I99"});
        let records = vec![
            (Entity::Works, record_work),
            (Entity::Institutions, record_inst),
        ];
        let em = to_emission(&records, &[]);
        assert_eq!(em.records.len(), 1);
        assert_eq!(em.skipped_unmappable, 1);
        assert_eq!(em.records[0].kind, "Work");
    }

    #[test]
    fn to_emission_edge_shape() {
        let pairs = vec![
            ("https://openalex.org/W111".to_string(), "https://openalex.org/W222".to_string()),
        ];
        let em = to_emission(&[], &pairs);
        assert_eq!(em.edges.len(), 1);
        assert_eq!(em.edges[0].src.scheme, "openalex");
        assert_eq!(em.edges[0].src.value, "W111");
        assert_eq!(em.edges[0].dst.value, "W222");
        assert_eq!(em.edges[0].relation, "cites");
        assert_eq!(em.edges[0].source, "openalex");
        assert!(!em.edges[0].fetched_at.is_empty());
    }
}
