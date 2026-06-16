# .rhidoc/ Manifest

Machine-readable index for AI navigation. Read this file first, then open only the docs relevant to your query.

**Retrieval strategy:** See doc00.04 for AI retrieval patterns.

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

## 02-architecture — Architecture

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.00 | `00-index.md` | Monorepo split into three deliverables — core (the layered knowledge-graph engine), cloudflare (the deployment target), and skill (the agent-facing surface). | architecture, monorepo, overview | — | — | — |

### Core

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.01.00 | `01-core/00-index.md` | The platform-agnostic knowledge-graph engine, decomposed into the three goals layers — corpus (identity + payloads), neutral metadata graph, and per-consumer projection graphs. | architecture, core, layers | — | — | — |
| doc02.01.01 | `01-core/01-layers.md` | Core splits into two storage worlds — Layer 1 is raw bytes in a blob store keyed by canonical id, Layer 2 is the metadata database that holds every fact, including the facts about the bytes. Identity is the shared spine both depend on. | architecture, core, layers, storage, separation | — | — | — |

### Cloudflare

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.02.00 | `02-cloudflare/00-index.md` | The deployment target — maps core abstractions onto Cloudflare edge primitives (Workers, D1, R2, KV, Durable Objects, Queues, Vectorize). Durable-Object-per-work-id coalesces cache misses and enforces upstream rate budgets. | architecture, cloudflare, deployment, infrastructure | — | — | — |

### Skill

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.03.00 | `03-skill/00-index.md` | The agent-facing surface — how a consuming agent drives the graph API. Builds coverage once via the corpus/metadata layers, then applies decisiveness as a query-time lens over a consumer projection. | architecture, skill, agent, consumer | — | — | — |

## Tag Index

Quick lookup for file-path→doc mapping:

| Tag | Relevant Docs |
|-----|---------------|
| `agent` | doc02.03.00 |
| `ai` | doc00.04 |
| `architecture` | doc02.00, doc02.01.00, doc02.01.01, doc02.02.00, doc02.03.00 |
| `cloudflare` | doc02.02.00 |
| `consumer` | doc02.03.00 |
| `conventions` | doc00.03 |
| `core` | doc02.01.00, doc02.01.01 |
| `deployment` | doc02.02.00 |
| `docs` | doc00.01, doc00.02, doc00.03, doc00.04 |
| `index` | doc00.00 |
| `infrastructure` | doc02.02.00 |
| `layers` | doc02.01.00, doc02.01.01 |
| `maintenance` | doc00.02 |
| `meta` | doc00.00, doc00.01 |
| `monorepo` | doc02.00 |
| `overview` | doc02.00 |
| `philosophy` | doc00.02 |
| `retrieval` | doc00.04 |
| `separation` | doc02.01.01 |
| `skill` | doc02.03.00 |
| `storage` | doc02.01.01 |
| `theory` | doc00.01 |
