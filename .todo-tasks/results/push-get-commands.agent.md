# Agent Result: push-get-commands

date: 2026-06-26T17:44:39-04:00
session: completed
verification: passed
commits: 3
branch: feat/milestone01_claude_push-get-commands
surface deviations: none
turns: 47/100
cost: $1.6844821999999997/$5.00
uncommitted: none
session id: 86e5d0ce-c427-4af0-bd48-24d6858da647


## Summary

None.

## Commits

```
eb219fd test: custom-role round-trip and ArtifactRole::parse validation
aecdccc rename push-pdf/get-pdf → push/get, add --role arg
c45d6ed open ArtifactRole: add Other(String) + as_str/parse, route all serialize/parse sites
```

## Build & Test Output (last 30 lines)

```
      --text                     Output as compact text (one result per line)
      --limit <LIMIT>            Maximum number of results to return
      --source-url <SOURCE_URL>  Optional source URL recorded with the artifact
      --all                      Return all results, ignoring limit
      --role <ROLE>              Artifact role slug (e.g. fulltext, abstract, map) [default: fulltext]
      --fields <FIELDS>          Comma-separated list of fields to include in output
      --full                     Return full record (all fields)
      --skip-push                Do not push results to the local store
      --abstract                 Include reconstructed abstract text (bulky; off by default)
  -h, --help                     Print help
Read a stored artifact's bytes to stdout

Usage: braincrawl get [OPTIONS] <ID>

Arguments:
  <ID>  Work id in ns:value form (e.g. openalex:W2304167012)

Options:
      --json             Output as JSON (default)
  -o, --output <OUTPUT>  Write bytes to this path instead of stdout
      --role <ROLE>      Artifact role slug (e.g. fulltext, abstract, map) [default: fulltext]
      --text             Output as compact text (one result per line)
      --limit <LIMIT>    Maximum number of results to return
      --all              Return all results, ignoring limit
      --fields <FIELDS>  Comma-separated list of fields to include in output
      --full             Return full record (all fields)
      --skip-push        Do not push results to the local store
      --abstract         Include reconstructed abstract text (bulky; off by default)
  -h, --help             Print help
push-pdf removed OK
```
