# Enforce the bearer-token auth gate

## Motivation

Auth is designed (doc02.05 — Auth & Tenancy) but not enforced. The `impl-engine`
chain shipped both entry points (`apps/server` axum, `apps/worker` workers-rs) with
**no auth** — any caller reaching the server/worker is served. The thin client already
sends `Authorization: Bearer <token>` (`~/.braincrawl/braincrawl`), so the credential is
in flight with nothing validating it. This task closes that gap by adding the gate at the
**edge only**, per doc02.05: `401` missing/malformed, `403` unknown/revoked, resolve a
tenant. Core stays auth-blind.

Triage settled four decisions: (1) the shared gate logic lives in a new `braincrawl-auth`
crate; (2) this task implements the **single-shared-secret** path on both runtimes (the
multi-token KV allowlist is deferred); (3) the gate config is passed **explicitly** into
`make_app` (not read from env inside the middleware) so tests are deterministic; (4) names
are `BRAINCRAWL_AUTH_TOKEN` / `BRAINCRAWL_AUTH_DISABLED` (server) and a `wrangler secret`
named `AUTH_TOKEN` (worker).

## Do NOT

- **Do NOT touch `crates/core`** (`traits.rs`, `types.rs`, `usecases.rs`). Core must stay
  auth-blind. The use-case methods on `Store` keep their exact current signatures — do
  **not** add a `tenant` parameter to any of them. L3 has no schema, so the resolved
  tenant is carried at the edge only (axum request extension / a local variable in the
  worker) and is not consumed by any use-case in this task.
- **Do NOT introduce a `Send` bound.** The gate runs on the raw HTTP request *before* the
  `run_blocking` bridge on the server and before `build_store` in the worker, so it never
  interacts with the `!Send` use-case futures. Keep it that way.
- **Do NOT implement the KV (`AUTH_KV`) allowlist lookup** or any `SHA-256(token) →
  {tenant,status}` table read in this task. Add the `AUTH_KV` binding to `wrangler.toml`
  as *declared intent* only. Shape the crate's `Allowlist` trait so KV is a clean drop-in
  later, but do not wire it.
- **Do NOT add per-tenant Layer 3 isolation, token issuance/rotation/revocation tooling,
  user accounts, or admin UX.**
- **Do NOT pull in `ring` or other C/OpenSSL-backed crypto** — it breaks the wasm32 build.
  Use the pure-Rust `sha2` crate (wasm-compatible) for hashing.
- **Do NOT make the server start silently unauthenticated.** If neither
  `BRAINCRAWL_AUTH_TOKEN` nor `BRAINCRAWL_AUTH_DISABLED` is set, `main` must refuse to
  start with a clear error.

## Plan

### 1. New crate `crates/auth` (`braincrawl-auth`)

Create `crates/auth/Cargo.toml` and `crates/auth/src/lib.rs`. Add `sha2` (and `hex`, or
format the digest with `{:02x}` to avoid a hex dep — prefer no extra dep) to the workspace
deps. This crate is **host + wasm** compatible: pure logic, no `worker`/`tokio`/`axum`
dependency.

Public surface:

```rust
/// The tenant a token resolves to (L3 isolation key; unused downstream this task).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tenant(pub String);

/// Hex-encoded SHA-256 of the raw token. The raw token is never persisted.
pub fn hash_token(token: &str) -> String;

/// Parse `Authorization: Bearer <token>` → the token, or None if absent/malformed.
pub fn parse_bearer(header: Option<&str>) -> Option<&str>;

/// Resolution result a runtime maps to a status code.
pub enum AuthOutcome {
    Authenticated(Tenant),
    Unauthenticated, // → 401: missing/malformed bearer
    Forbidden,       // → 403: well-formed token, not in allowlist
}

/// Allowlist abstraction. Shaped so a KV-backed impl is a later drop-in.
pub trait Allowlist {
    fn lookup(&self, token_hash: &str) -> Option<Tenant>;
}

/// Single shared secret → one fixed tenant. Stores only the hash.
pub struct SharedSecret { /* expected_hash: String, tenant: Tenant */ }
impl SharedSecret {
    pub fn new(secret: &str, tenant: &str) -> Self; // hashes `secret` on construction
}
impl Allowlist for SharedSecret { /* constant-time compare of hex hashes */ }

/// Decide an outcome from the raw Authorization header value.
pub fn authorize(allowlist: &dyn Allowlist, header: Option<&str>) -> AuthOutcome;
```

