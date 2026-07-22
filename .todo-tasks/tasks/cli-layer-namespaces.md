# Reorganize the CLI command tree by storage layer

## Motivation

The CLI's command tree does not match the layer model the rest of the system is
built on. Only Layer 3 has a namespace (`l3`). Layer 1's verbs are scattered as
top-level commands (`push`, `get`, `fetch-content`, `fetch-pdf`, `extract-text`,
`chunk`) and Layer 2's are split between a hidden dev namespace (`store have`,
`store get`) and two more top-level commands (`graph neighborhood`, `stats`).

The scattering also gives one word two meanings: top-level `push` writes artifact
bytes to the Library, while `--skip-push` on the provider commands refers to
writing metadata to the Catalog. A later phase adds a Catalog write verb, and
there is no unambiguous name available for it until the tree is reorganized.

This phase is a pure reorganization: one namespace per layer, named for the
glossary term, with every verb moved beneath its layer. No verb changes behavior.

## Do NOT

- Do NOT change any command's behavior, arguments, defaults, or output. This
  phase moves verbs and renames namespaces only. If a verb's observable result
  differs after your change, you have gone too far.
- Do NOT add `l1`, `l2`, or `l3` as clap aliases. The user chose one spelling per
  thing. There must be exactly one way to name each namespace.
- Do NOT keep the old top-level spellings as hidden aliases or deprecated
  variants. This is a hard rename; the old spellings disappear.
- Do NOT add the `catalog put` verb or the `--emission` flag — those are the next
  phase and depend on this one.
- Do NOT fold `fetch-pdf` into `fetch`, and do NOT make `extract-text` write to
  the store. Both are a later phase. In this phase `fetch-pdf` simply moves to
  `library fetch-pdf` unchanged.
- Do NOT rename the `l3` Rust module, the `L3Cmd` / `L3Args` type names, the
  `l3_repo` config key, the `.l3.md` file extension, or the worker's `/api/l3/*`
  routes. Only the user-facing clap namespace changes. The wire format and the
  config keep their existing names deliberately.
- Do NOT hand-edit `.luminous/cli-grammar.signal.json`, `.luminous/cli-grammar.graph.json`,
  or `.luminous/cli-grammar.pack.json` — they are generated from clap. Regenerate
  them with the example binary instead.

## Plan

### 1. Restructure the `Namespace` enum in `apps/cli/src/cli.rs`

The enum is at `cli.rs:41-79`. Replace these variants — `Store`, `Graph`,
`Stats`, `Push`, `Get`, `FetchContent`, `ExtractText`, `FetchPdf`, `Chunk`, `L3`
— with three layer namespaces:

```rust
#[command(about = "Artifact bytes held against a work (Layer 1)")]
Library(LibraryArgs),
#[command(about = "Work metadata, aliases, and citation edges (Layer 2)")]
Catalog(CatalogArgs),
#[command(about = "Research Documents — the consolidated store (Layer 3)")]
Collection(CollectionArgs),
```

Keep `Openalex`, `Semanticscholar`, `Crossref`, `Opencitations`, `Arxiv`, `Web`,
`MigrateStore`, and `Rename` exactly as they are — they are not layer verbs.

### 2. Define `LibraryArgs` / `LibraryCmd`

`LibraryCmd` reuses the existing argument structs verbatim, so the moves are
mechanical:

- `Put(PushArgs)` — "Store bytes from stdin or a file as an artifact of the given role"
- `Get(GetArgs)` — "Read a stored artifact's bytes to stdout"
- `Fetch(FetchContentArgs)` — "Acquire a work's fulltext artifact from the open web into the store"
- `FetchPdf(FetchPdfArgs)`, named `fetch-pdf` — "Resolve and download a work's fulltext artifact to stdout (no store write)"
- `ExtractText(ExtractTextArgs)`, named `extract-text` — "Extract text from a work's stored fulltext PDF artifact"
- `Chunk(ChunkArgs)` — "Partition a work's stored fulltext into a citation-carrying chunks artifact"

Do not rename `PushArgs`, `GetArgs`, `FetchContentArgs`, `FetchPdfArgs`,
`ExtractTextArgs`, or `ChunkArgs` — keeping the struct names stable keeps the
diff readable.

### 3. Define `CatalogArgs` / `CatalogCmd`

Fold in the old `StoreCmd` (`cli.rs:430-440`), the old `GraphCmd`
(`cli.rs:296-312`), and the old top-level `Stats`:

- `Get { id: String }` — "Retrieve a work by alias (ns:value)"
- `Have { ids: Vec<String> }` — "Check which of the given aliases are present in the store"
- `Neighborhood { seeds, dir, depth, max_nodes }` — carry the existing arg
  attributes and defaults across verbatim (`dir` default `"forward"`, `depth`
  default 1, `max_nodes` default 200 with `#[arg(long = "max-nodes")]`)
