# Graph neighborhood query — core + server (Phase 1 of 2)

## Motivation

The store accumulates works + directional citation edges, but the only read primitive is
one-hop `get_edges`. CASE-STUDY want #2 ("a neutral graph you can query for coverage")
needs a traversal primitive: seed → bounded BFS → a subgraph, with nodes ranked by
in-degree *within the returned subgraph* (the "canon" signal from GOALS.md §graph-building).

This phase adds the core use-case method and the server endpoint. The CLI surface is
Phase 2 (`graph-neighborhood-cli`). It builds only on the existing
`MetadataStore::read_edges` / `read_node` traits — **no per-backend changes**.

## Do NOT

- Do NOT modify any backend crate (`crates/backends/*`). Use only the existing
  `MetadataStore` trait methods (`get_alias`, `resolve_live`, `read_edges`, `read_node`).
  No new trait methods, no SQL changes.
- Do NOT add semantic/vector search, keyword search, or concept/topic ("citation drift")
  filtering — those are explicitly out of scope.
- Do NOT touch the CLI (`apps/cli/*`) — that is Phase 2.
- Do NOT do unbounded traversal: `depth` and `max_nodes` are hard caps and the closure
  must never be ingested past them.
- Do NOT add a new ranking trait or persist in-degree — it is computed at query time from
  the discovered edges.

## Plan

### 1. Core types (`crates/core/src/types.rs`)

Add three serializable structs near `EdgeView` (derive `Clone, Debug, PartialEq,
Serialize, Deserialize`, matching the existing derives in this file):

```rust
pub struct NeighborhoodNode {
    pub canonical_id: CanonicalId,
    pub kind: NodeKind,
    pub depth: u32,                  // BFS distance from the nearest seed (seeds = 0)
    pub in_degree: u32,             // # discovered edges (both endpoints included) whose dst == this node
    pub attrs: serde_json::Value,   // merged node attrs (same merge as get_work); {} for stubs
}

pub struct NeighborhoodEdge {
    pub src: CanonicalId,
    pub dst: CanonicalId,
    pub relation: String,
}

pub struct Neighborhood {
    pub nodes: Vec<NeighborhoodNode>,   // sorted: in_degree desc, then canonical_id asc
    pub edges: Vec<NeighborhoodEdge>,   // only edges where BOTH endpoints are included nodes
    pub truncated: bool,                // true if max_nodes capped the traversal
}
```

`CanonicalId` and `NodeKind` already serialize (they appear in `EdgeView` / `WorkView`).

### 2. Core use-case method (`crates/core/src/usecases.rs`)

Add a `neighborhood` method to the `Store` impl, next to `get_edges`:

```rust
pub async fn neighborhood(
    &self,
    seeds: Vec<Alias>,
    dir: EdgeDir,
    depth: u32,
    max_nodes: u32,
) -> Result<Neighborhood, DomainError>
```

Algorithm:

1. **Resolve seeds.** For each seed alias: `get_alias` → if `None`, skip it (unknown seed
   is not an error); else `resolve_live`. Collect the distinct live canonical ids; these
   are depth-0 nodes. Track a `visited: HashMap<String /*canonical id*/, u32 /*depth*/>`.
   If the seed count alone exceeds `max_nodes`, keep only the first `max_nodes` (sorted by
   id for determinism) and set `truncated = true`.
2. **BFS.** Let `frontier` = the seed ids. For `current_depth` in `0..depth`, while the
   frontier is non-empty:
   - For each node in the frontier, **drain `read_edges(node, dir, cursor, PAGE)`** across
     pages (loop until the returned cursor is `None`). Use a `const NEIGHBORHOOD_EDGE_PAGE:
     u32 = 200;`.
   - For each `EdgeView`: record the discovered edge `(src, dst, relation)` in a dedup set
     keyed by `(src.0, dst.0, relation)`. The **neighbor** is `ev.dst` when
     `dir == Forward` (traversal follows `src→dst`) and `ev.src` when `dir == Backward`
     (follows `dst→src`).
   - If the neighbor is not in `visited`: if `visited.len() < max_nodes`, insert it at
     `current_depth + 1` and add to the next frontier; otherwise set `truncated = true`
     and stop adding new nodes (keep draining current edges but admit no more nodes).
   - Advance to the next frontier.
3. **Build edges.** Keep only discovered edges where **both** `src` and `dst` are in
   `visited` (a closed subgraph). 
4. **Build nodes.** For each visited canonical id, `read_node` to get `(kind, assertions,
   _aliases)`; merge `assertions` into `attrs` using the existing `merge_assertions`
   helper (call it directly — same module). For a stub (`read_node` returns `None` or no
   assertions), use `NodeKind::Work` / `attrs = {}`. Compute `in_degree` = count of kept
   edges whose `dst.0 == this id`.
5. **Sort nodes** by `in_degree` descending, then `canonical_id` ascending.
6. Return `Neighborhood { nodes, edges, truncated }`.

Note in a doc comment that in-degree is computed over edges *discovered during traversal*
among included nodes (last-layer cross-edges that were never expanded are not counted) —
this is intentional and bounded by `max_nodes`.

### 3. Core test (`crates/core/tests/engine.rs`)

