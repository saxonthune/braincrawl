//! The block grammar: `*.l3.md` → (`Graph`, `Vec<Warning>`).

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::model::{Endpoint, Graph, Link, Node, NodeId, Provenance, Warning};

pub(crate) const FILE_SUFFIX: &str = ".l3.md";

/// Read every `*.l3.md` under `root` (skipping `_`-prefixed files and `INDEX.md`)
/// and lift a `Graph` from their bodies. Never fails — malformed input is
/// reported as a `Warning` and parsing continues.
pub fn parse(root: &Path) -> (Graph, Vec<Warning>) {
    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let mut warnings = Vec::new();

    for path in find_docs(root) {
        parse_file(&path, &mut nodes, &mut links, &mut warnings);
    }

    (Graph::build(nodes, links), warnings)
}

/// The same grammar as `parse`, over doc text held in memory rather than on
/// disk. Each tuple is `(doc slug, full markdown text)`; `Provenance.path` for
/// these nodes is the synthetic `<slug>.l3.md`. Semantics are identical to
/// `parse` on a directory containing exactly these docs, modulo path.
pub fn parse_sources(sources: &[(String, String)]) -> (Graph, Vec<Warning>) {
    let mut ordered: Vec<&(String, String)> = sources.iter().collect();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));

    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let mut warnings = Vec::new();

    for (slug, content) in ordered {
        let path = PathBuf::from(format!("{slug}{FILE_SUFFIX}"));
        let (mut n, mut l, mut w) = parse_one(slug, path, content);
        nodes.append(&mut n);
        links.append(&mut l);
        warnings.append(&mut w);
    }

    (Graph::build(nodes, links), warnings)
}

/// Every `*.l3.md` file under `root`, recursively, sorted for deterministic order.
pub(crate) fn find_docs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_docs(root, &mut out);
    out.sort();
    out
}

fn collect_docs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('_') {
            continue;
        }
        if path.is_dir() {
            collect_docs(&path, out);
            continue;
        }
        if name == "INDEX.md" || !name.ends_with(FILE_SUFFIX) {
            continue;
        }
        out.push(path);
    }
}

fn parse_file(path: &Path, nodes: &mut Vec<Node>, links: &mut Vec<Link>, warnings: &mut Vec<Warning>) {
    let Ok(content) = fs::read_to_string(path) else { return };
    let stem = path
        .file_name()
        .map(|n| n.to_string_lossy().trim_end_matches(FILE_SUFFIX).to_string())
        .unwrap_or_default();
    let (mut n, mut l, mut w) = parse_one(&stem, path.to_path_buf(), &content);
    nodes.append(&mut n);
    links.append(&mut l);
    warnings.append(&mut w);
}

/// The per-doc parse core shared by `parse` (real files) and `parse_sources`
/// (in-memory docs). `doc_hint` names the doc when its frontmatter has no
/// `doc:` key (the file stem, or the in-memory slug); `path` is the
/// `Provenance.path` to record (real or synthetic).
pub(crate) fn parse_one(doc_hint: &str, path: PathBuf, content: &str) -> (Vec<Node>, Vec<Link>, Vec<Warning>) {
    let mut nodes = Vec::new();
    let mut links = Vec::new();
    let mut warnings = Vec::new();

    let (fm, body) = split_frontmatter(content);
    let doc = fm_get(&fm, "doc").unwrap_or_else(|| doc_hint.to_string());

    if fm.is_empty() {
        warnings.push(warning(&doc, &path, 0, "no YAML frontmatter block"));
    }

    // Frontmatter occupies lines 1..=fm.len()+2 (the two `---` fences), so body
    // line numbers are offset to point warnings at the real file line.
    let body_offset = if fm.is_empty() { 0 } else { fm.len() + 2 };

    parse_body(&doc, &path, &body, body_offset, &mut nodes, &mut links, &mut warnings);

    (nodes, links, warnings)
}

