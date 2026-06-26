# braincrawl

A reusable academic knowledge graph built from open scholarly metadata, with a
clean split between shared general knowledge and per-consumer domain projections.

- **Library (L1)**: works keyed by a UUID; abstracts inline, full text on demand.
- **Catalog (L2)**: the citation graph — catalog entries (works, authors) and the edges between them — an accreting cache over upstream providers, built once and shared.
- **Research Collection (L3)**: per-domain Research Documents that *reference* Catalog UUIDs, never copying them. Managed via `braincrawl l3` in a consolidated, config-driven store.

Sources: OpenAlex (backbone), Semantic Scholar, Crossref, OpenCitations, Unpaywall.

## Use

braincrawl fetches and stores scholarly metadata and content from third-party
sources. You are responsible for ensuring your use complies with the copyright,
licensing, and terms of service of those sources. The software is provided
"as is", without warranty (see LICENSE §15–16).

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
