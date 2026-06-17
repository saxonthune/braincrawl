# braincrawl openalex — push / store mapping

## Motivation

Wire **push-by-default** for `braincrawl openalex` (doc02.06, doc02.06.01.01). Each
verb already produces a `PushBatch` (from `braincrawl-openalex-read`) that is
currently discarded. This phase maps those records onto the store's node model and
persists them via the foundation `StoreClient`, alongside the context return.
`--skip-push` suppresses persistence. Entities without a node kind stay context-only.

## Preconditions (already on trunk)

The foundation crate `apps/cli` (`Config`, `StoreClient`, `Envelope`) and the read
phase's `openalex` module (`PushBatch`, `Entity`, verbs) are on trunk — read live
code. **The server enforces bearer auth** (`crates/auth`, `make_app(store, auth)`):
the gate reads `Authorization: Bearer <token>` unless `AuthConfig.disabled`. The
current `StoreClient` sends NO auth header and `Config` has no token, so push WILL
`401` against an enabled server. Wiring auth is part of this phase (step 0 below).

## Do NOT

- Do NOT resolve or merge identity in the CLI. Emit aliases and let the server's
  upsert API own resolution (doc02.01.02). One node per fetched entity; the server
  dedupes by alias.
- Do NOT push entities without a node kind. Institutions, publishers, funders,
  keywords have NO mapping — skip silently (they remain context-only) regardless of
  `--skip-push`.
- Do NOT invent server endpoints or change request shapes. Use `StoreClient.put_work`
  / `put_edges` with the exact `WorkRecord` / `EdgeInput` JSON shapes.
- Do NOT change the read-phase surface (verbs, envelope, client, trim). Only consume
  `PushBatch` and add the mapping + push wiring.
- Do NOT fail the whole command if a push fails — return context, report the push
  error on stderr, exit non-zero. The fork's job is to show upstream data.

## Plan

### 0. Auth wiring (`src/config.rs` + `src/store_client.rs`)

The server requires `Authorization: Bearer <token>` unless auth is disabled. Wire it:
- `Config`: add `auth_token: Option<String>` resolved from `BRAINCRAWL_AUTH_TOKEN`
  (env) and the config file, same precedence as the existing fields.
- `StoreClient`: hold an optional token (e.g. `StoreClient::new(base_url)` unchanged +
  `with_token(Option<String>)` builder, or a new ctor) and attach
  `.bearer_auth(token)` to every request (`put_work`, `put_edges`, `have`, `get_work`)
  when a token is present.
- `main.rs`: build the `StoreClient` with `Config.auth_token`.
- A `401`/`403` from the server must surface as a clear push error (token missing/
  wrong), not a silent skip.

### 1. Mapping module (`src/openalex/mapping.rs`)

- `node_kind(Entity) -> Option<&'static str>`:
  works→`"Work"`, authors→`"Author"`, sources→`"Venue"`, topics→`"Topic"`,
  concepts→`"Concept"`; everything else → `None` (not pushable).
- `extract_aliases(Entity, &Value) -> Vec<Alias>` (alias = `{namespace,value}` JSON):
  - works: `openalex`(W…), `doi`, `pmid`, `pmcid`, `mag` from `ids`.
  - authors: `openalex`, `orcid`.
  - sources: `openalex`, `issn_l`, plus each `issn`.
  - topics: `openalex`. concepts: `openalex`, `wikidata`.
  - Normalize: strip URL prefixes to bare values (e.g. `https://openalex.org/W…`→`W…`,
    DOI URL→bare DOI), matching how the server's `parse_alias` expects `ns:value`.
- `to_work_record(Entity, &Value) -> Option<Value>`: build
  `{ "source":"openalex", "kind":<node_kind>, "aliases":[…], "attrs":<the record> }`.
  `attrs` is the trimmed record (free-form JSON) — reuse the read phase's `trim`
  output, or `--full` record when set.

### 2. Edge mapping

- `to_edges(pairs: &[(String,String)]) -> Vec<Value>`: for each `(citing, cited)`
  OpenAlex-id pair build `EdgeInput` JSON:
  `{ "src":{"namespace":"openalex","value":citing},
     "dst":{"namespace":"openalex","value":cited},
     "relation":"cites", "source":"openalex", "attrs":null,
     "fetched_at":<RFC3339 now> }`.
- `cited-by <id>`: pairs are `(result_id, id)`. `refs <id>`: pairs are `(id, ref_id)`.

