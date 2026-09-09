//! Node-level diff/merge rules for Research Documents (doc02.01.04).
//!
//! The unit of comparison is the **research node** — a `## ` heading carrying a
//! store-global `^r-…` anchor — not the file. A doc is a container: frontmatter
//! plus an ordered list of nodes. Anchors are minted once (`crates/l3/src/ids.rs`,
//! pure random) and never reused, so an anchor present on one store and absent
//! on the other was created there, and additions merge as set union with no
//! recorded state. Recorded per-node base hashes arbitrate only edits and
//! deletions.
//!
//! ## The rules (`plan_merge`, directional: source → dest)
//!
//! | Situation | Action |
//! |---|---|
//! | anchor only on source, no base | **add** to dest (same doc slug; doc created if absent) |
//! | anchor only on source, base recorded | dest deleted it — **skip**, report |
//! | anchor only on dest, base recorded | source deleted it — **keep**, report (deletions never propagate) |
//! | both sides, same hash | in sync; record hash |
//! | both sides differ, dest == base | source edit — **replace** dest node |
//! | both sides differ, source == base | dest edit — **keep** dest |
//! | both sides differ, neither == base, same heading | **line union**: dest lines + source-only lines appended |
//! | both sides differ, headings differ | **conflict** — report, transfer nothing |
//! | same anchor, different doc | **moved** — report only (no action) |
//! | node without an anchor | invisible to sync; counted |
//!
//! Line union can resurrect a line deleted on one side — accepted for the
//! accretive annotation model ("reference, never copy"; nodes mostly gain
//! lines).
//!
//! **File-second rules.** A doc's frontmatter+prologue block is compared as one
//! unit under its own base key (`doc:<slug>`): a one-sided change propagates,
//! a both-sided change is a doc-level conflict, and otherwise the dest's copy
//! stands. Opaque files with no node grammar (agent context files) sync whole,
//! by the same base logic, via [`plan_file_sync`].
//!
//! **Base invariant:** `state_after` records a hash only for content BOTH
//! sides hold when the run ends (in-sync, addition, source-edit-applied). A
//! run that leaves the sides different — dest-wins, line-union — records
//! nothing, so the reverse-direction sync still sees the difference and
//! converges the pair instead of mistaking a stale side for current.
//!
//! This crate is deliberately pure (text in, text out, no HTTP and no
//! filesystem) so the rules can be revisited in one place.

use std::collections::{BTreeMap, HashMap};

use sha2::{Digest, Sha256};

// ─── textual decomposition (round-trips exactly) ─────────────────────────────

/// One research node as raw text: the full heading line plus every body line
/// up to (not including) the next `## ` heading.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeUnit {
    /// The anchor without its `^` (e.g. `r-abc2345`); `None` for an
    /// unanchored heading.
    pub anchor: Option<String>,
    /// The full `## …` heading line, verbatim.
    pub heading: String,
    /// Body lines, verbatim.
    pub body: Vec<String>,
}

/// A doc split into its sync units. `render` reassembles the exact input.
#[derive(Debug, Clone, PartialEq)]
pub struct DocUnits {
    /// Raw frontmatter block including both `---` fences (empty if none).
    pub frontmatter: Vec<String>,
    /// Lines between frontmatter and the first node (H1 title, blanks).
    pub prologue: Vec<String>,
    pub nodes: Vec<NodeUnit>,
}

/// Trailing `^anchor` token of a heading, if any (mirrors the parser's rule).
fn heading_anchor(heading: &str) -> Option<String> {
    let trimmed = heading.trim_end();
    let last = trimmed.rsplit(char::is_whitespace).next()?;
    if last.starts_with('^') && last.len() > 1 {
        Some(last[1..].to_string())
    } else {
        None
    }
}

/// A heading's title: the heading line without `## `, the anchor, and edge whitespace.
fn heading_title(heading: &str) -> String {
    let text = heading.trim_end().strip_prefix("## ").unwrap_or(heading);
    match text.rsplit_once(char::is_whitespace) {
        Some((rest, last)) if last.starts_with('^') && last.len() > 1 => rest.trim_end().to_string(),
        _ => text.to_string(),
    }
}