fn warning(doc: &str, path: &Path, line: usize, message: &str) -> Warning {
    Warning { doc: doc.to_string(), path: path.to_path_buf(), line, message: message.to_string() }
}

// ── body grammar ──────────────────────────────────────────────────────────────

/// A node under construction, before it is finalized into a `Node`.
struct OpenNode {
    id: Option<NodeId>,
    title: String,
    labels: Vec<String>,
    properties: Map<String, Value>,
    heading_line: usize,
}

/// One buffered bullet: raw text (continuations folded in) plus its starting line.
struct Bullet {
    text: String,
    line: usize,
}

fn parse_body(
    doc: &str,
    path: &Path,
    body: &str,
    body_offset: usize,
    nodes: &mut Vec<Node>,
    links: &mut Vec<Link>,
    warnings: &mut Vec<Warning>,
) {
    let mut order = 0usize;
    let mut current: Option<OpenNode> = None;
    let mut bullet: Option<Bullet> = None;

    for (i, raw_line) in body.lines().enumerate() {
        let line_no = body_offset + i + 1;

        if let Some(heading) = raw_line.strip_prefix("## ") {
            if let Some(b) = bullet.take() {
                classify_bullet(doc, path, &b, current.as_mut(), links, warnings);
            }
            if let Some(node) = current.take() {
                finish_node(doc, path, node, order, nodes);
                order += 1;
            }
            let (title, anchor) = split_anchor(heading);
            let id = match anchor {
                Some(a) => Some(NodeId(a)),
                None => {
                    warnings.push(warning(doc, path, line_no, "heading without anchor"));
                    None
                }
            };
            current = Some(OpenNode { id, title, labels: Vec::new(), properties: Map::new(), heading_line: line_no });
            continue;
        }

        if current.is_none() {
            // Content before the first node (e.g. an H1 title) is not a node.
            continue;
        }

        let trimmed = raw_line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("- ") {
            if let Some(b) = bullet.take() {
                classify_bullet(doc, path, &b, current.as_mut(), links, warnings);
            }
            bullet = Some(Bullet { text: rest.to_string(), line: line_no });
        } else if raw_line.starts_with(char::is_whitespace) {
            // Continuation line: folds into the previous bullet's value.
            if let Some(b) = bullet.as_mut() {
                b.text.push(' ');
                b.text.push_str(trimmed);
            }
        }
    }

    if let Some(b) = bullet.take() {
        classify_bullet(doc, path, &b, current.as_mut(), links, warnings);
    }
    if let Some(node) = current.take() {
        finish_node(doc, path, node, order, nodes);
    }
}

fn finish_node(doc: &str, path: &Path, node: OpenNode, order: usize, nodes: &mut Vec<Node>) {
    let mut properties = node.properties;
    properties.insert("title".to_string(), Value::String(node.title));
    nodes.push(Node {
        id: node.id,
        labels: node.labels,
        properties,
        provenance: Provenance {
            doc: doc.to_string(),
            path: path.to_path_buf(),
            order,
            heading_line: node.heading_line,
        },
    });
}

/// Split a `##` heading into (title, anchor-without-caret). The anchor is the
/// trailing whitespace-separated token starting with `^`, if any.
fn split_anchor(heading: &str) -> (String, Option<String>) {
    let heading = heading.trim_end();
    match heading.rsplit_once(char::is_whitespace) {
        Some((rest, last)) if last.starts_with('^') && last.len() > 1 => {
            (rest.trim_end().to_string(), Some(last[1..].to_string()))
        }
        None if heading.starts_with('^') && heading.len() > 1 => (String::new(), Some(heading[1..].to_string())),
        _ => (heading.to_string(), None),
    }
}

