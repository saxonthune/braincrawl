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
            println!("{}", render_text_line(r, &opts.fields));
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

/// Render a single result as one tab-separated text line.
/// When `fields` is set, prints exactly those fields in order (no id prepended).
/// When `fields` is empty, prints `id<TAB>display` where display is the first of
/// title/display_name/name.
///
/// The id column is the work's identity per the glossary (`doc01.01`): a store row
/// (`WorkView`) carries it as `canonical_id`, so that is resolved first — a provider
/// id is never the identity. See `resolve_field`.
fn render_text_line(r: &serde_json::Value, fields: &[String]) -> String {
    if fields.is_empty() {
        let id = resolve_field(r, "id")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        let display = pick_display(r)
            .or_else(|| r.get("attrs").and_then(pick_display))
            .unwrap_or("");
        format!("{id}\t{}", sanitize_text_value(display))
    } else {
        fields
            .iter()
            .map(|f| match resolve_field(r, f) {
                None | Some(serde_json::Value::Null) => String::new(),
                Some(serde_json::Value::String(s)) => sanitize_text_value(s),
                Some(other) => sanitize_text_value(&other.to_string()),
            })
            .collect::<Vec<_>>()
            .join("\t")
    }
}

/// Resolve a projection field on a result row, honoring the identity model in the
/// glossary (`doc01.01`). Two row shapes reach here: a store row (`WorkView`), whose
/// identity is `canonical_id` and whose title lives under `attrs`; and a provider row,
/// which carries provider-native top-level keys (a provider row has no canonical id —
/// it is not yet in the store). `canonical_id` and `id` both name the work's identity,
/// resolved as `canonical_id` first so a store row never reports a provider id as its
/// identity; `title` falls back to `attrs.title`. Any other name is a verbatim
/// top-level lookup, which is what keeps provider-native fields projecting unchanged.
fn resolve_field<'a>(r: &'a serde_json::Value, field: &str) -> Option<&'a serde_json::Value> {
    match field {
        "canonical_id" | "id" => r.get("canonical_id").or_else(|| r.get("id")),
        "title" => r
            .get("title")
            .or_else(|| r.get("attrs").and_then(|a| a.get("title"))),
        other => r.get(other),
    }
}

/// First human-readable label on an object, in title/display_name/name order.
fn pick_display(v: &serde_json::Value) -> Option<&str> {
    v.get("title")
        .or_else(|| v.get("display_name"))
        .or_else(|| v.get("name"))
        .and_then(|v| v.as_str())
}

/// Flatten a field value to a single row: replace tab/newline/carriage-return
/// with a single space so each result stays exactly one line.
fn sanitize_text_value(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

fn project(v: &serde_json::Value, fields: &[String]) -> serde_json::Value {
    if v.as_object().is_none() {
        return v.clone();
    }
    let mut out = serde_json::Map::new();
    for f in fields {
        if let Some(val) = resolve_field(v, f) {
            out.insert(f.clone(), val.clone());
        }
    }
    serde_json::Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(s: &[&str]) -> Vec<String> {
        s.iter().map(|f| f.to_string()).collect()
    }

    #[test]
    fn text_fields_projection_in_order_no_id_prepended() {
        let r = serde_json::json!({
            "id": "W123",
            "display_name": "A Title",
            "cited_by_count": 42,
        });
        let line = render_text_line(&r, &fields(&["display_name", "cited_by_count"]));
        assert_eq!(line, "A Title\t42");
    }

    #[test]
    fn text_field_value_with_newline_is_flattened() {
        let r = serde_json::json!({
            "abstract": "line one\nline two\twith tab\r\nend",
        });
        let line = render_text_line(&r, &fields(&["abstract"]));
        assert_eq!(line, "line one line two with tab  end");
        assert!(!line.contains('\n'));
        assert!(!line.contains('\t'));
        assert!(!line.contains('\r'));
    }

    #[test]
    fn text_no_fields_prints_id_tab_display() {
        let r = serde_json::json!({
            "id": "W123",
            "title": "Some Work",
        });
        let line = render_text_line(&r, &[]);
        assert_eq!(line, "W123\tSome Work");
    }

    #[test]
    fn store_row_fields_resolve_canonical_id_and_nested_title() {
        // A WorkView-shaped row: identity is `canonical_id`, title lives under `attrs`.
        let r = serde_json::json!({
            "canonical_id": "018f-uuid",
            "attrs": { "title": "Salinization of Mesopotamia" },
        });
        // `id` and `canonical_id` both name the identity; neither blanks.
        let line = render_text_line(&r, &fields(&["id", "title"]));
        assert_eq!(line, "018f-uuid\tSalinization of Mesopotamia");
        let line2 = render_text_line(&r, &fields(&["canonical_id", "title"]));
        assert_eq!(line2, "018f-uuid\tSalinization of Mesopotamia");
    }

    #[test]
    fn store_row_json_projection_resolves_identity_and_title() {
        let r = serde_json::json!({
            "canonical_id": "018f-uuid",
            "attrs": { "title": "Salinization of Mesopotamia" },
        });
        let projected = project(&r, &fields(&["id", "title"]));
        assert_eq!(
            projected,
            serde_json::json!({ "id": "018f-uuid", "title": "Salinization of Mesopotamia" })
        );
    }

    #[test]
    fn store_row_no_fields_prints_canonical_id_not_dash() {
        let r = serde_json::json!({
            "canonical_id": "018f-uuid",
            "attrs": { "title": "A Work" },
        });
        let line = render_text_line(&r, &[]);
        assert_eq!(line, "018f-uuid\tA Work");
    }

    #[test]
    fn provider_row_id_still_projects_when_no_canonical_id() {
        // A provider row has no canonical id; `id` resolves to its provider-native value.
        let r = serde_json::json!({ "id": "https://openalex.org/W123", "title": "T" });
        let line = render_text_line(&r, &fields(&["id"]));
        assert_eq!(line, "https://openalex.org/W123");
    }

    #[test]
    fn text_fields_missing_renders_empty_and_objects_compact_json() {
        let r = serde_json::json!({
            "display_name": "T",
            "nested": { "a": 1 },
        });
        let line = render_text_line(&r, &fields(&["display_name", "missing", "nested"]));
        assert_eq!(line, "T\t\t{\"a\":1}");
    }
}
