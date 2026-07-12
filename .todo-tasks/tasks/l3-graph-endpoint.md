# The research graph endpoint on the server

## Motivation

The Web UI is plugin-driven and plugins have full read access to the research
graph — the server's whole L3 surface is one live endpoint. The server parses
the store on request (milliseconds at this scale); data is current by
construction. Decided 2026-07-12; see `.rhidoc/01-product/03-web-ui.md`.

## Do NOT

- Do NOT add a query registry, a query listing endpoint, or per-query
  endpoints. Queries are a client-side capability, not server entities. One
  graph endpoint only.
- Do NOT cache the parsed graph in server state — parse per request. The
  ETag is computed from the serialized bytes, not from a stored version.
- Do NOT make the L3 root mandatory: if `BRAINCRAWL_L3_ROOT` is unset, the
  endpoint returns 404 with a clear message, and everything else works.
- Do NOT touch the Worker (`apps/worker`) — native server only.

## Plan

### 1. Endpoint

- New `apps/server/src/handlers/l3.rs`: `handler_l3_graph` — read the L3
  root from state, call `l3::parse()`, serialize the graph (the crate's wire
  shape), respond with `ETag` (hex of a content hash) and honor
  `If-None-Match` with 304.
- `apps/server/src/lib.rs`: thread an `Option<PathBuf>` L3 root through the
  app state (follow how `LocalStore`/`AuthConfig` are threaded into
  `make_app` at `lib.rs:417`); add `.route("/api/l3/graph", get(handler_l3_graph))`
  to the authed router (`lib.rs:424`).
- `apps/server/Cargo.toml`: depend on the `l3` crate.

### 2. Config

- `apps/server-bin/src/main.rs`: read `BRAINCRAWL_L3_ROOT` (optional, no
  default) alongside the existing vars (`main.rs:29–44`), pass into
  `make_app`; extend the env-var doc table at the top of the file.
- `scripts/braincrawl-server.service`: add the `BRAINCRAWL_L3_ROOT` line to
  the unit template, matching the CLI's `l3_repo` value pattern used by the
  other paths there.

### 3. `braincrawl web` verb

- `apps/cli/src/cli.rs`: top-level `Web` command (no args).
- New thin handler (follow the existing module layout in `apps/cli/src`):
  resolve `server_url` from config precedence (see `config.rs`), print
  `<server_url>/web` to stdout. Nothing else — no browser launching in this
  phase, no health probing.
- `braincrawl-cli.tsp` + `braincrawl-cli.smithy`: add the verb.

## Files to Modify

- `apps/server/src/handlers/l3.rs` — new; `handlers/mod.rs` — register
- `apps/server/src/lib.rs` — state + route
- `apps/server/Cargo.toml` — dep on `l3`
- `apps/server-bin/src/main.rs` — env var
- `scripts/braincrawl-server.service` — unit template line
- `apps/cli/src/cli.rs` (+ small handler file) — `web` verb
- `braincrawl-cli.tsp`, `braincrawl-cli.smithy` — grammar entries

## Verification

```bash
cargo build
cargo test -p braincrawl-server
cargo test
```

## Out of Scope

- Serving `web/dist` assets (lands with the web-ui phases)
- SSE / watching (task l3-live-refresh)
- Worker/edge backend; file export

## Notes

- Add an axum-level test in `apps/server` (follow the existing handler test
  style if present, else a `tower::ServiceExt::oneshot` test): temp dir with
  one small `.l3.md`, assert 200 + `nodes`/`links` keys + ETag round-trip
  304; and the unset-root 404.
- The response shape is exactly the `l3` crate's serialization — the handler
  must not reshape it. That shape is the contract future backends (edge,
  hosted) reproduce.

## Surface after this phase

- `GET /api/l3/graph` on the native server: 200 with the wire shape
  (`nodes`: `id`/`labels`/`properties`; `links`:
  `source`/`target`/`type`/`properties`), ETag + 304 support, 404 when no
  L3 root is configured.
- `BRAINCRAWL_L3_ROOT` in the service template and the env docs.
- `braincrawl web` prints the UI URL and is in the CLI spec.
