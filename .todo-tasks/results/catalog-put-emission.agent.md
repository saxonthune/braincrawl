# Agent Result: catalog-put-emission

date: 2026-07-22T14:12:22-04:00
session: completed
verification: passed
commits: 3
branch: chain-cli-layer-grammar_claude_catalog-put-emission
surface deviations: none
turns: 48/200
cost: $1.5499289999999992/$5.00
uncommitted: none
session id: 85668a41-3ad9-48c5-a7be-d71bf686db37


## Summary

None.

## Commits

```
60b5d09 test: fix OutputOpts literals for new emission field
d16041e docs: document --emission and catalog put identity
5e686cd feat: --emission flag and catalog put verb (agent)
```

## Build & Test Output (last 30 lines)

```
      --fields <FIELDS>  Comma-separated list of fields to include in output
      --full             Return full record (all fields)
      --skip-push        Do not push results to the local store
      --abstract         Include reconstructed abstract text (bulky; off by default)
      --emission         Emit the store-ready emission frame instead of the display envelope
  -h, --help             Print help
Query the OpenAlex scholarly-data API

Usage: braincrawl openalex [OPTIONS] <COMMAND>

Commands:
  get           Fetch a single entity by ID (entity type inferred from ID prefix)
  search        Full-text search over an entity collection
  find          Filter an entity collection by key:value expressions
  autocomplete  Autocomplete entity names by prefix
  cited-by      List works that cite the given work
  refs          List works referenced by the given work
  help          Print this message or the help of the given subcommand(s)

Options:
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
```
