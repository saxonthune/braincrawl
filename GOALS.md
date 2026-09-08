# braincrawl — Goals

A general-purpose system for building a **reusable academic knowledge graph** from open
scholarly metadata, with a clean separation between general (shared) knowledge and
domain-specific (per-consumer) projections. It can be queried separately from any single
research task, so general knowledge accretes once and is reused.

## Motivation

The trigger was a methodology problem with one-shot "deep research" pipelines. Those are
*falsificationist*: they extract claims and run them through an adversarial kill-gate, so
material is discarded before it is ever inventoried. That optimizes for precision over
recall and answers a *fact-check* question.

But entering a new field is a *coverage* problem, not a fact-check problem. A pre-AI
researcher works a funnel:

```
tertiary (handbooks, encyclopedias)   → the map + the names
   ↓
reviews / syntheses                   → the debates, inventoried (non-decisive on purpose)
   ↓
citation mining  (backward = canon, forward = rebuttals)
   ↓
primary / granular sources            → read critically, last
```

Verification happens *late* and *contextually* — claims are judged by where they sit in
the citation network, not refuted up front. The first three-quarters of the funnel is
inventory, not decisiveness.

braincrawl's thesis: **that funnel is a graph traversal.** Build the citation graph once,
persist it, and separate *coverage* (build — expensive, done once, inventory-shaped) from
*decisiveness* (query — cheap, repeatable, whatever lens you want at query time). The
verification step stops being a kill-gate and becomes a lens you choose when querying.

## Architecture — three layers

### Layer 1 — Corpus (work payloads + identity)

Individual works keyed by a **canonical internal id**, with external identifiers in an
alias table. Each work has:

- A **node materialization status**: `stub → metadata → abstract → fulltext`
  (distinct from the payload availability below).
- A **payload/rights status** gating what may be stored:
  `open` (store bytes) · `link_only` (store the OA/publisher URL, not bytes) ·
  `restricted` (fetch-and-discard at query time, never persist).
- Abstracts inline (small); full text in object storage **only** when rights allow.

Rights gating is structural, not advisory: it must be impossible to persist text we have
no right to store. Legal OA only (Unpaywall-resolved / public domain). No piracy caches.

### Layer 2 — Neutral metadata graph

Links works together: nodes (works/authors/venues/concepts) + citation edges. This is an
**accreting cache over upstream providers**, NOT a mirror of all scholarship (the full
graph is ~250M works / billions of edges and will not fit a single edge DB). It is
lazily populated from upstream on cache miss — the union of what consumers have touched.

Key properties:

- **Open-world**: citation edges reference works not yet ingested → **stub nodes** are
  first-class (a known identifier with no metadata yet).
- **Forward-citations are never "complete"** — new citing works appear forever. Nodes
  carry a TTL/refresh policy; there is no "done" flag for in-edges.
- **Provenance per field/edge**: the merge of OpenAlex/Crossref/S2 is not "neutral."
  Record which source asserted each field, with a documented merge policy.
- **Edges carry the citer's concept/topic** so domain projections can filter by field
  (see "citation drift" below).

### Layer 3 — Consumer graphs (per-domain projections)

Each consumer maintains its own graph as **annotation-over-reference, never a copy**:

- a **selection set** of canonical ids,
- private **annotations** (tags, weights, notes),
- consumer-defined **edges** (e.g. `supports`, `refutes`, domain relations).

Shared metadata is referenced from Layer 2 and hydrated at read time, so projections
never diverge from the general graph as it refreshes. When a projection needs more, it
queries Layer 2; the consuming app decides whether to materialize additional works.

This is the seam: general knowledge stays DRY and reusable; domain needs are isolated.

## Data sources

