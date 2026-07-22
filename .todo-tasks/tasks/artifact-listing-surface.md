# Artifact listing, part 2 — HTTP route and `library list`

## Motivation

The previous phase added artifact listing at the store seam: `ArtifactStore::list_artifacts`,
`usecases::Store::list_artifacts`, the SQL, and all three backends. None of it is
reachable — the store's HTTP contract has no listing route, `StoreClient` has no
method, and the CLI has no verb.

Until it is reachable, a work's artifacts remain findable only by guessing role
names. That matters more now than it did: after the derive-verb phase, ordinary
use puts artifacts at `fulltext`, `text`, and `chunks`, and nothing tells a user
or an agent which of those a given work actually holds.

This phase carries the read out through the HTTP contract to the CLI, and pins it
in the conformance suite so both store implementations stay in step.

## Do NOT

- Do NOT change `crates/core`, `crates/sql`, or any backend. The store seam is
  finished; this phase only exposes it. If you find yourself editing
  `traits.rs`, stop — the method you need is already there.
- Do NOT add the listing to `GET /works/*id`. It is its own route, so a metadata
  read does not get heavier.
- Do NOT return `r2_key` or `content_hash` in the CLI's rendered output. They are
  internal addressing, not user-facing. They may pass through the HTTP payload.
- Do NOT make the route or the verb fetch blob bytes.
- Do NOT let the route collide with the existing `/content/{role}` parsing. Both
  server and worker match `/works/*path` by searching for `/content/` in the
  remainder; the new suffix must be matched before or distinctly from that, and
  a work id containing the literal text `artifacts` must not be misrouted.
- Do NOT add a `--json`/`--text` mechanism of your own. The CLI's existing
  `Envelope` plus `render` already handles both.

## Plan

### 1. Add the route to the native server

In `apps/server/src/lib.rs`, `handler_works_get` dispatches `/works/*path` by
inspecting the remainder — `/edges` around line 280 and `/content/` at line 303.
Add a `/artifacts` suffix arm alongside them:

```
GET /works/*id/artifacts?role=<slug>&all_versions=<bool>
```

Parse the alias with the existing `parse_alias`, parse the optional `role` query
parameter with `parse_artifact_role` (a malformed role is `400`, consistent with
the content routes), read `all_versions` from the query as a boolean defaulting
to false, call `store.list_artifacts(...)`, and return the descriptors as JSON
under an `artifacts` key. Map errors through the existing `domain_status` helper
so an unknown work behaves the way the content routes do.

### 2. Add the same route to the worker

In `apps/worker/src/lib.rs`, the `/works/` dispatch is around line 556, with the
`/edges` and `/content/` arms just below. Add the matching `/artifacts` arm with
identical semantics, identical query parameters, and an identical response body.
Update the route list in the module doc comment at the top of the file (lines
9-17), which enumerates the surface.

The two implementations must agree exactly — the conformance suite in step 3 is
what holds them to it.

### 3. Add a conformance check

In `crates/conformance/src/lib.rs`, `content_roundtrip` (line 526) already puts
and gets artifact bytes. Add an `artifact_listing` check in the same style and
register it in `run_all` (line 5).

It must: put a work, put content at two distinct roles, put a second version at
one of those roles, then assert that listing returns one descriptor per role by
default with `is_current` true, that `all_versions=true` returns the extra
version, that `role=<slug>` restricts correctly, and that the descriptor fields
(role, version, byte size, mime) match what was written.

### 4. Add the `StoreClient` method

In `apps/cli/src/store_client.rs`, add `list_artifacts` beside `get_content`
(line 229), following the crate's existing request-building and error idiom:

```rust
pub fn list_artifacts(
    &self,
    alias: &str,
    role: Option<&str>,
    all_versions: bool,
) -> Result<Vec<serde_json::Value>>
```

Returning `serde_json::Value` matches how `get_work`, `neighborhood`, and `stats`
already hand payloads to the renderers.

### 5. Add the `library list` verb

In `apps/cli/src/cli.rs`, add to `LibraryCmd`:

```rust
/// List every artifact a work holds — role, version, size, mime, provenance
List(LibraryListArgs),
```

with

```rust
pub struct LibraryListArgs {
    /// Work id in ns:value form
    pub id: String,
    /// Restrict to a single artifact role
    #[arg(long)]
    pub role: Option<String>,
    /// Include superseded versions, not just the current one per role
    #[arg(long = "all-versions")]
    pub all_versions: bool,
}
```

