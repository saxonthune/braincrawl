# braincrawl CLI Foundation

## Motivation

`braincrawl` is the monolithic CLI (doc02.06). This phase stands up the binary and
the shared infrastructure every namespace reuses: argument dispatch, global output
flags, config resolution, the structured output envelope, and a thin HTTP client to
the metadata server (`apps/server`). No provider logic — that lands in later phases.
The CLI is a thin client: it holds no database and reaches the server over HTTP.

## Do NOT

- Do NOT add any OpenAlex / provider logic here. No `openalex` subcommand, no
  upstream HTTP. That is the next phase (`braincrawl-openalex-read`).
- Do NOT link `crates/core` or any backend crate. The CLI talks to the server over
  HTTP only; it must not depend on the engine or a database.
- Do NOT implement the routing/"smart path" verbs (search/get that choose between
  server and provider). Only the foundation + a `store` debug surface for round-trip.
- Do NOT invent new server endpoints. Use the existing `PUT /works`, `PUT /edges`,
  `POST /works/have`, `GET /works/*path` exactly as `apps/server/src/lib.rs` defines.
- Do NOT introduce async coloring unless needed — use `reqwest::blocking`.

## Plan

### 1. Create the `apps/cli` crate

- New crate at `apps/cli`. `Cargo.toml`: package `braincrawl-cli`, `edition.workspace`,
  `version.workspace`; `[[bin]]` name `braincrawl` → `src/main.rs`. Deps: `clap`
  (derive), `serde`/`serde_json` (workspace), `reqwest` (`default-features=false`,
  features `["json","blocking","rustls-tls"]`), `thiserror` (workspace).
- Register in root `Cargo.toml` `members` AND `default-members` so `cargo build`/
  `cargo test` include it.

### 2. Argument dispatch (`src/cli.rs`)

- A clap derive `Cli` with a top-level `#[command(subcommand)]`. Define an enum
  `Namespace` that is the extension point for future provider namespaces; for this
  phase include only a hidden/dev `store` namespace used to prove the HTTP round-trip
  (e.g. `braincrawl store have <ns:value>...`, `braincrawl store get <ns:value>`).
- Global flags as a clap `#[command(flatten)]` `GlobalArgs` struct available to all
  subcommands: `--json`/`--text` (default json), `--limit <N>`, `--all`,
  `--fields <a,b,c>`, `--full`, `--skip-push`. Parse into an `OutputOpts` value.

### 3. Config resolution (`src/config.rs`)

- `Config { server_url: String, openalex_api_key: Option<String> }`.
- Resolve with precedence **flag > env > file > default**. Env:
  `BRAINCRAWL_SERVER_URL` (default `http://127.0.0.1:8787`), `BRAINCRAWL_OPENALEX_API_KEY`.
  File: optional TOML at `$BRAINCRAWL_CONFIG` or `~/.config/braincrawl/config.toml`.
- Provider keys live in config now even though no provider uses them yet, so the next
  phase consumes `Config` unchanged.

### 4. Output envelope (`src/output.rs`)

- Serializable `Envelope { query: QueryMeta, count: u64, returned: usize,
  truncated: bool, next_cursor: Option<String>, results: Vec<serde_json::Value> }`
  and `QueryMeta { entity: Option<String>, resolved_filter: Option<String>,
  url: Option<String> }`.
- `render(&Envelope, &OutputOpts)`: `--json` prints pretty JSON; `--text` prints a
  compact one-line-per-result view (stub is acceptable: id + a display field).
  `--fields` projects top-level keys of each result before rendering.

### 5. Store HTTP client (`src/store_client.rs`)