| Source | Role | Notes |
|---|---|---|
| **OpenAlex** | backbone | best coverage incl. humanities; full citation graph both directions; concept/topic taxonomy; free, no key |
| **Semantic Scholar** | enrichment | citation **contexts + intent** (background vs. built-upon); abstracts; thinner humanities coverage |
| **Crossref** | identity | DOI authority, dedup, reference lists |
| **Unpaywall** | full text | legal OA resolution (Layer 1), DOI-keyed |
| **PubMed** | enrichment | MeSH terms (biomed) |
| arXiv | (general-purpose only) | STEM preprints; irrelevant to humanities domains |
| CDLI / ORACC | domain primary sources | separate id namespace, outside the citation graph |

**OpenAlex is the primary backbone.** Semantic Scholar complements it with citation
intent. The rest are keyed lookups off the ids OpenAlex provides.

## Identity & crosswalk

The OpenAlex `ids` block is the crosswalk hub. One work fetch yields the other providers'
native keys; from there every complementary lookup is a **keyed GET, no fuzzy matching**:

- Crossref: `api.crossref.org/works/{doi}`
- Semantic Scholar: `…/paper/DOI:{doi}` (also accepts `MAG:`, `PMID:`, `ARXIV:`, `CorpusId:`)
- Unpaywall: `api.unpaywall.org/v2/{doi}?email=`
- PubMed: E-utilities by `{pmid}`

**Determinism is conditional** — it holds iff the node carries a shared strong id. No DOI
(common for humanities books/chapters) → DOI-keyed crosswalks are unavailable, fall back
to title+author+year matching or accept OpenAlex-only coverage. No ORCID (older records)
→ author identity is non-deterministic; lean on OpenAlex author clustering or VIAF/ISNI.

### Identifier formats to store

The alias table must be a **namespaced multimap** (`canonical_id → {namespace, value}[]`),
NOT DOI-centric — for much of the humanities the only strong id is OCLC or ISBN.

- **Canonical**: OpenAlex ids (`W…` works, `A…` authors, `S…` sources, `C…`/`T…` concepts/topics).
- **Strong join keys (index these)**: DOI, Semantic Scholar `CorpusId` + `paperId`, MAG id,
  PMID/PMCID, arXiv id.
- **Books & humanities (coverage savers)**: ISBN (10 + 13), OCLC/WorldCat number, LCCN.
- **Entity-level**: ORCID + VIAF/ISNI (authors); ISSN-L + ISSN (venues).
- **Domain primary sources**: CDLI P-numbers, ORACC ids (separate namespace, linked to the
  works that edit/translate them).

Practical default: canonical = OpenAlex `W…`; index DOI + ISBN + OCLC + S2 CorpusId as join
columns; keep the rest as alias rows. ISBN + OCLC are specifically what keep humanities
coverage from collapsing to "DOI or nothing."

## Graph-building strategy (validated against the live OpenAlex API)

Tested on the landmark Jacobsen & Adams 1958, "Salt and Silt in Ancient Mesopotamian
Agriculture" (`W2031938753`, 542 citations). Findings that shape the strategy:

