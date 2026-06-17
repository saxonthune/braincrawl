use serde_json::Value;

/// Extract the DOI from a store work record's aliases array.
/// The store returns aliases as [{namespace, value}] objects.
/// Returns the bare DOI (no https://doi.org/ prefix) or None.
pub fn extract_doi_from_work(work: &Value) -> Option<String> {
    let aliases = work.get("aliases").and_then(|a| a.as_array())?;
    for alias in aliases {
        if alias.get("namespace").and_then(|n| n.as_str()) == Some("doi") {
            if let Some(v) = alias.get("value").and_then(|v| v.as_str()) {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Build doi-namespaced EdgeInput JSON values for a slice of cited DOIs.
/// Both src and dst use the "doi" namespace; `source` identifies the provider.
pub fn doi_edges(citing_doi: &str, cited_dois: &[String], source: &str) -> Vec<Value> {
    let fetched_at = rfc3339_now();
    cited_dois
        .iter()
        .map(|cited| {
            serde_json::json!({
                "src": {"namespace": "doi", "value": citing_doi},
                "dst": {"namespace": "doi", "value": cited},
                "relation": "cites",
                "source": source,
                "attrs": null,
                "fetched_at": fetched_at
            })
        })
        .collect()
}

pub fn rfc3339_now() -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_doi_present() {
        let work = serde_json::json!({
            "aliases": [
                {"namespace": "openalex", "value": "W123"},
                {"namespace": "doi", "value": "10.1000/test"},
                {"namespace": "mag", "value": "999"}
            ]
        });
        assert_eq!(extract_doi_from_work(&work), Some("10.1000/test".to_string()));
    }

    #[test]
    fn extract_doi_absent_returns_none() {
        let work = serde_json::json!({
            "aliases": [
                {"namespace": "openalex", "value": "W123"},
                {"namespace": "mag", "value": "999"}
            ]
        });
        assert_eq!(extract_doi_from_work(&work), None);
    }

    #[test]
    fn extract_doi_empty_aliases() {
        let work = serde_json::json!({"aliases": []});
        assert_eq!(extract_doi_from_work(&work), None);
    }

    #[test]
    fn doi_edges_shape_and_source_crossref() {
        let edges = doi_edges("10.1000/citing", &["10.2000/cited".to_string()], "crossref");
        assert_eq!(edges.len(), 1);
        let e = &edges[0];
        assert_eq!(e["src"]["namespace"].as_str(), Some("doi"));
        assert_eq!(e["src"]["value"].as_str(), Some("10.1000/citing"));
        assert_eq!(e["dst"]["namespace"].as_str(), Some("doi"));
        assert_eq!(e["dst"]["value"].as_str(), Some("10.2000/cited"));
        assert_eq!(e["relation"].as_str(), Some("cites"));
        assert_eq!(e["source"].as_str(), Some("crossref"));
        assert!(e["fetched_at"].as_str().is_some());
        assert_eq!(e["attrs"], serde_json::Value::Null);
    }

    #[test]
    fn doi_edges_source_opencitations() {
        let edges = doi_edges("10.1000/citing", &["10.2000/cited".to_string()], "opencitations");
        assert_eq!(edges[0]["source"].as_str(), Some("opencitations"));
    }

    #[test]
    fn doi_edges_multiple_cited() {
        let cited = vec!["10.1/a".to_string(), "10.2/b".to_string(), "10.3/c".to_string()];
        let edges = doi_edges("10.0/src", &cited, "crossref");
        assert_eq!(edges.len(), 3);
        for e in &edges {
            assert_eq!(e["src"]["value"].as_str(), Some("10.0/src"));
            assert_eq!(e["relation"].as_str(), Some("cites"));
        }
        assert_eq!(edges[0]["dst"]["value"].as_str(), Some("10.1/a"));
        assert_eq!(edges[1]["dst"]["value"].as_str(), Some("10.2/b"));
        assert_eq!(edges[2]["dst"]["value"].as_str(), Some("10.3/c"));
    }
}