fn classify_bullet(
    doc: &str,
    path: &Path,
    bullet: &Bullet,
    node: Option<&mut OpenNode>,
    links: &mut Vec<Link>,
    warnings: &mut Vec<Warning>,
) {
    let Some(node) = node else { return };
    let text = bullet.text.trim();

    if let Some((source, kind, target, rest)) = try_explicit_link(text) {
        let (properties, malformed) = link_properties(rest);
        if malformed {
            warnings.push(warning(doc, path, bullet.line, "malformed flow map"));
        }
        links.push(Link {
            source: Endpoint::resolve(&source),
            kind,
            target: Endpoint::resolve(&target),
            properties,
            recorded_in: node.id.clone(),
        });
        return;
    }
    if let Some((kind, target, rest)) = try_implicit_link(text) {
        let (properties, malformed) = link_properties(rest);
        if malformed {
            warnings.push(warning(doc, path, bullet.line, "malformed flow map"));
        }
        let source = node.id.clone().map(Endpoint::Node).unwrap_or_else(|| Endpoint::Node(NodeId(String::new())));
        links.push(Link {
            source,
            kind,
            target: Endpoint::resolve(&target),
            properties,
            recorded_in: node.id.clone(),
        });
        return;
    }
    if let Some((key, raw_value)) = split_bullet_key(text) {
        if key == "tags" {
            for tok in raw_value.split_whitespace() {
                node.labels.push(tok.trim_start_matches('#').to_string());
            }
        } else {
            node.properties.insert(key.to_string(), parse_value(doc, path, bullet.line, raw_value, warnings));
        }
        return;
    }
    if text.contains("[[") {
        warnings.push(warning(doc, path, bullet.line, "link line that parses as neither form"));
    }
}

/// `- key: value` (not a link line). Splits on the first `:` when the part
/// before it looks like a bare key (no space, no bracket).
fn split_bullet_key(text: &str) -> Option<(&str, &str)> {
    let (key, value) = text.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || key.contains(' ') || key.contains('[') {
        return None;
    }
    Some((key, value.trim()))
}

/// `[[src]] kind [[dst]] {props}` — explicit two-endpoint link.
fn try_explicit_link(text: &str) -> Option<(String, String, String, &str)> {
    let (src, after_src) = take_bracket(text)?;
    let after_src = after_src.trim_start();
    let (kind, after_kind) = take_token(after_src)?;
    if !is_valid_kind(kind) {
        return None;
    }
    let after_kind = after_kind.trim_start();
    let (dst, rest) = take_bracket(after_kind)?;
    Some((src, kind.to_string(), dst, rest))
}

/// `kind [[target]] {props}` — implicit-source link (source = enclosing node).
fn try_implicit_link(text: &str) -> Option<(String, String, &str)> {
    let (kind, after_kind) = take_token(text)?;
    if !is_valid_kind(kind) {
        return None;
    }
    let after_kind = after_kind.trim_start();
    let (target, rest) = take_bracket(after_kind)?;
    Some((kind.to_string(), target, rest))
}

/// If `s` starts with `[[`, returns (inner text, remainder after `]]`).
fn take_bracket(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let inner = s.strip_prefix("[[")?;
    let end = inner.find("]]")?;
    Some((inner[..end].trim().to_string(), &inner[end + 2..]))
}

/// The first whitespace-separated token, and the remainder starting right after it.
fn take_token(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    match s.find(char::is_whitespace) {
        Some(i) => Some((&s[..i], &s[i..])),
        None => Some((s, "")),
    }
}

fn is_valid_kind(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// A trailing `{…}` on a link line, if any, parsed as link properties.
/// Returns (properties, whether the map was malformed).
fn link_properties(rest: &str) -> (Map<String, Value>, bool) {
    let rest = rest.trim();
    let Some(inner) = rest.strip_prefix('{').and_then(|s| s.strip_suffix('}')) else {
        return (Map::new(), false);
    };
    match parse_flow_map(inner) {
        Ok(map) => (map, false),
        Err(_) => (Map::new(), true),
    }
}

fn parse_value(doc: &str, path: &Path, line: usize, raw: &str, warnings: &mut Vec<Warning>) -> Value {
    let raw = raw.trim();
    if let Some(inner) = raw.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        return match parse_flow_map(inner) {
            Ok(map) => Value::Object(map),
            Err(_) => {
                warnings.push(warning(doc, path, line, "malformed flow map"));
                Value::String(raw.to_string())
            }
        };
    }
    Value::String(raw.to_string())
}

