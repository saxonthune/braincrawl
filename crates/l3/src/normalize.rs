//! The PUT-doc normalize pipeline, shared by the worker and the native server:
//! parse the incoming doc, gate on blocking warnings, check anchor conflicts
//! against the rest of the store, assign ids. Pure string/in-memory logic —
//! no HTTP, no IO; callers own slug validation, force-flag parsing, loading
//! `other_docs`, and writing the result.

use std::collections::HashSet;

use crate::assign::{assign_ids_source, collect_anchors};
use crate::model::Warning;
use crate::parse::parse_sources;

/// Result of normalizing one incoming doc write.
pub enum NormalizeOutcome {
    /// Accepted: `content` is the final markdown (ids assigned).
    Normalized { content: String, assigned: Vec<crate::assign::Assigned> },
    /// Rejected: one or more blocking warnings, and `force` was not set.
    BlockingWarnings(Vec<Warning>),
    /// Rejected: an incoming anchor collides with an anchor already used by another doc.
    AnchorConflicts(Vec<String>),
}

/// Normalize `body` as the new content of doc `slug`, given every other doc
/// in the store as `(slug, markdown)` pairs. `force` bypasses the
/// blocking-warnings gate only.
pub fn normalize_doc(
    slug: &str,
    body: &str,
    other_docs: &[(String, String)],
    force: bool,
) -> NormalizeOutcome {
    let (incoming_graph, incoming_warnings) = parse_sources(&[(slug.to_string(), body.to_string())]);

    // "heading without anchor" is the expected, normal case a write resolves via
    // assign_ids_source below — it never blocks a write. Every other warning kind
    // (malformed flow map, missing frontmatter, ...) means a garbled edit and blocks.
    let blocking_warnings: Vec<Warning> = incoming_warnings
        .into_iter()
        .filter(|w| w.message != "heading without anchor")
        .collect();
    if !blocking_warnings.is_empty() && !force {
        return NormalizeOutcome::BlockingWarnings(blocking_warnings);
    }

    let other_anchors = collect_anchors(other_docs);

    let incoming_ids: HashSet<String> =
        incoming_graph.nodes.iter().filter_map(|n| n.id.as_ref().map(|id| id.0.clone())).collect();
    let mut conflicts: Vec<String> = incoming_ids.intersection(&other_anchors).cloned().collect();
    if !conflicts.is_empty() {
        conflicts.sort();
        return NormalizeOutcome::AnchorConflicts(conflicts);
    }

    let mut anchors = other_anchors;
    anchors.extend(incoming_ids);
    let (assigned_content, assigned) = assign_ids_source(body, &mut anchors);

    NormalizeOutcome::Normalized { content: assigned_content, assigned }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_write_without_force() {
        let malformed = "---\ndoc: d\n---\n\n## Node ^r-cnfrm\n- [[dangling\n";
        match normalize_doc("d", malformed, &[], false) {
            NormalizeOutcome::BlockingWarnings(warnings) => {
                assert!(!warnings.is_empty());
            }
            _ => panic!("expected BlockingWarnings"),
        }
    }

    #[test]
    fn forced_write_bypasses_blocking_warnings() {
        let malformed = "---\ndoc: d\n---\n\n## Node ^r-cnfrm\n- [[dangling\n";
        match normalize_doc("d", malformed, &[], true) {
            NormalizeOutcome::Normalized { .. } => {}
            _ => panic!("expected Normalized"),
        }
    }

    #[test]
    fn anchor_conflict_with_another_doc() {
        let other = vec![("other".to_string(), "---\ndoc: other\n---\n\n## Existing ^r-dupe\n- tags: #a\n".to_string())];
        let incoming = "---\ndoc: d\n---\n\n## New ^r-dupe\n- tags: #b\n";
        match normalize_doc("d", incoming, &other, false) {
            NormalizeOutcome::AnchorConflicts(conflicts) => {
                assert_eq!(conflicts, vec!["r-dupe".to_string()]);
            }
            _ => panic!("expected AnchorConflicts"),
        }
    }

    #[test]
    fn happy_path_assigns_ids() {
        let incoming = "---\ndoc: d\nschema: freeform\n---\n\n# L3 — D\n\n## A heading with no anchor\n- tags: #smoke\n";
        match normalize_doc("d", incoming, &[], false) {
            NormalizeOutcome::Normalized { content, assigned } => {
                assert_eq!(assigned.len(), 1);
                assert!(content.contains(" ^r-"));
            }
            _ => panic!("expected Normalized"),
        }
    }
}
