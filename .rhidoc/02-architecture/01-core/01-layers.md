---
title: Layers
summary: Core splits into two storage worlds — Layer 1 is raw bytes in a blob store keyed by canonical id, Layer 2 is the metadata database that holds every fact, including the facts about the bytes. Identity is the shared spine both depend on.
tags: [architecture, core, layers, storage, separation]
deps: []
---

# Layers

The separation is by **storage kind**, not by feature:

- **Layer 1 — blob store (R2).** Raw bytes only: a work's abstract, its full-text
  PDF, or nothing. Keyed by `{canonical_id}/{kind}/v{version}`. Holds no truth and
  is never queried for facts — it is addressed by Layer 2.
- **Layer 2 — metadata DB (D1).** The source of truth for everything relational:
  identity, the citation graph, and the descriptors of what sits in the blob store.
  If a thing can be queried, filtered, or versioned, it lives here, not on the object.

> Object stores carry per-object custom metadata, but it is not queryable and drifts
> easily. Any such metadata is a redundant convenience copy — Layer 2 remains the
> single source of truth.

## Identity — the shared spine

Both layers point at a **canonical id** — a braincrawl-minted GUID. Every external id
(doi, isbn, oclc, pmid, OpenAlex `W…`, …) is an alias of that GUID. Identity is its
own concern, resolved by braincrawl on the consumer's behalf; see Id Resolution
(`doc02.01.02`) for how external ids converge onto one GUID.

The backing store lives in the metadata DB: an **alias multimap**
(`canonical_id → {namespace, value}[]`) and a flat **KV** resolver projection
(`namespace:value → canonical_id`). The citation graph and `payloads` rows store
**canonical ids only**. A `dst_id` with no backing `works` row is a **stub** — a known
identifier with no metadata yet.

## What describes the blobs

A `payloads` table in Layer 2 records one row per stored blob and answers both
"what is in the bucket" and "where did it come from":

| Column | Role |
|---|---|
| `canonical_id` | which work |
| `kind` | `abstract` \| `fulltext` — what the blob is |
| `version` | bumped on re-fetch; old bytes keep their key |
| `r2_key` | `{canonical_id}/{kind}/v{version}` |
| `content_hash` | dedup / change detection |
| `byte_size`, `mime` | |
| `rights` | `open` \| `link_only` \| `restricted` |
| `source`, `source_url`, `fetched_at` | provenance + freshness, per version |
| `is_current` | newest-version flag |

Consequences:

- **abstract vs fulltext vs nothing** is the `kind` column; "nothing" is the absence
  of any `payloads` row (which is what makes a work a metadata-only node).
- **Versioned per-blob metadata** (fetch date, source) is a new row per re-fetch with
  `version + 1`; full history is retained.
- **Node materialization status** (`stub → metadata → abstract → fulltext`) is
  *derived* from the presence of `works` and `payloads` rows, not stored separately.

## Building Layer 1 and Layer 2 separately

Once identity is extracted as the spine, the two layers are independent workstreams:

- Layer 1 work is byte handling — fetch, store, rights-gate, version. It depends on
  the spine for the id, nothing more.
- Layer 2 work is relational — aliases, edges, provenance, `payloads`. It creates
  stubs through the spine but never reads bytes.

The single ordering constraint is **spine first**. The ingestion fetch stays in the
spine's per-work coordinator so one upstream call feeds both layers and respects the
provider rate budget.