- `StoreClient { base_url, http: reqwest::blocking::Client }` with methods mapping to
  the server contract exactly:
  - `put_work(&self, record: &serde_json::Value) -> Result<String>` → `PUT {base}/works`,
    returns the `id` from `{"id":"guid:…"}`.
  - `put_edges(&self, edges: &[serde_json::Value]) -> Result<u64>` → `PUT {base}/edges`,
    returns `count`.
  - `have(&self, ids: &[String]) -> Result<Vec<String>>` → `POST {base}/works/have`
    body `{"ids":[...]}`.
  - `get_work(&self, alias: &str) -> Result<Option<serde_json::Value>>` →
    `GET {base}/works/{alias}` (alias in `ns:value` form); `404` → `None`.
- Records are passed as `serde_json::Value` so this phase needs no core types; the
  push phase builds the WorkRecord-shaped JSON.

### 6. Wire `main.rs`

- Parse `Cli`, build `Config`, dispatch the `store` namespace through `StoreClient`,
  render results via the envelope. Errors print to stderr with a non-zero exit.

## Files to Modify

- `Cargo.toml` — add `apps/cli` to `members` and `default-members`.
- `apps/cli/Cargo.toml` — new crate manifest.
- `apps/cli/src/main.rs` — entry + dispatch.
- `apps/cli/src/cli.rs` — clap `Cli`, `Namespace`, `GlobalArgs`/`OutputOpts`.
- `apps/cli/src/config.rs` — `Config` + precedence resolution.
- `apps/cli/src/output.rs` — `Envelope`, `QueryMeta`, `render`.
- `apps/cli/src/store_client.rs` — `StoreClient`.
- `apps/cli/tests/foundation.rs` — round-trip test (see Verification).

## Verification

```bash
cargo build -p braincrawl-cli
cargo run -p braincrawl-cli -- --help
cargo test -p braincrawl-cli
```

The test in `apps/cli/tests/foundation.rs` spins the real server in-process using
`braincrawl_server_lib::{make_app, make_store}` (add `braincrawl-server` + `tempfile`
as `[dev-dependencies]`, mirroring `apps/server/tests/smoke.rs`), binds it to an
ephemeral port, points `StoreClient` at it, then asserts a `have` round-trip returns
the expected aliases and `get_work` of a missing id returns `None`.

## Out of Scope

- OpenAlex and all provider logic (next phases).
- The smart routing verbs and the `--text` polished renderer.
- Auth headers (doc02.05 is not wired server-side yet; add later).

## Notes

- `kind` on `WorkRecord` serializes as the PascalCase enum variant (`"Work"`,
  `"Author"`, `"Venue"`, `"Topic"`, `"Concept"`) — relevant to the push phase, not here.
- Keep the `Namespace` enum trivially extensible; phase 2 adds an `Openalex` variant.

## Surface after this phase

- Crate `apps/cli` exists; package `braincrawl-cli`; binary `braincrawl`. It is in
  root `Cargo.toml` `members` and `default-members` (plain `cargo test` covers it).
- `apps/cli/src/cli.rs`: clap `Cli` with a `Namespace` subcommand enum (extension
  point for provider namespaces) and a flattened `GlobalArgs` exposing
  `--json/--text/--limit/--all/--fields/--full/--skip-push`, reduced to a public
  `OutputOpts`.
- `apps/cli/src/config.rs`: `Config { server_url, openalex_api_key }` with
  flag > env > file > default resolution. Env `BRAINCRAWL_SERVER_URL`,
  `BRAINCRAWL_OPENALEX_API_KEY`.
- `apps/cli/src/output.rs`: `Envelope`, `QueryMeta`, and `render(&Envelope,&OutputOpts)`
  applying `--fields`/`--json`/`--text`.
- `apps/cli/src/store_client.rs`: `StoreClient` with `put_work`, `put_edges`, `have`,
  `get_work` over the existing server HTTP contract; records/edges passed as
  `serde_json::Value`.
- Records are untyped JSON at this layer — no dependency on `crates/core`.
- Negative space: there is NO `openalex`/provider subcommand and NO push logic yet;
  the `store` namespace is a dev round-trip surface and may be kept or hidden later.
