---
title: API
summary: The consumer-facing store API — upsert and read for works, content, and citation edges. Every id parameter accepts any external identifier; braincrawl resolves it to a UUID internally, so consumers never resolve identity themselves.
tags: [architecture, core, api, contract]
deps: [doc02.01.01, doc02.01.02]
---

# API

The public contract a consumer (via its thin client / "magic script") uses to talk to
a braincrawl instance. The consumer fetches metadata and content from upstream itself;
braincrawl is the **place it puts that data and reads it back**, so research
accumulates across questions and projects instead of being re-fetched.

## Any id in, UUID out

**Every id argument accepts any external identifier** — DOI, ISBN, OCLC, PMID,
OpenAlex `W…`, or a braincrawl UUID. The API resolves it to the UUID
internally before doing anything (see Id Resolution, `doc02.01.02`). Consumers never
resolve, crosswalk, or dedupe identity themselves — they store and query by
whatever id they happen to hold, and braincrawl guarantees all ids of one resource
land on the same node.

## Surface

### Works & content (Library + identity)

```
put_work(id, metadata, aliases[])      # upsert; resolves/mints UUID, records aliases
get_work(id)                           # → metadata; id = any alias or the UUID
put_content(id, kind, bytes,           # kind = abstract | fulltext
            source, fetched_at)
get_content(id, kind)                  # → bytes | pending | absent
```

Writes are **idempotent upserts**: putting the same work twice converges on one node
rather than duplicating. Content provenance rides with each put (see
Layers, `doc02.01.01`).

### Citation edges (Catalog)

```
put_edges([{ src, dst, relation,       # src/dst accept any id; resolved to UUIDs
             source, attrs }])         # unknown endpoints become catalog entries with no metadata
get_edges(id, dir)                     # dir = forward (cited_by) | backward (references)
```

Each put is a provider **assertion**, not an overwrite: the same logical edge asserted
by several providers is deduped on `(src, dst, relation)` while each provider's claim
and its sparse `attrs` (e.g. S2 intent/context, reference position) are retained for
read-time merging.

### Accumulation check

```
have(ids[])                            # → which ids are already present
```

Lets a consumer skip re-fetching what an earlier question already stored — the
accumulation payoff that distinguishes braincrawl from one-shot research.

> A later session fleshes out request/response shapes, pagination on `get_edges`
> (hubs are large), and the read-time edge merge policy.
