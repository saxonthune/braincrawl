# CLI `rename` subcommand — deterministic bibliographic filenames

## Motivation

An agent that obtains a research file should read author/year/title off the title
page and rename it to a canonical bibliographic filename — deterministically,
without reading a format spec and without building the filename string itself. The
format lives in one tested function; the agent supplies facts as flags, the tool
renders and renames. Works for PDFs and any other extension.

Canonical form: `{Surname}[EtAl]{Year}-{title-slug}.<ext>`
- solo author → `Piwowar2018-state-of-oa.pdf`
- multiple authors → `PiwowarEtAl2018-state-of-oa.pdf`

The filename is a bibliographic label only. It is deliberately decoupled from the L1
store id (UUID) and from any provider id (OpenAlex, etc.).

## Do NOT

- Do NOT extract author/year/title FROM the file. The tool takes them as explicit
  flags. Reading the title page is the agent's job, upstream of this command.
- Do NOT couple the filename to the store id, a UUID, an OpenAlex id, or any
  provider id. Nothing from the store or a provider appears in the name.
- Do NOT let the agent assemble the name — every mechanical choice (CamelCasing,
  EtAl word, slug, collision suffix) belongs in the tool.
- Do NOT reuse `l3.rs::stem_slug` for the title slug — it does not drop stopwords,
  cap word count, or fold diacritics. Write a purpose-built slug function.
- Do NOT touch the OpenAlex-as-canonical-id drift here — that is a separate task.
- Do NOT hardcode `.pdf`; preserve the input file's existing extension.

## Plan

### 1. Add the `deunicode` dependency

In `apps/cli/Cargo.toml` under `[dependencies]`, add `deunicode = "1"` (pure-Rust
Unicode→ASCII transliteration; used to fold diacritics in surname and title, e.g.
`Şenyurt` → `Senyurt`, `Ünal` → `Unal`).

### 2. Define `RenameArgs` and the `Namespace::Rename` variant

In `apps/cli/src/cli.rs`:
- Add to `Namespace`:
  ```rust
  #[command(about = "Rename a work file to its canonical bibliographic filename")]
  Rename(RenameArgs),
  ```
- Define:
  ```rust
  #[derive(Args)]
  pub struct RenameArgs {
      /// Path to the file to rename (extension is preserved)
      pub file: String,
      /// First author's surname (may contain spaces/particles, e.g. "Van De Mieroop")
      #[arg(long)]
      pub author: String,
      /// The work has more than one author (renders the EtAl marker)
      #[arg(long = "et-al")]
      pub et_al: bool,
      /// Publication year
      #[arg(long)]
      pub year: u32,
      /// Work title (slugified for the filename)
      #[arg(long)]
      pub title: String,
      /// Print the proposed new path; do not rename
      #[arg(long = "dry-run")]
      pub dry_run: bool,
  }
  ```

### 3. New module `apps/cli/src/rename.rs`

Add `pub mod rename;` to `apps/cli/src/lib.rs` (keep the list alphabetical:
after `provider`, before `refs_backfill`).

Implement `pub fn run(args: RenameArgs, opts: &OutputOpts) -> Result<(), Box<dyn std::error::Error>>`
composed of small, individually-tested pure functions:

- `fn surname_key(author: &str) -> String` — transliterate to ASCII (`deunicode`),
  split on whitespace and hyphens, CamelCase each part (first char upper, rest
  lower), concatenate. `"Van De Mieroop"` → `"VanDeMieroop"`; `"piwowar"` →
  `"Piwowar"`. Strip any remaining non-alphanumeric.

- `fn title_slug(title: &str) -> String` — transliterate to ASCII, lowercase, split
  into word tokens on non-alphanumeric boundaries, drop stopwords (see set below),
  take the first 5 surviving tokens, join with `-`. Empty result (all stopwords)
  falls back to the first non-stopword-less token, or `untitled` if truly empty.
  Stopword set (lowercase): `a an the of on in at to for and or nor but is are was
  were be been being with from by as into`.

