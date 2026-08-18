use serde_json::Value;

use crate::provider::{Alias, Emission, WorkRecord};

pub fn node_kind() -> &'static str {
    "Work"
}

/// Extract aliases from a parsed arXiv record.
/// Always emits `arxiv:<bare-id>`. Emits `doi:<bare>` when the record has a DOI.
pub fn extract_aliases(record: &Value) -> Vec<Alias> {
    let mut aliases = Vec::new();

    if let Some(id) = record["arxiv_id"].as_str() {
        aliases.push(Alias { scheme: "arxiv".to_string(), value: id.to_string() });
    }
    if let Some(doi) = record["doi"].as_str() {
        if !doi.is_empty() {
            aliases.push(Alias { scheme: "doi".to_string(), value: doi.to_string() });
        }
    }

    aliases
}

/// Lower a slice of parsed arXiv records into a fully-typed `Emission`.
/// Edges are always empty for arXiv (no citation graph).
pub fn to_emission(records: &[Value]) -> Emission {
    let work_records: Vec<WorkRecord> = records
        .iter()
        .map(|r| WorkRecord {
            source: "arxiv".to_string(),
            kind: node_kind().to_string(),
            aliases: extract_aliases(r),
            attrs: r.clone(),
        })
        .collect();

    Emission {
        records: work_records,
        edges: vec![],
        skipped_unmappable: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn aliases_with_doi() {
        let record = json!({
            "arxiv_id": "2301.07041",
            "title": "Test",
            "doi": "10.18653/v1/2023.acl-long.108"
        });
        let aliases = extract_aliases(&record);
        let map: std::collections::HashMap<&str, &str> = aliases
            .iter()
            .map(|a| (a.scheme.as_str(), a.value.as_str()))
            .collect();
        assert_eq!(map.get("arxiv"), Some(&"2301.07041"));
        assert_eq!(map.get("doi"), Some(&"10.18653/v1/2023.acl-long.108"));
    }

    #[test]
    fn aliases_without_doi() {
        let record = json!({
            "arxiv_id": "2206.06336",
            "title": "Test"
        });
        let aliases = extract_aliases(&record);
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].scheme, "arxiv");
        assert_eq!(aliases[0].value, "2206.06336");
    }

    #[test]
    fn to_emission_edges_always_empty() {
        let records = vec![
            json!({"arxiv_id": "2301.07041", "doi": "10.1/test"}),
            json!({"arxiv_id": "2206.06336"}),
        ];
        let em = to_emission(&records);
        assert_eq!(em.records.len(), 2);
        assert!(em.edges.is_empty());
        assert_eq!(em.skipped_unmappable, 0);
    }

    #[test]
    fn to_emission_source_and_kind() {
        let records = vec![json!({"arxiv_id": "2301.07041"})];
        let em = to_emission(&records);
        assert_eq!(em.records[0].source, "arxiv");
        assert_eq!(em.records[0].kind, "Work");
    }

    #[test]
    fn fixture_entry_with_doi_has_both_aliases() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/arxiv_search.xml");
        let xml = std::fs::read_to_string(&path).unwrap();
        let (entries, _) = crate::arxiv::parse::parse_feed(&xml).unwrap();
        // entry 0 has DOI
        let aliases = extract_aliases(&entries[0]);
        let map: std::collections::HashMap<&str, &str> = aliases
            .iter()
            .map(|a| (a.scheme.as_str(), a.value.as_str()))
            .collect();
        assert!(map.contains_key("arxiv"), "must have arxiv alias");
        assert!(map.contains_key("doi"), "must have doi alias when DOI present");
        // entry 1 has no DOI
        let aliases2 = extract_aliases(&entries[1]);
        assert_eq!(aliases2.len(), 1);
        assert_eq!(aliases2[0].scheme, "arxiv");
    }
}
