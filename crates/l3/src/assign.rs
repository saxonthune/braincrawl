//! `assign-ids`: assign anchors for every `##` heading that lacks one. The one
//! file-mutating verb in the research graph — node identity is tooling-owned.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ids::new_id;
use crate::model::Endpoint;
use crate::parse::{find_docs, parse_one, parse_sources, FILE_SUFFIX};

/// One heading a run assigned (or, under `--dry-run`, would assign) an anchor to.
pub struct Assigned {
    pub doc: String,
    pub path: PathBuf,
    pub heading_line: usize,
    pub id: String,
    pub title: String,
    /// The temporary anchor slug (without `^`) this id resolved, when the id came
    /// from a `^t-…` rather than from an anchor-less heading.
    pub temp: Option<String>,
}

/// Every existing `NodeId` across `sources` — the anchor set a fresh assignment
/// run must avoid colliding with, store-wide.
pub fn collect_anchors(sources: &[(String, String)]) -> HashSet<String> {
    let (graph, _warnings) = parse_sources(sources);
    graph.nodes.iter().filter_map(|n| n.id.as_ref().map(|id| id.0.clone())).collect()
}

/// Assign anchors for every id-less node heading in one doc's markdown,
/// appending ` ^<id>` to its heading line (byte-preserving otherwise, like
/// `assign_ids`'s file write-back). `existing` is the anchor set already taken
/// store-wide; each newly assigned id is inserted into it as it goes, so a
/// caller can run this doc-by-doc across a store and keep ids unique overall.
pub fn assign_ids_source(content: &str, existing: &mut HashSet<String>) -> (String, Vec<Assigned>) {
    let (nodes, _links, _warnings) = parse_one("", PathBuf::from("unknown.l3.md"), content);
    let doc = nodes.first().map(|n| n.provenance.doc.clone()).unwrap_or_default();
    let synthetic_path = PathBuf::from(format!("{doc}{FILE_SUFFIX}"));

    let mut assigned = Vec::new();
    for node in &nodes {
        if node.id.is_some() {
            continue;
        }
        let id = new_id(existing);
        existing.insert(id.clone());
        let title =
            node.properties.get("title").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        assigned.push(Assigned {
            doc: doc.clone(),
            path: synthetic_path.clone(),
            heading_line: node.provenance.heading_line,
            id,
            title,
            temp: None,
        });
    }

    let refs: Vec<&Assigned> = assigned.iter().collect();
    let new_content = append_anchors(content, &refs);
    (new_content, assigned)
}

/// A `^t-…` heading found while scanning for temporary anchor definitions.
struct DefinedTemp {
    doc: String,
    heading_line: usize,
    title: String,
}

/// Assign anchors for every id-less node in the store and append them to their
/// heading lines. Existing anchors are never touched or re-keyed; uniqueness is
/// checked against every anchor in the store, not just the file being edited.
///
/// Before that pass runs, every `^t-<slug>` temporary anchor defined on a heading
/// is resolved to one fresh `^r-…` and substituted everywhere it is written —
/// heading and references alike, across every doc — so a `^t-…` heading is never
/// silently skipped by the anchor-less-heading pass below (see `parse::split_anchor`:
/// a `^t-…` heading already has `id.is_some()`).
pub fn assign_ids(root: &Path, dry_run: bool) -> Result<Vec<Assigned>, std::io::Error> {
    let mut docs: Vec<(PathBuf, String, String)> = Vec::new();
    for path in find_docs(root) {
        let Ok(content) = fs::read_to_string(&path) else { continue };
        let slug = path
            .file_name()
            .map(|n| n.to_string_lossy().trim_end_matches(FILE_SUFFIX).to_string())
            .unwrap_or_default();
        docs.push((path, slug, content));
    }

    let sources: Vec<(String, String)> =
        docs.iter().map(|(_, slug, content)| (slug.clone(), content.clone())).collect();

    let (graph, _warnings) = parse_sources(&sources);
    let mut defined: BTreeMap<String, DefinedTemp> = BTreeMap::new();
    for node in &graph.nodes {
        let Some(id) = &node.id else { continue };
        if !id.0.starts_with("t-") {
            continue;
        }
        if let Some(prev) = defined.get(&id.0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "duplicate temporary anchor ^{} defined in both {} and {}",
                    id.0, prev.doc, node.provenance.doc
                ),
            ));
        }
        let title =
            node.properties.get("title").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        defined.insert(
            id.0.clone(),
            DefinedTemp { doc: node.provenance.doc.clone(), heading_line: node.provenance.heading_line, title },
        );
    }

    let mut existing = collect_anchors(&sources);
    let mut temp_mapping: BTreeMap<String, String> = BTreeMap::new();
    for slug in defined.keys() {
        let id = new_id(&existing);
        existing.insert(id.clone());
        temp_mapping.insert(slug.clone(), id);
    }

    let mut assigned = Vec::new();
    let mut writes: Vec<(PathBuf, String)> = Vec::new();
    for (path, slug, content) in &docs {
        let resolved = resolve_temp_ids_source(content, &temp_mapping);
        let (new_content, mut doc_assigned) = assign_ids_source(&resolved, &mut existing);
        for item in &mut doc_assigned {
            item.path = path.clone();
        }

        for (t_slug, info) in &defined {
            if &info.doc != slug {
                continue;
            }
            assigned.push(Assigned {
                doc: slug.clone(),
                path: path.clone(),
                heading_line: info.heading_line,
                id: temp_mapping[t_slug].clone(),
                title: info.title.clone(),
                temp: Some(t_slug.clone()),
            });
        }
        assigned.extend(doc_assigned);

        if !dry_run && new_content != *content {
            writes.push((path.clone(), new_content));
        }
    }

    for (path, content) in writes {
        fs::write(path, content)?;
    }

    Ok(assigned)
}

