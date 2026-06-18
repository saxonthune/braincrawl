# .luminous/ — derived structure canvases

Generated [Luminous](https://luminous) canvas models of braincrawl's structure.
Everything here is **derived from the code**, never hand-maintained — re-run the
pipeline after a change and diff the result. The point is to *see the outlines*
the code defines, at altitudes the source itself doesn't show at a glance.

## cli-grammar

A model of the **CLI grammar**: every command, subcommand, and flag/positional,
exactly as clap defines them in `apps/cli/src/cli.rs`.

| File | What it is |
|------|------------|
| `cli-grammar.signal.json` | The **source signal** — clap's command tree dumped to JSON. A faithful, readable enumeration of the whole grammar. Useful on its own as a command-tree outline. |
| `cli-grammar.graph.json` | Luminous v3 graph: a `cli.command` node per command, a `cli.flag` node per arg, `cli.contains` edges nesting flags and subcommands under their parent. |
| `cli-grammar.pack.json` | The vocabulary the graph renders against (node/edge kinds + the `cli-grammar` view). |

### Regenerate

```bash
just luminous-cli
# or:
cargo run -p braincrawl-cli --example luminous_cli_grammar
```

The pipeline lives at `apps/cli/examples/luminous_cli_grammar.rs`. It compiles
against the real `Cli` type via clap's `CommandFactory`, so the signal can never
drift from the code — if a verb is added to `cli.rs`, it appears here on the next
run. IDs are derived from the command path (`command.braincrawl.openalex.get`,
`flag.braincrawl.openalex.get.id`), so re-running produces a diffable update, not
duplicates.

### Finding similarities across the tree

Args are modeled as `cli.flag` **subnodes nested inside their command** (via the
`cli.contains` containment edge). Because each command owns its own flag nodes,
the same arg appearing in different branches would otherwise look unrelated. To
make those repeats visible, nodes carry `tags`:

- every `cli.flag` is tagged with its bare arg name and `kind:<positional|flag|option>`,
- every `cli.command` is tagged with its terminal name.

So filtering/highlighting by tag in Luminous surfaces the structural echoes: the
`id` positional recurs across 14 commands, `entity` across 4, `--from` across 2;
the verb `get` appears under three providers, `refs` across four. That repetition
is the similarity signal — a candidate for shared grammar you might want to
factor or keep deliberately uniform.

### What the signal does and doesn't capture

Captured (all from clap): the full command tree, every flag/option/positional
with its `--long`/`-short`, help text, `required`, `global`, defaults, and
possible-values; whether a command is `hidden` (the dev `store` namespace shows
up with `hidden: true`).

Not captured here — these are *intent*, not grammar, and would come from a second
source (`.rhidoc/` docs or a `cli.rs`→handler join): which handler implements a
command, web-vs-store semantics, push-by-default behavior. Arg-group mutual
exclusion (e.g. `--json` vs `--text`) is also not yet modeled; it's a natural next
enrichment as a `cli.excludes` edge.

## Roadmap

- **rust-internals** — a sibling canvas from rustdoc JSON: traits, impls, and
  signatures (the inner boundaries). Not built yet.