/// `{k: v, k2: 'v, with comma'}` → a flat string map. Commas inside quotes don't
/// split; each part splits on its first `:`. No nested maps, no non-string values —
/// a value containing a comma must be quoted.
fn parse_flow_map(inner: &str) -> Result<Map<String, Value>, String> {
    let parts = split_outside_quotes(inner, ',');
    let mut map = Map::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let Some((k, v)) = part.split_once(':') else {
            return Err(format!("no `:` in flow-map entry: {part}"));
        };
        let k = k.trim().to_string();
        let v = unquote(v.trim());
        map.insert(k, Value::String(v));
    }
    Ok(map)
}

fn split_outside_quotes(s: &str, sep: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match quote {
            Some(q) if c == q => {
                quote = None;
                current.push(c);
            }
            Some(_) => current.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                current.push(c);
            }
            None if c == sep => parts.push(std::mem::take(&mut current)),
            None => current.push(c),
        }
    }
    parts.push(current);
    parts
}

fn unquote(s: &str) -> String {
    if s.len() >= 2 {
        let bytes = s.as_bytes();
        let first = bytes[0] as char;
        let last = bytes[bytes.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            return s[1..s.len() - 1].to_string();
        }
    }
    s.to_string()
}

// ── frontmatter (ported from apps/cli/src/l3.rs — line-preserving, dep-free) ──

fn split_frontmatter(content: &str) -> (Vec<String>, String) {
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return (Vec::new(), content.to_string());
    }
    let mut fm = Vec::new();
    let mut rest = Vec::new();
    let mut in_fm = true;
    for line in lines {
        if in_fm && line == "---" {
            in_fm = false;
            continue;
        }
        if in_fm {
            fm.push(line.to_string());
        } else {
            rest.push(line);
        }
    }
    if in_fm {
        return (Vec::new(), content.to_string());
    }
    let mut body = rest.join("\n");
    if !body.is_empty() {
        body.push('\n');
    }
    (fm, body)
}

