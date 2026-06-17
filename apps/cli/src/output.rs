use serde::Serialize;

use crate::cli::OutputOpts;

#[derive(Serialize, Default)]
pub struct QueryMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Serialize)]
pub struct Envelope {
    pub query: QueryMeta,
    pub count: u64,
    pub returned: usize,
    pub truncated: bool,
    pub next_cursor: Option<String>,
    pub results: Vec<serde_json::Value>,
}

/// Render an envelope to stdout according to the output options.
/// `--json` (default): pretty-printed JSON envelope.
/// `--text`: one compact line per result (id + display field).
/// `--fields`: projects top-level keys before rendering.
pub fn render(envelope: &Envelope, opts: &OutputOpts) {
    let results: Vec<serde_json::Value> = if opts.fields.is_empty() {
        envelope.results.clone()
    } else {
        envelope.results.iter().map(|v| project(v, &opts.fields)).collect()
    };

    if opts.text && !opts.json {
        for r in &results {
            let id = r.get("id").and_then(|v| v.as_str()).unwrap_or("-");
            let display = r
                .get("title")
                .or_else(|| r.get("display_name"))
                .or_else(|| r.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            println!("{id}\t{display}");
        }
    } else {
        let out = serde_json::json!({
            "query": envelope.query,
            "count": envelope.count,
            "returned": results.len(),
            "truncated": envelope.truncated,
            "next_cursor": envelope.next_cursor,
            "results": results,
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    }
}

fn project(v: &serde_json::Value, fields: &[String]) -> serde_json::Value {
    let Some(obj) = v.as_object() else {
        return v.clone();
    };
    let mut out = serde_json::Map::new();
    for f in fields {
        if let Some(val) = obj.get(f) {
            out.insert(f.clone(), val.clone());
        }
    }
    serde_json::Value::Object(out)
}
