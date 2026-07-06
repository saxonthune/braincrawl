---
title: Glossary
summary: Plain-language names for braincrawl's three layers (Library, Catalog, Research Collection), written as ORM-style verbalized facts; defines topic coverage.
tags: [glossary, vocabulary, terms, product, facts, coverage]
deps: []
---
# Glossary
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
- **Research Collection** (Layer 3) — a consumer's own research, made of **Research Documents** that point at UUIDs in the catalog rather than copying them. A Research Document is the singular artifact: questions, selected works, findings, and relations for one line of work.
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
- **Citation scatter** — a heavily-cited source is referenced by works across many unrelated domains, so following its forward citations (`cited-by`) by raw influence pulls in off-topic works and marches out of the field. It is a single-hop property of an influential node, not a gradual wandering — and not to be called **drift**, which in braincrawl means a copy diverging from its source of truth.
- **Topic-gating** — the control for scatter: filter forward expansion by an OpenAlex concept/topic so the citing works stay in-domain. Rank canon by in-degree *within* the topic-gated subgraph, not by global citation count.
## The library: artifacts
An **artifact** is a stored piece of content attached to a catalog entry — an abstract, a fulltext, an LLM-summary projection, or another asset.

- An artifact's **role** says what the content is to the work: `abstract`, `fulltext`, or a custom slug such as `map`. Its **mime** says how the bytes are encoded — `application/pdf`, `image/png`. The two are independent.
- An artifact also records its **provenance**: a source label, a source URL, and when it was fetched. Each push of the same role is kept in sequence, with the latest marked current.
- An artifact attaches to a catalog entry that already exists; pushing content never creates the entry. Writing to the catalog is a separate door.
- **`chunk`** is a self-contained CLI tool braincrawl provides that partitions a stored fulltext into citation-carrying pieces, each carrying page provenance — a secondary derived artifact of a work.
## Providers
A provider has two roles: pulling information from the provider's store, and pushing it to the user's braincrawl store.

- `pulls-from(provider, source)` and `pushes-to(provider, braincrawl-store)` — the two provider facts.
- A provider's store is usually accessed via an API.
- What a provider pulls and pushes is **catalog entries**, **artifacts**, or both, depending on the operation.
