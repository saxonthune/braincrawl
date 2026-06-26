---
title: Monorepo & Runtimes
summary: A single Cargo workspace in Rust. The platform-agnostic core defines traits; concrete backends bind them to Cloudflare or to a local stack. Two entry points — a Wasm Worker and a native dev server — wire the backends, and are the only crates that name infrastructure.
tags: [architecture, monorepo, rust, cloudflare, runtime, traits-and-backends, separation]
deps: [doc02.00, doc02.01.01, doc02.02.00]
---

# Monorepo & Runtimes

braincrawl is a single **Cargo workspace** written in **Rust**. The core domain is
platform-agnostic; every dependency on infrastructure sits behind a trait, with one
concrete **backend** per technology. The same engine runs in two places — Cloudflare's
edge and a local machine — by swapping backends, never by forking logic.

## Why one language, one workspace

The serverless target is Cloudflare Workers, which run Rust compiled to
`wasm32-unknown-unknown` via `workers-rs`. Writing the core in Rust lets the exact
domain logic that runs at the edge also compile to a native binary for local
development and tests — no second implementation to keep in sync. A single workspace
keeps the core, its backends, and both entry points under one build graph and one
dependency direction.

## Traits and backends

The seam from `doc02.00` — core owns the model, infrastructure is one binding —
is just plain Rust: **traits in core, backends that implement them**. There is no
extra abstraction layer; the trait *is* the interface.

- **Traits** live in the core crate, phrased in domain vocabulary (UUID,
  kind, provenance), naming no platform. The Library/Catalog split from `doc02.01.01` maps to a
  `BlobStore` trait (opaque key → bytes) and `PayloadsRepo` / `MetadataStore` traits
  (the queryable facts, including each blob's key). Identity resolution is an
  `IdResolver` trait.
- **Backends** are separate crates under `crates/backends/`, each implementing a
  trait against one technology (`store-sqlite`, `store-d1`, `blob-r2`, …).
- **The dependency arrow always points inward to core.** Core depends on nothing;
  backends depend on core; the entry points depend on both. No core file names a
  concrete backend.

## Dual runtime

Each trait has a Cloudflare backend and a local backend, and the two are
interchangeable at the trait boundary:

| Trait | Cloudflare backend | Local backend |
|---|---|---|
| `BlobStore` | R2 | filesystem (plus in-memory for unit tests) |
| `PayloadsRepo` / `MetadataStore` | D1 | SQLite |
| `IdResolver` | KV | SQLite / in-memory |

D1 speaks the SQLite dialect, so a single set of migrations and query strings serves
both database backends; the backends differ only in driver. The local stack is both
the development environment and the substrate integration tests run against.

## Entry points

Exactly two crates name concrete backends — they are the only place infrastructure
is wired:

- A **Worker** crate (`cdylib`, `wasm32`) is the Cloudflare entry point. It binds
  R2 + D1 + KV from the runtime environment and routes requests into core
  use-cases. The Durable-Object-per-work-id coordinator from `doc02.02.00` is a
  backend concern here, invisible to core.
- A **native server** crate is the local entry point. It binds the filesystem +
  SQLite backends and exposes the same use-cases over HTTP for development and tests.

Every other crate depends only on core's traits, so the choice of runtime collapses
to which backends an entry point constructs.

## Async traits across runtimes

Worker futures are `!Send` — the edge runtime is single-threaded. The traits
are therefore declared without a `Send` bound, and the native server runs on a
single-threaded async runtime to satisfy the same signatures. One trait definition
holds for both runtimes; the production target is single-threaded regardless.

## Boundaries

The agent-facing skill deliverable (`doc02.03.00`) stays outside the Cargo
workspace — it is a consumer of the API, not part of the engine's build graph.
Within the workspace, the core crate carries no Cloudflare or database dependency in
its manifest, which is what keeps it compiling for both the Wasm and native targets.
