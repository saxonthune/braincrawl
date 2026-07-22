# Make every Library derive verb store its output by default

## Motivation

The Library holds many artifacts per work, keyed by role and version — the schema
supports `abstract`, `fulltext`, and any `[a-z0-9_-]+` slug, with a monotonic
version per role. Three CLI verbs derive or acquire content, and they do not
agree on what to do with the result.

`library chunk` does the right thing: it writes its output at role `chunks` and
treats `--stdout` as the opt-out. `library extract-text` prints extracted text to
stdout and stores nothing, so the extracted text is a pipe rather than an
artifact and must be re-extracted on every use. `library fetch-pdf` fetches from
the open web straight to stdout or a file and never touches the store, making it
a second acquisition verb sitting beside `library fetch`, which differs only in
where the bytes land.

This phase applies one rule to all of them: a derive verb stores at a role by
default, and `--stdout` or `-o` is the escape. That folds `fetch-pdf` into
`fetch` and gives `extract-text` a default role it currently lacks.

## Do NOT

- Do NOT change `library put`, `library get`, `catalog *`, `collection *`, or any
  provider verb.
- Do NOT keep `library fetch-pdf` as a deprecated or hidden alias. It is removed;
  its behavior becomes `library fetch --stdout` or `library fetch -o <path>`.
- Do NOT add OCR, or relax `extract-text`'s refusal to process a PDF with no text
  layer. Refusing unfaithful extraction is deliberate.
- Do NOT change `library chunk` — it already follows the target rule. Read it as
  the pattern to copy, and leave it alone.
- Do NOT invent a new role name for extracted text. It is `text`.
- Do NOT make the store write conditional on anything other than the explicit
  `--stdout` / `-o` flags. Storing is the default in every other case.
- Do NOT change the `ArtifactRole` type, the artifacts schema, or any migration.
  Arbitrary role slugs already work.

## Plan

### 1. Give `library fetch` the two sink flags

In `apps/cli/src/cli.rs`, add to `FetchContentArgs`:

```rust
/// Emit bytes to stdout instead of storing
#[arg(long)]
pub stdout: bool,
/// Write bytes to this path instead of storing
#[arg(long, short = 'o')]
pub output: Option<String>,
```

`--force` and `--require-pdf` stay as they are. `--from` stays as it is.

### 2. Branch `library fetch` on the sink

In `apps/cli/src/main.rs`, the `Fetch` arm currently always calls
`fetch_content::fetch_content(...)`, which resolves and stores. Change it so:

- When neither `--stdout` nor `-o` is given, behavior is exactly what it is today
  — `fetch_content::fetch_content(...)` with its three outcomes (`Stored`,
  `AlreadyPresent`, `NoOaFound`) reported as they are now.
- When either flag is given, call `fetch_content::fetch_artifact_bytes(...)`
  instead — the same function the removed `fetch-pdf` arm used — and write the
  bytes to the path from `-o`, or to stdout. Report the existing stderr line
  (`fetched: N bytes, mime=…, url=…`). In this branch `--force` is irrelevant
  because nothing is being stored; do not error on it, simply do not consult it.

Both functions already exist in `apps/cli/src/fetch_content.rs`; this arm only
chooses between them.

### 3. Remove `library fetch-pdf`

Delete the `FetchPdf` variant from `LibraryCmd` in `cli.rs`, delete `FetchPdfArgs`,
and delete its dispatch arm from `main.rs`. Nothing else references them.

### 4. Make `library extract-text` store its output

In `apps/cli/src/cli.rs`, add to `ExtractTextArgs`:

```rust
/// Artifact role slug to store the extracted text at
#[arg(long, default_value = "text")]
pub role: String,
/// Re-extract even if the role already holds an artifact
#[arg(long)]
pub force: bool,
/// Emit text to stdout instead of storing
#[arg(long)]
pub stdout: bool,
```

In `apps/cli/src/main.rs`, the `ExtractText` arm reads the `fulltext` artifact,
rejects non-PDF input, extracts, and prints. Keep the read, the rejection, and
the extraction exactly as they are. Change what happens to the result:

- With `--stdout`, print the text and keep the existing stderr summary line. This
  is today's behavior, now opt-in.
- Without `--stdout`, store the text at `args.role` via
  `store.put_content(&args.id, &args.role, bytes, "text/plain", Some("extract-text"), None)`,
  and report a stored-summary line to stderr rather than printing the text to
  stdout.
- Before storing, when `--force` is not set, check for an existing artifact at
  that role with `store.get_content(&args.id, &args.role)` and skip with an
  `already-present` message if one is there. `run_chunk` in `main.rs` does exactly
  this check — copy its shape and its message wording.

