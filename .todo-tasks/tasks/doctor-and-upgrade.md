# `braincrawl doctor` and a single converging `just upgrade`

## Motivation

braincrawl has several artifacts with independent lifecycles — the CLI binary on PATH, the
server binary systemd executes, the web bundle, the config file, and the database schema.
Nothing records which version any of them is, so drift is invisible until it surfaces as an
unrelated error several layers away: `error: unrecognized subcommand 'push'` (stale PATH
binary), `error: server returned 500: backend error: no such table: artifacts` (stale server
binary serving a database migrated by a newer one).

The current response is a sprawl of imperative repair recipes — `upgrade`, `upgrade-cli`,
`restart`, `restart-cli`, `update-local` — each fixing a drift already suffered. Choosing the
right one requires a diagnosis nothing supports. The fix is one read-only command that shows
every watched thing, and one converging command that repairs all of them.

This is live right now: `GET /health` on the running server returns
`{"service":"braincrawl","status":"ok"}` with no `version` field, though
`apps/server/src/lib.rs:468` emits one. That field was added 2026-07-24; the running process
started 2026-07-22.

Two failures this must make obvious, both hit in real sessions: a PATH binary older than the
running server, and a CLI pointed at a different store than the operator assumes (the
author's `~/.config/braincrawl/config.toml` sets `server_url = "https://bcp.saxon.zone"`
while the local server runs on `127.0.0.1:8787` with different contents).

## Do NOT

- Do NOT change how the PATH CLI is installed. `cargo install --path apps/cli --force`
  copies a binary and that stays true. Switching to a symlink is a separate, unapproved
  decision.
- Do NOT add repo-relative checks to `braincrawl doctor`. The CLI runs from any directory
  and must not assume a checkout exists. Checks that need the working tree belong in the
  justfile.
- Do NOT make `doctor` mutate anything — no rebuilds, no restarts, no writes to the store,
  no scaffolding of config. It is read-only.
- Do NOT invent a second name for the update command. `just upgrade` already exists; extend
  it. Do not add `sync`, `converge`, `refresh`, or similar.
- Do NOT touch `apps/worker/` or any Cloudflare deploy path. The worker is a proof of
  concept and is not in the update path.
- Do NOT write a check that needs network access beyond the configured `server_url`.
- Do NOT let a single failing probe abort the run. Every check reports its own status;
  `doctor` always prints the full list.

## Plan

### 1. Give the CLI a build stamp

`apps/cli` has no `build.rs`, so `braincrawl --version` prints `0.1.0` — the crate version,
which never changes and identifies nothing.

Copy `apps/server/build.rs` to `apps/cli/build.rs` unchanged. It runs
`git describe --always --dirty --tags` and emits `cargo:rustc-env=BRAINCRAWL_BUILD`, with
`rerun-if-changed` on `.git/HEAD` and `.git/index`. The relative paths (`../../.git/HEAD`)
are already correct for a crate at `apps/cli`.

Surface it as `env!("BRAINCRAWL_BUILD")` and include it in `--version` output via clap's
`#[command(version = ...)]` on `Cli` in `apps/cli/src/cli.rs`.

### 2. Report applied migrations from `/health`

`apps/server/src/lib.rs:464` (`handler_health`) currently returns `status`, `service`, and
`version`. Add the names of the migrations applied in the live database, and the names
compiled into the running binary (`braincrawl_sql::migrations()` returns
`&[(&str, &str)]` — take the `.0` of each).

Reading the applied set needs a new read-only method on the metadata store. Add
`applied_migrations()` to the `MetadataStore` trait in `crates/core/src/traits.rs`
returning `Result<Vec<String>, DomainError>`, implement it in
`crates/backends/store-sqlite/src/lib.rs` (`SELECT name FROM _migrations ORDER BY name`),
and give `store-mem` and `store-d1` implementations that return an empty vector.

`handler_health` sits outside the auth gate (`apps/server/src/lib.rs:565`) — keep it there.

Note: migrations are applied on store construction (`apply_migrations`, called from
`SqliteStore::open` at `crates/backends/store-sqlite/src/lib.rs:134`), so a running server
cannot have an un-migrated database relative to its own binary. The mismatch this detects is
the opposite one — a database migrated by a newer binary, then served by an older one.

### 3. The registry — `apps/cli/src/doctor.rs`

One new file. Its whole point is that adding a watched thing is a small, local addition, so
keep the entry shape flat and obvious.

Define a `Check` with: a stable kebab-case `name`, a short `what` describing what it
inspects, a probe returning a `Status` (`Ok`, `Warn`, `Fail`, or `Info` for facts that
cannot fail, like store identity), a rendered `detail` line, and a `remedy` string printed
only when the status is not `Ok`.

Registry entries, in this order:

- `server-reachable` — `GET {server_url}/health` responds. Fail stops dependent checks from
  probing, but they still print as `Fail` with "server unreachable".
- `server-url` — the effective `server_url` and where it came from (env, file, or default).
  Always `Info`.
- `cli-build` — this binary's `BRAINCRAWL_BUILD`. Always `Info`.
- `server-build` — the `version` field from `/health`. `Warn` if the field is absent, which
  itself means the server predates the stamp.
- `build-match` — `Ok` when the two stamps are equal, `Warn` otherwise. Remedy: `just upgrade`.
- `store-identity` — works, edges, `library_bytes`, `catalog_bytes` from `GET /stats`.
  Always `Info`. This is what lets an operator tell two stores apart.
- `l3-root` — the CLI's `config.l3_root()` against the root the server reports. The server
  does not expose it today; add it to the `/health` body as `l3_root` (a string, or null
  when `BRAINCRAWL_L3_ROOT` is unset). `Warn` on mismatch, `Info` when the server reports
  null.
