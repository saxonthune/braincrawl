---
name: openalex-reference
description: "Reference for the OpenAlex scholarly API — endpoints, entity schemas, filter/search/pagination mechanics, IDs, rate limits. Use when querying OpenAlex (works, authors, sources, institutions, topics, publishers, funders) or building/debugging OpenAlex requests."
---

# openalex-reference

A contract reference for the [OpenAlex](https://openalex.org) scholarly-data API. OpenAlex is
a free index of ~250M scholarly works, authors, sources (journals), institutions, topics,
publishers, and funders, with citation edges between works.

## When This Triggers
- Building, reading, or debugging any request to `https://api.openalex.org`
- "look this paper/author up on OpenAlex", "filter works by ...", "how do I page OpenAlex"
- Mapping OpenAlex records into braincrawl's store (works, content, citation edges)
- `/openalex-reference`

## How To Use This Skill
The full raw contract lives in **`reference.md`** alongside this file. Read it for exact field
names, filter keys, and ID formats — don't guess them, OpenAlex returns `200` with empty
`results` on a malformed filter rather than erroring, so a wrong key fails silently.

Lookup order:
1. Need an **endpoint or entity schema**? → `reference.md` §3–9 (one section per entity).
2. Need **filter / search / sort / pagination / group_by syntax**? → `reference.md` §2.
3. Need **IDs, auth, rate limits, error codes**? → `reference.md` §1.

## The 10-Second Model
- **Base URL** `https://api.openalex.org`. JSON. List endpoints wrap results in
  `{meta, results, group_by}`; single-entity endpoints return the bare object.
- **Entities**: `/works`, `/authors`, `/sources`, `/institutions`, `/topics`, `/keywords`,
  `/publishers`, `/funders`, `/concepts` (deprecated → topics), plus `/countries`,
  `/continents`. Each has a list form and a `/{id}` single form.
- **Any ID works in `/{id}`**: OpenAlex ID (`W…`/`A…`/`S…`/`I…`/…) or an external ID
  (DOI, ORCID, ROR, ISSN, PMID…). E.g. `/works/doi:10.7717/peerj.4375`.
- **Query mechanics** all ride on the list endpoints: `filter=`, `search=`, `sort=`,
  `select=`, `group_by=`, `sample=`, and paging via `page`/`per_page` (≤10k results) or
  `cursor=*` (beyond 10k).

## Gotchas That Bite Agents
- **Filter syntax is stringly-typed**: AND = `,`, OR = `|`, NOT = `!`, range = `>`/`<`.
  A typo'd filter key → silent empty `results`, not an error. Verify keys in `reference.md`.
- **Abstracts** come as `abstract_inverted_index` (`{word: [positions]}`) — reconstruct by
  ordering words by position; there is no plain-text abstract field.
- **`select=` is top-level only** — `select=open_access.is_oa` errors; use `select=open_access`.
- **Basic paging caps at 10,000 results** (`page * per_page ≤ 10000`); use cursor paging past
  that. Sampling (`sample=`) can't combine with `sort` or `page`.
- **Auth is now `?api_key=` (freemium), not the old `mailto` polite pool.** `mailto` no longer
  does anything in the current contract. Anonymous calls get a tiny daily budget.
- **Concepts are deprecated** — prefer `topics`/`primary_topic` on works.

## Relationship to braincrawl
OpenAlex is a natural **upstream source** for braincrawl's corpus/metadata layers (doc02.01):
its works carry DOI/PMID/MAG/OpenAlex IDs that braincrawl's id-resolution (doc02.01.02) can
union into one canonical GUID, and `referenced_works` maps directly onto braincrawl's citation
edges (doc02.01.03). When ingesting, hand OpenAlex's external IDs straight to braincrawl's
upsert API and let it resolve identity — don't resolve it yourself.

See `reference.md` for the complete contract.
