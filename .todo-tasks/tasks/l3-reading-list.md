# Reading list: convention, plugin, and CLI verb

## Motivation

Finding works marked as worth reading should take zero recall — "what did I mark, anywhere"
as one page and one command. Read-later intent lives in L3 (not the Catalog), as a typed
`reading` property the tooling recognizes, plus a `catalog` link to the work it concerns.
This phase adds the convention, a default reading-list plugin in the Web UI shell, and a
`braincrawl l3 reading-list` CLI verb.

## Do NOT

- Do NOT store read-later state in the Catalog (L2) — no L2 schema change of any kind.
- Do NOT add an `order`/sequence field. Ordering is by role class only; global integers are
  brittle (a node can be extracted at any time).
- Do NOT make the reading-list a query registry or a new endpoint. The plugin reads the same
  shared graph as every other plugin (`useGraph()`); the CLI verb reads `l3::parse()` directly.
- Do NOT invent a closed role vocabulary — `start-here|core|rigor|reference` is a blessed
  default set, but an unknown role must still render (as its own group), not error.
- Do NOT change the plugin interface or the shell (they are the previous phase's Surface).

## Plan

### 1. The `reading` convention

A node marks a recommended reading with a `reading` property (a YAML flow map) plus a `catalog`
link to the work:

```
## Strogatz, Nonlinear Dynamics and Chaos — the standard entry text ^r-xxxxxxx
- reading: {role: start-here, why: the friendliest on-ramp to limit cycles}
- catalog [[openalex:W2001886606]]
```

Document this in the L3 conventions spec (`.rhidoc/02-architecture/01-core/04-l3-conventions.md`)
as a settled convention: `reading` is a property whose value is a flow map with `role`
(one of the blessed set, or any string) and `why` (free text); the node carries a `catalog`
link to the work the reading refers to.

### 2. Reading-list plugin (`web/src/plugins/reading-list.tsx`)

- Ships as a default plugin: add it to `web/src/plugins/index.ts`.
- From the shared graph: select nodes whose `properties.reading` is present. For each, read
  `role`/`why` from the flow map, resolve the node's `catalog` link (a link with `type: "catalog"`
  whose `target` is a catalog id like `openalex:W…`), and read the node's `doc` for the source column.
- Render a grouped, sortable table via the shared `DataTable`: grouped by `role` (blessed roles
  first in the order start-here → core → rigor → reference, unknown roles after), columns = work
  (the catalog id, linking to `#/work/:id` if that route exists, else plain text), why, source doc
  (linking to `#/doc/:slug`).
- Markup and interactivity are the plugin's own; it imports `DataTable` and `useGraph`/`endpointNodeId`.

### 3. `braincrawl l3 reading-list` CLI verb

- Add `L3Cmd::ReadingList` to `apps/cli/src/cli.rs` with standard text/JSON output options.
- Implement in `apps/cli/src/l3.rs` as plain Rust over `l3::parse()`: same selection (nodes with a
  `reading` property + their `catalog` link target), emit rows of `{doc, role, why, work_id}`.
  Text output groups by role; JSON emits an array. A verb is not a plugin — no trait ceremony.
- Add the spec entries in the tsp/smithy CLI grammar files if they model `l3` subcommands
  (check first; the schema-removal cleanup found they only model `assign-ids`, so this may be a
  no-op — mirror whatever precedent exists).

### 4. Document the convention

Add the `reading` convention to `.claude/skills/braincrawl/SKILL.md`'s L3 cheat-sheet (one line)
and to the conventions spec (step 1).

## Files to Modify

- `.rhidoc/02-architecture/01-core/04-l3-conventions.md` — the `reading` convention (run
  `rhidoc --workspace .rhidoc regenerate` only if a `summary:` changes).
- `.claude/skills/braincrawl/SKILL.md` — one-line note on the convention.
- `web/src/plugins/reading-list.tsx` — the plugin.
- `web/src/plugins/index.ts` — add it to the array.
- `apps/cli/src/cli.rs` — `L3Cmd::ReadingList`.
- `apps/cli/src/l3.rs` — the verb implementation.
- tsp/smithy specs — only if they model `l3` subcommands.

## Verification

```bash
cargo build -p braincrawl-cli 2>&1 | tail -5
cargo run -q -p braincrawl-cli -- l3 reading-list 2>&1 | head -20
cargo run -q -p braincrawl-cli -- l3 reading-list --json 2>&1 | head -20
(cd /home/saxon/code/github/saxonthune/braincrawl/web && vp check && vp build) 2>&1 | tail -20
```

The migrated store may not yet carry any `reading` property (the migration used
`relates-to`/`supports`, not `reading`). To exercise the verb/plugin end-to-end, add a `reading`
property to one node in `~/code/github/saxonthune/braincrawl-l3/simulating-dynamic-systems.l3.md`
(the Strogatz W2001886606 node) as a smoke fixture, and note it in the result so the user can keep
or revert it.

## Out of Scope

- Reading *sequence* (a future `read-after` link deriving order by topological sort — not built).
- Any Catalog (L2) change; a `#/work/:id` catalog-detail plugin (the reading list may link to it,
  but building it is separate).

## Notes

- Chain position: phase 6. Depends on the web-ui-shell Surface (the `plugins` array, `Plugin`
  interface, `useGraph`/`endpointNodeId`, `DataTable`, the `doc` field on nodes).
- Role vocabulary `start-here|core|rigor|reference` is a blessed default, not a closed set.

## Surface after this phase

- The `reading` property convention is documented (a `reading: {role, why}` flow map + a `catalog`
  link) in the conventions spec and SKILL.md.
- The reading-list plugin is in the default `plugins` array and renders a grouped table.
- `braincrawl l3 reading-list [--json]` lists every node with a `reading` property and its work.
