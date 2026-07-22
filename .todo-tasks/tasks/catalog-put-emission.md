# `catalog put` and `--emission` — decompose a provider read into lookup plus store write

## Motivation

A provider command fetches from upstream and writes the result to the Catalog as
a fused side effect. `--skip-push` suppresses the write, giving a lookup atom,
but there is no counterpart verb that ingests a held result into the Catalog. So
`lookup | ingest` pipelines are impossible and the default push behavior cannot
be reproduced from parts. An agent already holding provider results — from an
earlier `--skip-push` run, a file, or another tool — has no way to land them.

Research surfaced that the lookup half does not exist either. A provider verb
produces two independent products from the same upstream response: `Envelope`
(the trimmed display shape, from each provider's `shape::build_envelope`) and
`Emission` (the store-ready shape, from `mapping::to_emission`). Only the
`Envelope` is ever printed — `output.rs::render` renders it and nothing else, so
the `Emission` is discarded. `--skip-push --json` therefore yields a display
shape a store-write verb cannot consume.

Re-lowering the envelope is not an option: lowering is provider-specific and the
envelope has already dropped fields the mapping needs. So this phase adds both
halves — a flag that emits the `Emission`, and the verb that consumes it.

## Do NOT

- Do NOT make `catalog put` accept envelope-shaped provider output, and do NOT
  write any re-lowering logic. Its only input is a serialized `Emission`.
- Do NOT change the push-on-by-default behavior of the provider commands.
- Do NOT make `--emission` imply `--skip-push`, or vice versa. They are separate
  concerns and compose; the user passes both when they want a pure lookup.
- Do NOT apply `--fields` projection or `--text` rendering to the `Emission`. It
  is a wire frame, not a display shape.
- Do NOT change the `Emission`, `WorkRecord`, `EdgeInput`, or `Alias` struct
  definitions in `apps/cli/src/provider/mod.rs`. Their serialized shape is the
  wire contract and is already pinned by tests in that file.
- Do NOT duplicate the push logic. `catalog put` must call the existing
  `push_emission` helper.
- Do NOT touch the subprocess plugin protocol, or add a provider-plugin
  mechanism of any kind.

## Plan

### 1. Add the `--emission` global flag

In `apps/cli/src/cli.rs`, add a field to `GlobalArgs` beside the existing global
flags:

```rust
/// Emit the store-ready emission frame instead of the display envelope
#[arg(long, global = true, conflicts_with = "text")]
pub emission: bool,
```

Using clap's `conflicts_with` rather than a hand-rolled runtime check makes the
`--emission --text` combination unrepresentable and produces a clap-standard
error message.

Add a matching `pub emission: bool` field to `OutputOpts` and carry it across in
the `From<GlobalArgs> for OutputOpts` impl.

### 2. Emit the `Emission` from `run_provider`

In `apps/cli/src/main.rs`, `run_provider` currently calls `render(&envelope, opts)`
unconditionally after dispatch. Change it so that when `opts.emission` is set it
prints `serde_json::to_string_pretty(&emission)` to stdout instead of rendering
the envelope. Exactly one of the two is printed, never both — a pipeline reading
stdout must not have to skip past a display envelope.

The push behavior below that line is unchanged: `--skip-push` alone governs
whether `push_emission` runs.

The `crossref` and `opencitations` paths do not go through `run_provider` — they
run a hand-rolled refs path in `main.rs` that pushes edges directly. Leave them
alone; `--emission` is a no-op for those two verbs in this phase, and the Notes
section records that gap.

### 3. Add `catalog put`

In `apps/cli/src/cli.rs`, add a variant to `CatalogCmd`:

```rust
/// Write works and edges from an emission frame (stdin or a file) to the catalog
Put {
    /// Read the emission from this file instead of stdin
    file: Option<String>,
},
```

In `apps/cli/src/main.rs`, add the dispatch arm inside the `Namespace::Catalog`
match. It must:

1. Read the input — from `file` when given, else from stdin to end. Mirror the
   read shape `library put` already uses (the former top-level `push` arm).
2. Return a clear error on empty input, matching the wording style of
   `library put`'s "no input bytes (provide a file arg or pipe bytes on stdin)".
3. Parse the bytes with `serde_json::from_slice::<Emission>`. On a parse failure,
   return an error naming the file or stdin as the source so the user knows which
   side of the pipe is wrong.
4. Call the existing `push_emission(&client, &emission)` helper.
5. Report through the existing `report_push_summary`, and `std::process::exit(1)`
   when `summary.errors` is non-empty — the same contract `run_provider` follows,
   so a failed write in a pipeline is detectable by exit code.

### 4. Test the round trip

Add unit tests. `apps/cli/src/provider/mod.rs` already has serde shape tests
(`emission_serde_round_trip`, `work_record_serialized_shape`,
`edge_input_serialized_shape`) — follow their style and assertion idiom.

Cover at least: an `Emission` serialized with `to_string_pretty` deserializes
back to an equal-content `Emission` (this is the exact path `--emission` writes
and `catalog put` reads), and that an empty-`records`, empty-`edges` emission
round-trips without error.

Do not write tests that require a live store — `push_emission` needs a running
server and is out of reach of `cargo test`.

### 5. Document the identity

Add a line to `.claude/skills/braincrawl/SKILL.md` and `README.md` recording the
decomposition, in the form that makes the equality explicit:

```
braincrawl openalex search works "…" --skip-push --emission | braincrawl catalog put
```

is equivalent to plain `braincrawl openalex search works "…"`.

## Files to Modify

- `apps/cli/src/cli.rs` — `--emission` on `GlobalArgs`, `emission` on `OutputOpts`
  and its `From` impl, `CatalogCmd::Put`
- `apps/cli/src/main.rs` — emission branch in `run_provider`, the `catalog put`
  dispatch arm
- `apps/cli/src/provider/mod.rs` — added round-trip tests only; no type changes
- `README.md` — document the flag, the verb, and the identity
- `.claude/skills/braincrawl/SKILL.md` — same

## Verification

```bash
cargo build --bin braincrawl
cargo test
cargo run --quiet --bin braincrawl -- catalog --help
cargo run --quiet --bin braincrawl -- catalog put --help
cargo run --quiet --bin braincrawl -- openalex --help
```

`catalog --help` must list `put`. Every invocation must exit 0.

## Out of Scope

- Wiring `--emission` through the `crossref` and `opencitations` refs paths.
- Any change to `library` verbs.
- The `library list` verb and the artifact-listing store surface.
- An `--emission` equivalent for reads that come from the store rather than a
  provider.

## Notes

- `push_emission` reports per-record errors rather than failing fast, so a
  partially-successful `catalog put` exits 1 with the successful writes already
  landed. That matches the fused path's behavior exactly and is intentional.
- The `Emission` type is already `Serialize`/`Deserialize` and was designed as
  the future plugin-protocol frame. This phase makes it a public wire format for
  the first time, which is why the existing shape tests matter.
- Reviewer watch item: confirm nothing prints the envelope *and* the emission on
  a single run. A pipeline reading stdout breaks on any leading display output.

## Surface after this phase

- Global flag `--emission` on every command, defined on `GlobalArgs` in
  `apps/cli/src/cli.rs`, conflicting with `--text` at the clap level. Independent
  of `--skip-push`.
- `pub struct OutputOpts` gains `pub emission: bool`, carried across by the
  `From<GlobalArgs>` impl. Its other fields are unchanged.
- `braincrawl catalog put [FILE]` — reads a serialized `Emission` from stdin or
  `FILE`, writes its works and edges to the Catalog, reports counts to stderr in
  the existing `report_push_summary` format, exits 1 if any write errored.
- `CatalogCmd::Put { file: Option<String> }` exists in `apps/cli/src/cli.rs`.
- Provider verbs reached through `run_provider` (`openalex`, `semanticscholar`,
  `arxiv`) print the `Emission` as pretty JSON on stdout instead of the display
  envelope when `--emission` is set, and print exactly one of the two.
- Negative space: `crossref` and `opencitations` still ignore `--emission` and
  still push edges directly from their hand-rolled path in `main.rs`.
  `Emission`, `WorkRecord`, `EdgeInput`, and `Alias` in
  `apps/cli/src/provider/mod.rs` are unchanged in shape. `push_emission` and
  `report_push_summary` in `main.rs` keep their current signatures. Every
  `library` verb still behaves exactly as it did after the previous phase —
  `extract-text` and `fetch-pdf` still write nothing to the store.
