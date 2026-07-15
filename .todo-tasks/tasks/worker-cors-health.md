# Worker: CORS support + unauthenticated /health

## Motivation

A browser-based PWA client (SolidJS, phase later in this chain) will call the
Cloudflare worker cross-origin. Browsers send an OPTIONS preflight before any
request carrying an `Authorization` header, and the worker currently answers
every unmatched request — including preflights — with 401/404 and no CORS
headers, so all browser calls fail. The worker also lacks the unauthenticated
`GET /health` liveness probe that `apps/server` exposes, so clients cannot
distinguish "server down" from "bad token".

## Do NOT

- Do NOT touch `apps/server` or `apps/server-bin` — this task is worker-only.
- Do NOT wire `AUTH_KV` or change auth semantics for any non-OPTIONS request.
  The bearer-token gate stays exactly as it is for every real route.
- Do NOT add a CORS dependency/crate — hand-set the headers; it is a handful
  of lines.
- Do NOT reflect the request's `Origin` or requested headers back dynamically.
  Use the fixed values below (`*` origin; this API authenticates via a bearer
  header, not cookies, so wildcard origin is safe and correct).
- Do NOT edit `wrangler.toml`.
- Do NOT add CORS assertions inside `braincrawl_conformance::run_all` — that
  suite may run against the native server, which has no CORS. Add separate
  public check functions instead (step 3).

## Plan

### 1. Restructure the worker fetch handler so every response gets CORS headers

In `apps/worker/src/lib.rs`, the `#[event(fetch)] async fn main` currently
returns from many branches. Move the existing body (auth gate + routing) into
an inner `async fn route(req: Request, env: Env) -> worker::Result<Response>`,
and make `main`:

1. If `req.method() == Method::Options`: return `204` immediately (before any
   auth) with headers:
   - `Access-Control-Allow-Origin: *`
   - `Access-Control-Allow-Methods: GET, PUT, POST, OPTIONS`
   - `Access-Control-Allow-Headers: Authorization, Content-Type`
   - `Access-Control-Max-Age: 86400`
2. Otherwise call `route(req, env)` and, whatever comes back (200, 202, 401,
   404, 500 — all of them), set `Access-Control-Allow-Origin: *` on the
   response headers before returning it. Use a small helper
   (e.g. `fn with_cors(resp: Response) -> worker::Result<Response>`).

### 2. Add unauthenticated GET /health

Inside `route`, BEFORE the auth gate: if `Method::Get` and path == `/health`,
return `200` with JSON body `{"service":"braincrawl","status":"ok"}` — the
same shape `apps/server` returns from its `/health` handler (verify the exact
field order/shape by reading the server's health handler and match it).
Everything else stays behind the auth gate as today.

### 3. Conformance checks

In `crates/conformance/src/lib.rs`, add two public functions alongside
`run_all` (same style: take `base_url` and use the existing HTTP client
pattern in that file):

- `check_health(base_url)` — GET `/health` with NO Authorization header;
  expect 200 and JSON containing `"status":"ok"`.
- `check_cors_preflight(base_url)` — OPTIONS `/works/have` with headers
  `Origin: https://example.invalid` and
  `Access-Control-Request-Method: POST`, NO Authorization header; expect a
  2xx status (204) and response headers `Access-Control-Allow-Origin: *` and
  an `Access-Control-Allow-Headers` value containing `Authorization`.

In `crates/conformance/tests/external.rs`, call both new functions after
`run_all` (they apply to the worker, which is what this external test drives).

## Files to Modify

- `apps/worker/src/lib.rs` — OPTIONS handling, `with_cors` helper, `/health`
  route before the auth gate, `main`/`route` split
- `crates/conformance/src/lib.rs` — `check_health`, `check_cors_preflight`
- `crates/conformance/tests/external.rs` — invoke the two new checks

## Verification

```bash
./scripts/worker-conformance.sh
```

## Out of Scope

- CORS on the native axum server (PWA will be served same-origin there; the
  vite dev server proxies).
- Multi-token auth via AUTH_KV (separate inbox task `worker-auth-kv-tenancy`).
- Restricting the allowed origin to a configured value (can be tightened once
  the PWA's origin is settled).

## Notes

- `worker::Response::error(...)` responses must also carry the CORS header —
  that's why the wrap happens once in `main` around `route`'s result, not
  per-handler.
- The conformance script (`scripts/worker-conformance.sh`) boots the worker
  under `wrangler dev --local` with a test token and runs
  `cargo test -p braincrawl-conformance --test external`; no network or real
  Cloudflare account needed.

## Surface after this phase

- Worker exposes `GET /health` with no auth → 200,
  `{"service":"braincrawl","status":"ok"}`.
- Worker answers `OPTIONS <any path>` with 204 + CORS preflight headers
  (`Access-Control-Allow-Origin: *`, methods `GET, PUT, POST, OPTIONS`,
  headers `Authorization, Content-Type`), no auth required.
- Every worker response (success and error) carries
  `Access-Control-Allow-Origin: *`.
- `braincrawl_conformance::check_health(url)` and
  `braincrawl_conformance::check_cors_preflight(url)` exist as public fns.
- Negative space: auth gate, all existing routes, `wrangler.toml`, and the
  native server are unchanged. `run_all` is unchanged.
