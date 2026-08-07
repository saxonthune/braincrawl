# Hand-entry catalog verb, so a book can be named by ISBN

## Motivation

Naming a book by its ISBN fails: `error: cannot infer entity from ID: isbn:9780691215105`.
Books are load-bearing works in several research domains, so this is the ordinary path for a
whole class of source, not an edge case. Because a book cannot be named, sessions park them in
a prose "Not-yet-catalogued keystones" node, which now appears near-verbatim in at least six
Research Documents — a data gap that has become a recurring format violation.

Triage established that this is **not** an alias-namespace problem and **not** a docs problem.
The store already accepts any namespace: `Alias` is `{namespace, value}` with a free-string
namespace (`crates/core/src/types.rs:10`) and `parse_alias` splits on the first colon
(`apps/server/src/lib.rs:194`). There is no allowlist to widen. The docs already say the right
thing: `.rhidoc/02-architecture/01-core/02-id-resolution.md:14-20` states the canonical id is a
UUID and names ISBN explicitly, and `.claude/skills/braincrawl/SKILL.md:43` agrees.

The real gap is a missing verb. `put_content` resolves the alias before storing
(`crates/core/src/usecases.rs:194`), so `library put isbn:… book.pdf` fails until some node
carries that alias — and the only way to create one by hand is `catalog put`, which wants a
hand-authored `Emission` frame on stdin (`apps/cli/src/provider/mod.rs:66-71`). The
`cannot infer entity from ID` error comes only from provider dispatch
(`apps/cli/src/openalex/mod.rs:24`), where refusing an ISBN is correct — the user simply had
nowhere else to go.

## Do NOT

- Do **not** add an `isbn` case to any provider's id inference
  (`apps/cli/src/openalex/mod.rs`, `apps/cli/src/semanticscholar/mod.rs`). Those errors are
  correct: OpenAlex genuinely cannot resolve an ISBN. Leave them alone.
- Do **not** add a namespace allowlist, validation, or `enum` of known namespaces anywhere.
  The free-string namespace is deliberate. Nothing needs to learn the word "isbn".
- Do **not** add an ISBN metadata provider, or fetch anything from the network.
- Do **not** rewrite `.rhidoc/02-architecture/01-core/02-id-resolution.md` to explain that the
  UUID is canonical — it already says so, correctly, and re-stating it is not the fix.
- Do **not** touch the existing `catalog put` emission-frame path. The new verb is an
  additional, friendlier entry point, not a replacement.
- Do **not** retrofit the existing "Not-yet-catalogued keystones" nodes in the L3 store.
- Do **not** add book citation edges, or attempt anything about books being dead ends in the
  citation graph.

## Plan

### 1. Add a `catalog add` subcommand

In `apps/cli/src/cli.rs`, add a variant to `CatalogCmd` (the enum at line 322):

```rust
/// Mint a work in the catalog by hand, for a source no provider has a record for
Add {
    /// Aliases in ns:value form; at least one required (e.g. isbn:9780691215105)
    #[arg(long = "alias", required = true)]
    aliases: Vec<String>,
    /// Work title
    #[arg(long)]
    title: String,
    /// Author, repeatable in citation order
    #[arg(long = "author")]
    authors: Vec<String>,
    /// Publication year
    #[arg(long)]
    year: Option<u32>,
    /// Node kind (default: Work)
    #[arg(long, default_value = "work")]
    kind: String,
},
```

### 2. Dispatch it in `apps/cli/src/main.rs`

Beside the existing `CatalogCmd::Get` arm (line 60) and the `CatalogCmd::Put` arm, add an
`Add` arm that builds a single-record `Emission` and pushes it with the existing
`push_emission` helper (`apps/cli/src/main.rs:674`) — reuse that path rather than calling the
store client directly, so hand entry and provider entry converge on one write.

Build the record as `braincrawl_cli::provider::WorkRecord` (`apps/cli/src/provider/mod.rs:40`,
note `kind` is a `String` here, unlike the core type):

- `source: "manual"` — a new source label, so provenance distinguishes hand entry from a
  provider pull.
- `kind` — the `--kind` value, lowercased. Reject anything other than the kinds
  `crates/core/src/types.rs:16` names (`work`, `author`, `venue`, `concept`, `topic`) with a
  plain error naming the accepted values.
