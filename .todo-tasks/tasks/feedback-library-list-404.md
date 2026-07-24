# Make a work's stored artifacts visible from the plain work GET, and make a 404 say what it looked up

## Motivation

`braincrawl library list <id>` returned `error: server returned 404: not found` on a work
whose `library put` and `library extract-text` had just succeeded. Diagnosis cost a detour
into the server source and finished in the wrong place.

The route and the alias resolution are both correct. The CLI reads
`~/.config/braincrawl/config.toml`, which points `server_url` at the deployed Cloudflare
Worker, and the whole `library list` feature — CLI verb, server route, worker route — landed
in commit `c771942` on an unmerged branch. The deployed Worker predates it, so it has no
`/artifacts` route. Redeploying the Worker fixes that symptom and is out of scope here.

Three things made a stale deployment look like a broken route, and those are what this task
fixes on the local server and the CLI:

1. **The plain work GET does not say what the store holds.** `GET /works/{alias}` returns the
   graph node only — canonical id, kind, attrs, provenance, aliases. Whether the Library
   holds a fulltext for that work is reachable only from a second route the caller has to
   know about. Asking the obvious place got a narrower answer than the question.
2. **CLI errors do not name the server they came from.** `ClientError::Server` carries a
   status and a body but no URL, so nothing in the output revealed that the CLI was talking
   to a remote host while the operator's `curl` check was hitting localhost.
3. **An unrecognized `/works/…` suffix is swallowed into the alias.** The dispatcher falls
   through to the plain work lookup (`apps/server/src/lib.rs:375`), so
   `/works/openalex:W3027234447/artifacts` is parsed as an alias whose value ends in
   `/artifacts`, and the honest answer to that question is `not found`. The fallthrough must
   stay — DOI aliases legitimately contain slashes — but the message must show what was
   actually looked up.

Plus a build version on `/health`, since a version is what actually distinguishes a stale
server from a current one, and the workspace version is a static `0.1.0` that never moves.

## Do NOT

- **Do NOT touch `apps/worker/`.** No worker code changes, no deploy. The worker's plain work
  GET keeps its current shape. This divergence is accepted and recorded below.
- **Do NOT add assertions to `crates/conformance/`** or to `apps/server/tests/conformance.rs`.
  That suite is shared — `scripts/worker-conformance.sh` runs it against the live Worker, so
  a new assertion about the artifacts key would fail there. All new tests go in
  `apps/server/tests/smoke.rs`.
- **Do NOT add `Serialize`/`Deserialize` to `Artifact` or `ArtifactRole`** in
  `crates/core/src/types.rs`, and do not add a field to `WorkView`. `ArtifactRole` has an
  `Other(String)` variant that would need custom serde, and changing those shared types
  reaches the worker and both backends. Compose the response in the server handler instead,
  reusing the existing `artifact_json` helper.
- **Do NOT return superseded artifact versions** from the work GET. Current versions only.
  History stays behind `/works/{id}/artifacts?all_versions=true`.
- **Do NOT change `Engine::list_artifacts` or `Engine::get_work`** in
  `crates/core/src/usecases.rs`. Both are correct; the composition happens above them.
- **Do NOT remove the plain-work fallthrough** in the suffix dispatcher, and do not add a
  reserved-suffix guard. Only the message changes.
- **Do NOT add a build dependency for the version string.** `std::process::Command` in a
  `build.rs` is enough.

## Plan

### 1. Include current artifacts in `GET /works/{alias}`

In `apps/server/src/lib.rs`, the plain-work branch of `handler_works_get` (`:374-385`)
currently calls `store.get_work(a)` and serializes the `WorkView` with `Json(view)`.

Change it so that when the work is found, the response body is the serialized `WorkView`
with one added key, `artifacts`, holding the work's current artifacts. Get them from
`store.list_artifacts(a, None, false)` — role filter `None`, `all_versions` `false` — and
serialize each through the existing `artifact_json` helper (`:208`), so the entries are
field-for-field identical to what `GET /works/{id}/artifacts` returns.

Both store calls need the same `Alias`, and the existing closure moves it, so clone the alias
(or do both calls inside one `run_blocking` closure — either is fine). A work with no
artifacts gets `"artifacts": []`, not a missing key.

To merge the key: serialize the `WorkView` with `serde_json::to_value`, then insert
`artifacts` into the resulting object. Keep `Ok(None) => 404` behaviour as it is apart from
the body change in step 2.

### 2. Make the plain-work 404 name the alias it looked up

Same branch. `Ok(None)` currently returns a bare `StatusCode::NOT_FOUND` with no body.
Return `(StatusCode::NOT_FOUND, format!("no work with alias '{}'", path))` instead, where
`path` is the raw captured tail. `path` is still in scope after `parse_alias(&path)` borrows
it.

This is the whole point of the change: a request to `/works/openalex:W123/artifacts` against
a server without that route now answers `no work with alias 'openalex:W123/artifacts'`, and
the swallowed suffix is visible in the message.