- `fn stem(surname_key: &str, et_al: bool, year: u32, title_slug: &str) -> String`
  — assemble `{surname_key}{"EtAl" if et_al}{year}-{title_slug}`.

- `fn resolve_collision(dir: &Path, stem: &str, ext: &str, current: &Path) -> String`
  — the final filename. If `{stem}.{ext}` does not exist in `dir` (or already IS the
  file being renamed), use it. Otherwise append the lowest free letter: `{stem}b`,
  `{stem}c`, … (note: bare stem is the implicit `a`; first collision gets `b`).
  Exclude `current` from the collision check so re-running on an already-named file
  is idempotent.

`run` then:
1. Resolve `args.file` to an absolute path; error if it does not exist.
2. Extract extension (lowercased) from the original filename; empty if none.
3. Compute `stem` → `resolve_collision` → new filename → new path in the same dir.
4. If `dry_run`, print the new path (respect `opts` text/json like other commands —
   a minimal `Envelope` with the single path is fine; match the style used by
   `l3` "path" output) and return without renaming.
5. Otherwise `std::fs::rename(old, new)` and print the new absolute path.

### 4. Dispatch in `main.rs`

In `apps/cli/src/main.rs`, add the match arm:
```rust
Namespace::Rename(args) => {
    braincrawl_cli::rename::run(args, &opts)?;
}
```
Add `use braincrawl_cli::cli::RenameArgs;` to the existing `cli::{…}` import if the
match needs it (it does not if you fully-qualify the variant).

### 5. Unit tests in `rename.rs`

Cover the pure functions (no filesystem needed except the collision test, which can
use `tempfile`, already a dev-dependency):
- `surname_key`: `"piwowar"`→`Piwowar`; `"Van De Mieroop"`→`VanDeMieroop`;
  `"Şenyurt"`→`Senyurt`; hyphenated `"García-López"`→`GarciaLopez`.
- `title_slug`: `"The state of OA: a large-scale analysis"` →
  `state-oa-large-scale-analysis` (stopwords `the/of/a` dropped, `:` handled, capped
  at 5 tokens); diacritic title folds to ASCII.
- `stem`: solo → `Piwowar2018-...`; et_al → `PiwowarEtAl2018-...`.
- `resolve_collision`: in a temp dir, first call yields bare stem; with the bare stem
  file present, next yields `{stem}b`; passing `current` == the bare-stem path yields
  the bare stem (idempotent).

## Files to Modify

- `apps/cli/Cargo.toml` — add `deunicode = "1"`.
- `apps/cli/src/cli.rs` — `Namespace::Rename` variant + `RenameArgs`.
- `apps/cli/src/lib.rs` — `pub mod rename;`.
- `apps/cli/src/rename.rs` — new module: logic + unit tests.
- `apps/cli/src/main.rs` — dispatch arm.

## Verification

```bash
cargo build -p braincrawl-cli
cargo test -p braincrawl-cli rename
cargo run -p braincrawl-cli -- rename /tmp/x.pdf --author "Piwowar" --year 2018 --title "The state of OA: a large-scale analysis" --dry-run
cargo run -p braincrawl-cli -- rename /tmp/x.pdf --author "Van De Mieroop" --et-al --year 2004 --title "A History of the Ancient Near East" --dry-run
```

First dry-run must print a path ending `Piwowar2018-state-oa-large-scale-analysis.pdf`;
second must print one ending `VanDeMieroopEtAl2004-history-ancient-near-east.pdf`.

## Out of Scope

- Reading metadata from the file; provider resolution; store-id coupling.
- Undated works (no `--year`).
- Fixing the OpenAlex-as-canonical-id drift (separate task).

## Notes

- The whole point is that determinism lives in the tool, judgment in the agent. Keep
  the format functions pure and total so they are trivially testable.
- `deunicode` transliterates all Unicode, so no per-language diacritic table is
  needed and non-Latin scripts degrade to a best-effort ASCII romanization rather
  than being dropped.
