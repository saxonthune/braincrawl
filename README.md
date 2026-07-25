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

## Setup

Needs Rust and cargo, [`just`](https://just.systems), and a systemd user session
(the server runs as a user service). SQLite and TLS are compiled in, so there are no
system libraries to install.

```sh
just install              # build the `braincrawl` CLI into ~/.cargo/bin
just systemd-install-cli  # install + start the shared server, no Web UI
just skill-install        # install the braincrawl skill for Claude Code
just systemd-status       # active + {"service":"braincrawl","status":"ok"}
```

`systemd-install-cli` skips the Web UI entirely, so it needs neither the Vite+
toolchain nor pnpm. To serve the browser UI at `/web` as well, use `just systemd-install`
instead — it builds `web/` first. After pulling changes, run `just upgrade-cli` (or
`just upgrade` for the Web UI build) to refresh both the CLI on your PATH and the
running server.

Then write `~/.config/braincrawl/config.toml`:

```toml
server_url = "http://127.0.0.1:8787"
# unpaywall_email = "you@example.com"   # required by `fetch-content --from unpaywall|auto`
# crossref_mailto = "you@example.com"   # Crossref polite pool
# l3_repo = "~/code/braincrawl-l3"      # Research Collection store; see below
```

OpenAlex needs no key. Config precedence is environment variable, then this file,
then the default.

`l3_repo` sets where Research Documents live. Left unset it defaults to
`~/.local/share/braincrawl/l3`, which is where the installed unit already points
`BRAINCRAWL_L3_ROOT` — so the CLI and server agree with no further setup. If you point
it at a git repo of your own, set `BRAINCRAWL_L3_ROOT` to match in
`~/.config/braincrawl/server.env`, or the two will read different directories.

Create your first collection and check it:

```sh
braincrawl collection new my-first-topic
braincrawl collection index
```

## Build

```sh
just            # list recipes
cargo test      # workspace tests
cargo run -p braincrawl-cli -- --help
```

Server env: `BRAINCRAWL_DB`, `BRAINCRAWL_BLOB_ROOT`, `BRAINCRAWL_BIND`,
`BRAINCRAWL_AUTH_TOKEN` (or `BRAINCRAWL_AUTH_DISABLED=1`), `BRAINCRAWL_L3_ROOT`,
`BRAINCRAWL_WEB_ROOT`, `BRAINCRAWL_CROSSREF_MAILTO`, `BRAINCRAWL_UNPAYWALL_EMAIL`.

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
