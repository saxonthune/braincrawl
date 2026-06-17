---
title: CLI
summary: The braincrawl script — a single monolithic CLI whose subcommands route between the metadata server and external providers. Results persist to the metadata server by default; --skip-push opts out.
tags: [cli, script, braincrawl, routing, providers]
deps: [doc02.01.03]
---

# CLI

`braincrawl` is a **single monolithic command-line tool**. Its subcommands are
grouped into namespaces; one namespace per concern, sharing one grammar and one
set of global flags.

## The routing intent

The plain top-level verbs (`braincrawl search`, `braincrawl get`, …) are the
**smart path**: they route a request between braincrawl's own metadata server
(doc02.01) and external providers, deciding where an answer comes from and
folding provider results back into the store.

A **provider namespace** (`braincrawl <provider> …`, see doc02.06.01) is the
**deliberate fork**: it bypasses routing and talks straight to one provider's
API. It exists for when the caller intends to reach past the metadata server —
to see exactly what an upstream returns, unmediated.

## Push by default

Every command — routed or forked — **persists what it fetches to the metadata
server by default**, alongside returning it to the caller's context. A provider
fork is about *where the query goes*, not whether its results are kept: data
retrieved from a provider still lands in the store unless the caller opts out
with `--skip-push`. Identity resolution for pushed records is owned by the
metadata server's upsert API (doc02.01.03) — no CLI command resolves or merges
identity itself.

## Talking to the store

The CLI is a thin client: it reaches the metadata server **over HTTP** (the
server's upsert/read API, doc02.01.03) rather than linking the engine in-process.
It holds no database of its own. Connection and credential settings — server URL,
provider API keys (e.g. OpenAlex) — resolve with precedence **flag > environment
> config file**.

## Global grammar

Shaping is uniform across every namespace, so a caller learns it once:

| Flag | Effect |
|---|---|
| `--json` / `--text` | Structured envelope (default) vs. compact human view |
| `--limit N` / `--all` | Cap results, or drain all pages |
| `--fields a,b,c` | Project to selected top-level fields |
| `--full` | Untrimmed raw records (escape hatch) |
| `--skip-push` | Do not persist results to the metadata server |

## Contents

- **doc02.06.01 — Providers**: the provider-fork namespaces. One today: OpenAlex.