fn fm_get(fm: &[String], key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    for line in fm {
        if let Some(rest) = line.strip_prefix(&prefix) {
            if line.starts_with(&format!("{key}: ")) || line == &format!("{key}:") {
                let v = rest.trim();
                if v.is_empty() || v == ">" || v == "|" {
                    return Some(String::new());
                }
                return Some(v.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Endpoint;
    use std::path::PathBuf;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("l3-crate-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_doc(root: &Path, name: &str, content: &str) {
        fs::write(root.join(format!("{name}.l3.md")), content).unwrap();
    }

    const FIXTURE: &str = r#"---
doc: fixture-doc
schema: research
---

# L3 — Fixture Doc

## A landmark work — with an em dash and [[openalex:W999]] inside ^r-aaa1
- tags: #landmark #key
- reading: {role: start-here, why: 'first, read this'}
- remark: a long remark that
  wraps onto a continuation line
- catalog [[openalex:W1]] {why: 'cites, directly'}

## A second node ^r-bbb2
- contradicts [[^r-aaa1]] {why: scope}
- [[openalex:W1]] contradicts [[^r-bbb2]] {why: 'method, mismatch'}

## No anchor here
- key: plain value

## Forward ref node ^r-ccc3
- siblings [[some-other-doc]]
- broken: {no colon here}
- [[dangling
"#;

    #[test]
    fn full_fixture_parses_expected_shape() {
        let root = temp_root("fixture");
        write_doc(&root, "fixture-doc", FIXTURE);
        let (graph, warnings) = parse(&root);

        assert_eq!(graph.nodes.len(), 4);
        let n1 = graph.node_by_id("r-aaa1").expect("r-aaa1 present");
        assert_eq!(n1.labels, vec!["landmark", "key"]);
        assert_eq!(
            n1.properties.get("title").and_then(Value::as_str),
            Some("A landmark work — with an em dash and [[openalex:W999]] inside")
        );
        assert_eq!(
            n1.properties.get("reading"),
            Some(&serde_json::json!({"role": "start-here", "why": "first, read this"}))
        );
        assert_eq!(
            n1.properties.get("remark").and_then(Value::as_str),
            Some("a long remark that wraps onto a continuation line")
        );

        let n3 = &graph.nodes[2];
        assert_eq!(n3.id, None);
        assert_eq!(n3.properties.get("key").and_then(Value::as_str), Some("plain value"));

        assert_eq!(graph.links.len(), 4);
        let catalog_link = graph.links.iter().find(|l| l.kind == "catalog").unwrap();
        assert_eq!(catalog_link.source, Endpoint::Node(NodeId("r-aaa1".to_string())));
        assert!(
            matches!(&catalog_link.target, Endpoint::Catalog(a) if a.namespace == "openalex" && a.value == "W1")
        );

        let implicit_contradicts = graph.links.iter().find(|l| l.recorded_in.as_ref().map(|i| i.0.as_str()) == Some("r-bbb2") && l.kind == "contradicts" && matches!(l.source, Endpoint::Node(_))).unwrap();
        assert_eq!(implicit_contradicts.source, Endpoint::Node(NodeId("r-bbb2".to_string())));
        assert_eq!(implicit_contradicts.target, Endpoint::Node(NodeId("r-aaa1".to_string())));

        let explicit = graph
            .links
            .iter()
            .find(|l| matches!(&l.source, Endpoint::Catalog(a) if a.namespace == "openalex" && a.value == "W1"))
            .unwrap();
        assert_eq!(explicit.target, Endpoint::Node(NodeId("r-bbb2".to_string())));
        assert_eq!(explicit.properties.get("why").and_then(Value::as_str), Some("method, mismatch"));

        let siblings_link = graph.links.iter().find(|l| l.kind == "siblings").unwrap();
        assert_eq!(siblings_link.target, Endpoint::Node(NodeId("doc:some-other-doc".to_string())));

        let n4 = &graph.nodes[3];
        // malformed flow map falls back to the raw brace text, plus a warning (never dropped).
        assert_eq!(n4.properties.get("broken").and_then(Value::as_str), Some("{no colon here}"));

        assert!(warnings.iter().any(|w| w.message == "heading without anchor"));
        assert!(warnings.iter().any(|w| w.message == "malformed flow map"));
        assert!(warnings.iter().any(|w| w.message == "link line that parses as neither form"));
    }

    #[test]
    fn resolve_any_namespace_as_catalog() {
        assert!(
            matches!(Endpoint::resolve("isbn:9780521179799"), Endpoint::Catalog(a) if a.namespace == "isbn" && a.value == "9780521179799")
        );
        assert!(
            matches!(Endpoint::resolve("uuid:0f9a1b2c-0000-0000-0000-000000000000"), Endpoint::Catalog(a) if a.namespace == "uuid" && a.value == "0f9a1b2c-0000-0000-0000-000000000000")
        );
        assert_eq!(
            Endpoint::resolve("some-doc-slug"),
            Endpoint::Node(NodeId("doc:some-doc-slug".to_string()))
        );
        assert_eq!(Endpoint::resolve("^r-aaa1"), Endpoint::Node(NodeId("r-aaa1".to_string())));
    }

    #[test]
    fn forward_reference_to_nonexistent_doc() {
        let root = temp_root("forward-ref");
        write_doc(
            &root,
            "d",
            "---\ndoc: d\n---\n## Node ^r-x\n- siblings [[nonexistent-doc]]\n",
        );
        let (graph, _warnings) = parse(&root);
        let link = &graph.links[0];
        assert_eq!(link.target, Endpoint::Node(NodeId("doc:nonexistent-doc".to_string())));
    }

    #[test]
    fn title_is_opaque() {
        let root = temp_root("title-opaque");
        write_doc(
            &root,
            "d",
            "---\ndoc: d\n---\n## kind: weird — [[openalex:W1]] title ^r-x\n- tags: #a\n",
        );
        let (graph, _) = parse(&root);
        assert_eq!(
            graph.nodes[0].properties.get("title").and_then(Value::as_str),
            Some("kind: weird — [[openalex:W1]] title")
        );
    }

    #[test]
    fn deterministic_serialization() {
        let root = temp_root("determinism");
        write_doc(&root, "fixture-doc", FIXTURE);
        let (graph, _) = parse(&root);
        let a = serde_json::to_string(&graph).unwrap();
        let b = serde_json::to_string(&graph).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn legacy_format_does_not_panic() {
        let root = temp_root("legacy");
        let legacy = "---\ndoc: legacy\nschema: spine\n---\n\n\
## Questions (aim-directed frontier)\n- Q1 …\n\n\
## Selection set  (canonical id → tags · note)\n- openalex:W…  #landmark #Q1  one-line note\n\n\
## Domain edges  (src --type--> dst)\n- openalex:W… --supports--> Q1\n\n\
## Findings / annotations\n- [Q1] …\n";
        write_doc(&root, "legacy", legacy);
        let (graph, warnings) = parse(&root);
        assert!(graph.nodes.iter().all(|n| n.id.is_none()));
        assert!(!warnings.is_empty());
    }

    #[test]
    fn missing_frontmatter_warns() {
        let root = temp_root("no-fm");
        write_doc(&root, "d", "## Node ^r-x\n- tags: #a\n");
        let (_graph, warnings) = parse(&root);
        assert!(warnings.iter().any(|w| w.message == "no YAML frontmatter block"));
    }

    #[test]
    fn parse_sources_matches_parse_on_equivalent_docs() {
        let root = temp_root("sources-equivalence");
        write_doc(&root, "fixture-doc", FIXTURE);
        write_doc(&root, "d2", "---\ndoc: d2\n---\n\n## Node two ^r-two1\n- tags: #x\n- catalog [[openalex:W2]] {why: 'cites'}\n");

        let (from_dir, _) = parse(&root);
        let sources = vec![
            ("fixture-doc".to_string(), FIXTURE.to_string()),
            ("d2".to_string(), "---\ndoc: d2\n---\n\n## Node two ^r-two1\n- tags: #x\n- catalog [[openalex:W2]] {why: 'cites'}\n".to_string()),
        ];
        let (from_sources, _) = parse_sources(&sources);

        assert_eq!(from_dir.nodes.len(), from_sources.nodes.len());
        for (a, b) in from_dir.nodes.iter().zip(from_sources.nodes.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.labels, b.labels);
            assert_eq!(a.properties, b.properties);
            assert_eq!(a.provenance.doc, b.provenance.doc);
            assert_eq!(a.provenance.order, b.provenance.order);
            assert_eq!(a.provenance.heading_line, b.provenance.heading_line);
        }
        assert_eq!(from_dir.links.len(), from_sources.links.len());
        for (a, b) in from_dir.links.iter().zip(from_sources.links.iter()) {
            assert_eq!(a.source, b.source);
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.target, b.target);
            assert_eq!(a.properties, b.properties);
            assert_eq!(a.recorded_in, b.recorded_in);
        }
    }

    #[test]
    fn flow_map_quoted_comma_and_malformed() {
        let map = parse_flow_map("role: start-here, why: 'first, read this'").unwrap();
        assert_eq!(Value::Object(map), serde_json::json!({"role": "start-here", "why": "first, read this"}));
        assert!(parse_flow_map("no colon here").is_err());
    }
}
