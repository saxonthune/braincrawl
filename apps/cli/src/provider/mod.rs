use serde::{Deserialize, Serialize};

use crate::cli::OutputOpts;
use crate::output::Envelope;

/// The union of every provider verb. Individual providers support a subset and
/// return `ProviderError::UnsupportedVerb` for the rest. This is the input half of
/// the (future) subprocess plugin protocol, hence Serialize/Deserialize.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
pub enum ProviderCmd {
    Get { id: String },
    Search { entity: Option<String>, query: String },
    Find { entity: String, filters: Vec<String> },
    Autocomplete { entity: String, q: String },
    CitedBy { id: String },
    Refs { id: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("provider '{provider}' does not support verb '{verb}'")]
    UnsupportedVerb { provider: &'static str, verb: &'static str },
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

pub trait Provider {
    fn name(&self) -> &'static str;
    fn dispatch(
        &self,
        cmd: ProviderCmd,
        opts: &OutputOpts,
    ) -> std::result::Result<(Envelope, Emission), ProviderError>;
}

/// A store-ready work record: already lowered to the catalog's neutral vocabulary.
/// Mirrors exactly what `StoreClient::put_work` accepts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkRecord {
    pub source: String,
    pub kind: String,
    pub aliases: Vec<Alias>,
    pub attrs: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alias {
    pub namespace: String,
    pub value: String,
}

/// A store-ready citation edge. Mirrors what `StoreClient::put_edges` accepts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeInput {
    pub src: Alias,
    pub dst: Alias,
    pub relation: String,
    pub source: String,
    pub attrs: serde_json::Value,
    pub fetched_at: String,
}

/// Everything a provider verb produces for the store, fully lowered.
/// This is the output half of the (future) subprocess plugin protocol.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Emission {
    pub records: Vec<WorkRecord>,
    pub edges: Vec<EdgeInput>,
    pub skipped_unmappable: usize,
}

impl Emission {
    pub fn empty() -> Self { Self::default() }
}

/// Counts from a push operation; reported to stderr.
pub struct PushSummary {
    pub nodes_pushed: usize,
    pub edges_pushed: u64,
    pub skipped_unmappable: usize,
    pub errors: Vec<String>,
}

