use crate::provider::Alias;

/// Parse a CLI-supplied work id into its alias namespace and value.
/// Returns None when the string names no recognizable scheme.
pub fn parse(id: &str) -> Option<Alias> {
    const DOI_URL_PREFIXES: &[&str] = &[
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
        "http://dx.doi.org/",
    ];
    for prefix in DOI_URL_PREFIXES {
        if let Some(rest) = id.strip_prefix(prefix) {
            return Some(Alias { namespace: "doi".to_string(), value: rest.to_string() });
        }
    }

    if let Some((ns, value)) = id.split_once(':') {
        return Some(Alias { namespace: ns.to_lowercase(), value: value.to_string() });
    }

    if id.starts_with("10.") && id.contains('/') {
        return Some(Alias { namespace: "doi".to_string(), value: id.to_string() });
    }

    None
}

/// Pull the value of one alias namespace out of a store work record.
/// The store returns aliases as [{namespace, value}] objects.
pub fn alias_of(work: &serde_json::Value, ns: &str) -> Option<String> {
    let aliases = work.get("aliases").and_then(|a| a.as_array())?;
    for alias in aliases {
        if alias.get("namespace").and_then(|n| n.as_str()) == Some(ns) {
            if let Some(v) = alias.get("value").and_then(|v| v.as_str()) {
                return Some(v.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_doi_url_https() {
        let a = parse("https://doi.org/10.1000/test").unwrap();
        assert_eq!(a.namespace, "doi");
        assert_eq!(a.value, "10.1000/test");
    }

    #[test]
    fn parse_doi_url_http() {
        let a = parse("http://doi.org/10.1000/test").unwrap();
        assert_eq!(a.namespace, "doi");
        assert_eq!(a.value, "10.1000/test");
    }

    #[test]
    fn parse_doi_url_dx_https() {
        let a = parse("https://dx.doi.org/10.1000/test").unwrap();
        assert_eq!(a.namespace, "doi");
        assert_eq!(a.value, "10.1000/test");
    }

    #[test]
    fn parse_doi_url_dx_http() {
        let a = parse("http://dx.doi.org/10.1000/test").unwrap();
        assert_eq!(a.namespace, "doi");
        assert_eq!(a.value, "10.1000/test");
    }

    #[test]
    fn parse_generic_ns_value() {
        let a = parse("openalex:W123").unwrap();
        assert_eq!(a.namespace, "openalex");
        assert_eq!(a.value, "W123");
    }

    #[test]
    fn parse_generic_doi_colon() {
        let a = parse("doi:10.1000/test").unwrap();
        assert_eq!(a.namespace, "doi");
        assert_eq!(a.value, "10.1000/test");
    }

    #[test]
    fn parse_bare_doi() {
        let a = parse("10.1000/test").unwrap();
        assert_eq!(a.namespace, "doi");
        assert_eq!(a.value, "10.1000/test");
    }

    #[test]
    fn parse_unrecognized_returns_none() {
        assert!(parse("unknown").is_none());
    }

    #[test]
    fn parse_preserves_case() {
        let a = parse("doi:10.1000/ABC-XYZ").unwrap();
        assert_eq!(a.value, "10.1000/ABC-XYZ");
    }

    #[test]
    fn alias_of_present() {
        let work = serde_json::json!({
            "aliases": [
                {"namespace": "openalex", "value": "W123"},
                {"namespace": "doi", "value": "10.1000/test"},
                {"namespace": "mag", "value": "999"}
            ]
        });
        assert_eq!(alias_of(&work, "doi"), Some("10.1000/test".to_string()));
    }

    #[test]
    fn alias_of_absent_returns_none() {
        let work = serde_json::json!({
            "aliases": [
                {"namespace": "openalex", "value": "W123"},
                {"namespace": "mag", "value": "999"}
            ]
        });
        assert_eq!(alias_of(&work, "doi"), None);
    }

    #[test]
    fn alias_of_empty_aliases() {
        let work = serde_json::json!({"aliases": []});
        assert_eq!(alias_of(&work, "doi"), None);
    }
}
