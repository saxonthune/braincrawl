# Worker: L3 doc endpoints (R2-backed, file-like read/write + graph)

## Motivation

The phone client reads and writes Research Collection docs against the
deployed worker. DECIDED: docs live as one R2 object per doc (verbatim
`.l3.md` markdown) under a dedicated key prefix; the write surface is
file-shaped — clients PUT whole markdown docs, and the worker normalizes
(validate, assign `^r-` anchors, stamp `updated:`) on write. Read side
also serves the whole-store graph JSON in the same shape as the native
server's `/api/l3/graph`.

## Do NOT

- Do NOT store parsed nodes in D1 — R2 markdown blobs are the only L3
  storage. No new D1 tables, no schema migrations.
- Do NOT route L3 blobs through the `Store` usecase / `BlobStore` trait or
  the `<guid>/<role>/v<n>` artifact key scheme. Use the raw
  `env.bucket("BLOB_BUCKET")` binding directly with the `l3/` key prefix.
- Do NOT add L3 checks to `braincrawl_conformance::run_all` (it also runs
  against the native server, which lacks these routes). Worker-only checks
  go in new public fns called from `tests/external.rs`, like
  `check_health` / `check_cors_preflight` already do.
- Do NOT touch `apps/server`, `apps/cli`, or `crates/l3` (the in-memory
  API from the previous phase is complete — build on its Surface).
- Do NOT put any L3 route above the auth gate — all L3 endpoints require
  the bearer token.

## Plan

All handlers live in a new `apps/worker/src/l3.rs` module; `route` in
`apps/worker/src/lib.rs` dispatches to it after the auth gate. R2 key for
a doc: `l3/<slug>.l3.md`. Slug validation: `[a-z0-9-]+` (reject otherwise,
400) so keys are safe. Use `worker::Bucket` directly (`env.bucket`), with
its native `.list()` for enumeration (`prefix = "l3/"`).

### 1. GET /api/l3/docs

List docs: R2 list with prefix `l3/`, return JSON array of
`{"doc": "<slug>", "size": <bytes>, "modified": "<r2 uploaded ts>"}`.

### 2. GET /api/l3/docs/{slug}

Return the raw markdown (`Content-Type: text/markdown; charset=utf-8`),
404 if absent.

### 3. PUT /api/l3/docs/{slug}

Body = full markdown. Pipeline:
1. Load ALL existing docs from R2 (`list` + `get` — the store is a few
   dozen small markdown files; per-PUT full scan is acceptable at this
   scale and keeps anchor state index-free. Note this in a code comment.)
2. Parse the incoming doc via `parse_sources(&[(slug, body)])`. If it
   yields `Warning`s and the query string does NOT contain `force=1`,
   return 400 with JSON `{"warnings": [{doc, line, message}, ...]}` — a
   garbled LLM edit must bounce with diagnostics, not corrupt the store.
3. `collect_anchors` over all OTHER docs' contents, then
   `assign_ids_source(body, &mut anchors)` to stamp missing `^r-` anchors.
   If an incoming anchor collides with an anchor owned by a DIFFERENT doc,
   return 409 with the offending ids (store-global uniqueness invariant).
4. `upsert_frontmatter_key(content, "updated", <today UTC yyyy-mm-dd>)`
   (date from `js_sys::Date` — same approach as `WasmClock`).
5. Write normalized text to `l3/<slug>.l3.md`; respond 200 with the
   normalized markdown (`text/markdown`) so the client adopts it.

### 4. GET /api/l3/graph

Load all docs, `parse_sources` over the full set, serialize the `Graph`
(same `{"nodes": [...], "links": [...]}` JSON as the native server — the
crate's serde impls guarantee the shape). Compute
`ETag: "<sha256-hex of body>"` and honor `If-None-Match` → 304, matching
`apps/server/src/handlers/l3.rs` semantics.

### 5. Conformance checks

In `crates/conformance/src/lib.rs`, add
`pub fn check_l3_docs(base_url: &str, token: &str) -> Result<...>`
(same error/reporting style as `check_health`): PUT a small valid doc with
one anchor-less `## heading` → assert 200 and that the returned markdown
contains a ` ^r-` anchor and an `updated:` line; GET it back → equals the
normalized text; GET `/api/l3/docs` → contains the slug; GET
`/api/l3/graph` → 200 JSON with ≥1 node and an ETag, and `If-None-Match`
replay → 304; PUT a doc with a malformed link line → 400 with warnings.
Call it from `crates/conformance/tests/external.rs` after the existing
checks.

## Files to Modify

- `apps/worker/src/l3.rs` — new module with the four handlers
- `apps/worker/src/lib.rs` — dispatch `/api/l3/*` inside `route`; add
  `braincrawl-l3` dependency usage
- `apps/worker/Cargo.toml` — depend on `braincrawl-l3` (+ sha2 if not
  already available for the ETag)
- `crates/conformance/src/lib.rs` — `check_l3_docs`
- `crates/conformance/tests/external.rs` — invoke it

## Verification

```bash
./scripts/worker-conformance.sh
```

## Out of Scope

- CLI push/pull (next phase)
- Native-server parity for these routes (server keeps its fs-based
  `/api/l3/graph`; per-doc routes stay worker-only for now)
- Anchor index optimization (KV/D1) — revisit if doc count makes per-PUT
  scans slow

## Notes

- `crates/l3` compiles to wasm; only its in-memory API is used here — the
  fs entry points are never called from the worker.
- Ensure route dispatch order: `/api/l3/graph` and `/api/l3/docs` checks
  must come before the generic `/works/` prefix block (they don't collide,
  but keep the L3 block together and early for readability).

## Surface after this phase

- Worker routes (all bearer-authed, all CORS-wrapped):
  - `GET /api/l3/docs` → `[{doc, size, modified}]`
  - `GET /api/l3/docs/{slug}` → raw markdown, 404 if absent
  - `PUT /api/l3/docs/{slug}` (`?force=1` to bypass warning rejection) →
    200 + normalized markdown; 400 `{warnings}` on grammar problems;
    409 on cross-doc anchor collision
  - `GET /api/l3/graph` → `{nodes, links}` JSON, ETag + If-None-Match/304,
    same shape as the native server's endpoint
- R2 key scheme `l3/<slug>.l3.md`, slugs `[a-z0-9-]+`.
- Normalization contract: response markdown is canonical (anchors
  assigned, `updated:` stamped); clients replace their local copy with it.
- `braincrawl_conformance::check_l3_docs(url, token)` exists.
- Negative space: works/edges/content routes, auth, run_all, native
  server, CLI — all unchanged.