`authorize`: `None`/non-`Bearer ` → `Unauthenticated`; else hash the token, `lookup` →
`Some(tenant)` → `Authenticated(tenant)`, `None` → `Forbidden`. Use a length-checked
constant-time byte comparison on the hex hashes inside `SharedSecret::lookup` (the hashes
are fixed length, so this is simple) to avoid timing leaks.

Unit tests at the bottom of `lib.rs`: `parse_bearer` happy/empty/wrong-scheme; `hash_token`
is stable and 64 hex chars; `authorize` returns each of the three variants against a
`SharedSecret`.

### 2. Register the crate

`Cargo.toml` (workspace root) — add `crates/auth` to `members`. Add `sha2` to
`[workspace.dependencies]` (a recent 0.10.x). Re-export through whatever dependency style
the existing crates use (look at how `resolver-kv` declares `braincrawl-core`).

### 3. Server gate (`apps/server/src/lib.rs`)

- Add `braincrawl-auth` as a dep in `apps/server/Cargo.toml`.
- Add an `AuthConfig` to the server lib:
  ```rust
  pub struct AuthConfig {
      pub disabled: bool,
      pub allowlist: braincrawl_auth::SharedSecret,
  }
  ```
- Change `make_app` to `pub fn make_app(store: Arc<LocalStore>, auth: Arc<AuthConfig>) -> Router`.
- Add an auth middleware via `axum::middleware::from_fn_with_state(auth.clone(), gate)`
  applied with `.layer(...)` to the router (so it runs for **all** routes). The gate:
  ```rust
  async fn gate(State(cfg): State<Arc<AuthConfig>>, req: Request, next: Next) -> Response
  ```
  - if `cfg.disabled` → `next.run(req).await`;
  - else read the `AUTHORIZATION` header, call `braincrawl_auth::authorize(&cfg.allowlist, hdr)`:
    - `Authenticated(tenant)` → insert `tenant` into `req.extensions_mut()`, then
      `next.run(req).await`;
    - `Unauthenticated` → `StatusCode::UNAUTHORIZED`;
    - `Forbidden` → `StatusCode::FORBIDDEN`.
  - Mind axum 0.7/0.8 middleware imports (`axum::middleware::Next`, `axum::extract::Request`).
    Match the axum version already in `Cargo.lock`.

### 4. Server main (`apps/server/src/main.rs`)

Build `AuthConfig` from env and pass it to `make_app`:
- `BRAINCRAWL_AUTH_DISABLED` set (any non-empty value, or `=1`/`true`) → `disabled = true`,
  allowlist constructed from an empty/placeholder secret.
- else read `BRAINCRAWL_AUTH_TOKEN`; if unset → `panic!`/`expect` with a clear message
  ("set BRAINCRAWL_AUTH_TOKEN or BRAINCRAWL_AUTH_DISABLED=1"). Tenant for the shared secret
  is the literal `"default"`.
- Document the two new env vars in the header doc-comment table.

### 5. Server tests

- `apps/server/tests/smoke.rs` — the 3 existing tests assert functional behavior, not auth.
  Update `start_server` to build `make_app(store, Arc::new(AuthConfig{ disabled: true, .. }))`
  so they keep passing unchanged. (Construct the disabled config directly — no env.)
- New `apps/server/tests/auth.rs` — boot the app with auth **enabled**
  (`disabled: false`, `SharedSecret::new("bc_test_token", "default")`):
  - no `Authorization` header → `401`;
  - `Authorization: Bearer wrong` → `403`;
  - `Authorization: Bearer bc_test_token` on `PUT /works` → `200`;
  - a separate disabled-config app serves `PUT /works` with no header → `200` (bypass).
  Follow smoke.rs's `start_server` harness pattern (random port, `reqwest`).

### 6. Worker gate (`apps/worker/src/lib.rs`)

