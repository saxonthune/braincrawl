---
title: L3 Conventions
summary: The settled contract for an L3 Research Document — two required frontmatter fields (doc, updated) plus one node grammar every doc's body follows (headings, property lines, link lines, and the controlled edge vocabulary). Reference the ids, never copy their metadata.
tags: [architecture, core, l3, research-collection, conventions, node-grammar]
deps: [doc02.01.01]
---

# L3 Conventions

An L3 Research Document is a plain markdown file, one per item, under the consolidated
store. Its contract has two parts: a small required frontmatter, and a body written in one
shared node grammar.

## Frontmatter

Two required fields, checked by `braincrawl l3 check`:

- `doc:` — a kebab-case slug. It is both the primary key and the filename stem.
- `updated:` — a date, stamped by the CLI.

## The node grammar

Below the frontmatter, everything is a sequence of research nodes. There is no loose prose
and no top-level heading (H1) — the file goes straight from frontmatter to the first node.

**A node** opens with a `## ` heading. Its title is opaque prose — write anything that helps
a reader. The tooling appends a trailing `^r-…` anchor to the heading; agents never write one
themselves — `braincrawl l3 assign-ids` assigns it.

**A property line** is `- key: value`. A value may hold one YAML flow map, e.g.
`{k: v, k2: 'v, with a comma'}` (quote any value containing a comma). One key is special:
`- tags: #a #b` — each `#`-prefixed token becomes a label on the node, not a property.

**A link line** connects two endpoints:

- `- kind [[target]]` — the source is implicit (the enclosing node).
- `- [[src]] kind [[dst]]` — both endpoints written out.
- optional trailing `{props}` on either form.

A target is one of:

- `^r-…` — a node in this doc.
- `slug#^r-…` — a node in another doc.
- a bare `slug` — a doc-level forward reference, pointing at that doc's intro node.
- `openalex:…` or `doi:…` — a catalog entry (a work, not a node).

`kind` is lowercase-kebab vocabulary. `catalog` is the convention for a work-link (node →
catalog id). `contradicts` is the blessed word for a claim-link — never write `refutes`.
`supports`, `builds-on`, `relates-to`, `bridges`, and `complicates` are free domain
vocabulary for how one node or claim relates to another.

A `[[wikilink]]` written inside a property *value* is just text — it is not a graph edge.
Only a `- kind [[target]]` bullet line becomes a link.

## The one hard invariant

**Reference, never copy.** An L3 doc stores ids, not metadata. Look facts back up from the
server at read time, so a doc never drifts from the graph it annotates. UUIDs are the join
key — prefer `openalex:W…`; the store resolves other id forms (DOI, ISBN, …) to the same
UUID, so any form is safe as long as it is consistent within a doc.

## Experimental conventions

Two leans from real sessions, not yet promoted into the required grammar above:

- **Attributed voice for a source's claims.** A source's contested theoretical claim is
  reported ("Sohn-Rethel holds that value is a real abstraction"), not asserted in the doc's
  own voice. The doc may assert as settled only graph facts (ids, edges) and the user's own
  positions.
- **Discourage catalog/artifact-state markers in L3.** Don't annotate a node with facts about
  what the store currently holds (e.g. "fulltext stored", "no abstract found") — those are
  look-up-able store facts that go stale silently when an artifact is re-fetched or evicted.
  If "which of my selections can I read deeply right now" becomes a real need, it belongs in
  a store query, not a written note.