/// Format the current time as an RFC3339 UTC timestamp without external deps.
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
    use serde_json::json;

    #[test]
    fn work_record_serialized_shape() {
        let wr = WorkRecord {
            source: "openalex".to_string(),
            kind: "Work".to_string(),
            aliases: vec![
                Alias { namespace: "openalex".to_string(), value: "W123".to_string() },
                Alias { namespace: "doi".to_string(), value: "10.1000/test".to_string() },
            ],
            attrs: json!({"title": "Test paper"}),
        };
        let v = serde_json::to_value(&wr).unwrap();
        let expected = json!({
            "source": "openalex",
            "kind": "Work",
            "aliases": [
                {"namespace": "openalex", "value": "W123"},
                {"namespace": "doi", "value": "10.1000/test"}
            ],
            "attrs": {"title": "Test paper"}
        });
        assert_eq!(v, expected);
    }

    #[test]
    fn edge_input_serialized_shape() {
        let e = EdgeInput {
            src: Alias { namespace: "openalex".to_string(), value: "W111".to_string() },
            dst: Alias { namespace: "openalex".to_string(), value: "W222".to_string() },
            relation: "cites".to_string(),
            source: "openalex".to_string(),
            attrs: serde_json::Value::Null,
            fetched_at: "2026-01-01T00:00:00Z".to_string(),
        };
        let v = serde_json::to_value(&e).unwrap();
        let expected = json!({
            "src": {"namespace": "openalex", "value": "W111"},
            "dst": {"namespace": "openalex", "value": "W222"},
            "relation": "cites",
            "source": "openalex",
            "attrs": null,
            "fetched_at": "2026-01-01T00:00:00Z"
        });
        assert_eq!(v, expected);
    }

    #[test]
    fn emission_serde_round_trip() {
        let em = Emission {
            records: vec![WorkRecord {
                source: "semanticscholar".to_string(),
                kind: "Work".to_string(),
                aliases: vec![Alias { namespace: "s2".to_string(), value: "abc123".to_string() }],
                attrs: serde_json::Value::Null,
            }],
            edges: vec![],
            skipped_unmappable: 2,
        };
        let json_str = serde_json::to_string(&em).unwrap();
        let restored: Emission = serde_json::from_str(&json_str).unwrap();
        assert_eq!(restored.records.len(), 1);
        assert_eq!(restored.records[0].source, "semanticscholar");
        assert_eq!(restored.skipped_unmappable, 2);
    }

    #[test]
    fn rfc3339_now_format() {
        let ts = rfc3339_now();
        assert!(ts.ends_with('Z'), "timestamp must end with Z: {ts}");
        assert_eq!(ts.len(), 20, "expected YYYY-MM-DDTHH:MM:SSZ length: {ts}");
    }

    #[test]
    fn days_to_ymd_epoch() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
        assert_eq!(days_to_ymd(365), (1971, 1, 1));
        assert_eq!(days_to_ymd(365 + 365), (1972, 1, 1)); // 1972 is leap
        assert_eq!(days_to_ymd(365 + 365 + 366), (1973, 1, 1));
    }

    #[test]
    fn provider_cmd_get_tagged_shape() {
        let cmd = ProviderCmd::Get { id: "W123".to_string() };
        let v = serde_json::to_value(&cmd).unwrap();
        assert_eq!(v, json!({"verb": "get", "id": "W123"}));
        let rt: ProviderCmd = serde_json::from_value(v).unwrap();
        assert!(matches!(rt, ProviderCmd::Get { id } if id == "W123"));
    }

    #[test]
    fn provider_cmd_search_with_entity() {
        let cmd = ProviderCmd::Search {
            entity: Some("works".to_string()),
            query: "salinization".to_string(),
        };
        let v = serde_json::to_value(&cmd).unwrap();
        assert_eq!(v, json!({"verb": "search", "entity": "works", "query": "salinization"}));
        let rt: ProviderCmd = serde_json::from_value(v).unwrap();
        assert!(matches!(rt, ProviderCmd::Search { entity: Some(e), query: q } if e == "works" && q == "salinization"));
    }

    #[test]
    fn provider_cmd_search_entity_none() {
        let cmd = ProviderCmd::Search { entity: None, query: "arxiv".to_string() };
        let v = serde_json::to_value(&cmd).unwrap();
        assert_eq!(v, json!({"verb": "search", "entity": null, "query": "arxiv"}));
        let rt: ProviderCmd = serde_json::from_value(v).unwrap();
        assert!(matches!(rt, ProviderCmd::Search { entity: None, .. }));
    }

    #[test]
    fn provider_cmd_find_round_trip() {
        let cmd = ProviderCmd::Find {
            entity: "authors".to_string(),
            filters: vec!["country:US".to_string()],
        };
        let s = serde_json::to_string(&cmd).unwrap();
        let rt: ProviderCmd = serde_json::from_str(&s).unwrap();
        assert!(matches!(rt, ProviderCmd::Find { entity, .. } if entity == "authors"));
    }

    #[test]
    fn provider_cmd_verb_tags() {
        let cases: &[(ProviderCmd, &str)] = &[
            (ProviderCmd::CitedBy { id: "x".to_string() }, "cited_by"),
            (ProviderCmd::Refs { id: "x".to_string() }, "refs"),
            (ProviderCmd::Autocomplete { entity: "works".to_string(), q: "sal".to_string() }, "autocomplete"),
        ];
        for (cmd, expected_verb) in cases {
            let v = serde_json::to_value(cmd).unwrap();
            assert_eq!(v["verb"].as_str(), Some(*expected_verb), "wrong tag for {:?}", cmd);
        }
    }
}