- Add `braincrawl-auth` to `apps/worker/Cargo.toml` deps.
- At the **top of `#[event(fetch)] async fn main`**, before `build_store`:
  - read the shared secret from `env.secret("AUTH_TOKEN")`; if it errors/missing →
    `Response::error("auth not configured", 500)`.
  - build `SharedSecret::new(&secret, "default")`, read the `Authorization` header from
    `req.headers().get("Authorization")?`, call `authorize`:
    - `Unauthenticated` → `Response::error("unauthorized", 401)`;
    - `Forbidden` → `Response::error("forbidden", 403)`;
    - `Authenticated(_tenant)` → fall through to existing routing (tenant unused this task).
  - The worker has **no** disable flag — it's always on in production.
- Keep everything `?Send`-free: this is synchronous header work before any store future.

### 7. Worker config (`apps/worker/wrangler.toml`)

- Add a declared-intent KV binding (unused this task):
  ```toml
  [[kv_namespaces]]
  binding = "AUTH_KV"
  id = "placeholder-fill-at-deploy"
  ```
- Add a comment noting the shared secret is provisioned via `wrangler secret put AUTH_TOKEN`.

### 8. OpenAPI (`.rhidoc/02-architecture/01-core/03-openapi.yaml`)

Add the security scheme and apply it globally (matches doc02.05 §API surface):
```yaml
components:
  securitySchemes:
    bearerAuth: { type: http, scheme: bearer }
security:
  - bearerAuth: []
```
Place `security:` at the top level (sibling of `paths:`); add `securitySchemes` under the
existing `components:`. Do not change request/response schemas. (Edit via the file directly —
this is a referenced asset, not a structural rhidoc doc op.)

## Files to Modify

- `crates/auth/Cargo.toml` — new crate manifest (deps: `braincrawl-core` not needed; `sha2`).
- `crates/auth/src/lib.rs` — `Tenant`, `hash_token`, `parse_bearer`, `AuthOutcome`,
  `Allowlist`, `SharedSecret`, `authorize` + unit tests.
- `Cargo.toml` (workspace) — add `crates/auth` member; add `sha2` workspace dep.
- `apps/server/Cargo.toml` — add `braincrawl-auth` dep.
- `apps/server/src/lib.rs` — `AuthConfig`, `gate` middleware, `make_app` new signature.
- `apps/server/src/main.rs` — build `AuthConfig` from env; refuse silent unauthed start.
- `apps/server/tests/smoke.rs` — pass a disabled `AuthConfig` to `make_app`.
- `apps/server/tests/auth.rs` — new: 401 / 403 / 200 / disabled-bypass.
- `apps/worker/Cargo.toml` — add `braincrawl-auth` dep.
- `apps/worker/src/lib.rs` — bearer gate at top of `fetch`.
- `apps/worker/wrangler.toml` — `AUTH_KV` binding (declared intent) + `AUTH_TOKEN` secret note.
- `.rhidoc/02-architecture/01-core/03-openapi.yaml` — `bearerAuth` scheme + global `security`.

## Verification

```bash
# auth crate builds + its unit tests pass
cargo test -p braincrawl-auth

# host workspace (worker is wasm-only, excluded) builds and all tests pass,
# including the new server auth gate tests and the unchanged smoke tests
cargo build --workspace --exclude braincrawl-worker
cargo test --workspace --exclude braincrawl-worker

# best-effort wasm check of the worker gate (skip if target unavailable)
cargo check -p braincrawl-worker --features cloudflare --target wasm32-unknown-unknown || \
  echo "wasm32 target not installed — worker gate not type-checked here"
```

## Out of Scope

- KV (`AUTH_KV`) allowlist lookup; multi-token issuance / rotation / revocation.
- Per-tenant Layer 3 isolation, per-tenant D1.
- Consuming the resolved tenant in any use-case (L3 has no schema yet).
- Any change to `crates/core`.

## Notes

- doc02.05 is the design source of truth. Honor its asymmetry in spirit (L1/L2 shared, L3
  tenant-scoped) — but L3 enforcement is explicitly deferred; the tenant is resolved and
  parked at the edge only.
- The worker crate only compiles under the `cloudflare` feature on wasm32, so host
  `--workspace` builds exclude it; the gate there is verified by the best-effort wasm check
  (or in CI/`wrangler` build), not by host tests.
- `~/.braincrawl/config` holds the live token (`bc_` prefix). For real local use the user
  sets `BRAINCRAWL_AUTH_TOKEN` to that same value; tests use their own throwaway token.
- Reviewer watch-points: middleware ordering (gate must wrap *all* routes incl. wildcards),
  axum version-specific middleware imports, and that no use-case signature changed.
