# `push-pdf` rejects large files with 413 — disable the request-body limit on the content route

## Motivation

`push-pdf` 413s on book-sized PDFs (`413: Failed to buffer the request body:
length limit exceeded`). A 787 KB article succeeds; a 12 MB book fails. Root
cause confirmed: the inbound bytes land on axum's `Bytes` extractor in
`handler_works_put`, which is gated by axum's **default 2 MB
`DefaultBodyLimit`**. No `DefaultBodyLimit` layer is configured in `make_app`,
so the framework default applies. The cap is purely an HTTP-layer guard sitting
upstream of `put_content`/`blob.put` — lifting it on the byte-upload route is the
whole fix. Books are destined for object storage, so no body cap should gate
that route.

## Do NOT

- Do NOT touch `const MAX_BYTES` in `apps/server/src/handlers/fulltext.rs`. That
  caps the server-side *download/fetch* path (`reqwest`), a different code path
  from the inbound HTTP request body. It is unrelated to this 413 and must stay.
- Do NOT disable the body limit globally on the whole router. Scope the override
  to the wildcard `/works/*path` route only, so the JSON routes (`/works`,
  `/edges`, `/jobs`, `/graph/neighborhood`) keep axum's small default.
- Do NOT attempt to convert `handler_works_put` to streaming-to-blob. The body is
  buffered in memory (`body: Bytes` → `.to_vec()`); that stays as-is. Streaming is
  explicitly out of scope.
- Do NOT touch `apps/worker` (Cloudflare entry point). Out of scope.
- Do NOT add a new explicit cap value — the decision is to fully disable the
  limit on this route, not raise it to a number.

## Plan

### 1. Import `DefaultBodyLimit`

In `apps/server/src/lib.rs`, add `DefaultBodyLimit` to the axum imports. It lives
at `axum::extract::DefaultBodyLimit`. Extend the existing
`extract::{Path, Query, Request, State}` import group accordingly (becomes
`extract::{DefaultBodyLimit, Path, Query, Request, State}`).

### 2. Scope the limit override to the `/works/*path` route in `make_app`

In `make_app` (`apps/server/src/lib.rs:435`), the wildcard route is currently:

```rust
.route("/works/*path", get(handler_works_get).put(handler_works_put))
```

This route handles the fulltext byte upload (PUT) and must accept arbitrarily
large bodies, while the other routes in the `authed` router should keep the
default 2 MB cap. Because `DefaultBodyLimit` applies to the router scope it is
layered onto, pull the wildcard route into its own `Router` with the limit
disabled, then merge it into `authed`. Concretely:

- Build the wildcard route as a separate `Router::new().route("/works/*path",
  get(handler_works_get).put(handler_works_put)).layer(DefaultBodyLimit::disable())`.
- `.merge(...)` it into the `authed` router (keep the `gate` auth middleware and
  `.with_state(store)` applied to the combined `authed` router so the wildcard
  route is still authed and stateful — i.e. merge before `.layer(gate)` /
  `.with_state`).

The exact-match static routes (`/works/have`, `/works`, etc.) must still be
registered before/independently of the wildcard so they continue to win over it.
Verify route precedence is preserved (smoke tests already cover `/works`,
`/works/have`, and GET `/works/*path`).

## Files to Modify

- `apps/server/src/lib.rs` — import `DefaultBodyLimit`; in `make_app`, scope
  `DefaultBodyLimit::disable()` to the `/works/*path` route only.
- `apps/server/tests/smoke.rs` — add a test that PUTs a content payload larger
  than 2 MB and asserts it stores and round-trips (proving the cap is lifted).

## Verification

```bash
cargo test -p braincrawl-server-lib
cargo build -p braincrawl-server-lib
```

The new smoke test must PUT a >2 MB payload (e.g. a ~5 MB `Vec<u8>` body) to
`/works/{id}/content/fulltext?mime=application/pdf&rights=open&fetched_at=...`,
assert 200, then GET it back and assert the bytes match. Without the fix this
PUT returns 413; with it, 200. Also confirm the existing
`test_content_roundtrip`, `test_put_and_get_work`, and `test_have` still pass
(route precedence unbroken).

## Out of Scope

- Streaming uploads / avoiding in-memory buffering of the request body.
- The `MAX_BYTES` download cap in `fulltext.rs`.
- `apps/worker` (Cloudflare) request-body handling.
- The `restricted`-rights tombstone behavior (separate task
  `feedback-restricted-rights-rejects-bytes`).
- `extract-text` output quality / stdout noise.

## Notes

- The body is fully buffered in memory before reaching the blob layer, so
  "no limit" means "bounded by server memory." That is acceptable for this
  single-tenant local research server and matches the task's intent; the real
  streaming fix is deliberately deferred.
- A reviewer should confirm the merge/layer ordering keeps both the auth `gate`
  middleware and shared `store` state applied to the wildcard route — a common
  mistake when splitting a route into its own `Router` is dropping state or auth.

## Surface after this phase

- `make_app` still returns the same `Router` with identical route paths and auth
  behavior; the only change is that `PUT /works/*path/content/{kind}` accepts
  request bodies larger than 2 MB (limit disabled on that route).
- All other authed routes (`/works`, `/works/have`, `/edges`, `/jobs`,
  `/graph/neighborhood`, `/stats`) and the unauthenticated `/health` are
  unchanged, including their default request-body limit.
- The in-memory buffering of `handler_works_put` (`body: Bytes`) is unchanged;
  streaming-to-blob still does not exist.
