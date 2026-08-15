use serde_json::Value;

use crate::provider::{Alias, EdgeInput, Emission};

/// Lower a set of cited DOIs into a fully-typed `Emission`. Crossref has no
/// work-record shape to emit, only citation edges.
pub fn to_emission(citing_doi: &str, cited_dois: &[String], skipped: usize) -> Emission {
    let fetched_at = crate::provider::rfc3339_now();
    let edges: Vec<EdgeInput> = cited_dois
        .iter()
        .map(|cited| EdgeInput {
            src: Alias { namespace: "doi".to_string(), value: citing_doi.to_string() },
            dst: Alias { namespace: "doi".to_string(), value: cited.clone() },
            relation: "cites".to_string(),
            source: "crossref".to_string(),
            attrs: Value::Null,
            fetched_at: fetched_at.clone(),
        })
        .collect();

    Emission {
        records: vec![],
        edges,
        skipped_unmappable: skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_emission_edge_shape() {
        let em = to_emission("10.1000/citing", &["10.2000/cited".to_string()], 0);
        assert_eq!(em.records.len(), 0);
        assert_eq!(em.edges.len(), 1);
        let e = &em.edges[0];
        assert_eq!(e.src.namespace, "doi");
        assert_eq!(e.src.value, "10.1000/citing");
        assert_eq!(e.dst.namespace, "doi");
        assert_eq!(e.dst.value, "10.2000/cited");
        assert_eq!(e.relation, "cites");
        assert_eq!(e.source, "crossref");
        assert_eq!(e.attrs, Value::Null);
        assert!(!e.fetched_at.is_empty());
    }

    #[test]
    fn to_emission_multiple_cited() {
        let cited = vec!["10.1/a".to_string(), "10.2/b".to_string(), "10.3/c".to_string()];
        let em = to_emission("10.0/src", &cited, 0);
        assert_eq!(em.edges.len(), 3);
        for e in &em.edges {
            assert_eq!(e.src.value, "10.0/src");
            assert_eq!(e.relation, "cites");
        }
        assert_eq!(em.edges[0].dst.value, "10.1/a");
        assert_eq!(em.edges[1].dst.value, "10.2/b");
        assert_eq!(em.edges[2].dst.value, "10.3/c");
    }

    #[test]
    fn to_emission_skipped_unmappable_carries_through() {
        let em = to_emission("10.0/src", &[], 4);
        assert!(em.edges.is_empty());
        assert_eq!(em.skipped_unmappable, 4);
    }
}
