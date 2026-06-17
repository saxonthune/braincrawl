# Graph neighborhood query — CLI surface (Phase 2 of 2)

## Motivation

Phase 1 (`graph-neighborhood-core`) added the core `Store::neighborhood` method and the
`POST /graph/neighborhood` server endpoint. This phase exposes it through the `braincrawl`
CLI so the "magic script" can query the neutral graph directly — the end-to-end payoff for
CASE-STUDY want #2.

This phase is triaged against Phase 1's declared **Surface** (the endpoint contract below),
which Phase 1 promises to leave behind:

> `POST /graph/neighborhood` (auth-gated). Request:
> `{ "seeds": ["ns:value", …], "dir": "forward"|"backward", "depth": <u32>, "max_nodes": <u32> }`.
> Response: `{ "nodes": [ { "canonical_id", "kind", "depth", "in_degree", "attrs" }, … ],
> "edges": [ { "src", "dst", "relation" }, … ], "truncated": <bool> }`.
> Unknown/unparseable seeds are skipped; bad `dir` → `400`.

## Do NOT

- Do NOT touch the server, core, or any backend crate — they are done (Phase 1). This is a
  CLI-only change under `apps/cli/`.
- Do NOT re-implement traversal in the CLI; it is a thin HTTP client over the Phase-1
  endpoint.
- Do NOT route graph queries through OpenAlex or any provider — graph queries hit the
  braincrawl store only (like the hidden `store` namespace, not the `openalex` fork).
- Do NOT force the neighborhood response through the existing `Envelope` (it has no `edges`
  field). Render it directly (see Plan).
- Do NOT add `--fields`/`--limit`/`--all` behavior to the graph command (those shape
  paginated provider lists; neighborhood is a single bounded response). They may be present
  as global flags but the graph command ignores them — note this.

## Plan

### 1. CLI grammar (`apps/cli/src/cli.rs`)

Add a new top-level namespace variant alongside `Store` and `Openalex`:

```rust
#[command(about = "Query the braincrawl neutral graph (store-only, no provider)")]
Graph(GraphArgs),
```

with:

```rust
#[derive(Args)]
pub struct GraphArgs { #[command(subcommand)] pub cmd: GraphCmd }

#[derive(Subcommand)]
pub enum GraphCmd {
    /// Bounded neighborhood traversal from one or more seed ids
    Neighborhood {
        /// Seed ids in ns:value form (e.g. openalex:W2031938753 doi:10.x/y)
        seeds: Vec<String>,
        /// Traversal direction: forward (src→dst) or backward (dst→src)
        #[arg(long, default_value = "forward")]
        dir: String,
        /// Max BFS depth from the seeds
        #[arg(long, default_value_t = 1)]
        depth: u32,
        /// Max nodes in the returned subgraph
        #[arg(long = "max-nodes", default_value_t = 200)]
        max_nodes: u32,
    },
}
```

### 2. Store client method (`apps/cli/src/store_client.rs`)

Add a `neighborhood` method on `StoreClient` following the existing pattern (`apply_auth`,
error mapping to `ClientError::Server`):

```rust
pub fn neighborhood(
    &self,
    seeds: &[String],
    dir: &str,
    depth: u32,
    max_nodes: u32,
) -> Result<serde_json::Value>
```

It POSTs `{ "seeds": seeds, "dir": dir, "depth": depth, "max_nodes": max_nodes }` to
`{base_url}/graph/neighborhood` and returns the parsed JSON response on success.

### 3. Wire the command (`apps/cli/src/main.rs`)

Add a `Namespace::Graph(graph)` arm to the `match cli.namespace` block. Construct a
`StoreClient` (same as the `Store` arm: `StoreClient::new(&config.server_url)
.with_token(config.auth_token.clone())`). For `GraphCmd::Neighborhood`, call
`client.neighborhood(&seeds, &dir, depth, max_nodes)?` and render (Plan step 4). Graph
queries never push, so there is no push step.

### 4. Rendering

Render the neighborhood response honoring `opts` (do not use `Envelope`):

- **`--json` (default):** pretty-print the full server response
  (`serde_json::to_string_pretty`) so both `nodes` and `edges` are visible.
- **`--text`:** one line per node, already in the server's in-degree-desc order:
  `{canonical_id}\t{in_degree}\t{title}` where title is `attrs.title` /
  `attrs.display_name` if present, else empty. After the node lines, print a trailing
  summary line to stderr: node count, edge count, and `truncated` if true.

Put this in a small helper (e.g. a `render_neighborhood(&value, &opts)` fn in
`apps/cli/src/main.rs`, or a new `apps/cli/src/graph.rs` module exported from
`apps/cli/src/lib.rs` if you prefer — either is fine, keep it cohesive).

### 5. CLI test

Add a test following the existing CLI test patterns in `apps/cli/tests/`. A focused test
of the `StoreClient::neighborhood` request/response against a mock or in-process server is
ideal; if the existing tests mock the store over HTTP, mirror that. If the existing harness
spins up the real server, reuse it. Assert the request body shape and that a returned
`nodes`/`edges`/`truncated` payload is parsed. If no store-mocking harness exists in
`apps/cli/tests/`, instead add a `clap` parse test asserting `graph neighborhood W1 W2
--dir backward --depth 2 --max-nodes 10` parses into the expected `GraphCmd::Neighborhood`
values, and keep the client method covered by the Phase-1 server test. State in Notes which
approach you took and why.

## Files to Modify

- `apps/cli/src/cli.rs` — `Graph` namespace, `GraphArgs`, `GraphCmd::Neighborhood`.
- `apps/cli/src/store_client.rs` — `neighborhood()` client method.
- `apps/cli/src/main.rs` — wire the `Graph` arm + rendering helper.
- (optional) `apps/cli/src/graph.rs` + `apps/cli/src/lib.rs` — if you split out rendering.
- `apps/cli/tests/*.rs` — a parse and/or client test (see Plan step 5).

## Verification

```bash
cargo test -p braincrawl-cli
cargo build -p braincrawl-cli
cargo clippy -p braincrawl-cli --all-targets
```

Manual smoke (optional, requires a running server):

```bash
BRAINCRAWL_AUTH_DISABLED=1 cargo run -p braincrawl-server &
cargo run -p braincrawl-cli -- graph neighborhood openalex:W2031938753 --dir backward --depth 1 --text
```

## Out of Scope

- Any server/core/backend change (done in Phase 1).
- `--fields`/`--limit`/`--all` semantics for the graph command.
- Semantic/keyword search, drift filtering, Layer 3 projections.

## Notes

- The graph namespace is store-only — it must not construct an `OpenAlexClient` or push.
- Seeds are passed through verbatim as `ns:value` strings; the server resolves/skips them.
- `dir` is forwarded as a string; the server validates it (bad `dir` → `400`, surface that
  error to the user via the existing `ClientError::Server` path).

## Surface after this phase

- `braincrawl graph neighborhood <seeds…> [--dir forward|backward] [--depth N]
  [--max-nodes N]` queries `POST /graph/neighborhood` and renders nodes (in-degree order)
  + edges, honoring `--json`/`--text`.
- `StoreClient::neighborhood(seeds, dir, depth, max_nodes) -> Result<serde_json::Value>`.
- Negative space: server/core/backends unchanged from Phase 1; the `openalex` and `store`
  namespaces are unchanged.