- `migrations` — applied set vs compiled set from `/health`. `Ok` when equal, `Warn` when
  the database has migrations the binary does not know, `Fail` when the binary has
  migrations the database lacks. Remedy: `just upgrade`.
- `config-source` — for each of `server_url`, `auth_token`, `openalex_api_key`,
  `semanticscholar_api_key`, `unpaywall_email`, `crossref_mailto`, `l3_repo`: whether the
  value came from env, the config file, or the default, and whether it is set at all. Never
  print secret values — for `auth_token` print only whether it is set. Always `Info`.

Render one line per check: status marker, name, detail. Print any remedy indented beneath
its check. Follow the existing text-rendering style in `apps/cli/src/main.rs`
(`render_stats`, around line 590) — plain `writeln!` into a `String`, no new dependency.

Honor the global `--json` flag by emitting the checks as an array of objects with `name`,
`status`, `detail`, and `remedy`; `--text` gives one tab-separated line per check.

Exit non-zero if any check is `Fail`, so a session can branch on it. `Warn` alone exits zero.

### 4. Wire it into the command tree

Add `#[command(about = "Report drift across the CLI, server, config, and store")] Doctor`
to the `Namespace` enum in `apps/cli/src/cli.rs` (near `Web`, which is the closest existing
zero-argument variant), and dispatch it in `apps/cli/src/main.rs` alongside the other
namespace arms. Add `pub mod doctor;` to `apps/cli/src/lib.rs`.

`doctor` builds its own `StoreClient` from `Config::resolve()` exactly as the other arms do.

### 5. Collapse the repair recipes in the justfile

Redefine `upgrade` as the one converging command. It must be idempotent and always safe to
run. In order:

1. Run `braincrawl doctor` and print its output. Do not abort on a non-zero exit — the point
   is to show the state before repairing it.
2. `cargo build --release --bin braincrawl-server --bin braincrawl`
3. `cargo install --path apps/cli --force`
4. Build the web bundle **only if** the installed unit references it — test for
   `BRAINCRAWL_WEB_ROOT` in
   `"${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/braincrawl-server.service"`. Skip
   silently when the unit is the CLI-only variant or absent.
5. `systemctl --user restart braincrawl-server`
6. Run `braincrawl doctor` again and print it, so the operator sees the result.

Delete `upgrade-cli`, `restart`, `restart-cli`, and `update-local`. Keep `install`,
`build`, `build-release`, `skill-install`, the `systemd-*` recipes, `watch`, and everything
under the Web UI and worker headings.

Update the two comments that name a deleted recipe: `systemd-restart`'s comment refers to
`just restart` (justfile line ~25), and `install`'s comment refers to `just watch` (fine, keep).

### 6. Update the README

The Setup section added recently names `just upgrade-cli`. Replace those references with
`just upgrade`, and mention `braincrawl doctor` as the first thing to run when something
looks wrong.

## Files to Modify

- `apps/cli/build.rs` — new; copy of `apps/server/build.rs`
- `apps/cli/src/doctor.rs` — new; the `Check` type, the registry, and rendering
- `apps/cli/src/lib.rs` — add `pub mod doctor;`
- `apps/cli/src/cli.rs` — `Doctor` variant on `Namespace`; `version` on `Cli`
- `apps/cli/src/main.rs` — dispatch arm for `Namespace::Doctor`
- `apps/server/src/lib.rs` — `handler_health` reports migrations and `l3_root`
- `crates/core/src/traits.rs` — `applied_migrations()` on `MetadataStore`
- `crates/backends/store-sqlite/src/lib.rs` — implement `applied_migrations`
- `crates/backends/store-mem/src/lib.rs` — implement, returning an empty vector
- `crates/backends/store-d1/src/lib.rs` — implement, returning an empty vector
- `justfile` — redefine `upgrade`; delete four recipes
- `README.md` — `just upgrade`, and a line on `braincrawl doctor`
- `apps/cli/tests/` or an inline `#[cfg(test)] mod tests` in `doctor.rs` — see Verification

Add tests for the pure parts of `doctor.rs`: status derivation from a pair of build stamps,
and the migration comparison (equal, database ahead, binary ahead). These take plain values,
not a live server, so they need no fixture. Follow the test style in
`crates/backends/store-sqlite/tests/stats.rs`.

## Verification

```bash
cargo build --release --bin braincrawl --bin braincrawl-server
cargo test
./target/release/braincrawl doctor --json
./target/release/braincrawl --version
just --list
```

`doctor --json` must emit one object per registry entry even when the server is unreachable.
`--version` must print a git describe stamp, not `0.1.0`. `just --list` must no longer show
`upgrade-cli`, `restart`, `restart-cli`, or `update-local`.

## Out of Scope

- Switching the PATH CLI install from a copy to a symlink
- Repo-relative checks inside `braincrawl doctor` (working tree newer than installed binary)
- Any change to `apps/worker/` or a Cloudflare deploy path
- Auto-repair from `doctor` — it reports and names the remedy, nothing more
- Store size or statistics work; `braincrawl catalog stats` already covers it

## Notes

- `braincrawl --version` printing `0.1.0` is the current, useless behavior — step 1 is the
  precondition for `cli-build` and `build-match`, so do it first.
- The author's config points at a remote store. `store-identity` and `server-url` exist
  specifically so that situation is visible in one line rather than discovered by
  confusion.
- Naming: `doctor` and `upgrade` are settled. Do not introduce alternative verb names.
- Never print the value of `auth_token`.