In `apps/cli/src/main.rs`, add the dispatch arm inside `Namespace::Library`. Call
the client, wrap the descriptors in an `Envelope` with
`entity: Some("library:list")` — matching the `catalog:get` and `catalog:have`
labels the catalog arms use — and pass it to `render(&envelope, &opts)`, so
`--json`, `--text`, and `--fields` all work without special handling.

Strip `r2_key` and `content_hash` from each descriptor before rendering.

### 6. Update the prose and canvases

- `README.md` and `.claude/skills/braincrawl/SKILL.md` — document `library list`,
  and use it in any example that previously had to assume a role name.
- `.luminous/braincrawl.atlas.json` — add a `cli.braincrawl.library.list` node
  under the library parent with an edge to `client.store-client`, and add the new
  route to the `contract.store.http` node's route listing.
- Regenerate the derived canvas:
  ```
  cargo run -p braincrawl-cli --example luminous_cli_grammar
  ```

## Files to Modify

- `apps/server/src/lib.rs` — `/works/*id/artifacts` GET route
- `apps/worker/src/lib.rs` — the same route, plus the module doc route list
- `crates/conformance/src/lib.rs` — `artifact_listing` check, registered in `run_all`
- `apps/cli/src/store_client.rs` — `list_artifacts`
- `apps/cli/src/cli.rs` — `LibraryCmd::List` and `LibraryListArgs`
- `apps/cli/src/main.rs` — the `library list` dispatch arm
- `README.md`, `.claude/skills/braincrawl/SKILL.md` — document the verb
- `.luminous/braincrawl.atlas.json` — new node, edge, and route line
- `.luminous/cli-grammar.{signal,graph,pack}.json` — regenerated, not hand-edited

## Verification

```bash
cargo build
cargo test
cargo run -p braincrawl-cli --example luminous_cli_grammar
cargo run --quiet --bin braincrawl -- library --help
cargo run --quiet --bin braincrawl -- library list --help
```

`library --help` must list `list`. `library list --help` must show `--role` and
`--all-versions`. Every invocation must exit 0.

The conformance suite needs a running server and is not part of this gate. Run it
by hand afterward with `just worker-test` (requires wrangler and network), or
against the native server, and report the result.

## Out of Scope

- Reading the bytes of a superseded version. This phase lists versions; `library get`
  still serves only the current one per role. A `--version` flag on `library get`
  is a separate task.
- Deleting or pruning artifacts.
- Showing artifacts in the Web UI.
- Any change to the store seam added in the previous phase.

## Notes

- The route-collision constraint is the main correctness risk. Both server and
  worker use `rfind("/content/")` on the remainder, so a work id is free-form
  text and a naive `contains("/artifacts")` could misfire. Match the suffix at
  the end of the path, and test with an id containing an unusual character.
- `just worker-test` boots the worker under `wrangler dev --local` with emulated
  D1, R2, and KV. It is the only way to exercise the D1 path end to end, and it
  needs network access, which is why it is outside the automated gate.
- Reviewer watch item: confirm the server and worker return byte-identical JSON
  shapes. The conformance suite runs against one base URL at a time, so a shape
  divergence only shows up when someone runs it against both.

## Surface after this phase

- `GET /works/*id/artifacts?role=<slug>&all_versions=<bool>` on both the native
  server and the worker, returning `{ "artifacts": [...] }` with one descriptor
  per artifact. Malformed role is `400`; unknown work follows `domain_status`.
- `crates/conformance` exposes an `artifact_listing` check registered in `run_all`.
- `StoreClient::list_artifacts(&self, alias: &str, role: Option<&str>, all_versions: bool) -> Result<Vec<serde_json::Value>>`
  in `apps/cli/src/store_client.rs`.
- `braincrawl library list <id> [--role <slug>] [--all-versions]` — renders
  through the standard `Envelope` and `render`, so `--json`, `--text`, and
  `--fields` all apply. `r2_key` and `content_hash` are stripped from the output.
- `LibraryCmd::List(LibraryListArgs)` and `pub struct LibraryListArgs { id, role, all_versions }`
  exist in `apps/cli/src/cli.rs`.
- Negative space: `crates/core`, `crates/sql`, and all three backends are exactly
  as the previous phase left them. `GET /works/*id` still returns a `WorkView`
  with no artifact field. `library get` still serves only the current version of
  a role and has no `--version` flag. Every other CLI verb — `library put`,
  `library fetch`, `library extract-text`, `library chunk`, `catalog put`,
  `catalog get`, `catalog have`, `catalog neighborhood`, `catalog stats`, and the
  whole `collection` namespace — is unchanged.
