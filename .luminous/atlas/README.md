# atlas/ — filling the braincrawl Atlas from the code

The Atlas of braincrawl is a hand-authored map: which components exist, how they
nest, what points at what, where each sits. Its *structure* is a human's theory of
the system and no script may touch it. Its *content* — the contract text a Node
shows — is another matter: a trait signature, a SQL schema, a CLI command's args
are all mechanically derivable from the source, and a copy of them drifts the
moment the code changes.

An **Atlas Data File** (`<name>.atlasdata.json`) is the seam. It maps a key to
text. An Atlas Node's Content may name a key; when the Data File provides it, the
Node draws the generated text, and otherwise it falls back to whatever a human
wrote there. Scripts in this directory write that file. They never add, remove, or
move a Node.

## gen-atlasdata.py

Projects the CLI grammar into one entry per command.

```bash
just luminous-atlas          # refresh the grammar signal, then rebuild the Data File
```

The input is `.luminous/cli-grammar.signal.json`, not `cli.rs` — so this script
parses no Rust. The signal is clap's own view of the real `Cli` type, dumped by
`apps/cli/examples/luminous_cli_grammar.rs`, which means the grammar projected
here cannot disagree with the grammar the binary actually accepts. The `just`
recipe regenerates the signal first for that reason; running the Python directly
against a stale signal is the one way to get a wrong answer.

Each entry holds a usage line, the command's `about` text, and a fenced block of
positionals and options with their help, defaults, and possible values. The block
is shaped like YAML for reading, not for parsing — clap help text is full of
colons and dashes.

### Keys

A key is the command path with dots: `braincrawl openalex get` becomes
`cli.braincrawl.openalex.get`. That is deliberately the same form the Atlas's CLI
Node ids already use, so binding a Node to its key means copying the id into the
Node's Content `from` field. Nothing enforces the match — a key is just a string —
but keeping the convention makes a drifting CLI obvious: rename a command and the
old key disappears, which surfaces in the canvas as a Node drawing its authored
fallback, and in `checkAtlasDocument` as a dangling key.

### Where the output goes

The Data File must sit beside the Atlas document it fills, because Luminous
derives the sidecar path from the document's own path. Both live in `.luminous/`
here: `braincrawl.atlas.json` (authored) and `braincrawl.atlasdata.json`
(generated). Serve them by adding this directory to a Luminous checkout's
`luminous.config.json` roots.

The Luminous repository keeps its own copy of the pair as a development fixture
and shipped example. That copy is a snapshot, refreshed by hand — its tests check
that it is internally valid, not that it still matches this repository.

To write straight into another checkout instead, point the script at it:

```bash
python3 .luminous/atlas/gen-atlasdata.py --out ../Luminous/.luminous/braincrawl.atlasdata.json
```

Prefer a real file over a symlink wherever the canvas should redraw on
regeneration: the file watcher reacts to directory entries, and rewriting a
symlink's target changes no entry in the directory being watched.

## Adding another script

The pattern generalizes to anything a repository can extract: rustdoc JSON for
trait signatures, the migrations directory for the SQL schema, the server's route
table. Write a script per source, have each emit its own keys into the same Data
File or a merge step, and keep the same rule — content only, never structure.