Follow the existing harness in this file (same store construction as the other tests).
Build a small graph with `put_work` + `put_edges`, e.g. a hub H cited by A, B, C (edges
`A→H`, `B→H`, `C→H`, plus `A→B`). Assert, via `neighborhood`:

- From seed A, `dir = Forward`, `depth = 2`: the subgraph includes A, B, H (follows
  `src→dst`); `edges` is the closed subset; H has the highest `in_degree`; nodes are
  sorted by `in_degree` desc.
- A `max_nodes` cap smaller than the reachable set yields `truncated == true` and no more
  than `max_nodes` nodes.
- `depth = 0` returns only the seed node(s) and no edges.
- An unknown seed alias is skipped (no error).

### 4. Server endpoint (`apps/server/src/lib.rs`)

Add a request type and handler, and register the route.

- Request (serde `Deserialize`):
  ```rust
  struct NeighborhoodHttpRequest {
      seeds: Vec<String>,   // each "ns:value"
      dir: String,          // "forward" | "backward"
      depth: u32,
      max_nodes: u32,
  }
  ```
- Handler `handler_neighborhood(State<Arc<LocalStore>>, Json<NeighborhoodHttpRequest>)`:
  parse each seed with the existing `parse_alias` (skip unparseable ones); map `dir` with
  the same forward/backward matching used in `handler_works_get` (`"forward"` →
  `EdgeDir::Forward`, `"backward"` → `EdgeDir::Backward`, else `400`); call
  `run_blocking(... store.neighborhood(seeds, dir, depth, max_nodes) ...)`. On `Ok`,
  `Json(neighborhood)`; on `Err`, `(domain_status(&e), e.to_string())`.
- Register `POST /graph/neighborhood` in `make_app`, before the `/works/*path` wildcard so
  routing is unambiguous, and inside the auth-gated layer (same as the other routes).

### 5. Server test (`apps/server/tests/smoke.rs`)

Follow the existing smoke-test harness (`make_app` + a test store, auth disabled). PUT a
couple of works and an edge, then POST `/graph/neighborhood` with one seed, `depth: 1`,
`max_nodes: 50`, and assert `200` + that the response `nodes`/`edges` contain the expected
ids and that the response shape (`nodes`, `edges`, `truncated`) is present.

## Files to Modify

- `crates/core/src/types.rs` — add `NeighborhoodNode`, `NeighborhoodEdge`, `Neighborhood`.
- `crates/core/src/usecases.rs` — add `neighborhood()` + `NEIGHBORHOOD_EDGE_PAGE`.
- `crates/core/tests/engine.rs` — add the traversal test(s).
- `apps/server/src/lib.rs` — request type, `handler_neighborhood`, route registration.
- `apps/server/tests/smoke.rs` — endpoint test.

## Verification

```bash
cargo test -p braincrawl-core -p braincrawl-server-lib
cargo build -p braincrawl-server
cargo clippy -p braincrawl-core -p braincrawl-server-lib --all-targets
```

## Out of Scope

- CLI surface (Phase 2: `graph-neighborhood-cli`).
- Semantic/vector search, keyword search, concept/topic drift filtering.
- Persisting or caching in-degree; any backend/SQL change.

## Notes

- `read_edges` is paginated; the BFS must drain all pages per frontier node or it will
  silently miss neighbors on hub nodes.
- `merge_assertions` is a private fn in `usecases.rs`; call it directly from the new method
  (same module) — do not re-implement merging.
- Edge direction is mechanical: Forward follows `src→dst`, Backward follows `dst→src`.
  Do not rely on the cited_by/references labels.

## Surface after this phase

- `braincrawl_core::types::Neighborhood { nodes: Vec<NeighborhoodNode>, edges:
  Vec<NeighborhoodEdge>, truncated: bool }`, `NeighborhoodNode { canonical_id: CanonicalId,
  kind: NodeKind, depth: u32, in_degree: u32, attrs: serde_json::Value }`, and
  `NeighborhoodEdge { src: CanonicalId, dst: CanonicalId, relation: String }` — all
  `Serialize + Deserialize`. Nodes are returned sorted by `in_degree` desc, then
  `canonical_id` asc.
- `Store::neighborhood(seeds: Vec<Alias>, dir: EdgeDir, depth: u32, max_nodes: u32) ->
  Result<Neighborhood, DomainError>` on the core `Store` use-case type.
- Server route **`POST /graph/neighborhood`** (auth-gated). Request body JSON:
  `{ "seeds": ["ns:value", …], "dir": "forward"|"backward", "depth": <u32>,
  "max_nodes": <u32> }`. Response body JSON: the serialized `Neighborhood`, i.e.
  `{ "nodes": [ { "canonical_id", "kind", "depth", "in_degree", "attrs" }, … ],
  "edges": [ { "src", "dst", "relation" }, … ], "truncated": <bool> }`.
  Unknown/unparseable seeds are skipped; bad `dir` → `400`.
- Negative space: the CLI (`apps/cli/*`) is unchanged and has no graph command yet — that
  is Phase 2. All existing routes (`/works`, `/edges`, `/works/have`, `/works/*path`) and
  existing core methods are unchanged.
