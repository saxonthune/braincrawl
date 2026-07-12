---
title: Glossary
summary: Plain-language names for braincrawl's three layers (Library, Catalog, Research Collection), written as ORM-style verbalized facts; defines topic coverage.
tags: [glossary, vocabulary, terms, product, facts, coverage]
deps: []
---
# Glossary
This glossary is a **controlled vocabulary**: one preferred term per concept, kept precise so it does not drift. Adhere to it — use the term it defines rather than a coined synonym — and contribute to it: when you need a concept it does not yet name, propose an addition here instead of minting a term in passing.

**Naming new concepts.** Names are load-bearing — every doc, type, and session inherits them — so the user chooses them. When work reaches a concept this glossary does not name, the agent lays out the naming decision (candidates, collisions with existing entries, tradeoffs) and the user commits the term here before anything fans out through it. Prefer combining existing terms over minting a new one; a candidate must be distinct from every existing entry and specific enough to stand alone out of context.
## Facts — the grammar
This glossary's relationships are written as **verbalized facts** (ORM-style): one affirmable subject–verb–object(–…) sentence per relationship, with an optional `predicate(role, role)` shadow where a fact wants to be linted or queried. Four kinds, one grammar:

- **Entity** — a term. *Catalog is-a shared index over Library.*
- **Split** — a partition by a criterion. *Knowledge splits into shared and owned, by ownership.*
- **Relation** — an n-ary directional fact. `pushes-to(provider, braincrawl-store)`, `pulls-from(provider, source)`.
- **Derived** — a fact computed from other facts. `coverage(topic) := count{ artifact : relates-to(artifact, topic) }`.

Facts are n-ary and native: a fact may bind two, three, or four roles without reifying into triples. A property a fact can compute is never stored as a standing status — that copy would drift; state the base fact and derive the quantity at read time. Not every entry is written as a fact.

## The three layers
braincrawl separates knowledge built once and shared — the first two layers — from a consumer's own research, the third.

- **Library** (Layer 1) — the store of works keyed by a UUID, holding each work's artifacts.
- **Catalog** (Layer 2) — the shared index over the library: every work's metadata, which artifacts are held and where, and the citation links between works.
- **Research Collection** (Layer 3) — a consumer's own research, made of **Research Documents** that point at UUIDs in the catalog rather than copying them. A Research Document is the singular artifact: questions, selected works, findings, and links for one line of work.
- **braincrawl store** (or sometimes just **braincrawl** as a noun) — refers to everything: L1, L2, L3.
- **L1 / L2 / L3** — shorthand for Library, Catalog, Research Documents.
## The catalog: nodes and edges
- The catalog stores **nodes** — most commonly articles, books, and authors — and the **edges** between them.
- The catalog is comprised of **catalog entries**, which form the nodes of the graph.
- A **citation** is the primary edge between catalog entries — one work citing another.
- braincrawl tries to map different identification schemes into a single **UUID**.
- An **identifier** (or alias) is one identification scheme — such as a DOI, ISBN, or OCLC number — that braincrawl maps to a UUID.
## Topics and coverage
- A **topic** (or question) is what works are *about* — an OpenAlex concept/topic, or a consumer's research question.
- `relates-to(artifact, topic)` — an artifact bears on a topic.
- `has-entries-on(catalog, topic)` — the catalog holds entries on a topic, or it does not.
- **Coverage** — `coverage(topic) := count{ artifact : relates-to(artifact, topic) }`. Coverage is a *number* — the count of artifacts related to a topic — not a state. "Thin" or "strong" coverage merely describes that number. `has-entries-on` and `coverage` are looked up from the store at read time, never written into a Research Document (where they would drift — see the L3 reference-never-copy invariant).
## Graph traversal
- **Fetching citations** — following a work's citation edges to pull related works into the Catalog: forward with `cited-by`, backward with `refs`. The plain names for the action are **fetch citations** and **search citations**; the opening lookup for a heavily cited work is a **search**. There is no other name for it — not "snowball", "seed", or "coverage push".
- **Heavily cited work** — a work with high in-degree, the plain name for an important node. Say **work** or **heavily cited work**, not "hub" or "keystone".
- **Citation scatter** — a heavily-cited source is referenced by works across many unrelated domains, so following its forward citations (`cited-by`) by raw influence pulls in off-topic works and marches out of the field. It is a single-hop property of an influential node, not a gradual wandering — and not to be called **drift**, which in braincrawl means a copy diverging from its source of truth.
- **Topic-gating** — the control for scatter: filter forward expansion by an OpenAlex concept/topic so the citing works stay in-domain. Rank canon by in-degree *within* the topic-gated subgraph, not by global citation count.
## The library: artifacts
An **artifact** is a stored piece of content attached to a catalog entry — an abstract, a fulltext, an LLM-summary projection, or another asset.

- An artifact's **role** says what the content is to the work: `abstract`, `fulltext`, or a custom slug such as `map`. Its **mime** says how the bytes are encoded — `application/pdf`, `image/png`. The two are independent.
- An artifact also records its **provenance**: a source label, a source URL, and when it was fetched. Each push of the same role is kept in sequence, with the latest marked current.
- An artifact attaches to a catalog entry that already exists; pushing content never creates the entry. Writing to the catalog is a separate door.
- **`chunk`** is a self-contained CLI tool braincrawl provides that partitions a stored fulltext into citation-carrying pieces, each carrying page provenance — a secondary derived artifact of a work.
## The research graph
The catalog is a graph and the Research Collection carries a graph; they are separate graphs, and neither owns the bare word "graph."

- The **research graph** is the property graph lifted from a Research Collection's Research Documents by the tooling. A Research Document is the authoring surface; the research graph is what a parser extracts from it.
- A **research node** (code-facing shorthand: **l3-node**) is the atomic unit of a Research Document — an id-bearing block of content. A Research Document is a collection of research nodes.
- A **property** is a typed grouping of data on a research node, shared in format across nodes — remarks (freeform prose), catalog-references (pointers at catalog entries), tags.
- A **link** is a typed connection in the research graph. Its two endpoints are each a research node or a catalog entry; its type is a plain word, said before the word link — a *contradicts link*, a *supports link*, a *catalog link*. A link can carry properties of its own (a why, a basis).
- A **catalog link** is a link with a catalog entry at an endpoint — how a research node cites, uses, or disputes a work. The link always lives in the Research Collection; a catalog link never writes to the catalog.
- A **query** obtains a subset of the research graph by filters and constraints; its result can carry order and shape (the database sense of query). The plugin interface provides the entire research graph, which a query filters.
- A **Web UI plugin** provides views of the Web UI: its author decides what data to look for in the research graph and how the views render. The contract lives in the Web UI doc (doc01.03).

## Providers
A provider has two roles: pulling information from the provider's store, and pushing it to the user's braincrawl store.

- `pulls-from(provider, source)` and `pushes-to(provider, braincrawl-store)` — the two provider facts.
- A provider's store is usually accessed via an API.
- What a provider pulls and pushes is **catalog entries**, **artifacts**, or both, depending on the operation.
