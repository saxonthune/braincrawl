# Agent Result: artifact-listing-surface

date: 2026-07-22T14:30:17-04:00
session: completed
verification: passed
commits: 2
branch: chain-cli-layer-grammar_claude_artifact-listing-surface
surface deviations: none
turns: 86/200
cost: $3.0536833999999997/$5.00
uncommitted: 1 files, 585 lines
session id: ccd65ef7-ccdc-4e1a-b6a2-cad1287a4321


## Summary

None. `GET /works/*id/artifacts`, `crates/conformance::artifact_listing`, `StoreClient::list_artifacts`, `LibraryCmd::List`/`LibraryListArgs`, and the `library list` CLI verb all match the declared Surface exactly, and `crates/core`, `crates/sql`, and all three backends are untouched.

## Commits

```
a7c4d2f docs: document library list verb and regenerate CLI grammar canvas
776af5c feat: expose artifact listing via HTTP route and library list CLI verb
```

## Build & Test Output (last 30 lines)

```
      --json             Output as JSON (default)
      --text             Output as compact text (one result per line)
      --limit <LIMIT>    Maximum number of results to return
      --all              Return all results, ignoring limit
      --fields <FIELDS>  Comma-separated list of fields to include in output
      --full             Return full record (all fields)
      --skip-push        Do not push results to the local store
      --abstract         Include reconstructed abstract text (bulky; off by default)
      --emission         Emit the store-ready emission frame instead of the display envelope
  -h, --help             Print help
List every artifact a work holds — role, version, size, mime, provenance

Usage: braincrawl library list [OPTIONS] <ID>

Arguments:
  <ID>  Work id in ns:value form (e.g. openalex:W2304167012)

Options:
      --json             Output as JSON (default)
      --role <ROLE>      Restrict to a single artifact role
      --all-versions     Include superseded versions, not just the current one per role
      --text             Output as compact text (one result per line)
      --limit <LIMIT>    Maximum number of results to return
      --all              Return all results, ignoring limit
      --fields <FIELDS>  Comma-separated list of fields to include in output
      --full             Return full record (all fields)
      --skip-push        Do not push results to the local store
      --abstract         Include reconstructed abstract text (bulky; off by default)
      --emission         Emit the store-ready emission frame instead of the display envelope
  -h, --help             Print help
```
