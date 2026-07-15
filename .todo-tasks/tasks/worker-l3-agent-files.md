# Worker: agent context-file endpoints + sync carry

## Motivation

The phone agent loads two freeform context files at session start — the
user's principles and the agent's memory (drafts exist at
`braincrawl-l3/_agent/{principles,memory}.md`). These don't fit the L3
doc surface: they aren't node-grammar docs (no anchors, no validation)
and their local names are `_`-prefixed, which the L3 scan/parse/sync
deliberately skip. Give them their own tiny worker surface and carry them
in `l3 push`/`l3 pull`.

## Do NOT

- Do NOT run L3 parsing, anchor assignment, warning validation, or
  `updated:` stamping on these files — they are opaque markdown.
- Do NOT let them appear in `GET /api/l3/docs`, `/api/l3/graph`, or
  INDEX.md.
- Do NOT invent extra file names/roles — the surface is generic by name,
  but only `principles` and `memory` are expected; do not hardcode those
  two names as the only allowed values (a third file later shouldn't need
  a code change).
- Do NOT touch `crates/l3`.

## Plan

### 1. Worker routes

In `apps/worker/src/l3.rs` (extend the existing module), behind the auth
gate:
- `GET /api/l3/agent` — list: R2 prefix `l3-agent/`, return
  `[{name, size, modified}]` (name without the `.md`).
- `GET /api/l3/agent/{name}` — raw markdown, 404 if absent. Name
  validation `[a-z0-9-]+` (400 otherwise).
- `PUT /api/l3/agent/{name}` — store body verbatim at
  `l3-agent/<name>.md`, respond 204. Enforce a size cap (64 KB, 413) so a
  runaway agent write can't balloon the always-in-context files.

Dispatch from `route` in `apps/worker/src/lib.rs` alongside the existing
`/api/l3/` block (the `/api/l3/agent` prefix must be checked BEFORE the
`/api/l3/docs/{slug}` matching so names never parse as doc slugs — check
how the merged dispatch is ordered and slot in accordingly).

### 2. CLI sync carry

Extend the merged `l3 push` / `l3 pull` (apps/cli/src/l3.rs,
store_client.rs):
- store_client gains `l3_agent_list()`, `l3_agent_get(name)`,
  `l3_agent_put(name, body)`.
- Local home: `<l3_root>/_agent/<name>.md`. Map local `_agent/<name>.md`
  ↔ remote `agent/<name>` in the same push/pull planning, reusing the
  existing hash classification (state-file key `agent:<name>` so it can't
  collide with doc slugs).
- Same conflict rule: both-changed → report, skip, exit 1.
- `--dry-run` covers them like docs.

### 3. Conformance

`crates/conformance/src/lib.rs`: `check_l3_agent_files(base_url, token)`
— PUT `principles` with a small body → 204; GET it back → identical
bytes; list contains it; GET a missing name → 404; oversize PUT → 413;
confirm the file does NOT appear in `GET /api/l3/docs`. Call from
`crates/conformance/tests/external.rs`.

## Files to Modify

- `apps/worker/src/l3.rs` — three handlers
- `apps/worker/src/lib.rs` — dispatch
- `apps/cli/src/store_client.rs` — three client methods
- `apps/cli/src/l3.rs` — include agent files in push/pull planning
- `crates/conformance/src/lib.rs` + `tests/external.rs` — new check

## Verification

```bash
./scripts/worker-conformance.sh
cargo test -p braincrawl-cli
```

## Out of Scope

- Native-server versions of these routes (server-l3-parity task)
- Any UI

## Notes

- The existing sync state file and classify logic are the merged
  cli-l3-sync surface — reuse, don't reimplement.

## Surface after this phase

- Worker routes (authed, CORS-wrapped): `GET /api/l3/agent` (list),
  `GET /api/l3/agent/{name}` (raw markdown | 404),
  `PUT /api/l3/agent/{name}` (204; 400 bad name; 413 over 64 KB).
  R2 keys `l3-agent/<name>.md`. No parsing/normalization; excluded from
  docs list, graph, and INDEX.
- `l3 push`/`l3 pull` carry `_agent/*.md` with the same hash/conflict
  semantics; state keys `agent:<name>`.
- `braincrawl_conformance::check_l3_agent_files(url, token)` exists.
- Negative space: doc endpoints, graph, crates/l3, chat plugin unchanged.
