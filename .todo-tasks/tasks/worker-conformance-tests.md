# Worker conformance tests — shared HTTP suite + wrangler-dev harness

## Motivation

There is no way to test the Cloudflare Worker. `cargo test` cannot touch it: the
Worker is a `wasm32` cdylib whose D1/R2/KV/DO bindings only exist inside the
Workers runtime. The native server has a good in-process HTTP suite
(`apps/server/tests/smoke.rs`) that exercises every core use-case via the local
backends, but nothing exercises the Worker's routing or its D1/R2/KV backends.

This phase builds **one conformance suite both entry points must pass**, plus a
harness that runs it against the real Worker under `wrangler dev --local`
(workerd + miniflare emulate D1/R2/KV/DO locally). `wrangler` and the
`wasm32-unknown-unknown` target are already installed in this environment.

The headless verification gate runs the suite against an **in-process native
server** (fully runnable here). The `wrangler dev` path is delivered as a script
+ `just` recipe and documented as the edge/CI gate — it is NOT in the Verification
block because the sandbox may lack network to install `worker-build` or to run the
wrangler daemon.

## Do NOT

- Do NOT put the conformance HTTP bodies inside the `braincrawl-worker` crate or
  `apps/server` — they go in a standalone, entry-point-agnostic crate so the same
  assertions run against either server. The worker crate is wasm-only and cannot
  be compiled for the host test target.
- Do NOT delete or rewrite `apps/server/tests/smoke.rs`. Leave it as-is; the new
  conformance crate is additive.
- Do NOT put the `wrangler dev` invocation in the `## Verification` block. It is
  the manual/CI gate, invoked via `just worker-test`. Verification must contain
  only commands that pass headless with no network daemon.
- Do NOT change `crates/core`, the backends, or the Worker routing. (The Worker's
  `/stats` and `/graph/neighborhood` come from the predecessor phase; see Surface
  below — assume they exist, do not add them here.)
- Do NOT add auth-tenancy or real DO locking. Out of scope.

## Plan

### 1. New crate `crates/conformance` (entry-point-agnostic suite)

- Add `crates/conformance` to `members` AND `default-members` in the root
  `Cargo.toml`.
