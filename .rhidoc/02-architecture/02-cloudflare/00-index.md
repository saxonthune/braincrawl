---
title: Cloudflare
summary: The deployment target — maps core abstractions onto Cloudflare edge primitives (Workers, D1, R2, KV, Durable Objects, Queues, Vectorize). Durable-Object-per-work-id coalesces cache misses and enforces upstream rate budgets.
tags: [architecture, cloudflare, deployment, infrastructure]
deps: []
---

# Cloudflare — deployment target

One concrete binding of the core model (`doc02.01`) onto Cloudflare edge primitives.

| Concern | Primitive |
|---|---|
| API / compute | Workers |
| Catalog graph + Research Collections | D1 (SQLite) — consider per-tenant DB for Research Collection isolation |
| Full-text blobs (OA / public-domain only) | R2 |
| Identity-resolution + status hot cache | KV |
| Per-work ingestion coordinator (dedup, upstream rate-limit) | Durable Objects |
| Async ingestion pipeline | Queues |
| Semantic entry / similarity | Vectorize |

The **Durable-Object-per-work-id** pattern prevents the thundering-herd problem:
concurrent cache misses for the same work coalesce into one upstream fetch, and it is
the natural place to enforce OpenAlex/S2 rate budgets.
