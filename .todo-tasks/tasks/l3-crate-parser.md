# l3 crate: graph model and parser

## Motivation

An L3 Research Document becomes a collection of research nodes — id-bearing
blocks a parser lifts into a property graph (the research graph). This is the
foundation phase: the types and parser everything downstream builds on.
Glossary: `.rhidoc/01-product/01-glossary.md` §"The research graph".

## Do NOT

- Do NOT modify any existing file in `apps/cli` — this phase only adds the
  new crate. `apps/cli/src/l3.rs` keeps working unchanged.
- Do NOT write to any file from library code. `parse()` is read-only.
- Do NOT add heavy dependencies. Allowed: `serde`, `serde_json` (already in
  the workspace). Hand-roll the flow-map parser; do NOT add serde_yaml.
- Do NOT parse heading text for structure — the title is an opaque string
  property. No kind-prefixes, no handle extraction.
- Do NOT hard-fail on malformed lines: emit a warning and continue. Unknown
  property keys and unknown link kinds are valid free vocabulary, not
  warnings.
- Do NOT mint or invent node ids — a `##` heading without an anchor parses
  as a node with no id, reported in warnings (assign-ids is the next phase).

## Plan

### 1. New crate `crates/l3`

Package name `l3`, lib only. Add `"crates/l3"` to both `members` and
`default-members` in the root `Cargo.toml`. Depends on `serde` (derive),
`serde_json`, and `braincrawl-core` (for `CanonicalId` — check the actual
package name in `crates/core/Cargo.toml` and import its `CanonicalId`).

### 2. `crates/l3/src/model.rs` — the types

```rust
pub struct NodeId(pub String);                 // anchor without the ^, e.g. "r-x7k2m"
pub enum Endpoint { Node(NodeId), Catalog(CanonicalId) }
pub struct Node {
    pub id: Option<NodeId>,                    // None until assign-ids has run
    pub labels: Vec<String>,                   // from the tags line
    pub properties: serde_json::Map<String, serde_json::Value>,
    pub provenance: Provenance,
}
pub struct Provenance { pub doc: String, pub path: PathBuf, pub order: usize, pub heading_line: usize }
pub struct Link {
    pub source: Endpoint,
    pub kind: String,                          // serialize as "type"
    pub target: Endpoint,
    pub properties: serde_json::Map<String, serde_json::Value>,
    pub recorded_in: Option<NodeId>,
}
pub struct Graph { pub nodes: Vec<Node>, pub links: Vec<Link>, /* private indexes */ }
```

Wire naming via serde attributes: `kind` → `"type"`; field names otherwise as
written (`labels`, `properties`, `source`, `target`). `Endpoint` serializes
as a tagged string: `"node:r-x7k2m"` / `"openalex:W123"` — a Catalog endpoint
serializes as the canonical id itself; a Node endpoint with the `node:`
prefix. Deterministic ordering everywhere (sort by doc then node order).

Private indexes rebuilt in `Graph::build()`: id→node index, label→nodes,
adjacency. Never serialized.

### 3. `crates/l3/src/parse.rs` — the block grammar

`pub fn parse(root: &Path) -> (Graph, Vec<Warning>)` — reads every
`*.l3.md` under root (skip `_`-prefixed and `INDEX.md`), per file:

- Frontmatter: split on leading `---` blocks (port the line-preserving
  `split_frontmatter`/`fm_get` approach from `apps/cli/src/l3.rs` — copy the
  logic into this crate, do not import from the CLI). Envelope keys: `doc`
  (defaults to filename stem), `schema`.
- A node = a `##` heading. Anchor = trailing `^r-…` token on the heading
  line (regex-free: last whitespace-separated token starting with `^`).
  Heading text minus the anchor = `title` property (opaque). Nodes get
  `order` by position in the file.
- Bullets under a node, classified by first token:
  - `- key: value` → property. Value: scalar string, or one YAML-flow-style
    map `{k: v, k2: v2}` parsed by a small hand parser: strip braces, split
    on commas *outside* single/double quotes, each part splits on first `:`.
    Values with commas must be quoted — document this in a comment.
  - `- kind [[target]] …` (kind = `[a-z][a-z0-9-]*`) → link, source = the
    enclosing node.
  - `- [[src]] kind [[dst]] …` → link with explicit endpoints; enclosing
    node is `recorded_in`.
  - A trailing `{…}` on a link line → link properties (same map parser).
  - `- tags: #a #b` → the node's `labels` (strip `#`).
  - Continuation lines (indented, no leading `-`) belong to the previous
    bullet's value (multi-line remarks and wrapped flow maps).
- `[[target]]` resolution: `openalex:…`/`doi:…` prefix → `Endpoint::Catalog`;
  `^r-…` → node in this doc; `slug#^r-…` → node in another doc; bare `slug`
  → the doc-level target (represent as `Endpoint::Node(NodeId("doc:slug"))`
  — a forward reference; unresolved targets are kept, never dropped).
- Warnings (not errors): no frontmatter, heading without anchor, malformed
  flow map, link line that parses as neither form.

### 4. Tests — co-locate in the crate

Cover: property vs link classification; flow maps (quoted commas, malformed);
one- and two-endpoint links; explicit catalog source
(`- [[openalex:W1]] contradicts [[^r-abc]] {why: x}`); tags→labels; title
opacity (heading with `—` and `[[…]]` inside stays verbatim); anchor-less
node warning; forward reference to a nonexistent doc; deterministic
serialization (serialize twice, byte-equal). Write one full fixture doc as a
raw string constant exercising the whole grammar (model it on the grammar
above; use `contradicts` as the claim-link kind, `catalog` for work links,
a `reading: {role: start-here, why: …}` property).

## Files to Modify

- `Cargo.toml` — workspace members + default-members
- `crates/l3/Cargo.toml` — new
- `crates/l3/src/lib.rs`, `model.rs`, `parse.rs` — new

## Verification

```bash
cargo build
cargo test -p l3
cargo test
```

## Out of Scope

- Writing anchors back (phase 2), the server endpoint (phase 3), rewiring
  CLI verbs (phase 4)
- Harvesting bare inline ids from remarks ("mentions") — deferred

## Notes

- The word for the claim link is `contradicts` (never `refutes`) — decided
  2026-07-12.
- Legacy docs in the old format (selection-set bullets, `--supports-->`
  arrows) should parse without panicking: mostly-empty graphs + warnings.
  Add one test with an old-format body proving no panic.

## Surface after this phase

- Crate `l3` exporting `Graph`, `Node`, `Link`, `Endpoint`, `NodeId`,
  `Provenance`, `Warning`, and `parse(root: &Path) -> (Graph, Vec<Warning>)`.
- Wire serialization: `nodes` (`id`, `labels`, `properties`), `links`
  (`source`, `target`, `type`, `properties`); deterministic output.
- Nodes may have `id: None` (anchor-less) — downstream phases rely on this
  to find work for assign-ids.
- `apps/cli` is untouched and behaves exactly as before.
