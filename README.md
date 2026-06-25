# braincrawl

A reusable academic knowledge graph built from open scholarly metadata, with a
clean split between shared general knowledge and per-consumer domain projections.

- **L1 — Corpus**: works keyed by canonical id; abstracts inline, full text on demand.
- **L2 — Metadata graph**: citation network, accreting cache over upstream providers, built once and shared.
- **L3 — Consumer projection**: per-domain selection + annotations *referencing* L2, never copying it. Managed via `braincrawl l3` in a consolidated, config-driven store.

Sources: OpenAlex (backbone), Semantic Scholar, Crossref, OpenCitations, Unpaywall.

## Legal

Full-text storage is **open-access only** — resolved via OpenAlex OA fields and
Unpaywall, gated structurally per work (`open` / `link_only` / `restricted`). The
base never fetches or persists paywalled or pirated text.

## Build

```sh
just            # list recipes
cargo test      # workspace tests
cargo run -p braincrawl-cli -- --help
```

Server env: `BRAINCRAWL_DB`, `BRAINCRAWL_BLOB_ROOT`, `BRAINCRAWL_BIND`,
`BRAINCRAWL_AUTH_TOKEN` (or `BRAINCRAWL_AUTH_DISABLED=1`),
`BRAINCRAWL_CROSSREF_MAILTO`, `BRAINCRAWL_UNPAYWALL_EMAIL`.

## Docs

Architecture and conventions live in [`.rhidoc/`](.rhidoc/MANIFEST.md).

## License

[AGPL-3.0-or-later](LICENSE).