- `crates/conformance/Cargo.toml`: package `braincrawl-conformance`; deps
  `reqwest = { version = "*", features = ["blocking", "json"] }` (match the version
  already resolved in `Cargo.lock` for the CLI's `reqwest`), `serde_json`.
- `crates/conformance/src/lib.rs`: a single public entry
  `pub fn run_all(base_url: &str, token: Option<&str>) -> Result<(), String>`
  using `reqwest::blocking`. Port the assertion bodies from
  `apps/server/tests/smoke.rs`, one private fn per case, all called by `run_all`:
  - put + get work (`guid:` id, 404 on unknown)
  - `have` (absent → present after put)
  - content roundtrip (PUT then GET `abstract`, byte-identical)
  - large content (~5 MB `fulltext`, must not 413, round-trips)
  - `/stats` (empty zeros; after a described work + a stub-minting edge, the
    documented counts)
  - `/graph/neighborhood` (src→dst edge, depth 1, nodes/edges/truncated shape;
    bad `dir` → 400)
  - Each request attaches `Authorization: Bearer <token>` when `token` is `Some`.
  - On any assertion failure return `Err(String)` describing which case failed
    (do not `panic!`/`unwrap` inside `run_all` on assertion failures — return the
    message so callers can decide how to surface it).

### 2. Headless gate: native-server conformance test

- `apps/server/tests/conformance.rs`: boot the in-process axum server exactly like
  `smoke.rs` does (`make_store` + `make_app` with `AuthConfig { disabled: true }`,
  `TcpListener` on `127.0.0.1:0`), then run the suite. Because `run_all` is
  blocking, call it via `tokio::task::spawn_blocking` so it does not stall the
  runtime; `unwrap` the `Result` (a returned `Err` fails the test). Pass
  `token: None` (auth disabled).
- Add `braincrawl-conformance = { path = "../../crates/conformance" }` to
  `apps/server`'s `[dev-dependencies]`.

### 3. External (live-URL) test driver

- `crates/conformance/tests/external.rs`: read env `BRAINCRAWL_CONFORMANCE_URL`
  and optional `BRAINCRAWL_CONFORMANCE_TOKEN`. If the URL var is unset, print a
  "skipped (no URL)" line and return `Ok`/pass — so `cargo test -p
  braincrawl-conformance` is green headless. If set, call `run_all(url,
  token.as_deref())` and `unwrap`.

### 4. `wrangler dev --local` harness script

- `scripts/worker-conformance.sh` (executable, `set -euo pipefail`):
  1. Write a `apps/worker/.dev.vars` containing `AUTH_TOKEN=<fixed test token>`
     (this is how `wrangler dev --local` injects the secret). Gitignore it.
  2. Apply migrations to the local D1:
     `wrangler d1 migrations apply braincrawl-db --local` (run with the worker dir
     as cwd via a subshell, since wrangler reads `wrangler.toml`).
  3. Start `wrangler dev --local --port 8787` in the background (it runs the
     `[build]` command from `wrangler.toml` itself), capture its PID, and ensure a
     trap kills it on exit.
  4. Readiness probe: poll with `curl` (POST `/works/have` with the bearer token,
     `--retry`/loop until a 200) before running tests — the Worker has no
     `/health` route.
  5. `BRAINCRAWL_CONFORMANCE_URL=http://127.0.0.1:8787
     BRAINCRAWL_CONFORMANCE_TOKEN=<test token> cargo test -p braincrawl-conformance
     --test external -- --nocapture`.
  6. Also assert a negative auth case: `curl` `/works/have` WITHOUT the token
     expects HTTP 401.
- Add to `.gitignore`: `apps/worker/.dev.vars`.

### 5. `just` recipe

- In `justfile`, add `worker-test:` running `./scripts/worker-conformance.sh`,
  with a one-line comment that this is the edge/CI gate requiring `wrangler` +
  network (unlike `just test`).

## Files to Modify

- `Cargo.toml` — add `crates/conformance` to `members` + `default-members`.
- `crates/conformance/Cargo.toml` — new.
- `crates/conformance/src/lib.rs` — new; `run_all`.
- `crates/conformance/tests/external.rs` — new; env-driven live driver.
- `apps/server/Cargo.toml` — add conformance dev-dependency.
- `apps/server/tests/conformance.rs` — new; native-backed gate.
- `scripts/worker-conformance.sh` — new; wrangler-dev harness.
- `justfile` — add `worker-test` recipe.
- `.gitignore` — add `apps/worker/.dev.vars`.

## Verification

```bash
cargo build
cargo test -p braincrawl-conformance
cargo test -p braincrawl-server --test conformance
```

## Out of Scope

- Running the `wrangler dev` path inside this verification (it is `just
  worker-test`, a manual/CI gate).
- `/jobs`, real DO locking, KV-allowlist auth.

## Notes

- The live edge gate is `just worker-test`. Run it locally/CI where `wrangler` and
  network are available; it builds the Worker via `worker-build`, boots it under
  `wrangler dev --local` with emulated D1/R2/KV, and runs the same `run_all` suite
  the native gate runs — proving Worker/server parity.
- Keep `run_all` auth-agnostic: it takes an optional bearer token and never
  asserts auth-rejection itself. The 401-without-token check lives in the wrangler
  script (the native gate runs with auth disabled).
- Match the `reqwest` version to what `Cargo.lock` already pins for `apps/cli` to
  avoid pulling a second major version into the build graph.

## Surface after this phase

- A `braincrawl-conformance` crate exposing
  `run_all(base_url: &str, token: Option<&str>) -> Result<(), String>` — the
  shared HTTP conformance suite, runnable against any braincrawl entry point.
- `apps/server/tests/conformance.rs` — a headless, in-process native-server gate
  that runs the suite (auth disabled).
- `scripts/worker-conformance.sh` + `just worker-test` — boot the real Worker
  under `wrangler dev --local` and run the same suite against it, plus a negative
  401 check.
- Verification of Worker/server parity now exists; the DO lock is still the
  advisory stub and auth is still the single shared secret (unchanged here).
