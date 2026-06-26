---
title: Glossary
summary: Plain-language names for braincrawl's three layers — Library, Catalog, and Research Collection.
tags: [glossary, vocabulary, terms, product]
deps: []
---
# Glossary
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
## The library: artifacts
An **artifact** is a stored piece of content attached to a catalog entry — an abstract, a fulltext, an LLM-summary projection, or another asset.

- An artifact's **role** says what the content is to the work: `abstract`, `fulltext`, or a custom slug such as `map`. Its **mime** says how the bytes are encoded — `application/pdf`, `image/png`. The two are independent.
- An artifact also records its **provenance**: a source label, a source URL, and when it was fetched. Each push of the same role is kept in sequence, with the latest marked current.
- An artifact attaches to a catalog entry that already exists; pushing content never creates the entry. Writing to the catalog is a separate door.
## Providers
A provider has two roles: pulling information from the provider's store, and pushing it to the user's braincrawl store.

- A provider's store is usually accessed via an API.
- What a provider pulls and pushes is **catalog entries**, **artifacts**, or both, depending on the operation.