Leave the `parse_alias` failure branch (`"expected namespace:value"`) alone. Leave the
`/edges`, `/artifacts`, and `/content/` branches' error responses alone.

### 3. Report a build version from `/health`

Add `apps/server/build.rs` (new file, crate root, auto-detected by cargo — no `Cargo.toml`
change needed). It runs `git describe --always --dirty --tags` via `std::process::Command`
and emits the result as a compile-time env var:

```rust
println!("cargo:rustc-env=BRAINCRAWL_BUILD={}", value);
```

The build must never fail because of this: if the command errors, returns non-zero, or
produces empty output, emit `unknown`. Also emit `cargo:rerun-if-changed=` lines for
`.git/HEAD` and `.git/index` so the value refreshes when the checkout moves — and guard
those so a missing `.git` (agent worktrees, source tarballs) does not break the build.

In `handler_health` (`:445`), add a `version` key holding `env!("BRAINCRAWL_BUILD")`:

```json
{"status": "ok", "service": "braincrawl", "version": "c771942-dirty"}
```

Keep `status` and `service` exactly as they are — an existing consumer may read them.

### 4. Put the server URL in CLI error messages

In `apps/cli/src/store_client.rs`, add a `url: String` field to `ClientError::Server`
(`:7-8`) and update the `#[error(...)]` format to name it, e.g.:

```
server returned {status} from {url}: {body}
```

Then thread the request URL through all 15 construction sites in that file. Each already has
the URL in scope as a local (usually `url`), so pass `url.clone()` — or the formatted URL
where the site builds it inline. Do not change any other error variant.

`apps/cli/src/migrate.rs:30` only holds a `#[from]` conversion and needs no change.

### 5. Tests

Add to `apps/server/tests/smoke.rs`, following the existing `start_server` + `reqwest`
pattern in that file:

- **Work GET carries artifacts.** `PUT /works` to create a work, `PUT
  /works/{alias}/content/fulltext` to store an artifact, then `GET /works/{alias}` and assert
  `json["artifacts"]` is an array of length 1 whose `[0]["role"]` is `"fulltext"` and whose
  fields match what `GET /works/{alias}/artifacts` returns for the same work.
- **Work with no artifacts.** `PUT /works` only, then `GET /works/{alias}` and assert
  `json["artifacts"]` is present and empty.
- **404 names the alias.** `GET /works/doi:10.99/does-not-exist` returns 404 and a body
  containing `doi:10.99/does-not-exist`.
- **Health reports a version.** `GET /health` returns 200 with a non-empty `version` string,
  and `status` still `"ok"`.

## Files to Modify

- `apps/server/src/lib.rs` — artifacts in the plain work GET (`:374-385`); 404 body names the
  alias; `version` in `handler_health` (`:445`).
- `apps/server/build.rs` — **new**; emits `BRAINCRAWL_BUILD` from `git describe`, falling back
  to `unknown`.
- `apps/cli/src/store_client.rs` — `url` field on `ClientError::Server` (`:7-8`) and its 15
  construction sites.
- `apps/server/tests/smoke.rs` — four new tests per step 5.

## Verification

```bash
cargo test -p braincrawl-server-lib
cargo test -p braincrawl-cli
cargo build --bin braincrawl-server --bin braincrawl
```

## Out of Scope

- Deploying the Cloudflare Worker (`just deploy-worker`) — the operator's action, and the fix
  for the originally reported 404.
- Any change under `apps/worker/`, including giving the worker's work GET the same artifacts
  key. The worker and server responses diverge after this task.
- A reserved-suffix guard that answers "unknown sub-resource" instead of falling through to
  the work lookup. Considered and dropped: with artifacts on the plain GET, a caller has much
  less reason to go hunting for a sub-resource route.
- A CLI `health` verb, showing the configured server URL, or warning on a CLI/server version
  mismatch. That belongs to `feedback-cli-health-config-server-introspection`, which consumes
  the `version` field this task adds.
- Deleting artifacts or superseded versions — see `feedback-no-delete-in-library-or-catalog`.

## Notes

- The server and worker responses for `GET /works/{alias}` diverge after this task: the
  server carries `artifacts`, the worker does not. Deliberate, per the scope decision above.
  A follow-up task should close the gap before the worker becomes the primary target again.
- `list_artifacts` is one indexed lookup on `canonical_id`, and `get_work` is only ever
  called for single ids — the callers are `apps/cli/src/main.rs:61,148,204` and
  `apps/cli/src/fetch_content.rs:43,105`, plus `apps/server/src/handlers/fulltext.rs:45`.
  Neighborhood traversal has its own query and does not go through `get_work`. So the extra
  query per work read is on a cold path.
- Artifact counts per work are small: roles are `abstract`, `fulltext`, and free-form slugs,
  one current version each, and `library chunk` writes a single JSON artifact rather than one
  per chunk (`apps/cli/src/main.rs:552`). Nothing fans out.
- A reviewer should confirm the shared conformance suite still passes unchanged, since the
  worker must keep satisfying it.
