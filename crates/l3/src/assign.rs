//! `assign-ids`: assign anchors for every `##` heading that lacks one. The one
//! file-mutating verb in the research graph — node identity is tooling-owned.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::ids::new_id;
use crate::parse::parse;

/// One heading a run assigned (or, under `--dry-run`, would assign) an anchor to.
pub struct Assigned {
    pub doc: String,
    pub path: PathBuf,
    pub heading_line: usize,
    pub id: String,
    pub title: String,
}

/// Assign anchors for every id-less node in the store and append them to their
/// heading lines. Existing anchors are never touched or re-keyed; uniqueness is
/// checked against every anchor in the store, not just the file being edited.
pub fn assign_ids(root: &Path, dry_run: bool) -> Result<Vec<Assigned>, std::io::Error> {
    let (graph, _warnings) = parse(root);

    let mut existing: HashSet<String> =
        graph.nodes.iter().filter_map(|n| n.id.as_ref().map(|id| id.0.clone())).collect();

    let mut assigned = Vec::new();
    for node in &graph.nodes {
        if node.id.is_some() {
            continue;
        }
        let id = new_id(&existing);
        existing.insert(id.clone());
        let title =
            node.properties.get("title").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        assigned.push(Assigned {
            doc: node.provenance.doc.clone(),
            path: node.provenance.path.clone(),
            heading_line: node.provenance.heading_line,
            id,
            title,
        });
    }

    if dry_run || assigned.is_empty() {
        return Ok(assigned);
    }

    let mut by_path: BTreeMap<&Path, Vec<&Assigned>> = BTreeMap::new();
    for a in &assigned {
        by_path.entry(a.path.as_path()).or_default().push(a);
    }

    for (path, items) in by_path {
        write_back(path, &items)?;
    }

    Ok(assigned)
}

/// Append ` ^<id>` to each named heading line in `path`; every other line is
/// written back byte-for-byte.
fn write_back(path: &Path, items: &[&Assigned]) -> Result<(), std::io::Error> {
    let content = fs::read_to_string(path)?;
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
    fs::write(path, out)
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