1. **Seed via search** works well (landmark returned as the #1 hit).
2. **Expand forward, not backward.** The 1958 paper has only **2** `referenced_works` —
   backward edges are largely missing for old/humanities works. Forward (`cited_by`) is
   rich (542). Snowball forward, the opposite of how a human reads bibliographies.
3. **Citation drift is the main failure mode.** The most-cited descendants of this
   ancient-history paper are modern **plant-biology/agronomy** works ("Improving crop salt
   tolerance", etc.). Naive forward-mining sorted by influence marches out of the field.
4. **Fix drift at the edge query**: filter `cites:{id}` by `concepts.id:{field}` so
   expansion stays in-domain. Drift control belongs in the edge query, and the concept
   taxonomy is the spine that keeps Layer 3 projections coherent.
5. **Keyword search is noisy** ("debt cancellation mesopotamia" surfaced an IMF
   macroeconomics paper, missed the actual canon). Use concept/topic filters + semantic
   similarity (vector over abstracts) to recover what keyword search misses, and rank canon
   by in-degree **within the topic-filtered subgraph**, not by global citation count.

## API surface (sketch)

Canonical ids everywhere; every response carries provenance + freshness.

**Layer 1 — Corpus**
```
POST /works:resolve        { ids:[doi|isbn|oclc|arxiv|…] } → { canonical_id, aliases[] }
GET  /works/{id}           → metadata (+provenance, +freshness)
GET  /works/{id}/abstract
GET  /works/{id}/fulltext  → 200 bytes | 202 pending | 451 restricted | 302 link_only
POST /works:ingest         { id } → { job_id, status }   (async materialize)
GET  /works/{id}/status    → { node_status, payload_status, rights, fetched_at, ttl }
```

**Layer 2 — Metadata graph**
```
GET  /works/{id}/edges?dir=out|in&limit=&cursor=     (paginated; hubs are huge)
POST /graph/neighborhood   { seeds[], depth, dir, max_nodes, filters } → bounded subgraph
GET  /search?q=&type=topic|title|author&filter=
POST /search/semantic      { text, k }               (vector over abstracts)
GET  /authors/{id} | /venues/{id} | /topics/{id}
```

**Layer 3 — Consumer graphs**
```
POST /collections                          { name } → { collection_id }
POST /collections/{c}/nodes                { ids[] }                 (selection set, refs only)
POST /collections/{c}/edges                { src, dst, type, weight } (domain edges)
POST /collections/{c}/annotations          { node_id, tags, notes, scores }
GET  /collections/{c}/graph?hydrate=true   → projection hydrated from Layer 2
POST /collections/{c}/expand               { policy, budget } → candidate works to pull in
```

`expand` is the "do I need more works?" hook: it queries Layer 2 from the current
selection's frontier, ranks candidates by a consumer policy (centrality / recency /
semantic distance), and returns *proposals*. The app confirms; confirmed ids trigger
Layer 1 ingestion.

## Target platform — Cloudflare

| Concern | Primitive |
|---|---|
| API / compute | Workers |
| Layer 2 graph + Layer 3 collections | D1 (SQLite) — consider per-tenant DB for L3 isolation |
| Full-text blobs (OA / public-domain only) | R2 |
| Identity-resolution + status hot cache | KV |
| Per-work ingestion coordinator (dedup, upstream rate-limit) | Durable Objects |
| Async ingestion pipeline | Queues |
| Semantic entry / similarity | Vectorize |

The Durable-Object-per-work-id pattern is what prevents the thundering-herd problem:
concurrent cache misses for the same work coalesce into one upstream fetch, and it is the
natural place to enforce OpenAlex/S2 rate budgets.

## Known design flaws to handle (do not skip)

1. **DOI/ISBN as primary key fragments the graph** — use a canonical internal id + namespaced alias multimap.
2. **Open-world edges** — stub nodes are first-class; never ingest the transitive closure.
3. **Layer 2 is a cache, not a mirror** — lazily populated; "complete" is unreachable for forward edges; TTL everything.
4. **"Neutral" metadata isn't neutral** — per-field provenance + explicit merge policy.
5. **Synchronous cache-miss fetch = thundering herd** — async ingestion + per-id coordinator (Durable Objects/Queues).
6. **Layer 3 must be annotation-over-reference**, not a copy, or projections diverge.
7. **Full-text rights gating** — structural, per-work; legal OA only.
8. **N+1 traversal** — batch neighborhood retrieval, not node-at-a-time.
9. **Bootstrapping gap** — search + semantic entry, because entering a new field means you don't know the seeds yet.
10. **Citation drift** — gate forward expansion by concept/topic at the edge query.

## Open decision for implementation

Is Layer 2 strictly a **lazy cache** (only ever the accreted union of consumer interest —
bounded, cheap) or does it also get **periodic bulk backfill from OpenAlex snapshots**
(eager, expensive, better cold-start for new domains)? This drives whether the OpenAlex
data-dump pipeline is needed at all, and is the main fork in build cost. Leaning lazy-cache
first; add backfill only if cold-start coverage proves inadequate.