- `Stats` — "Show aggregate statistics about the metadata network"

Delete `StoreArgs`, `StoreCmd`, `GraphArgs`, and `GraphCmd`. The `hide = true`
attribute that was on the `Store` namespace does not carry over — `catalog` is a
visible, user-facing namespace.

### 4. Define `CollectionArgs`

`CollectionArgs` is a new `#[derive(Args)]` struct wrapping the **existing**
`L3Cmd` subcommand enum unchanged:

```rust
#[derive(Args)]
pub struct CollectionArgs {
    #[command(subcommand)]
    pub cmd: L3Cmd,
}
```

Leave `L3Cmd` and all its variants alone. `L3Args` becomes unused — delete it.

### 5. Rewrite the dispatch in `apps/cli/src/main.rs`

The `match cli.namespace` block starts at `main.rs:34`. Restructure the arms to
match the new tree. Every arm's body moves across unchanged:

- `Namespace::Library(lib)` wraps a `match lib.cmd` whose arms are the former
  `Namespace::Push` (`main.rs:316`), `Namespace::Get` (~`main.rs:377`),
  `Namespace::FetchContent` (`main.rs:267`), `Namespace::FetchPdf` (`main.rs:294`),
  `Namespace::ExtractText` (`main.rs:234`), and `Namespace::Chunk` (`main.rs:354`).
- `Namespace::Catalog(cat)` wraps a `match cat.cmd` whose arms are the former
  `Namespace::Store` inner arms (`main.rs:39-77`), the former `Namespace::Graph`
  inner arm (`main.rs:84-88`), and the former `Namespace::Stats` (`main.rs:90-95`).
- `Namespace::Collection(c)` calls `braincrawl_cli::l3::dispatch(c.cmd, &config, &opts)?`
  exactly as the former `Namespace::L3` arm did (`main.rs:96-98`).

Each arm constructs its own `StoreClient` today; hoisting that construction is
allowed but not required. Update the `use` statement at `main.rs:2` for the
renamed types.

The `Envelope` `entity` labels in the catalog arms currently read `"store:have"`
and `"store:get"` (`main.rs:48`, `main.rs:67`). Change them to `"catalog:have"`
and `"catalog:get"` so the machine-readable output names the layer it came from.

### 6. Update the prose that names these commands

Three files outside the source reference the old spellings. Update every command
example in each to the new grammar:

- `README.md`
- `.claude/skills/braincrawl/SKILL.md`
- `.rhidoc/01-product/01-glossary.md`

Rewrite the examples in place. Do not add notes about the commands having been
renamed — the docs state current truth, and history lives in git.

### 7. Update the hand-maintained atlas

`.luminous/braincrawl.atlas.json` carries nodes for the CLI surface. Update the
node ids, names, and content text for every moved verb — `cli.braincrawl.store.get`
becomes `cli.braincrawl.catalog.get`, `cli.braincrawl.store.have` becomes
`cli.braincrawl.catalog.have`, `cli.braincrawl.graph` becomes
`cli.braincrawl.catalog.neighborhood`, `cli.braincrawl.push` becomes
`cli.braincrawl.library.put`, and so on for `get`, `fetch-content`, `fetch-pdf`,
`extract-text`, `chunk`, `stats`, and the `cli.braincrawl.l3*` family.

Any node id you change must have its `edges` entries updated to match — the file
has edges referencing `cli.braincrawl.store.get`, `cli.braincrawl.store.have`,
`cli.braincrawl.stats`, `cli.braincrawl.graph`, `cli.braincrawl.push`,
`cli.braincrawl.get`, `cli.braincrawl.l3.push`, and `cli.braincrawl.l3.pull`.
Leaving a dangling edge endpoint corrupts the document.

Also drop the "Hidden dev round-trip surface" sentences from the two catalog node
bodies — the namespace is no longer hidden.

### 8. Regenerate the derived grammar canvas

```
cargo run -p braincrawl-cli --example luminous_cli_grammar
```

This rewrites `.luminous/cli-grammar.signal.json`, `.luminous/cli-grammar.graph.json`,
and `.luminous/cli-grammar.pack.json` from the clap definition. Commit the result.

## Files to Modify

- `apps/cli/src/cli.rs` — restructure `Namespace`; add `LibraryArgs`/`LibraryCmd`,
  `CatalogArgs`/`CatalogCmd`, `CollectionArgs`; delete `StoreArgs`/`StoreCmd`,
  `GraphArgs`/`GraphCmd`, `L3Args`
- `apps/cli/src/main.rs` — restructure the namespace dispatch; update imports and
  the two `Envelope` entity labels
