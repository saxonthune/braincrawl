# CLI: l3 push / l3 pull against the worker L3 endpoints

## Motivation

Phone sessions write L3 docs on the worker (R2); desktop sessions edit the
markdown repo. DECIDED: minimal viable sync — explicit `braincrawl l3 push`
and `braincrawl l3 pull` verbs, whole docs, content-hash skip, warn-and-stop
when both sides changed since the last sync. Git in the l3 repo stays the
history; no merge logic.

## Do NOT

- Do NOT implement node-level merging, three-way diffs, or a daemon/watch
  mode. Doc-granular copy in an explicitly chosen direction only.
- Do NOT auto-resolve a both-sides-changed doc — report it and exit 1;
  the human resolves in the markdown and re-pushes.
- Do NOT touch the worker or `crates/l3` parsing logic (build on the
  previous phases' Surface).
- Do NOT store sync state anywhere except a single `_`-prefixed file in
  the l3 root (`_` prefix is the established "tooling file, skip me"
  convention honored by `scan()` and `parse()`; a dotfile or new dot-dir
  is NOT).
- Do NOT git-commit anything in the l3 repo from these verbs.

## Plan

### 1. Store client methods

`apps/cli/src/store_client.rs` gains (same auth/base-url plumbing as the
existing methods):
- `l3_list() -> Vec<L3RemoteDoc { doc, size, modified }>` — GET /api/l3/docs
- `l3_get(slug) -> Option<String>` — GET /api/l3/docs/{slug}
- `l3_put(slug, body, force: bool) -> Result<String, L3PutError>` —
  PUT /api/l3/docs/{slug}; surface the 400-with-warnings and 409 cases as
  typed errors so the CLI can print diagnostics.

### 2. Sync state

`_sync-state.json` in the l3 root: `{ "<slug>": { "hash": "<sha256 of the
doc content as of last successful sync>" } }`. Read/write helpers in
`apps/cli/src/l3.rs`. Absent file = never synced.

Change classification per doc (local hash `L`, remote content hash `R`,
recorded `S`):
- `L == R` → in sync (refresh `S`, no transfer)
- `L != S && R == S` → local-only change → push candidate
- `L == S && R != S` → remote-only change → pull candidate
- `L != S && R != S && L != R` → CONFLICT → report, skip, exit 1 at end
- doc missing on one side → candidate in the direction that creates it
  (never delete on either side; deletion propagation is out of scope)

### 3. Verbs

In `apps/cli/src/l3.rs` + the clap enum in `apps/cli/src/cli.rs`:
- `braincrawl l3 push [doc] [--dry-run] [--force]` — for each push
  candidate (or the one named doc): `l3_put`; on success the worker
  returns NORMALIZED markdown — write it back over the local file (the
  worker may have stamped anchors/updated), then record its hash in
  `_sync-state.json`. `--force` passes `force=1` (bypass warning bounce).
- `braincrawl l3 pull [doc] [--dry-run]` — for each pull candidate:
  write remote content to `<slug>.l3.md`, record hash. After any writes,
  call the module's `reindex(root)` once (it is module-internal, so the
  verbs living in `l3.rs` can call it directly).
- Both verbs: print a per-doc action line (`push <slug>`, `pull <slug>`,
  `skip <slug> (in sync)`, `CONFLICT <slug> (changed on both sides)`), a
  summary count, and exit 1 if any conflicts were seen. `--dry-run` prints
  the plan without transferring or touching state.
- Named-doc form operates on that doc regardless of candidate
  classification EXCEPT conflicts, which still require the human (the
  escape hatch is: resolve locally, then `l3 push <doc>` after a `pull`
  brought the remote version in under a different name — do NOT add a
  `--theirs/--ours` flag).

### 4. Tests

The classification logic (step 2) must be a pure function
(`fn classify(local: Option<&str>, remote: Option<&str>, recorded: Option<&str>) -> SyncAction`
over hashes) with unit tests covering all branches, in `apps/cli`
following its existing test conventions. HTTP paths are exercised
manually (documented in Notes), not by the test gate.

## Files to Modify

- `apps/cli/src/store_client.rs` — three L3 methods + types
- `apps/cli/src/l3.rs` — state file, classify, push/pull implementations
- `apps/cli/src/cli.rs` — clap variants `Push`/`Pull` under the l3
  subcommand with the flags above

## Verification

```bash
cargo test -p braincrawl
cargo build --workspace
```

## Out of Scope

- Deletion propagation, rename handling, merge of concurrent edits
- Scheduled/automatic sync
- Server-side (native) L3 doc endpoints

## Notes

- Env-var mismatch to be careful with: the CLI resolves the l3 root via
  `BRAINCRAWL_L3_REPO`/config `l3_repo`; the server uses
  `BRAINCRAWL_L3_ROOT`. These verbs are CLI-side: use `Config::l3_root()`.
- Manual smoke (document in the result notes, don't automate here):
  `wrangler dev --local` + `BRAINCRAWL_SERVER_URL=http://127.0.0.1:8799
  braincrawl l3 push <doc>` round-trip.
- Hash = sha256 over the exact file bytes; compute the remote hash over
  the fetched body. The normalized-write-back on push keeps local bytes
  identical to remote bytes afterward, so `L == R` detection stays exact.

## Surface after this phase

- CLI verbs `braincrawl l3 push [doc] [--dry-run] [--force]` and
  `braincrawl l3 pull [doc] [--dry-run]` with the semantics above.
- `_sync-state.json` in the l3 root (per-doc last-synced sha256), ignored
  by parse/scan/index.
- `store_client` exposes `l3_list` / `l3_get` / `l3_put`.
- Push adopts the worker's normalized markdown locally; pull reindexes
  INDEX.md.
- Negative space: no worker changes; no git operations; existing l3 verbs
  (new/path/list/check/index/import/rm/assign-ids/reading-list) unchanged.