- `aliases` — each `--alias` split on the first colon, exactly as `parse_alias` does. An
  argument with no colon is an error naming the `ns:value` form.
- `attrs` — a JSON object carrying `title`, `authors` (array, omitted when empty), and
  `publication_year` (omitted when absent). Use `title` and `publication_year` because
  `render_text_line` (`apps/cli/src/output.rs:60-65`) already looks for `title`, and OpenAlex
  records already use `publication_year`, so a hand-entered work projects the same way a
  fetched one does.
- `edges: vec![]`, `skipped_unmappable: 0`.

Print the resulting canonical id to stdout so the user can immediately use it.

### 3. Stop `collection check` from demanding an OpenAlex or DOI reference

`apps/cli/src/l3.rs:922` warns `no canonical ids (openalex:/doi:) referenced` when a document
body contains neither substring. A document whose works are all books, referenced by `isbn:`,
gets a false warning — this is the one place in the code that actually encodes the wrong
assumption the draft describes.

Replace the substring test with one that asks whether the body contains **any** `ns:value`
catalog reference inside a `[[…]]` link, whatever the namespace. Reuse the existing link
scanner in `l3.rs` if one is already extracting `[[…]]` targets for the Work index
(`l3.rs:807`) rather than writing a second parser. Reword the warning to say no catalog
reference of any namespace was found.

### 4. Document the hand path

Add a short paragraph to `.rhidoc/02-architecture/01-core/02-id-resolution.md`, under the
existing "Fuzzy fallback" section, stating that a work no provider describes is entered by
hand with `catalog add`, which mints the node and its aliases directly — and that this is the
supported path for books, which typically carry an ISBN and no DOI or OpenAlex id.

Add one line to `.claude/skills/braincrawl/SKILL.md` §5, where catalog references are already
described (around line 289, which already lists `isbn:`), pointing at `catalog add` as the way
to make such a reference resolvable.

## Files to Modify

- `apps/cli/src/cli.rs` — the `CatalogCmd::Add` variant
- `apps/cli/src/main.rs` — the `CatalogCmd::Add` dispatch arm
- `apps/cli/src/l3.rs` — namespace-agnostic catalog-reference check at line 922
- `apps/cli/tests/` — a new test file, or an existing catalog test file if one fits: cover
  alias parsing (`isbn:978…` splits correctly; a colon-less argument errors), attrs shape
  (`title`/`authors`/`publication_year`, absent keys omitted), and kind rejection
- `.rhidoc/02-architecture/01-core/02-id-resolution.md` — the hand-entry paragraph
- `.claude/skills/braincrawl/SKILL.md` — one line pointing at `catalog add`

## Verification

```bash
cargo test
cargo build --bin braincrawl
```

Then, against a running server, the end-to-end path the draft asked for:

```bash
braincrawl catalog add --alias isbn:9780691215105 --title "A Test Book" --author "Surname, A." --year 2020
braincrawl catalog get isbn:9780691215105
```

## Out of Scope

- Book citation edges and the "a book is a dead end in the graph" problem — see
  `feedback-book-citation-graphs.md`, for which this task is a prerequisite.
- Any ISBN metadata provider.
- Retrofitting existing "Not-yet-catalogued keystones" nodes in the L3 store.
- `catalog get --text` printing a row of empty fields. Diagnosed during triage as a separate
  defect: `WorkView` serializes to `{canonical_id, kind, attrs, provenance, aliases}`
  (`crates/core/src/types.rs:88-95`), with no top-level `id` or `title`, so
  `render_text_line` (`apps/cli/src/output.rs:57-66`) finds neither and prints `-\t`. Real,
  but a different fix; do not chase it here.

## Notes

- `l3-catalog-reference-aliases` (archived 2026-07-25) did earlier work on alias namespaces
  and an honest `Alias` type — read what it covers before touching alias handling.
- ISBN normalization (hyphens, ISBN-10 versus ISBN-13) is deliberately not specified. The
  namespace is a free string and the store's `UNIQUE (namespace, value)` treats
  `isbn:978-0-691-21510-5` and `isbn:9780691215105` as different works. Worth a follow-up
  ticket; do not silently normalize here, since that would be a store-visible policy decision.
- The verb name `catalog add` is provisional — it was proposed during triage, not committed to
  the glossary. It sits beside the existing `catalog put`, which keeps its meaning of "write
  an emission frame".