pub fn split_doc(text: &str) -> DocUnits {
    let mut lines = text.lines().peekable();
    let mut frontmatter = Vec::new();
    if lines.peek() == Some(&"---") {
        let mut taken = vec![lines.next().unwrap().to_string()];
        let mut closed = false;
        for line in lines.by_ref() {
            taken.push(line.to_string());
            if line == "---" {
                closed = true;
                break;
            }
        }
        if closed {
            frontmatter = taken;
        } else {
            // Unterminated fence: treat everything as body again.
            return split_body(taken.into_iter());
        }
    }
    let rest: Vec<String> = lines.map(|l| l.to_string()).collect();
    let mut units = split_body(rest.into_iter());
    units.frontmatter = frontmatter;
    units
}

fn split_body(lines: impl Iterator<Item = String>) -> DocUnits {
    let mut prologue = Vec::new();
    let mut nodes: Vec<NodeUnit> = Vec::new();
    for line in lines {
        if line.starts_with("## ") {
            nodes.push(NodeUnit { anchor: heading_anchor(&line), heading: line, body: Vec::new() });
        } else if let Some(node) = nodes.last_mut() {
            node.body.push(line);
        } else {
            prologue.push(line);
        }
    }
    DocUnits { frontmatter: Vec::new(), prologue, nodes }
}