- `README.md` — update command examples
- `.claude/skills/braincrawl/SKILL.md` — update command examples
- `.rhidoc/01-product/01-glossary.md` — update command examples
- `.luminous/braincrawl.atlas.json` — rename CLI nodes and their edge endpoints
- `.luminous/cli-grammar.{signal,graph,pack}.json` — regenerated, not hand-edited

## Verification

```bash
cargo build --bin braincrawl
cargo test
cargo run -p braincrawl-cli --example luminous_cli_grammar
cargo run --quiet --bin braincrawl -- --help
cargo run --quiet --bin braincrawl -- library --help
cargo run --quiet --bin braincrawl -- catalog --help
cargo run --quiet --bin braincrawl -- collection --help
cargo run --quiet --bin braincrawl -- library put --help
cargo run --quiet --bin braincrawl -- catalog neighborhood --help
```

Every `--help` invocation must exit 0. `braincrawl --help` must list `library`,
`catalog`, and `collection`, and must NOT list `store`, `graph`, `stats`, `push`,
`get`, `fetch-content`, `fetch-pdf`, `extract-text`, `chunk`, or `l3`.

## Out of Scope

- The `catalog put` verb and the `--emission` global flag (next phase).
- Folding `fetch-pdf` into `fetch`, and making `extract-text` store its output.
- The `library list` verb and the artifact-listing store surface.
- Renaming internal Rust modules, config keys, file extensions, or HTTP routes.
- The `braincrawl-cli.smithy` and `braincrawl-cli.tsp` specs — they already
  describe this target grammar and need no edit.

## Notes

- The specs `braincrawl-cli.smithy` and `braincrawl-cli.tsp` at the repo root
  already model the target surface, including verbs later phases add. Use them as
  the reference for namespace and verb naming, but implement only what this
  phase's plan lists.
- `cli.rs` currently has no unit tests and `main.rs` has helper-function tests
  only; `cargo test` passing plus the `--help` probes is the real gate here.
- Watch for the `l3::dispatch` signature — it takes `L3Cmd` directly, so the
  `Collection` arm must pass `c.cmd`, not the wrapper struct.

## Surface after this phase

- `braincrawl library` namespace with verbs `put`, `get`, `fetch`, `fetch-pdf`,
  `extract-text`, `chunk`. Each takes exactly the arguments its top-level
  predecessor took, with unchanged behavior and defaults. `extract-text` still
  writes nothing to the store — it prints extracted text to stdout. `fetch-pdf`
  still writes nothing to the store — it emits bytes to stdout or to `-o <path>`.
  `fetch` still always stores at role `fulltext` and has no `--stdout` or `-o`.
- `braincrawl catalog` namespace with verbs `get`, `have`, `neighborhood`,
  `stats`. Visible in `--help` (not hidden). No write verb yet.
- `braincrawl collection` namespace carrying every former `l3` verb unchanged:
  `new`, `path`, `list`, `check`, `index`, `import`, `rm`, `assign-ids`,
  `reading-list`, `push`, `pull`.
- In `apps/cli/src/cli.rs`: `pub enum Namespace` with variants `Library(LibraryArgs)`,
  `Catalog(CatalogArgs)`, `Collection(CollectionArgs)`, `Openalex`, `Semanticscholar`,
  `Crossref`, `Opencitations`, `Arxiv`, `Web`, `MigrateStore`, `Rename`.
  `pub struct LibraryArgs { pub cmd: LibraryCmd }`, `pub enum LibraryCmd` with
  variants `Put(PushArgs)`, `Get(GetArgs)`, `Fetch(FetchContentArgs)`,
  `FetchPdf(FetchPdfArgs)`, `ExtractText(ExtractTextArgs)`, `Chunk(ChunkArgs)`.
  `pub struct CatalogArgs { pub cmd: CatalogCmd }`, `pub enum CatalogCmd` with
  variants `Get { id }`, `Have { ids }`, `Neighborhood { seeds, dir, depth, max_nodes }`,
  `Stats`. `pub struct CollectionArgs { pub cmd: L3Cmd }`.
- `pub struct GlobalArgs` and `pub struct OutputOpts` are unchanged — same seven
  fields (`json`, `text`, `limit`, `all`, `fields`, `full`, `skip_push`,
  `include_abstract`). No `emission` field exists yet.
- `StoreArgs`, `StoreCmd`, `GraphArgs`, `GraphCmd`, and `L3Args` no longer exist.
- Negative space that later phases can rely on: `PushArgs`, `GetArgs`,
  `FetchContentArgs`, `FetchPdfArgs`, `ExtractTextArgs`, and `ChunkArgs` keep
  their current names and fields. The `l3` Rust module, `L3Cmd`, the `l3_repo`
  config key, the `.l3.md` extension, and the `/api/l3/*` worker routes are all
  unchanged. `braincrawl_cli::provider::Emission` and the `push_emission` helper
  in `main.rs` are unchanged. `StoreClient` gains no methods.