### 3. Push wiring (`src/openalex/verbs.rs` + `main.rs`)

- After a verb builds its `Envelope` and `PushBatch`, if `!skip_push`:
  - For each `(entity, record)` in `PushBatch.records` where `node_kind` is `Some`,
    `to_work_record` → `StoreClient.put_work`.
  - If `PushBatch.edges` non-empty, `to_edges` → `StoreClient.put_edges`.
  - Collect a small push summary (nodes pushed, edges pushed, skipped-unmappable).
- Render the envelope to context as before; when pushing, also print the push summary
  to stderr (so stdout stays the clean envelope).
- Add a timestamp helper (RFC3339 "now"); add `time`/`chrono` as a dep only if needed,
  otherwise format from `std::time::SystemTime`.

### 4. Config / client plumbing

- The verbs now need a `StoreClient` (built from `Config.server_url` + `auth_token`,
  see step 0) in addition to the `OpenAlexClient`. Thread it through the openalex
  dispatch in `main.rs`.

## Files to Modify

- `apps/cli/src/config.rs` — add `auth_token` (`BRAINCRAWL_AUTH_TOKEN`).
- `apps/cli/src/store_client.rs` — optional bearer token on all requests.
- `apps/cli/src/openalex/mapping.rs` — new: node kind, alias extraction, record/edge builders.
- `apps/cli/src/openalex/verbs.rs` — consume `PushBatch`, push when `!skip_push`.
- `apps/cli/src/openalex/mod.rs` — export mapping; push-summary type.
- `apps/cli/src/main.rs` — build/thread `StoreClient` (with token) into openalex dispatch.
- `apps/cli/Cargo.toml` — `braincrawl-server` + `braincrawl-auth` + `tempfile` dev-deps
  already present from the foundation; timestamp dep only if `std` is insufficient.
- `apps/cli/tests/openalex_push.rs` — integration test.

## Verification

```bash
cargo build -p braincrawl-cli
cargo test -p braincrawl-cli
```

`apps/cli/tests/openalex_push.rs` spins the server in-process on an ephemeral port
via `braincrawl_server::{make_app, make_store, AuthConfig}` + `braincrawl_auth::
SharedSecret` (mirror `apps/cli/tests/foundation.rs`, which is the working pattern).
Run the server with **auth enabled** (`AuthConfig { disabled: false, allowlist:
SharedSecret::new("test-token", "default") }`), point `StoreClient` at it with the
matching token, and assert: `to_work_record` produces the expected `kind`/aliases;
push via `StoreClient.put_work` succeeds and `get_work("openalex:W…")` returns the
node with matching `attrs`; a `StoreClient` with a wrong/absent token gets `401`/`403`
(auth wiring verified). Assert a fixture institution maps to `node_kind == None` and
is skipped; `to_edges` builds well-formed `cites` edges and `put_edges` returns the
expected count. Unit-test alias URL normalization. No live OpenAlex calls.

## Out of Scope

- Giving institutions/publishers/funders/keywords a push target (requires expanding
  `NodeKind`/server — separate, later work).
- The smart routing verbs and content/abstract payload upload (`PUT /works/*/content`).
- Multi-tenant tokens / KV allowlist — a single shared bearer token is enough here.

## Notes

- The server's `parse_alias` splits on the first `:` into `namespace`/`value`; aliases
  pushed via `WorkRecord` JSON are structured `{namespace,value}` so no string parsing
  is needed there, but keep values bare (no URL prefixes) for consistency with `have`/
  `get` which use `ns:value`.
- `kind` MUST be the PascalCase variant string (`"Work"` etc.) — that is how
  `NodeKind` deserializes server-side.
- Push failure is non-fatal to the context return but must surface (stderr + non-zero
  exit), so an agent never believes data persisted when it did not.

## Surface after this phase

- `Config.auth_token` (`BRAINCRAWL_AUTH_TOKEN`) and a `StoreClient` that sends
  `Authorization: Bearer <token>` on every request.
- `apps/cli/src/openalex/mapping.rs`: `node_kind`, `extract_aliases`, `to_work_record`,
  `to_edges`.
- `braincrawl openalex` verbs push by default (works/authors/sources/topics/concepts →
  nodes; `cited-by`/`refs` → `cites` edges); `--skip-push` opts out; unmappable
  entities are context-only.
- Push summary reported on stderr; stdout remains the clean `Envelope`.
- Negative space: non-pushable entity kinds and content-payload upload remain
  unimplemented; identity resolution remains server-owned.
