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

| doc00.00 | `00-index.md` |  |  | — | — | — |
| doc00.01 | `01-about.md` | Why this workspace exists, how to read it, two-sources-of-truth theory | docs, meta, theory | — | — | — |
| doc00.02 | `02-maintenance.md` | Doc philosophy — declarative intent, banned patterns, when to grow detail | docs, maintenance, philosophy | — | — | — |
| doc00.03 | `03-conventions.md` | Cross-reference syntax, frontmatter schema, file naming, writing style | docs, conventions | — | — | — |
| doc00.04 | `04-ai-retrieval.md` | How AI agents navigate this workspace — hierarchical retrieval, MANIFEST usage, token budgets | docs, ai, retrieval | — | — | — |

## 01-product — Product

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc01.00 | `00-index.md` |  |  | — | — | — |
| doc01.01 | `01-glossary.md` | Plain-language names for braincrawl's three layers (Library, Catalog, Research Collection), written as ORM-style verbalized facts; defines topic coverage. | glossary, vocabulary, terms, product, facts, coverage | — | doc01.02, doc01.03 | — |
| doc01.02 | `02-mental-model.md` | The agent as signal converter — it builds the Library and Catalog, and translates between the user and the library so a human reads only the best works directly. | product, mental-model, agent, role | doc01.01 | — | — |
| doc01.03 | `03-web-ui.md` | The Web UI is a feature of braincrawl providing read-only views of everything it holds, across all three layers; its views are provided by Web UI plugins. | product, web-ui, read-only, plugin | doc01.01 | — | — |

## 02-architecture — Architecture

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.00 | `00-index.md` |  |  | — | doc02.04 | — |
| doc02.04 | `04-monorepo-rust-runtimes.md` | A single Cargo workspace in Rust. The platform-agnostic core defines traits; concrete backends bind them to Cloudflare or to a local stack. Two entry points — a Wasm Worker and a native dev server — wire the backends, and are the only crates that name infrastructure. | architecture, monorepo, rust, cloudflare, runtime, traits-and-backends, separation | doc02.00, doc02.01.01, doc02.02.00 | — | — |
| doc02.05 | `05-auth-tenancy.md` | Access is a bearer token in the Authorization header, validated at the entry point against a hashed allowlist in KV. Each token maps to a tenant, and tenant is the isolation key for Research Collections. Core stays auth-blind; the entry points gate every request before any use-case runs. | architecture, auth, security, tenancy, token | doc02.01.03, doc02.02.00 | — | — |

### Core

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.01.00 | `01-core/00-index.md` |  |  | — | — | — |
| doc02.01.01 | `01-core/01-layers.md` | Core splits into two storage worlds — the Library (Layer 1) is raw bytes in a blob store keyed by UUID, the Catalog (Layer 2) is the metadata database that holds every fact, including the facts about the bytes. Identity is the shared spine both depend on. | architecture, core, layers, storage, separation | — | doc02.01.02, doc02.01.03, doc02.01.04, doc02.04 | — |
| doc02.01.02 | `01-core/02-id-resolution.md` | A core braincrawl feature — consumers hand in any external id and braincrawl routes every id of the same resource to one UUID. Resolution is incremental union-find over the alias table; convergence is guaranteed for any record that co-asserts two ids, and merges are confluent. | architecture, core, identity, id-resolution, union-find | doc02.01.01, doc02.01.03 | doc02.01.03 | — |
| doc02.01.03 | `01-core/03-api.md` | The consumer-facing store API — upsert and read for works, content, and citation edges. Every id parameter accepts any external identifier; braincrawl resolves it to a UUID internally, so consumers never resolve identity themselves. | architecture, core, api, contract | doc02.01.01, doc02.01.02 | doc02.01.02, doc02.05, doc02.06.01.01, doc02.06.01.02 | openapi.yaml |
| doc02.01.04 | `01-core/04-l3-conventions.md` | The settled contract for an L3 Research Document — two required frontmatter fields (doc, updated) plus one node grammar every doc's body follows (headings, property lines, link lines, and the controlled edge vocabulary). Reference the ids, never copy their metadata. | architecture, core, l3, research-collection, conventions, node-grammar | doc02.01.01 | — | — |

