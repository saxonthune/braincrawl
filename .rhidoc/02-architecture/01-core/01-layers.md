---
title: Layers
summary: Core splits into two storage worlds — the Library (Layer 1) is raw bytes in a blob store keyed by UUID, the Catalog (Layer 2) is the metadata database that holds every fact, including the facts about the bytes. Identity is the shared concern both depend on.
tags: [architecture, core, layers, storage, separation]
deps: []
---

# Layers

The separation is by **storage kind**, not by feature:

- **Library (Layer 1) — blob store (R2).** Raw bytes only: a work's abstract, its full-text
  PDF, or nothing. Keyed by `{canonical_id}/{kind}/v{version}`. Holds no truth and
  is never queried for facts — it is addressed by the Catalog.
- **Catalog (Layer 2) — metadata DB (D1).** The source of truth for everything relational:
  identity, the citation graph, and the descriptors of what sits in the blob store.
  If a thing can be queried, filtered, or versioned, it lives here, not on the object.

> Object stores carry per-object custom metadata, but it is not queryable and drifts
> easily. Any such metadata is a redundant convenience copy — the Catalog remains the
> single source of truth.

## Identity — the shared concern

Both layers point at a **UUID** — a braincrawl-generated identifier. Every external id
(doi, isbn, oclc, pmid, OpenAlex `W…`, …) is an alias of that UUID. Identity is its
own concern, resolved by braincrawl on the consumer's behalf; see Id Resolution
(`doc02.01.02`) for how external ids converge onto one UUID.

The backing store lives in the metadata DB: an **alias multimap**
(`canonical_id → {namespace, value}[]`) and a flat **KV** resolver projection
(`namespace:value → canonical_id`). The citation graph and `payloads` rows store
**UUIDs only**. A `dst_id` with no backing `works` row is a catalog entry with no
metadata yet — a citation target braincrawl knows exists but has not fetched.

## What describes the blobs

A `payloads` table in the Catalog records one row per stored blob and answers both
"what is in the bucket" and "where did it come from":

| Column | Role |
|---|---|
| `canonical_id` | which work |
| `kind` | an open-ended role slug — what the blob is (`abstract`, `fulltext`, or a derived artifact such as `chunks`) |
| `version` | bumped on re-fetch; old bytes keep their key |
| `r2_key` | `{canonical_id}/{kind}/v{version}` |
| `content_hash` | dedup / change detection |
| `byte_size`, `mime` | |
| `source`, `source_url`, `fetched_at` | provenance + freshness, per version |
| `is_current` | newest-version flag |

Consequences:

- A work holds raw `fulltext` plus arbitrarily many secondary derived or compressed
  artifacts (`chunks` is the first) under the same open-ended `kind` slug; "nothing"
  is the absence of any `payloads` row (which is what makes a work a metadata-only
  node).
- **Versioned per-blob metadata** (fetch date, source) is a new row per re-fetch with
  `version + 1`; full history is retained.
- **Node materialization status** (citation target with no metadata → metadata only →
  abstract → fulltext) is *derived* from the presence of `works` and `payloads` rows,
  not stored separately.

## Building the Library and Catalog separately

Once identity is extracted as its own concern, the two layers are independent lines of work:

- Library work is byte handling — fetch, store, version. It depends on
  identity for the id, nothing more.
- Catalog work is relational — aliases, edges, provenance, `payloads`. It records
  citation targets with no metadata yet through id resolution but never reads bytes.

The single ordering constraint is **identity first**. The ingestion fetch stays in the
per-work coordinator (`doc02.01.02`) so one upstream call feeds both layers and respects
the provider rate budget.
