# .rhidoc/ Manifest

Machine-readable index for AI navigation. Read this file first, then open only the docs relevant to your query.

**Retrieval strategy:** See doc00.00 (codex index) for how to find and read docs efficiently.

## Column Definitions

- **Ref**: Cross-reference ID (`docXX.YY.ZZ`)
- **File**: Path relative to title directory
- **Summary**: One-line description for semantic matching
- **Tags**: Keywords for file-path→doc mapping
- **Deps**: Doc refs to check when this doc changes
- **Refs**: Reverse deps — docs that list this one in their Deps (computed automatically)
- **Attachments**: Non-md files sharing the doc's numeric prefix. Sidecar artifacts that travel with the doc during structural operations. Purely filesystem-derived; not a frontmatter field.

Orphaned attachments (non-md files with no corresponding root .md) are reported as warnings on stderr during regeneration and do not appear in this table.

## 00-codex — Codex

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc00.00 | `00-index.md` | Meta-documentation — how to read this workspace | index, meta | — | — | — |
| doc00.01 | `01-about.md` | Why this workspace exists, how to read it, two-sources-of-truth theory | docs, meta, theory | — | — | — |
| doc00.02 | `02-maintenance.md` | Doc philosophy — declarative intent, banned patterns, when to grow detail | docs, maintenance, philosophy | — | — | — |
| doc00.03 | `03-conventions.md` | Cross-reference syntax, frontmatter schema, file naming, writing style | docs, conventions | — | — | — |
| doc00.04 | `04-ai-retrieval.md` | How AI agents navigate this workspace — hierarchical retrieval, MANIFEST usage, token budgets | docs, ai, retrieval | — | — | — |

## 01-product — Product

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc01.00 | `00-index.md` |  |  | — | — | — |
| doc01.01 | `01-glossary.md` | Plain-language names for braincrawl's three layers — Library, Catalog, and Research Collection. | glossary, vocabulary, terms, product | — | doc01.02 | — |
| doc01.02 | `02-mental-model.md` | The agent as signal converter — it builds the Library and Catalog, and translates between the user and the library so a human reads only the best works directly. | product, mental-model, agent, role | doc01.01 | — | — |

## 02-architecture — Architecture

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.00 | `00-index.md` | Monorepo split into three deliverables — core (the layered knowledge-graph engine), cloudflare (the deployment target), and skill (the agent-facing surface). | architecture, monorepo, overview | — | doc02.04 | — |
| doc02.04 | `04-monorepo-rust-runtimes.md` | A single Cargo workspace in Rust. The platform-agnostic core defines traits; concrete backends bind them to Cloudflare or to a local stack. Two entry points — a Wasm Worker and a native dev server — wire the backends, and are the only crates that name infrastructure. | architecture, monorepo, rust, cloudflare, runtime, traits-and-backends, separation | doc02.00, doc02.01.01, doc02.02.00 | — | — |
| doc02.05 | `05-auth-tenancy.md` | Access is a bearer token in the Authorization header, validated at the entry point against a hashed allowlist in KV. Each token maps to a tenant, and tenant is the isolation key for Research Collections. Core stays auth-blind; the entry points gate every request before any use-case runs. | architecture, auth, security, tenancy, token | doc02.01.03, doc02.02.00 | — | — |

### Core

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.01.00 | `01-core/00-index.md` | The platform-agnostic knowledge-graph engine, decomposed into the three goals layers — Library (identity + payloads), Catalog, and Research Collections. | architecture, core, layers | — | — | — |
| doc02.01.01 | `01-core/01-layers.md` | Core splits into two storage worlds — the Library (Layer 1) is raw bytes in a blob store keyed by UUID, the Catalog (Layer 2) is the metadata database that holds every fact, including the facts about the bytes. Identity is the shared spine both depend on. | architecture, core, layers, storage, separation | — | doc02.01.02, doc02.01.03, doc02.04 | — |
| doc02.01.02 | `01-core/02-id-resolution.md` | A core braincrawl feature — consumers hand in any external id and braincrawl routes every id of the same resource to one UUID. Resolution is incremental union-find over the alias table; convergence is guaranteed for any record that co-asserts two ids, and merges are confluent. | architecture, core, identity, id-resolution, union-find | doc02.01.01, doc02.01.03 | doc02.01.03 | — |
| doc02.01.03 | `01-core/03-api.md` | The consumer-facing store API — upsert and read for works, content, and citation edges. Every id parameter accepts any external identifier; braincrawl resolves it to a UUID internally, so consumers never resolve identity themselves. | architecture, core, api, contract | doc02.01.01, doc02.01.02 | doc02.01.02, doc02.05, doc02.06.00, doc02.06.01.01, doc02.06.01.02 | openapi.yaml |

