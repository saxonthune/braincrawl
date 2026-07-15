# L3 docs: one-deep previous-version backup on both backends

## Motivation

The Cloudflare copy of an L3 doc has no history (git only covers the
desktop repo; R2 has no versioning) — between syncs, a bad overwrite is
unrecoverable. Keep the prior version on every PUT, on BOTH backends to
preserve the parity contract.

## Do NOT

- Do NOT build multi-version history — exactly one previous version.
- Do NOT change the PUT success path's response or status codes.
- Do NOT back up agent files (`l3-agent/*`, `_agent/*`) — docs only.
- Do NOT let prev copies appear in doc lists, the graph, INDEX.md, or
  sync (`l3 push/pull` must not see them).

## Plan

Note: the server-l3-parity phase has merged before this one — the PUT
handlers on both backends share `l3::normalize_doc`; this task touches
only the storage step around it.

### 1. Worker

In `apps/worker/src/l3.rs` `handle_put_doc`: after normalization
succeeds and before writing, if `l3/<slug>.l3.md` exists, copy its
current bytes to `l3-prev/<slug>.l3.md`. Extend `handle_get_doc`: query
`?version=prev` reads `l3-prev/<slug>.l3.md` (404 if absent). No other
change.

### 2. Server

Same semantics in `apps/server/src/handlers/l3.rs`: before overwriting
`<root>/<slug>.l3.md`, copy it to `<root>/_prev/<slug>.l3.md` (create
`_prev/` lazily; `_` prefix keeps it invisible to scan/parse/index/sync).
`GET /api/l3/docs/{slug}?version=prev` reads it.

### 3. Conformance

`crates/conformance/src/lib.rs`: `check_l3_prev_backup(base_url, token)`
— PUT a doc (v1), PUT again with changed content (v2), GET with
`?version=prev` → equals normalized v1, plain GET → normalized v2;
`?version=prev` on a never-overwritten doc → 404. Invoke it everywhere
`check_l3_docs` is invoked (worker external test AND the server-side
harness the parity phase established — find its callers and mirror).

## Files to Modify

- `apps/worker/src/l3.rs`
- `apps/server/src/handlers/l3.rs`
- `crates/conformance/src/lib.rs` + the two harness call sites

## Verification

```bash
./scripts/worker-conformance.sh
cargo test --workspace
```

## Out of Scope

- Restore endpoint (client can GET prev and PUT it back)
- Any UI

## Surface after this phase

- Both backends: PUT preserves the prior doc at a hidden prev location;
  `GET /api/l3/docs/{slug}?version=prev` serves it (404 when none).
- `check_l3_prev_backup` runs in both conformance gates.
- Negative space: PUT responses, doc lists, graph, sync unchanged.