### Cloudflare

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.02.00 | `02-cloudflare/00-index.md` |  |  | — | doc02.04, doc02.05 | — |

### Skill

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.03.00 | `03-skill/00-index.md` |  |  | — | — | — |

### CLI

| Ref | File | Summary | Tags | Deps | Refs | Attachments |
|-----|------|---------|------|------|------|-------------|

| doc02.06.00 | `06-cli/00-index.md` |  |  | — | — | — |
| doc02.06.01.00 | `06-cli/01-providers/00-index.md` |  |  | — | — | — |
| doc02.06.01.01 | `06-cli/01-providers/01-openalex.md` | The braincrawl openalex namespace — a deliberate fork to the OpenAlex API exposing six read-only base verbs (get, search, find, autocomplete, cited-by, refs). Trimmed structured output, push-to-store by default. | cli, providers, openalex, fork, verbs, citations | doc02.06.01, doc02.01.03 | — | — |
| doc02.06.01.02 | `06-cli/01-providers/02-semantic-scholar.md` | The braincrawl semanticscholar namespace — a deliberate fork to the Semantic Scholar API exposing four read-only base verbs (get, search, cited-by, refs). Trimmed structured output, push-to-store by default. | cli, providers, semanticscholar, fork, verbs, citations | doc02.06.01, doc02.01.03 | — | — |

## Tag Index

Quick lookup for file-path→doc mapping:

| Tag | Relevant Docs |
|-----|---------------|
| `agent` | doc01.02 |
| `ai` | doc00.04 |
| `api` | doc02.01.03 |
| `architecture` | doc02.01.01, doc02.01.02, doc02.01.03, doc02.01.04, doc02.04, doc02.05 |
| `auth` | doc02.05 |
| `citations` | doc02.06.01.01, doc02.06.01.02 |
| `cli` | doc02.06.01.01, doc02.06.01.02 |
| `cloudflare` | doc02.04 |
| `contract` | doc02.01.03 |
| `conventions` | doc00.03, doc02.01.04 |
| `core` | doc02.01.01, doc02.01.02, doc02.01.03, doc02.01.04 |
| `coverage` | doc01.01 |
| `docs` | doc00.01, doc00.02, doc00.03, doc00.04 |
| `facts` | doc01.01 |
| `fork` | doc02.06.01.01, doc02.06.01.02 |
| `glossary` | doc01.01 |
| `id-resolution` | doc02.01.02 |
| `identity` | doc02.01.02 |
| `l3` | doc02.01.04 |
| `layers` | doc02.01.01 |
| `maintenance` | doc00.02 |
| `mental-model` | doc01.02 |
| `meta` | doc00.01 |
| `monorepo` | doc02.04 |
| `node-grammar` | doc02.01.04 |
| `openalex` | doc02.06.01.01 |
| `philosophy` | doc00.02 |
| `plugin` | doc01.03 |
| `product` | doc01.01, doc01.02, doc01.03 |
| `providers` | doc02.06.01.01, doc02.06.01.02 |
| `read-only` | doc01.03 |
| `research-collection` | doc02.01.04 |
| `retrieval` | doc00.04 |
| `role` | doc01.02 |
| `runtime` | doc02.04 |
| `rust` | doc02.04 |
| `security` | doc02.05 |
| `semanticscholar` | doc02.06.01.02 |
| `separation` | doc02.01.01, doc02.04 |
| `storage` | doc02.01.01 |
| `tenancy` | doc02.05 |
| `terms` | doc01.01 |
| `theory` | doc00.01 |
| `token` | doc02.05 |
| `traits-and-backends` | doc02.04 |
| `union-find` | doc02.01.02 |
| `verbs` | doc02.06.01.01, doc02.06.01.02 |
| `vocabulary` | doc01.01 |
| `web-ui` | doc01.03 |
