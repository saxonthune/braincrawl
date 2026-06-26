---
title: Semantic Scholar
summary: The braincrawl semanticscholar namespace — a deliberate fork to the Semantic Scholar API exposing four read-only base verbs (get, search, cited-by, refs). Trimmed structured output, push-to-store by default.
tags: [cli, providers, semanticscholar, fork, verbs, citations]
deps: [doc02.06.01, doc02.01.03]
---

# Semantic Scholar

`braincrawl semanticscholar` is the provider fork (doc02.06.01) onto the
[Semantic Scholar](https://www.semanticscholar.org) API — a free index of scholarly
works and authors maintained by the Allen Institute for AI, with citation edges between
works. It queries the S2 Academic Graph API directly, bypassing routing, and returns
trimmed, structured records.

The full upstream contract — endpoints, entity schemas, ID forms, rate limits, and paging
— lives in the S2 API documentation. This doc records *intent*: the base vocabulary,
output shape, upstream client behavior, and persistence behavior.

## Base vocabulary

S2 exposes **works** (`papers`) and **authors** as its entity kinds. The namespace
provides four orthogonal verbs. Complex workflows compose these verbs; they are not
extended with bespoke flags.

| Verb | Entity | Binds to | Intent |
|---|---|---|---|
| `get <id>` | paper or author | `GET /graph/v1/paper/{id}` / `GET /graph/v1/author/{id}` | One entity by any supported ID. Entity inferred from the ID form. |
| `search <entity> <query>` | papers, authors | `GET /graph/v1/paper/search` / `GET /graph/v1/author/search` | Full-text relevance search → trimmed list. |
| `cited-by <id>` | paper | `GET /graph/v1/paper/{id}/citations` | Works that cite the given paper. |
| `refs <id>` | paper | `GET /graph/v1/paper/{id}/references` | Works the given paper cites. |

S2 does not expose `find` (structured filter) or `autocomplete` — those verbs exist only
in the OpenAlex namespace (doc02.06.01.01). Do not document or implement them here.

All verbs honor the global output flags: `--text`, `--json`, `--limit N`, `--all`,
`--fields a,b,c`, `--full`, `--abstract`, `--skip-push`.

## Output

Default output is the **shared structured envelope** (same shape as doc02.06.01.01):

```json
{
  "query":    { "entity": "papers", "url": "..." },
  "count":    1234,
  "returned": 25,
  "truncated": true,
  "results":  [ /* trimmed records */ ]
}
```

S2 uses offset/next paging (not cursor). `--all` drains all pages to exhaustion.
`--text` renders the compact human view (one result per line, tab-separated).
`--abstract` reconstructs the abstract field from S2's plain-text payload.

## Upstream client

The namespace owns the deterministic plumbing of talking to S2 so callers never
reproduce it:

- The `x-api-key` header is set from configuration (env `BRAINCRAWL_SEMANTICSCHOLAR_API_KEY`
  > config `semanticscholar_api_key` > none). Keyless calls share a throttled pool (~1 req/s);
  a key raises the per-caller quota significantly.
- `429` and throttle responses are retried with backoff.
- `--all` drains offset/next pagination to exhaustion.

## Persistence

Per doc02.06, results **push to the metadata server by default** alongside the context
return, over HTTP to the server's upsert API (doc02.01.03). `--skip-push` suppresses
persistence for a read-only-to-context call.

Pushing maps an S2 entity onto the store's node model. Only identity and kind are
extracted; the trimmed record is the payload:

| S2 entity | Node kind | Alias namespaces extracted |
|---|---|---|
| papers | `Work` | `s2`, `corpusid`, `doi`, `arxiv`, `mag`, `pmid`, `pmcid` |
| authors | `Author` | `s2author`, `orcid` |

Citation traversal pushes edges: `cited-by` and `refs` emit `cites` edges
(`src` = citing alias, `dst` = cited alias) to the edges endpoint.

Pushed records carry `source = "semanticscholar"` in their provenance. The DOI alias
namespace is shared with OpenAlex (doc02.06.01.01) — when both providers fetch the same
paper, the store's id-resolution layer (doc02.01.02) merges them onto one UUID
via the DOI overlap. The namespace never merges identity itself.