The three existing error paths (fulltext absent, still pending, not a PDF) are
unchanged, except that the "run fetch-content first" hint in the absent-artifact
message must now read `library fetch`.

### 5. Update the prose

`README.md` and `.claude/skills/braincrawl/SKILL.md` both show these verbs.
Update the examples: `fetch-pdf` invocations become `library fetch --stdout` or
`library fetch -o <path>`, and any example that pipes `extract-text` output must
either add `--stdout` or be rewritten to store and then read back with
`library get --role text`. State current truth; do not narrate the change.

Also update `.luminous/braincrawl.atlas.json`: remove the `fetch-pdf` node and
its edges, and revise the `fetch` and `extract-text` node bodies to describe the
new sink flags and the stored roles.

### 6. Regenerate the derived grammar canvas

```
cargo run -p braincrawl-cli --example luminous_cli_grammar
```

Commit the regenerated `.luminous/cli-grammar.*` files.

## Files to Modify

- `apps/cli/src/cli.rs` — `stdout`/`output` on `FetchContentArgs`;
  `role`/`force`/`stdout` on `ExtractTextArgs`; delete `FetchPdfArgs` and the
  `LibraryCmd::FetchPdf` variant
- `apps/cli/src/main.rs` — sink branch in the `Fetch` arm; store-by-default in the
  `ExtractText` arm; delete the `FetchPdf` arm
- `README.md` — update examples
- `.claude/skills/braincrawl/SKILL.md` — update examples
- `.luminous/braincrawl.atlas.json` — drop the `fetch-pdf` node and its edges;
  revise `fetch` and `extract-text` bodies
- `.luminous/cli-grammar.{signal,graph,pack}.json` — regenerated, not hand-edited

## Verification

```bash
cargo build --bin braincrawl
cargo test
cargo run -p braincrawl-cli --example luminous_cli_grammar
cargo run --quiet --bin braincrawl -- library --help
cargo run --quiet --bin braincrawl -- library fetch --help
cargo run --quiet --bin braincrawl -- library extract-text --help
```

`library --help` must NOT list `fetch-pdf`. `library fetch --help` must show
`--stdout` and `-o/--output`. `library extract-text --help` must show `--role`
with default `text`, `--force`, and `--stdout`. Every invocation must exit 0.

## Out of Scope

- The `library list` verb and the artifact-listing store surface — the next
  phases. Note that until those land, an artifact stored at role `text` is only
  readable by someone who already knows the role name.
- Reading a superseded version of a role.
- Any change to `library chunk`.
- Deriving anything new (summaries, translations, embeddings).

## Notes

- This phase changes observable behavior: a bare `library extract-text <id>` no
  longer prints text to stdout. Any caller relying on the old default must add
  `--stdout`. That is the intended break.
- `store.put_content` mints a new version rather than overwriting, so re-running
  with `--force` accumulates versions rather than replacing. That is the schema's
  design and is correct here.
- Reviewer watch item: the `--stdout` and `-o` flags on `fetch` are two spellings
  of one intent. If both are passed, writing to the path and ignoring `--stdout`
  is acceptable; do not error.

## Surface after this phase

- `braincrawl library fetch <id>` — stores the acquired fulltext at role
  `fulltext` by default; `--stdout` emits bytes to stdout and `-o <path>` writes
  them to a file, both without any store write. Flags: `--from`, `--force`,
  `--require-pdf`, `--stdout`, `-o/--output`.
- `braincrawl library fetch-pdf` no longer exists. `FetchPdfArgs` and
  `LibraryCmd::FetchPdf` are deleted.
- `braincrawl library extract-text <id>` — extracts text from the stored
  `fulltext` PDF and stores it at role `text` by default. Flags: `--role`
  (default `text`), `--force`, `--stdout`. Without `--force` it skips when the
  role already holds an artifact. Still refuses PDFs with no text layer; no OCR.
- `FetchContentArgs` has fields `id`, `from`, `force`, `require_pdf`, `stdout`,
  `output`. `ExtractTextArgs` has fields `id`, `role`, `force`, `stdout`.
- A work may now hold artifacts at roles `fulltext`, `text`, and `chunks` through
  ordinary CLI use.
- Negative space: `library put`, `library get`, and `library chunk` are unchanged
  in arguments and behavior — `library get --role text` reads back what
  `extract-text` stored. `catalog put`, the `--emission` flag, and `OutputOpts`
  are unchanged from the previous phase. `ArtifactRole`, the artifacts schema, and
  all migrations are untouched. `StoreClient` has gained no methods; there is
  still no way to enumerate a work's artifacts.