/// Temporary anchors referenced by a link but never defined on a heading.
/// Reported by `assign-ids` so a typo surfaces instead of silently persisting.
pub fn dangling_temp_refs(sources: &[(String, String)]) -> Vec<String> {
    let (graph, _warnings) = parse_sources(sources);
    let defined: HashSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.id.as_ref())
        .map(|id| id.0.as_str())
        .filter(|id| id.starts_with("t-"))
        .collect();

    let mut referenced: BTreeSet<String> = BTreeSet::new();
    for link in &graph.links {
        for endpoint in [&link.source, &link.target] {
            if let Endpoint::Node(id) = endpoint {
                if id.0.starts_with("t-") {
                    referenced.insert(id.0.clone());
                }
            }
        }
    }

    referenced.into_iter().filter(|slug| !defined.contains(slug.as_str())).collect()
}

/// Replace every `^t-<slug>` token in `content` with `^<real>` per `mapping`,
/// matching only where the token ends at a boundary. Every other byte is
/// preserved. Both the heading anchor form (`## Title ^t-slug`) and the
/// reference form (`[[^t-slug]]`) are the same token, so one scan handles both.
fn resolve_temp_ids_source(content: &str, mapping: &BTreeMap<String, String>) -> String {
    let bytes = content.as_bytes();
    let mut out = String::with_capacity(content.len());
    let mut last = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'^' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() && is_anchor_char(bytes[end]) {
                end += 1;
            }
            let token = &content[start..end];
            if let Some(real) = mapping.get(token) {
                out.push_str(&content[last..i]);
                out.push('^');
                out.push_str(real);
                last = end;
                i = end;
                continue;
            }
            i = end;
        } else {
            i += 1;
        }
    }
    out.push_str(&content[last..]);
    out
}

fn is_anchor_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

