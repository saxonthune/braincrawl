//! `assign-ids`: assign anchors for every `##` heading that lacks one. The one
//! file-mutating verb in the research graph — node identity is tooling-owned.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::ids::new_id;
use crate::parse::{find_docs, parse_one, parse_sources, FILE_SUFFIX};

/// One heading a run assigned (or, under `--dry-run`, would assign) an anchor to.
pub struct Assigned {
    pub doc: String,
    pub path: PathBuf,
    pub heading_line: usize,
    pub id: String,
    pub title: String,
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
        });
    }

    let refs: Vec<&Assigned> = assigned.iter().collect();
    let new_content = append_anchors(content, &refs);
    (new_content, assigned)
}

/// Assign anchors for every id-less node in the store and append them to their
/// heading lines. Existing anchors are never touched or re-keyed; uniqueness is
/// checked against every anchor in the store, not just the file being edited.
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
    let mut existing = collect_anchors(&sources);

    let mut assigned = Vec::new();
    let mut writes: Vec<(PathBuf, String)> = Vec::new();
    for (path, _slug, content) in &docs {
        let (new_content, mut doc_assigned) = assign_ids_source(content, &mut existing);
        if doc_assigned.is_empty() {
            continue;
        }
        for item in &mut doc_assigned {
            item.path = path.clone();
        }
        if !dry_run {
            writes.push((path.clone(), new_content));
        }
        assigned.extend(doc_assigned);
    }

    for (path, content) in writes {
        fs::write(path, content)?;
    }

    Ok(assigned)
}

/// Append ` ^<id>` to each named heading line in `content`; every other line
/// is preserved byte-for-byte.
fn append_anchors(content: &str, items: &[&Assigned]) -> String {
    use std::collections::BTreeMap;

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
}