pub fn render_doc(units: &DocUnits) -> String {
    let mut out = String::new();
    for line in units.frontmatter.iter().chain(units.prologue.iter()) {
        out.push_str(line);
        out.push('\n');
    }
    for node in &units.nodes {
        out.push_str(&node.heading);
        out.push('\n');
        for line in &node.body {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Content hash of a node, insensitive to trailing whitespace and trailing
/// blank lines — the identity the sync rules compare by.
pub fn node_hash(node: &NodeUnit) -> String {
    let mut hasher = Sha256::new();
    hasher.update(node.heading.trim_end().as_bytes());
    hasher.update(b"\n");
    for line in normalized_body(node) {
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn normalized_body(node: &NodeUnit) -> Vec<&str> {
    let mut lines: Vec<&str> = node.body.iter().map(|l| l.trim_end()).collect();
    while lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

// ─── diff ────────────────────────────────────────────────────────────────────

/// Where a node lives: its doc, anchor, and title.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeRef {
    pub doc: String,
    pub anchor: String,
    pub title: String,
}

#[derive(Debug, Default)]
pub struct CollectionDiff {
    pub only_in_a: Vec<NodeRef>,
    pub only_in_b: Vec<NodeRef>,
    /// Same anchor and doc on both sides, different content.
    pub differing: Vec<NodeRef>,
    /// Same anchor, different doc (`NodeRef.doc` is the A side's doc).
    pub moved: Vec<NodeRef>,
    pub in_sync: usize,
    pub unanchored_a: usize,
    pub unanchored_b: usize,
}

struct Indexed {
    /// anchor → (doc slug, node hash, title)
    by_anchor: HashMap<String, (String, String, String)>,
    unanchored: usize,
}

fn index_docs(docs: &[(String, String)]) -> Indexed {
    let mut by_anchor = HashMap::new();
    let mut unanchored = 0;
    for (slug, content) in docs {
        for node in split_doc(content).nodes {
            match &node.anchor {
                Some(a) => {
                    by_anchor
                        .insert(a.clone(), (slug.clone(), node_hash(&node), heading_title(&node.heading)));
                }
                None => unanchored += 1,
            }
        }
    }
    Indexed { by_anchor, unanchored }
}

/// Compare two collections node-by-node. Stateless and read-only.
pub fn diff_collections(a: &[(String, String)], b: &[(String, String)]) -> CollectionDiff {
    let ia = index_docs(a);
    let ib = index_docs(b);
    let mut diff = CollectionDiff {
        unanchored_a: ia.unanchored,
        unanchored_b: ib.unanchored,
        ..Default::default()
    };
    let make_ref = |anchor: &str, entry: &(String, String, String)| NodeRef {
        doc: entry.0.clone(),
        anchor: anchor.to_string(),
        title: entry.2.clone(),
    };
    for (anchor, ea) in &ia.by_anchor {
        match ib.by_anchor.get(anchor) {
            None => diff.only_in_a.push(make_ref(anchor, ea)),
            Some(eb) if ea.0 != eb.0 => diff.moved.push(make_ref(anchor, ea)),
            Some(eb) if ea.1 != eb.1 => diff.differing.push(make_ref(anchor, ea)),
            Some(_) => diff.in_sync += 1,
        }
    }
    for (anchor, eb) in &ib.by_anchor {
        if !ia.by_anchor.contains_key(anchor) {
            diff.only_in_b.push(make_ref(anchor, eb));
        }
    }
    for list in [&mut diff.only_in_a, &mut diff.only_in_b, &mut diff.differing, &mut diff.moved] {
        list.sort_by(|x, y| (&x.doc, &x.anchor).cmp(&(&y.doc, &y.anchor)));
    }
    diff
}

// ─── merge ───────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct MergePlan {
    /// Dest docs to write: (slug, full new content).
    pub writes: Vec<(String, String)>,
    pub nodes_added: usize,
    /// Source edits applied (dest matched the recorded base).
    pub nodes_updated: usize,
    pub nodes_line_union: usize,
    pub conflicts: Vec<NodeRef>,
    /// Deleted on dest since the recorded base; not re-added.
    pub deleted_on_dest: Vec<NodeRef>,
    /// Deleted on source since the recorded base; kept on dest.
    pub deleted_on_source: Vec<NodeRef>,
    /// Same anchor on a different doc per side; reported, never acted on.
    pub moved: Vec<NodeRef>,
    pub unanchored_source: usize,
    /// Frontmatter/prologue blocks replaced from the source (one-sided change).
    pub meta_updated: usize,
    /// Docs whose frontmatter/prologue changed on both sides.
    pub meta_conflicts: Vec<String>,
    /// base key (anchor or `doc:<slug>`) → hash to record after the writes land.
    pub state_after: BTreeMap<String, String>,
}

/// Base-state key for a doc's frontmatter+prologue block.
pub fn doc_state_key(slug: &str) -> String {
    format!("doc:{slug}")
}

/// Hash of a doc's frontmatter+prologue block (trailing whitespace and
/// trailing blank lines ignored, like [`node_hash`]).
fn meta_hash(units: &DocUnits) -> String {
    let mut hasher = Sha256::new();
    let mut lines: Vec<&str> = units
        .frontmatter
        .iter()
        .chain(units.prologue.iter())
        .map(|l| l.trim_end())
        .collect();
    while lines.last() == Some(&"") {
        lines.pop();
    }
    for line in lines {
        hasher.update(line.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Plan a directional merge (source → dest). `base` is the pair's recorded
/// per-anchor hash from the last sync; pass an empty map for a first sync.
/// Pure: returns the docs to write, mutates nothing.
pub fn plan_merge(
    source: &[(String, String)],
    dest: &[(String, String)],
    base: &BTreeMap<String, String>,
) -> MergePlan {
    let mut plan = MergePlan::default();

    // Dest docs as mutable unit lists; source docs split once.
    let mut dest_units: BTreeMap<String, DocUnits> =
        dest.iter().map(|(slug, c)| (slug.clone(), split_doc(c))).collect();
    let source_units: BTreeMap<String, DocUnits> =
        source.iter().map(|(slug, c)| (slug.clone(), split_doc(c))).collect();

    // anchor → doc slug on the dest side (for move detection).
    let mut dest_doc_of: HashMap<String, String> = HashMap::new();
    for (slug, units) in &dest_units {
        for node in &units.nodes {
            if let Some(a) = &node.anchor {
                dest_doc_of.insert(a.clone(), slug.clone());
            }
        }
    }
    let mut source_anchors: HashMap<String, ()> = HashMap::new();
    let mut changed: BTreeMap<String, bool> = BTreeMap::new();

    for (slug, s_units) in &source_units {
        for s_node in &s_units.nodes {
            let Some(anchor) = &s_node.anchor else {
                plan.unanchored_source += 1;
                continue;
            };
            source_anchors.insert(anchor.clone(), ());
            let s_hash = node_hash(s_node);
            let node_ref = || NodeRef {
                doc: slug.clone(),
                anchor: anchor.clone(),
                title: heading_title(&s_node.heading),
            };

            match dest_doc_of.get(anchor) {
                None => {
                    if base.contains_key(anchor) {
                        plan.deleted_on_dest.push(node_ref());
                        continue;
                    }
                    // Addition: append into the same doc slug, creating the doc
                    // (with the source's frontmatter/prologue) if dest lacks it.
                    let target = dest_units.entry(slug.clone()).or_insert_with(|| DocUnits {
                        frontmatter: s_units.frontmatter.clone(),
                        prologue: s_units.prologue.clone(),
                        nodes: Vec::new(),
                    });
                    // Keep one blank line between nodes when appending.
                    if let Some(prev) = target.nodes.last_mut() {
                        if prev.body.last().map(|l| !l.trim_end().is_empty()).unwrap_or(true) {
                            prev.body.push(String::new());
                        }
                    }
                    target.nodes.push(s_node.clone());
                    dest_doc_of.insert(anchor.clone(), slug.clone());
                    changed.insert(slug.clone(), true);
                    plan.nodes_added += 1;
                    plan.state_after.insert(anchor.clone(), s_hash);
                }
                Some(d_slug) if d_slug != slug => {
                    plan.moved.push(node_ref());
                }
                Some(d_slug) => {
                    let d_slug = d_slug.clone();
                    let units = dest_units.get_mut(&d_slug).expect("indexed dest doc");
                    let d_node = units
                        .nodes
                        .iter_mut()
                        .find(|n| n.anchor.as_deref() == Some(anchor))
                        .expect("indexed dest node");
                    let d_hash = node_hash(d_node);
                    if d_hash == s_hash {
                        plan.state_after.insert(anchor.clone(), s_hash);
                        continue;
                    }
                    let recorded = base.get(anchor);
                    if recorded == Some(&d_hash) {
                        // Only the source changed since last sync.
                        *d_node = s_node.clone();
                        changed.insert(d_slug, true);
                        plan.nodes_updated += 1;
                        plan.state_after.insert(anchor.clone(), s_hash);
                    } else if recorded == Some(&s_hash) {
                        // Only the dest changed; it wins. The old base stays
                        // recorded — a base may only ever name content BOTH
                        // sides hold, and after this run they still differ.
                        // The reverse-direction sync carries the dest edit
                        // back and records the new base then.
                    } else if d_node.heading.trim_end() == s_node.heading.trim_end() {
                        // Both changed (or no base): union the body lines.
                        // No base is recorded — the source still holds its own
                        // version, so the sides differ until the reverse sync
                        // unions the other way and the pair converges.
                        if union_body(d_node, s_node) {
                            changed.insert(d_slug, true);
                            plan.nodes_line_union += 1;
                        }
                    } else {
                        plan.conflicts.push(node_ref());
                    }
                }
            }
        }
    }

    // Frontmatter/prologue, per doc present on both sides, under `doc:<slug>`.
    // Docs created above already carry the source's block and record below.
    for (slug, s_units) in &source_units {
        let Some(d_units) = dest_units.get(slug) else { continue };
        let key = doc_state_key(slug);
        let s_meta = meta_hash(s_units);
        let d_meta = meta_hash(d_units);
        if s_meta == d_meta {
            plan.state_after.insert(key, s_meta);
            continue;
        }
        let recorded = base.get(&key);
        if recorded == Some(&d_meta) {
            let d = dest_units.get_mut(slug).expect("doc present");
            d.frontmatter = s_units.frontmatter.clone();
            d.prologue = s_units.prologue.clone();
            changed.insert(slug.clone(), true);
            plan.meta_updated += 1;
            plan.state_after.insert(key, s_meta);
        } else if recorded == Some(&s_meta) {
            // Dest changed; it wins. No base recorded (base invariant).
        } else {
            plan.meta_conflicts.push(slug.clone());
        }
    }
    // New docs created for additions carry the source's block verbatim.
    for (slug, _) in &source_units {
        if !dest.iter().any(|(s, _)| s == slug) && dest_units.contains_key(slug) {
            let key = doc_state_key(slug);
            plan.state_after.insert(key, meta_hash(&source_units[slug]));
        }
    }

    // Dest-only anchors: a recorded base means the source deleted the node.
    for (anchor, d_slug) in &dest_doc_of {
        if !source_anchors.contains_key(anchor) && base.contains_key(anchor) {
            let title = dest_units[d_slug]
                .nodes
                .iter()
                .find(|n| n.anchor.as_deref() == Some(anchor))
                .map(|n| heading_title(&n.heading))
                .unwrap_or_default();
            plan.deleted_on_source.push(NodeRef {
                doc: d_slug.clone(),
                anchor: anchor.clone(),
                title,
            });
        }
    }

    for (slug, is_changed) in changed {
        if is_changed {
            plan.writes.push((slug.clone(), render_doc(&dest_units[&slug])));
        }
    }
    for list in [&mut plan.conflicts, &mut plan.deleted_on_dest, &mut plan.deleted_on_source, &mut plan.moved] {
        list.sort_by(|x, y| (&x.doc, &x.anchor).cmp(&(&y.doc, &y.anchor)));
    }
    plan.meta_conflicts.sort();
    plan
}

// ─── whole-file sync (opaque files: agent context files) ─────────────────────

/// Sync content hash of an opaque file (exact bytes).
pub fn content_hash(content: &str) -> String {
    Sha256::digest(content.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

/// A directional whole-file merge plan, same base logic as the node rules.
#[derive(Debug, Default)]
pub struct FilePlan {
    /// Files to write on dest: (name, content).
    pub writes: Vec<(String, String)>,
    pub added: usize,
    pub updated: usize,
    /// Changed on both sides since the base; reported, never transferred.
    pub conflicts: Vec<String>,
    /// Deleted on one side since the base; kept, reported.
    pub deleted_reported: usize,
    /// base key (`<prefix><name>`) → hash to record after the writes land.
    pub state_after: BTreeMap<String, String>,
}

/// Plan a directional whole-file sync (source → dest) for opaque files with no
/// node grammar. `key_prefix` namespaces the base keys (e.g. `"agent:"`). The
/// base invariant holds: a hash is recorded only for content both sides hold.
pub fn plan_file_sync(
    source: &[(String, String)],
    dest: &[(String, String)],
    base: &BTreeMap<String, String>,
    key_prefix: &str,
) -> FilePlan {
    let mut plan = FilePlan::default();
    let dest_by_name: BTreeMap<&str, &str> =
        dest.iter().map(|(n, c)| (n.as_str(), c.as_str())).collect();
    let source_names: BTreeMap<&str, ()> = source.iter().map(|(n, _)| (n.as_str(), ())).collect();

    for (name, s_content) in source {
        let key = format!("{key_prefix}{name}");
        let s_hash = content_hash(s_content);
        match dest_by_name.get(name.as_str()) {
            None => {
                if base.contains_key(&key) {
                    plan.deleted_reported += 1;
                } else {
                    plan.writes.push((name.clone(), s_content.clone()));
                    plan.added += 1;
                    plan.state_after.insert(key, s_hash);
                }
            }
            Some(d_content) => {
                let d_hash = content_hash(d_content);
                if d_hash == s_hash {
                    plan.state_after.insert(key, s_hash);
                } else if base.get(&key) == Some(&d_hash) {
                    plan.writes.push((name.clone(), s_content.clone()));
                    plan.updated += 1;
                    plan.state_after.insert(key, s_hash);
                } else if base.get(&key) == Some(&s_hash) {
                    // Dest changed; it wins. No base recorded (base invariant).
                } else {
                    plan.conflicts.push(name.clone());
                }
            }
        }
    }
    for (name, _) in dest {
        let key = format!("{key_prefix}{name}");
        if !source_names.contains_key(name.as_str()) && base.contains_key(&key) {
            plan.deleted_reported += 1;
        }
    }
    plan.conflicts.sort();
    plan
}

/// Union merge of two versions of the same node: keep the dest body, append
/// every source line the dest lacks (compared trailing-whitespace-insensitive),
/// before the dest's trailing blank run so inter-node spacing survives.
/// Returns whether the dest changed (false = dest already a superset).
fn union_body(dest: &mut NodeUnit, source: &NodeUnit) -> bool {
    let have: Vec<String> = dest.body.iter().map(|l| l.trim_end().to_string()).collect();
    let missing: Vec<String> = source
        .body
        .iter()
        .filter(|l| !l.trim_end().is_empty())
        .filter(|l| !have.contains(&l.trim_end().to_string()))
        .cloned()
        .collect();
    if missing.is_empty() {
        return false;
    }
    let mut tail_blanks = 0;
    while tail_blanks < dest.body.len()
        && dest.body[dest.body.len() - 1 - tail_blanks].trim_end().is_empty()
    {
        tail_blanks += 1;
    }
    let insert_at = dest.body.len() - tail_blanks;
    for (i, line) in missing.into_iter().enumerate() {
        dest.body.insert(insert_at + i, line);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC_A: &str = "---\ndoc: alpha\n---\n\n## First node ^r-aaa1111\n- tags: #x\n- catalog [[openalex:W1]]\n\n## Second node ^r-bbb2222\n- key: value\n";

    fn docs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(s, c)| (s.to_string(), c.to_string())).collect()
    }

    #[test]
    fn split_render_round_trips() {
        for text in [
            DOC_A,
            "no frontmatter\n## N ^r-x2345678\n- a: b\n",
            "---\ndoc: d\n---\nprologue only, no nodes\n",
            "",
        ] {
            assert_eq!(render_doc(&split_doc(text)), text);
        }
    }

    #[test]
    fn hash_ignores_trailing_whitespace_and_blanks() {
        let a = split_doc("## N ^r-x2345678\n- a: b\n").nodes.remove(0);
        let b = split_doc("## N ^r-x2345678  \n- a: b   \n\n\n").nodes.remove(0);
        assert_eq!(node_hash(&a), node_hash(&b));
    }

    #[test]
    fn diff_classifies_additions_edits_and_moves() {
        let a = docs(&[("alpha", DOC_A)]);
        let b = docs(&[(
            "alpha",
            "---\ndoc: alpha\n---\n\n## First node ^r-aaa1111\n- tags: #x\n- catalog [[openalex:W1]]\n- extra: line\n\n## Third node ^r-ccc3333\n- new: here\n",
        )]);
        let d = diff_collections(&a, &b);
        assert_eq!(d.only_in_a.len(), 1);
        assert_eq!(d.only_in_a[0].anchor, "r-bbb2222");
        assert_eq!(d.only_in_b.len(), 1);
        assert_eq!(d.only_in_b[0].anchor, "r-ccc3333");
        assert_eq!(d.differing.len(), 1);
        assert_eq!(d.differing[0].anchor, "r-aaa1111");
        assert_eq!(d.in_sync, 0);

        let moved = docs(&[("beta", "---\ndoc: beta\n---\n\n## First node ^r-aaa1111\n- tags: #x\n- catalog [[openalex:W1]]\n")]);
        let d2 = diff_collections(&a, &moved);
        assert!(d2.moved.iter().any(|n| n.anchor == "r-aaa1111"));
    }

    #[test]
    fn merge_adds_missing_nodes_and_docs() {
        let source = docs(&[("alpha", DOC_A)]);
        let dest = docs(&[]);
        let plan = plan_merge(&source, &dest, &BTreeMap::new());
        assert_eq!(plan.nodes_added, 2);
        assert_eq!(plan.writes.len(), 1);
        let (slug, content) = &plan.writes[0];
        assert_eq!(slug, "alpha");
        assert_eq!(content, DOC_A);
        // Two anchors plus the created doc's meta block.
        assert_eq!(plan.state_after.len(), 3);
        assert!(plan.state_after.contains_key(&doc_state_key("alpha")));
    }

    #[test]
    fn merge_appends_new_node_to_existing_doc_keeping_dest_frontmatter() {
        let source = docs(&[("alpha", DOC_A)]);
        let dest = docs(&[(
            "alpha",
            "---\ndoc: alpha\nupdated: 2026-08-01\n---\n\n## First node ^r-aaa1111\n- tags: #x\n- catalog [[openalex:W1]]\n",
        )]);
        let plan = plan_merge(&source, &dest, &BTreeMap::new());
        assert_eq!(plan.nodes_added, 1);
        let (_, content) = &plan.writes[0];
        assert!(content.contains("updated: 2026-08-01"), "dest frontmatter wins");
        assert!(content.contains("## Second node ^r-bbb2222"));
    }

    #[test]
    fn merge_source_edit_applies_when_dest_matches_base() {
        let dest = docs(&[("alpha", DOC_A)]);
        let source = docs(&[(
            "alpha",
            "---\ndoc: alpha\n---\n\n## First node ^r-aaa1111\n- tags: #x #y\n- catalog [[openalex:W1]]\n\n## Second node ^r-bbb2222\n- key: value\n",
        )]);
        let dest_first = split_doc(DOC_A).nodes.remove(0);
        let mut base = BTreeMap::new();
        base.insert("r-aaa1111".to_string(), node_hash(&dest_first));
        let plan = plan_merge(&source, &dest, &base);
        assert_eq!(plan.nodes_updated, 1);
        assert!(plan.writes[0].1.contains("#x #y"));
    }

    #[test]
    fn merge_dest_edit_wins_when_source_matches_base() {
        let source = docs(&[("alpha", DOC_A)]);
        let dest = docs(&[(
            "alpha",
            "---\ndoc: alpha\n---\n\n## First node ^r-aaa1111\n- tags: #x\n- catalog [[openalex:W1]]\n- dest-note: kept\n\n## Second node ^r-bbb2222\n- key: value\n",
        )]);
        let source_first = split_doc(DOC_A).nodes.remove(0);
        let mut base = BTreeMap::new();
        base.insert("r-aaa1111".to_string(), node_hash(&source_first));
        let plan = plan_merge(&source, &dest, &base);
        assert_eq!(plan.nodes_updated, 0);
        assert!(plan.writes.is_empty(), "dest already holds the newer version");
        // The sides still differ, so no new base is recorded (base invariant);
        // the reverse-direction sync will carry the dest edit back.
        assert!(!plan.state_after.contains_key("r-aaa1111"));
    }

    /// Regression: a union run must not record a base the source doesn't hold,
    /// or the reverse sync mistakes the stale source for a deliberate edit and
    /// the pair never converges (found live, 2026-08-14).
    #[test]
    fn union_then_reverse_sync_converges_both_sides() {
        let a = docs(&[("alpha", "---\ndoc: alpha\n---\n\n## Shared ^r-aaa1111\n- tags: #x\n\n## A-only ^r-aab1111\n- from: a\n")]);
        let b = docs(&[("alpha", "---\ndoc: alpha\n---\n\n## Shared ^r-aaa1111\n- tags: #x\n- b-note: added on b\n")]);

        // Run 1: a → b. Shared node: b is a superset — nothing to union.
        let plan1 = plan_merge(&a, &b, &BTreeMap::new());
        assert_eq!(plan1.nodes_added, 1);
        assert_eq!(plan1.nodes_line_union, 0);
        assert!(!plan1.state_after.contains_key("r-aaa1111"), "sides differ — no base");
        let b2 = docs(&[("alpha", plan1.writes[0].1.as_str())]);

        // Run 2: b → a with run 1's state. The b-note must reach a.
        let plan2 = plan_merge(&b2, &a, &plan1.state_after);
        assert_eq!(plan2.nodes_line_union, 1);
        let a2 = docs(&[("alpha", plan2.writes[0].1.as_str())]);
        assert!(a2[0].1.contains("- b-note: added on b"));

        // Run 3: a → b again — now identical, bases recorded, nothing to write.
        let mut state: BTreeMap<String, String> = plan1.state_after.clone();
        state.extend(plan2.state_after.clone());
        let plan3 = plan_merge(&a2, &b2, &state);
        assert!(plan3.writes.is_empty());
        assert!(plan3.state_after.contains_key("r-aaa1111"));
        assert!(plan3.state_after.contains_key("r-aab1111"));
    }

    #[test]
    fn merge_unions_lines_when_both_changed() {
        let source = docs(&[(
            "alpha",
            "---\ndoc: alpha\n---\n\n## First node ^r-aaa1111\n- tags: #x\n- catalog [[openalex:W1]]\n- source-note: s\n",
        )]);
        let dest = docs(&[(
            "alpha",
            "---\ndoc: alpha\n---\n\n## First node ^r-aaa1111\n- tags: #x\n- catalog [[openalex:W1]]\n- dest-note: d\n\n",
        )]);
        let plan = plan_merge(&source, &dest, &BTreeMap::new());
        assert_eq!(plan.nodes_line_union, 1);
        let content = &plan.writes[0].1;
        assert!(content.contains("- dest-note: d"));
        assert!(content.contains("- source-note: s"));
        // Appended before the trailing blank, keeping node spacing intact.
        assert!(content.ends_with("- source-note: s\n\n"));
    }

    #[test]
    fn merge_conflicts_on_title_change_both_sides() {
        let source = docs(&[("alpha", "---\ndoc: alpha\n---\n## Renamed by source ^r-aaa1111\n- tags: #x\n")]);
        let dest = docs(&[("alpha", "---\ndoc: alpha\n---\n## Renamed by dest ^r-aaa1111\n- tags: #x\n")]);
        let plan = plan_merge(&source, &dest, &BTreeMap::new());
        assert_eq!(plan.conflicts.len(), 1);
        assert!(plan.writes.is_empty());
        assert!(!plan.state_after.contains_key("r-aaa1111"));
    }

    #[test]
    fn merge_never_propagates_deletions() {
        // Source deleted r-bbb2222; dest deleted nothing.
        let source = docs(&[("alpha", "---\ndoc: alpha\n---\n\n## First node ^r-aaa1111\n- tags: #x\n- catalog [[openalex:W1]]\n")]);
        let dest = docs(&[("alpha", DOC_A)]);
        let first = split_doc(DOC_A).nodes.remove(0);
        let second = split_doc(DOC_A).nodes.remove(1);
        let mut base = BTreeMap::new();
        base.insert("r-aaa1111".to_string(), node_hash(&first));
        base.insert("r-bbb2222".to_string(), node_hash(&second));
        let plan = plan_merge(&source, &dest, &base);
        assert!(plan.writes.is_empty());
        assert_eq!(plan.deleted_on_source.len(), 1);
        assert_eq!(plan.deleted_on_source[0].anchor, "r-bbb2222");

        // Mirror: dest deleted a node the source still holds — not re-added.
        let plan2 = plan_merge(&dest, &source, &base);
        assert_eq!(plan2.deleted_on_dest.len(), 1);
        assert_eq!(plan2.nodes_added, 0);
    }

    #[test]
    fn merge_reports_moves_without_acting() {
        let source = docs(&[("beta", "---\ndoc: beta\n---\n## First node ^r-aaa1111\n- tags: #x\n- catalog [[openalex:W1]]\n")]);
        let dest = docs(&[("alpha", DOC_A)]);
        let plan = plan_merge(&source, &dest, &BTreeMap::new());
        assert_eq!(plan.moved.len(), 1);
        assert!(plan.writes.is_empty());
    }

    #[test]
    fn meta_one_sided_change_propagates_both_sided_conflicts() {
        let dest = docs(&[("alpha", DOC_A)]);
        let source = docs(&[(
            "alpha",
            "---\ndoc: alpha\ntopic: salt\n---\n\n## First node ^r-aaa1111\n- tags: #x\n- catalog [[openalex:W1]]\n\n## Second node ^r-bbb2222\n- key: value\n",
        )]);
        // Base = dest's current meta → only the source changed → propagate.
        let mut base = BTreeMap::new();
        base.insert(doc_state_key("alpha"), meta_hash(&split_doc(DOC_A)));
        let plan = plan_merge(&source, &dest, &base);
        assert_eq!(plan.meta_updated, 1);
        assert!(plan.writes[0].1.contains("topic: salt"));

        // No base → both-sided (unknown) → conflict, dest untouched.
        let plan2 = plan_merge(&source, &dest, &BTreeMap::new());
        assert_eq!(plan2.meta_conflicts, vec!["alpha".to_string()]);
        assert!(plan2.writes.is_empty());
        assert!(!plan2.state_after.contains_key(&doc_state_key("alpha")));
    }

    #[test]
    fn file_sync_add_update_conflict_and_deletion() {
        let base_hash = content_hash("v1");
        let mut base = BTreeMap::new();
        base.insert("agent:kept".to_string(), base_hash.clone());
        base.insert("agent:gone-on-dest".to_string(), base_hash.clone());

        let source = docs(&[("new-file", "hello"), ("kept", "v2"), ("gone-on-dest", "v1")]);
        let dest = docs(&[("kept", "v1")]);
        let plan = plan_file_sync(&source, &dest, &base, "agent:");
        assert_eq!(plan.added, 1, "new-file copied");
        assert_eq!(plan.updated, 1, "kept updated: dest matched base");
        assert_eq!(plan.deleted_reported, 1, "gone-on-dest not re-added");
        assert!(plan.conflicts.is_empty());
        assert_eq!(plan.writes.len(), 2);

        // Both changed since base → conflict, nothing written.
        let source2 = docs(&[("kept", "v2")]);
        let dest2 = docs(&[("kept", "v3")]);
        let plan2 = plan_file_sync(&source2, &dest2, &base, "agent:");
        assert_eq!(plan2.conflicts, vec!["kept".to_string()]);
        assert!(plan2.writes.is_empty());
        assert!(!plan2.state_after.contains_key("agent:kept"));
    }

    #[test]
    fn unanchored_nodes_are_counted_and_untouched() {
        let source = docs(&[("alpha", "---\ndoc: alpha\n---\n## No anchor here\n- key: v\n")]);
        let dest = docs(&[]);
        let plan = plan_merge(&source, &dest, &BTreeMap::new());
        assert_eq!(plan.unanchored_source, 1);
        assert!(plan.writes.is_empty());
    }
}