/// Append ` ^<id>` to each named heading line in `content`; every other line
/// is preserved byte-for-byte.
fn append_anchors(content: &str, items: &[&Assigned]) -> String {
    let had_trailing_newline = content.ends_with('\n');
    let mut lines: Vec<&str> = content.lines().collect();
    let mut owned: BTreeMap<usize, String> = BTreeMap::new();
    for item in items {
        let idx = item.heading_line - 1;
        let line = owned.entry(idx).or_insert_with(|| lines[idx].to_string());
        *line = format!("{line} ^{}", item.id);
    }
    for (idx, line) in &owned {
        lines[*idx] = line.as_str();
    }

    let mut out = lines.join("\n");
    if had_trailing_newline {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse;
    use std::path::PathBuf;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("l3-assign-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_doc(root: &Path, name: &str, content: &str) {
        fs::write(root.join(format!("{name}.l3.md")), content).unwrap();
    }

    const FIXTURE: &str = "---\ndoc: d\nschema: freeform\n---\n\n# L3 — D\n\n## Has anchor already ^r-existing\n- tags: #a\n\n## No anchor here\n- key: value\n\n## Also no anchor\n- key: value2\n";

    #[test]
    fn assigns_ids_only_to_headings_without_one() {
        let root = temp_root("assigns");
        write_doc(&root, "d", FIXTURE);

        let assigned = assign_ids(&root, false).unwrap();
        assert_eq!(assigned.len(), 2);

        let (graph, _) = parse(&root);
        assert!(graph.nodes.iter().all(|n| n.id.is_some()));
        assert!(graph.node_by_id("r-existing").is_some());
    }

    #[test]
    fn write_back_touches_only_the_heading_lines() {
        let root = temp_root("byte-compare");
        write_doc(&root, "d", FIXTURE);

        assign_ids(&root, false).unwrap();

        let after = fs::read_to_string(root.join("d.l3.md")).unwrap();
        let before_lines: Vec<&str> = FIXTURE.lines().collect();
        let after_lines: Vec<&str> = after.lines().collect();
        assert_eq!(before_lines.len(), after_lines.len());
        for (i, (b, a)) in before_lines.iter().zip(after_lines.iter()).enumerate() {
            let is_heading_without_anchor = b.starts_with("## ") && !b.contains('^');
            if is_heading_without_anchor {
                assert_ne!(b, a, "line {i} should have gained an anchor");
                assert!(a.starts_with(b));
            } else {
                assert_eq!(b, a, "line {i} should be untouched");
            }
        }
    }

    #[test]
    fn idempotent_second_run_assigns_nothing() {
        let root = temp_root("idempotent");
        write_doc(&root, "d", FIXTURE);

        assign_ids(&root, false).unwrap();
        let second = assign_ids(&root, false).unwrap();
        assert!(second.is_empty());
    }

    #[test]
    fn dry_run_writes_nothing() {
        let root = temp_root("dry-run");
        write_doc(&root, "d", FIXTURE);

        let before = fs::read_to_string(root.join("d.l3.md")).unwrap();
        let assigned = assign_ids(&root, true).unwrap();
        assert_eq!(assigned.len(), 2);
        let after = fs::read_to_string(root.join("d.l3.md")).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn existing_anchor_is_never_rekeyed() {
        let root = temp_root("no-rekey");
        write_doc(&root, "d", FIXTURE);

        assign_ids(&root, false).unwrap();
        let (graph, _) = parse(&root);
        assert!(graph.node_by_id("r-existing").is_some());
    }

    #[test]
    fn assign_ids_source_assigns_only_anchor_less_headings() {
        let mut existing = HashSet::new();
        existing.insert("r-existing".to_string());

        let (new_content, assigned) = assign_ids_source(FIXTURE, &mut existing);
        assert_eq!(assigned.len(), 2);
        assert!(assigned.iter().all(|a| a.doc == "d"));

        let before_lines: Vec<&str> = FIXTURE.lines().collect();
        let after_lines: Vec<&str> = new_content.lines().collect();
        assert_eq!(before_lines.len(), after_lines.len());
        for (b, a) in before_lines.iter().zip(after_lines.iter()) {
            let is_heading_without_anchor = b.starts_with("## ") && !b.contains('^');
            if is_heading_without_anchor {
                assert_ne!(b, a);
                assert!(a.starts_with(b));
            } else {
                assert_eq!(b, a);
            }
        }

        // ids inserted into `existing`, so a second call sees them as taken.
        assert!(existing.contains(&assigned[0].id));
        assert!(existing.contains(&assigned[1].id));
    }

    #[test]
    fn assign_ids_source_unique_against_passed_set() {
        let mut existing = HashSet::new();
        let content = "---\ndoc: d\n---\n\n## First\n- key: value\n\n## Second\n- key: value2\n";
        let (_new_content, assigned) = assign_ids_source(content, &mut existing);
        assert_eq!(assigned.len(), 2);
        assert_ne!(assigned[0].id, assigned[1].id);
        assert!(existing.contains(&assigned[0].id));
        assert!(existing.contains(&assigned[1].id));
    }

    #[test]
    fn resolves_temp_anchor_across_two_docs() {
        let root = temp_root("temp-cross-doc");
        write_doc(&root, "a", "---\ndoc: a\n---\n\n## X ^t-alpha\n- tags: #a\n");
        write_doc(&root, "b", "---\ndoc: b\n---\n\n## Y\n- builds-on [[^t-alpha]]\n");

        let assigned = assign_ids(&root, false).unwrap();
        let resolution = assigned.iter().find(|a| a.temp.as_deref() == Some("t-alpha")).unwrap();
        assert!(resolution.id.starts_with("r-"));

        let a_content = fs::read_to_string(root.join("a.l3.md")).unwrap();
        let b_content = fs::read_to_string(root.join("b.l3.md")).unwrap();
        assert!(!a_content.contains("^t-"));
        assert!(!b_content.contains("^t-"));
        assert!(a_content.contains(&format!("^{}", resolution.id)));
        assert!(b_content.contains(&format!("[[^{}]]", resolution.id)));
    }

    #[test]
    fn one_id_per_unique_temp_anchor() {
        let root = temp_root("temp-one-id");
        write_doc(&root, "a", "---\ndoc: a\n---\n\n## X ^t-alpha\n- tags: #a\n");
        write_doc(
            &root,
            "b",
            "---\ndoc: b\n---\n\n## Y\n- builds-on [[^t-alpha]]\n\n## Z\n- builds-on [[^t-alpha]]\n",
        );

        let assigned = assign_ids(&root, false).unwrap();
        let resolutions: Vec<&Assigned> =
            assigned.iter().filter(|a| a.temp.as_deref() == Some("t-alpha")).collect();
        assert_eq!(resolutions.len(), 1);

        let b_content = fs::read_to_string(root.join("b.l3.md")).unwrap();
        let id = &resolutions[0].id;
        assert_eq!(b_content.matches(&format!("[[^{id}]]")).count(), 2);
    }

    #[test]
    fn temp_anchor_boundary_correctness() {
        let root = temp_root("temp-boundary");
        write_doc(
            &root,
            "a",
            "---\ndoc: a\n---\n\n## Three ^t-three\n- tags: #a\n\n## Three textures ^t-three-textures\n- tags: #b\n",
        );

        let assigned = assign_ids(&root, false).unwrap();
        let three = assigned.iter().find(|a| a.temp.as_deref() == Some("t-three")).unwrap();
        let three_textures =
            assigned.iter().find(|a| a.temp.as_deref() == Some("t-three-textures")).unwrap();
        assert_ne!(three.id, three_textures.id);

        let content = fs::read_to_string(root.join("a.l3.md")).unwrap();
        assert!(content.contains(&format!("Three ^{}", three.id)));
        assert!(content.contains(&format!("Three textures ^{}", three_textures.id)));
        assert!(!content.contains("^t-"));
    }

    #[test]
    fn duplicate_temp_definition_errors_and_writes_nothing() {
        let root = temp_root("temp-duplicate");
        let a_src = "---\ndoc: a\n---\n\n## X ^t-alpha\n- tags: #a\n";
        let b_src = "---\ndoc: b\n---\n\n## Y ^t-alpha\n- tags: #b\n";
        write_doc(&root, "a", a_src);
        write_doc(&root, "b", b_src);

        let result = assign_ids(&root, false);
        assert!(result.is_err());

        assert_eq!(fs::read_to_string(root.join("a.l3.md")).unwrap(), a_src);
        assert_eq!(fs::read_to_string(root.join("b.l3.md")).unwrap(), b_src);
    }

    #[test]
    fn dangling_temp_reference_names_slug_and_mints_nothing() {
        let root = temp_root("temp-dangling");
        let src = "---\ndoc: a\n---\n\n## X\n- builds-on [[^t-ghost]]\n";
        write_doc(&root, "a", src);

        let sources = vec![("a".to_string(), src.to_string())];
        let dangling = dangling_temp_refs(&sources);
        assert_eq!(dangling, vec!["t-ghost".to_string()]);

        let assigned = assign_ids(&root, false).unwrap();
        assert!(assigned.iter().all(|a| a.temp.is_none()));
        let after = fs::read_to_string(root.join("a.l3.md")).unwrap();
        assert!(after.contains("[[^t-ghost]]"));
    }

    #[test]
    fn temp_resolution_touches_only_lines_with_the_token() {
        let root = temp_root("temp-byte-preserve");
        let src = "---\ndoc: a\n---\n\n## X ^t-alpha\n- tags: #a\n- remark: unrelated line\n\n## Y ^r-existing\n- builds-on [[^t-alpha]]\n";
        write_doc(&root, "a", src);

        assign_ids(&root, false).unwrap();
        let after = fs::read_to_string(root.join("a.l3.md")).unwrap();

        let before_lines: Vec<&str> = src.lines().collect();
        let after_lines: Vec<&str> = after.lines().collect();
        assert_eq!(before_lines.len(), after_lines.len());
        for (b, a) in before_lines.iter().zip(after_lines.iter()) {
            if b.contains("^t-") || b.contains("[[^t-") {
                assert_ne!(b, a);
            } else {
                assert_eq!(b, a);
            }
        }
    }
}