### Cloudflare

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.02.00 | `02-cloudflare/00-index.md` | The deployment target — maps core abstractions onto Cloudflare edge primitives (Workers, D1, R2, KV, Durable Objects, Queues, Vectorize). Durable-Object-per-work-id coalesces cache misses and enforces upstream rate budgets. | architecture, cloudflare, deployment, infrastructure | — | doc02.04, doc02.05 | — |

### Skill

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.03.00 | `03-skill/00-index.md` | The agent-facing surface — how a consuming agent drives the graph API. Gathers works once via the Library/Catalog, then reads and judges at query time over a Research Collection. | architecture, skill, agent, consumer | — | — | — |

### CLI

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.06.00 | `06-cli/00-index.md` | The braincrawl script — a single monolithic CLI whose subcommands route between the metadata server and external providers. Results persist to the metadata server by default; --skip-push opts out. | cli, script, braincrawl, routing, providers | doc02.01.03 | — | — |
| doc02.06.01.00 | `06-cli/01-providers/00-index.md` | The provider-fork namespaces — each talks straight to one external API, bypassing routing. One provider today (OpenAlex) has been joined by Semantic Scholar as a second. | cli, providers, fork, external | doc02.06 | — | — |
| doc02.06.01.01 | `06-cli/01-providers/01-openalex.md` | The braincrawl openalex namespace — a deliberate fork to the OpenAlex API exposing six read-only base verbs (get, search, find, autocomplete, cited-by, refs). Trimmed structured output, push-to-store by default. | cli, providers, openalex, fork, verbs, citations | doc02.06.01, doc02.01.03 | — | — |
| doc02.06.01.02 | `06-cli/01-providers/02-semantic-scholar.md` | The braincrawl semanticscholar namespace — a deliberate fork to the Semantic Scholar API exposing four read-only base verbs (get, search, cited-by, refs). Trimmed structured output, push-to-store by default. | cli, providers, semanticscholar, fork, verbs, citations | doc02.06.01, doc02.01.03 | — | — |

## Tag Index

Quick lookup for file-path→doc mapping:

| Tag | Relevant Docs |
|-----|---------------|
| `agent` | doc01.02, doc02.03.00 |
| `ai` | doc00.04 |
| `api` | doc02.01.03 |
| `architecture` | doc02.00, doc02.01.00, doc02.01.01, doc02.01.02, doc02.01.03, doc02.02.00, doc02.03.00, doc02.04, doc02.05 |
| `auth` | doc02.05 |
| `braincrawl` | doc02.06.00 |
| `citations` | doc02.06.01.01, doc02.06.01.02 |
| `cli` | doc02.06.00, doc02.06.01.00, doc02.06.01.01, doc02.06.01.02 |
| `cloudflare` | doc02.02.00, doc02.04 |
| `consumer` | doc02.03.00 |
| `contract` | doc02.01.03 |
| `conventions` | doc00.03 |
| `core` | doc02.01.00, doc02.01.01, doc02.01.02, doc02.01.03 |
| `deployment` | doc02.02.00 |
| `docs` | doc00.01, doc00.02, doc00.03, doc00.04 |
| `external` | doc02.06.01.00 |
| `fork` | doc02.06.01.00, doc02.06.01.01, doc02.06.01.02 |
| `glossary` | doc01.01 |
| `id-resolution` | doc02.01.02 |
| `identity` | doc02.01.02 |
| `index` | doc00.00 |
| `infrastructure` | doc02.02.00 |
| `layers` | doc02.01.00, doc02.01.01 |
| `maintenance` | doc00.02 |
| `mental-model` | doc01.02 |
| `meta` | doc00.00, doc00.01 |
| `monorepo` | doc02.00, doc02.04 |
| `openalex` | doc02.06.01.01 |
| `overview` | doc02.00 |
| `philosophy` | doc00.02 |
| `product` | doc01.01, doc01.02 |
| `providers` | doc02.06.00, doc02.06.01.00, doc02.06.01.01, doc02.06.01.02 |
| `retrieval` | doc00.04 |
| `role` | doc01.02 |
| `routing` | doc02.06.00 |
| `runtime` | doc02.04 |
| `rust` | doc02.04 |
| `script` | doc02.06.00 |
| `security` | doc02.05 |
| `semanticscholar` | doc02.06.01.02 |
| `separation` | doc02.01.01, doc02.04 |
| `skill` | doc02.03.00 |
| `storage` | doc02.01.01 |
| `tenancy` | doc02.05 |
| `terms` | doc01.01 |
| `theory` | doc00.01 |
| `token` | doc02.05 |
| `traits-and-backends` | doc02.04 |
| `union-find` | doc02.01.02 |
| `verbs` | doc02.06.01.01, doc02.06.01.02 |
| `vocabulary` | doc01.01 |
