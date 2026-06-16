---
title: Monorepo & Runtimes
summary: A single Cargo workspace in Rust. The platform-agnostic core defines port traits; concrete adapters bind them to Cloudflare or to a local stack. Two composition roots — a Wasm Worker and a native dev server — wire the adapters, and are the only crates that name infrastructure.
tags: [architecture, monorepo, rust, cloudflare, runtime, ports-and-adapters, separation]
deps: [doc02.00, doc02.01.01, doc02.02.00]
---

# Monorepo & Runtimes

braincrawl is a single **Cargo workspace** written in **Rust**. The core domain is
platform-agnostic; every dependency on infrastructure is an adapter behind a trait.
The same engine runs in two places — Cloudflare's edge and a local machine — by
swapping adapters, never by forking logic.

## Why one language, one workspace

The serverless target is Cloudflare Workers, which run Rust compiled to
`wasm32-unknown-unknown` via `workers-rs`. Writing the core in Rust lets the exact
domain logic that runs at the edge also compile to a native binary for local
development and tests — no second implementation to keep in sync. A single workspace
keeps the core, its adapters, and both entry points under one build graph and one
dependency direction.

## Ports and adapters

The seam from `doc02.00` — core owns the model, infrastructure is one binding —
is enforced in code as **ports and adapters**:

- **Ports** are trait definitions in the core crate, phrased in domain vocabulary
  (canonical id, kind, rights), naming no platform. The corpus split from
  `doc02.01.01` maps to a `BlobStore` port (opaque key → bytes) and a
  `PayloadsRepo` / `MetadataStore` port (the queryable facts, including each blob's
  key). Identity resolution is an `IdResolver` port.
- **Adapters** are separate crates, each implementing a port against one technology.
- **The dependency arrow always points inward to core.** Core depends on nothing;
  adapters depend on core; the composition roots depend on both. No core file names
  a concrete adapter.

## Dual runtime

Each port has a Cloudflare adapter and a local adapter, and the two are
interchangeable at the trait boundary:

| Port | Cloudflare adapter | Local adapter |
|---|---|---|
| `BlobStore` | R2 | filesystem (plus in-memory for unit tests) |
| `PayloadsRepo` / `MetadataStore` | D1 | SQLite |
| `IdResolver` | KV | SQLite / in-memory |

D1 speaks the SQLite dialect, so a single set of migrations and query strings serves
both database adapters; the adapters differ only in driver. The local stack is both
the development environment and the substrate integration tests run against.

## Composition roots

Exactly two crates name concrete adapters — they are the only place infrastructure
is wired:

- A **Worker** crate (`cdylib`, `wasm32`) is the Cloudflare entry point. It binds
  R2 + D1 + KV from the runtime environment and routes requests into core
  use-cases. The Durable-Object-per-work-id coordinator from `doc02.02.00` is an
  adapter concern here, invisible to core.
- A **native server** crate is the local entry point. It binds the filesystem +
  SQLite adapters and exposes the same use-cases over HTTP for development and tests.

Every other crate depends only on core's traits, so the choice of runtime collapses
to which adapters a composition root constructs.

## Async traits across runtimes

Worker futures are `!Send` — the edge runtime is single-threaded. The port traits
are therefore declared without a `Send` bound, and the native server runs on a
single-threaded async runtime to satisfy the same signatures. One trait definition
holds for both runtimes; the production target is single-threaded regardless.

## Boundaries

The agent-facing skill deliverable (`doc02.03.00`) stays outside the Cargo
workspace — it is a consumer of the API, not part of the engine's build graph.
Within the workspace, the core crate carries no Cloudflare or database dependency in
its manifest, which is what keeps it compiling for both the Wasm and native targets.
