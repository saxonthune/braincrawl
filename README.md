# braincrawl

A reusable academic knowledge graph built from open scholarly metadata, with a
clean split between shared general knowledge and per-consumer domain projections.

- **Library (L1)**: works keyed by a UUID; abstracts inline, full text on demand.
  `braincrawl library list <id>` shows every artifact a work holds — role, version,
  size, mime, provenance — instead of guessing a role name.
- **Catalog (L2)**: the citation graph — catalog entries (works, authors) and the edges between them — an accreting cache over upstream providers, built once and shared.
- **Research Collection (L3)**: per-domain Research Documents that *reference* Catalog UUIDs, never copying them. Managed via `braincrawl collection` in a consolidated, config-driven store.

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

## CLI: lookup and store-write decompose

Provider commands (`openalex`, `semanticscholar`, `arxiv`) push their results to the
Catalog by default. `--skip-push` suppresses that write, giving a pure lookup;
`--emission` prints the store-ready `Emission` frame (instead of the display envelope)
so a held lookup result can be written later with `braincrawl catalog put`, which reads
an `Emission` from stdin or a file. So:

```sh
braincrawl openalex search works "…" --skip-push --emission | braincrawl catalog put
```

is equivalent to plain `braincrawl openalex search works "…"`.

## Docs

Architecture and conventions live in [`.rhidoc/`](.rhidoc/MANIFEST.md).

## License

[AGPL-3.0-or-later](LICENSE).
