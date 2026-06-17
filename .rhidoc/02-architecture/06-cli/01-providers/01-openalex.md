---
title: OpenAlex
summary: The braincrawl openalex namespace — a deliberate fork to the OpenAlex API exposing six read-only base verbs (get, search, find, autocomplete, cited-by, refs). Trimmed structured output, push-to-store by default.
tags: [cli, providers, openalex, fork, verbs, citations]
deps: [doc02.06.01, doc02.01.03]
---

# OpenAlex

`braincrawl openalex` is the provider fork (doc02.06.01) onto the
[OpenAlex](https://openalex.org) API — a free index of scholarly works, authors,
sources, institutions, topics, publishers, and funders, with citation edges
between works. It queries OpenAlex directly, bypassing routing, and returns
trimmed, structured records.

The full upstream contract — endpoints, entity schemas, filter keys, ID forms,
rate limits — lives in the `openalex-reference` skill. This doc records *intent*:
the base vocabulary, output shape, and persistence behavior.

## Base vocabulary

A small, orthogonal set of verbs. Each is entity-generic where it makes sense
(`<entity>` ∈ works, authors, sources, institutions, topics, keywords,
publishers, funders). Complex workflows compose these verbs; they are not
extended with bespoke flags.

| Verb | Binds to | Intent |
|---|---|---|
| `get <id>` | `GET /{entity}/{id}` | One entity by *any* ID (DOI, ORCID, ROR, ISSN, OpenAlex ID …). Entity inferred from the ID. |
| `search <entity> <query>` | `?search=` | Full-text relevance search → trimmed list. |
| `find <entity> <k:v…>` | `?filter=` | Structured filter query. Filter keys are validated against the known set and **fail loudly** on an unknown key — OpenAlex itself returns an empty result rather than an error, so the fork refuses to hide a typo. |
| `autocomplete <entity> <q>` | `/autocomplete/{entity}` | Cheap type-ahead, for ID disambiguation. |
| `cited-by <id>` | `?filter=cites:<id>` | Works that cite the given work. Encodes the counterintuitive filter direction. |
| `refs <id>` | hydrate `referenced_works` | Works the given work cites. Reads the work, then batch-fetches its references (OR-by-id, ≤50 per request). Not a single filter call. |

`cited-by` and `refs` are first-class base verbs, not workflows, because each
hides non-obvious upstream mechanics the caller should not have to reproduce.

## Output

Default output is a **structured envelope**, machine-parseable so both an agent's
context and braincrawl's own routing layer can consume it:

```json
{
  "query":   { "entity": "works", "resolved_filter": "...", "url": "..." },
  "count":   1234,
  "returned": 25,
  "truncated": true,
  "next_cursor": "...",
  "results": [ /* trimmed records */ ]
}
```

`query.resolved_filter` and `truncated` are deliberate affordances: they let a
caller distinguish "valid query, zero matches" from "malformed query", and make
truncation explicit rather than silent. Trimmed records keep a curated top-level
field set per entity, shaped close to the metadata server's own model so the push
mapping is near-identity. `--full` returns untrimmed records; `--text` renders the
compact human view. Abstracts (stored upstream as an inverted index) are
reconstructed to plain text only on request.

## Upstream client

The namespace owns the deterministic plumbing of talking to OpenAlex so callers
never reproduce it: the `api_key` is read from configuration (doc02.06); `429`
and throttle responses are retried with backoff; `--all` drains cursor
pagination to exhaustion; a malformed filter is caught before the request, not
discovered as an empty result.

## Persistence

Per doc02.06, results **push to the metadata server by default** alongside the
context return, over HTTP to the server's upsert API (doc02.01.03). The fork is
about reaching OpenAlex directly, not about discarding what it returns.
`--skip-push` suppresses persistence for a read-only-to-context call.

Pushing maps an OpenAlex entity onto the store's node model. The mapping is thin
because a node's `attrs` is free-form JSON — the trimmed record is the payload;
only identity and kind are extracted:

| OpenAlex entity | Node kind | Alias namespaces extracted |
|---|---|---|
| works | `Work` | `openalex`, `doi`, `pmid`, `pmcid`, `mag` |
| authors | `Author` | `openalex`, `orcid` |
| sources | `Venue` | `openalex`, `issn_l`, `issn` |
| topics | `Topic` | `openalex` |
| concepts | `Concept` | `openalex`, `wikidata` |
| institutions, publishers, funders, keywords | — (no node kind) | not pushed — query-only |

Citation traversal pushes edges: `cited-by` and `refs` emit `cites` edges
(`src` = citing alias, `dst` = cited alias) to the edges endpoint. Entities
without a node kind have no push target, so their verbs are context-only
regardless of `--skip-push`. Pushed records hand their extracted aliases to the
upsert API and let the metadata server resolve identity (doc02.01.02); the
namespace never merges identity itself.
