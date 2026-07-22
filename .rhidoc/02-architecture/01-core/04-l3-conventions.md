---
title: L3 Conventions
summary: An L3 Research Document is markdown with two required frontmatter fields (doc, updated) and a body of research nodes — headings carrying property and link lines. Node anchors are store-global; reference ids, never copy metadata. A worked example ships with the braincrawl skill.
tags: [architecture, core, l3, research-collection, conventions, node-grammar]
deps: [doc02.01.01]
---

# L3 Conventions

One markdown file per item under the consolidated store. A worked example is
`.claude/skills/braincrawl/l3-example.l3.md`; the rules:

- **Frontmatter** — two required fields (`l3 check` warns if missing): `doc:`, a kebab-case
  slug that is the filename stem and primary key; and `updated:`, a date the CLI stamps.
- **Body** — only research nodes, no loose prose and no H1. A `## ` heading opens a node with
  an opaque-prose title; `l3 assign-ids` appends its `^r-…` anchor — never write one by hand.
- **Property line** — `- key: value`, the value optionally one YAML flow map
  (`{role: core, why: '…'}`); quote any value holding a comma. `- tags: #a #b` is special —
  each `#`-token is a node label, not a property.
- **Link line** — `- kind [[target]]` (source is the enclosing node) or `- [[src]] kind
  [[dst]]`, with an optional trailing `{props}`. A target is a node anchor `^r-…`, a bare
  `slug` (that doc's intro node), or any `namespace:value` catalog reference
  (`openalex:`/`doi:`/`isbn:`/`pmid:`/…, or `uuid:` for a work referenced by its canonical
  id directly). A `[[wikilink]]` inside a property value is prose, not a link.
- **Anchors are store-global.** `assign-ids` keeps every `^r-…` unique across the whole store,
  so a bare `^r-…` resolves to its node from **any** doc — no slug prefix — and the reference
  survives if that node later moves to another doc.
- **Link vocabulary** — `catalog` links a node to a work; `contradicts` is the standard
  claim-link (never `refutes`); `supports`/`builds-on`/`relates-to`/`bridges`/`complicates`
  are free domain words. A `reading: {role, why}` property plus a `catalog` link marks a
  recommended reading (`role` = `start-here`/`core`/`rigor`/`reference`, or any string).
- **The one hard rule** — reference, never copy: store references, look facts up from the server
  at read time so a doc never drifts. A work's canonical identity is its store-generated UUID; name
  it with an `openalex:`/`doi:` reference that resolves to that UUID — never treat the provider
  id as the identity itself.
