---
title: L3 Conventions
summary: An L3 Research Document is markdown with one required frontmatter field (doc) and a body of research nodes — headings carrying property and link lines. Node anchors are store-global; reference ids, never copy metadata. A worked example ships with the braincrawl skill.
tags: [architecture, core, l3, research-collection, conventions, node-grammar]
deps: [doc02.01.01]
---

# L3 Conventions

One markdown file per item under the consolidated store. A worked example is
`.agents/skills/braincrawl/l3-example.l3.md`; the rules:

- **Frontmatter** — one required field (`collection check` warns if missing): `doc:`, a kebab-case
  slug that is the filename stem and primary key.
- **Body** — only research nodes, no loose prose and no H1. A `## ` heading opens a node with
  an opaque-prose title; `collection assign-ids` appends its `^r-…` anchor — never write one by
  hand. To cross-link two nodes in the same batch of edits before an anchor exists, write a
  **temporary anchor** instead — see below.
- **Property line** — `- key: value`, the value optionally one YAML flow map
  (`{role: core, why: '…'}`); quote any value holding a comma. `- tags: #a #b` is special —
  each `#`-token is a node label, not a property.
- **Link line** — `- kind [[target]]` (source is the enclosing node) or `- [[src]] kind
  [[dst]]`, with an optional trailing `{props}`. A target is a node anchor `^r-…`, a bare
  `slug` (that doc's intro node), or any `scheme:value` catalog reference
  (`openalex:`/`doi:`/`isbn:`/`pmid:`/…, or `uuid:` for a work referenced by its canonical
  id directly). A `[[wikilink]]` inside a property value is prose, not a link.
- **Anchors are store-global.** `assign-ids` keeps every `^r-…` unique across the whole store,
  so a bare `^r-…` resolves to its node from **any** doc — no slug prefix — and the reference
  survives if that node later moves to another doc.
- **Temporary anchors** — an author may write `^t-<slug>` by hand on a heading (`## Title
  ^t-my-slug`) and reference it as `[[^t-<slug>]]` from any edge in any doc of the same batch.
  `collection assign-ids` resolves every `^t-<slug>` to a fresh store-global `^r-…`, rewriting the
  heading and every reference to it in one pass. A `^r-…` is still never hand-written.
- **Link vocabulary** — `catalog` links a node to a work; `contradicts` is the standard
  claim-link (never `refutes`); `supports`/`builds-on`/`relates-to`/`bridges`/`complicates`
  are free domain words. A `reading: {role, why}` property plus a `catalog` link marks a
  recommended reading (`role` = `start-here`/`core`/`rigor`/`reference`, or any string).
- **The one hard rule** — reference, never copy: store references, look facts up from the server
  at read time so a doc never drifts. A work's canonical identity is its store-generated UUID; name
  it with an `openalex:`/`doi:` reference that resolves to that UUID — never treat the provider
  id as the identity itself.

## Sync semantics — the node is the unit

`store diff` and `store sync` compare Research Collections **node by node**, keyed by the
store-global anchor; the doc is a container. The rules live in one place —
`crates/l3-sync` (`braincrawl-l3-sync`), whose module doc is the authoritative table — and
are pure text-in/text-out so they can be revisited without touching transport. In brief:

- An anchor present on one store only was created there (anchors are minted once, never
  reused), so additions merge as set union with no recorded state.
- Recorded per-anchor base hashes (kept per store pair under
  `~/.local/share/braincrawl/store-sync-state/`) arbitrate only edits and deletions. A base
  is recorded only for content **both** sides hold; a run that leaves the sides different
  records nothing and the reverse-direction sync converges the pair.
- Both-sides edits of the same node union their body lines when the heading matches;
  a heading changed on both sides is a per-node conflict, reported and never transferred.
- Deletions never propagate; moves (same anchor, different doc) are reported, not applied.
- Unanchored nodes are invisible to sync — run `collection assign-ids` first.
