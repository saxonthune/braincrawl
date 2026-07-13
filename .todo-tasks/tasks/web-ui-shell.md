# Web UI shell (SolidJS, plugin-driven)

## Motivation

The Web UI is a read-only feature of braincrawl (doc `.rhidoc/01-product/03-web-ui.md`):
every view comes from a **Web UI plugin** — a SolidJS module whose author decides what to
read from the research graph and how to render it. This phase builds only the *frame*: the
shared graph resource, the plugin interface, the plugins array, nav + hash routing, one
proof plugin (a document index), and a shared table primitive. The `web/` app is already
scaffolded (Vite+ Solid+TS, its own pnpm workspace, `just web-dev/web-build/web-check/web-test`);
this adds the SolidJS code, one small Rust wire-format change, and the dev proxy.

## Do NOT

- Do NOT add any write/mutation path. The UI is strictly read-only.
- Do NOT build the reading-list plugin (next phase) or the SSE live-refresh (later phase).
- Do NOT introduce a query registry, a plugin config DSL, or dynamic/runtime plugin loading.
  Plugins are TypeScript modules compiled into the app and listed in one explicit array.
- Do NOT fetch per-plugin or per-query endpoints. There is exactly ONE data source:
  `GET /api/l3/graph`. Every plugin reads the same in-memory graph object and filters client-side.
- Do NOT hardcode an auth token in client code. The local dev flow assumes the server runs
  with `BRAINCRAWL_AUTH_DISABLED=1`; the proxy simply forwards `/api`.
- Do NOT change the Vite+ toolchain, the `web/` workspace layout, or the justfile recipes —
  they exist and `vp build` is green.
- Do NOT add server-side static hosting (a `/web` ServeDir) — deferred; the POC flow is the
  Vite dev server. Leave the existing `braincrawl web` verb as-is.
- Do NOT rename or restructure the existing `crates/l3` types beyond adding the one `doc` field.

## Plan

### 1. Add `doc` to the Node wire format (`crates/l3/src/model.rs`)

The graph endpoint serializes each node as `{id, labels, properties}` and deliberately drops
`provenance`. The UI needs to group nodes by their source document, so extend `Node`'s
`Serialize` impl to also emit `doc` from `provenance.doc`:

```rust
let mut s = serializer.serialize_struct("Node", 4)?;
s.serialize_field("id", &self.id)?;
s.serialize_field("doc", &self.provenance.doc)?;
s.serialize_field("labels", &self.labels)?;
s.serialize_field("properties", &self.properties)?;
```

Update the doc-comment on `Provenance` (line ~62) that says the wire format is
`id/labels/properties` to include `doc`. Update the `deterministic_serialization` /
`full_fixture_parses_expected_shape` tests in `crates/l3/src/parse.rs` if they assert the exact
serialized node shape. `links` are unchanged.

### 2. Shared graph resource + types (`web/src/graph.tsx`)

- TypeScript types matching the wire shape exactly:
  ```ts
  export type Endpoint = string; // "node:<id>" for a node, or a catalog id like "openalex:W123"
  export interface GraphNode { id: string | null; doc: string; labels: string[]; properties: Record<string, unknown> & { title?: string } }
  export interface GraphLink { source: Endpoint; target: Endpoint; type: string; properties: Record<string, unknown> }
  export interface GraphData { nodes: GraphNode[]; links: GraphLink[] }
  ```
- One cached `createResource<GraphData>` that fetches `GET /api/l3/graph`. Expose it plus its
  `refetch` through a Solid context (`GraphProvider` + `useGraph()`), so every plugin reads the
  same object and a later phase can call `refetch()` to re-render in place.
- Helper `endpointNodeId(e: Endpoint): string | null` — returns the `<id>` when `e` starts with
  `node:`, else null (a catalog endpoint). Plugins use this to resolve link targets to nodes.

### 3. Plugin interface (`web/src/plugins/types.ts`)

```ts
import type { Component } from "solid-js";
import type { GraphData } from "../graph";
export interface PluginProps { graph: GraphData; params: Record<string, string> }
export interface Plugin {
  name: string;               // stable id, kebab-case
  title: string;              // nav label
  routes: string[];           // hash routes it claims, e.g. ["/", "/doc/:slug"]
  component: Component<PluginProps>;
}
```

### 4. Plugins array (`web/src/plugins/index.ts`)

The single web-side choke point — an explicit array `export const plugins: Plugin[] = [documentIndex]`.
Adding a plugin later is: write its file, add one line here. (This is the future enable/disable
seam; do not implement enable/disable now.)

### 5. Document-index plugin (`web/src/plugins/document-index.tsx`)

The proof plugin. From the shared graph, group nodes by `doc` and render a sortable table:
document slug, node count, and a per-label breakdown (e.g. how many `#question`/`#finding`).
Clicking a document routes to `#/doc/:slug`, which the same plugin renders as that document's
node list (title from `properties.title`, its labels, and its outgoing links). Uses the shared
table from step 7.

### 6. Shell: provider + nav + hash router (`web/src/App.tsx`, `web/src/index.tsx`)

- Wrap the app in `GraphProvider`.
- A hash router (hand-rolled over `window.location.hash` + a `hashchange` signal is fine — do
  not add a router dependency unless one ships with the Solid template): parse `#/...`, match it
  against each plugin's `routes` (support `:param` segments → `params`), and render the matching
  plugin's component with `{graph, params}`. An unmatched route renders a simple fallback.
- Nav renders from the `plugins` array (one link per plugin's first route). Include a manual
  **Refresh** button that calls the graph resource's `refetch()` (the SSE auto-refresh is a later
  phase; the button is the explicit control).
- Show a loading state while the resource is pending and an error state (with the status) if the
  fetch fails — the most likely error is the server being down or requiring a token.

### 7. Shared table primitive (`web/src/components/DataTable.tsx`)

A generic sortable table component (columns config + rows) that plugins may import. A building
block, not a registry tier. Keep it small and dependency-free.

### 8. Dev proxy (`web/vite.config.ts`)

Add a dev-server proxy so `/api/*` → `http://127.0.0.1:8787` (the server's default bind). No
build-time data, no token injection (local server runs auth-disabled).

## Files to Modify

- `crates/l3/src/model.rs` — add `doc` to `Node` serialize; update the `Provenance` doc-comment.
- `crates/l3/src/parse.rs` — update any test asserting the serialized node shape.
- `web/src/graph.tsx` — resource, context, types, `endpointNodeId` helper.
- `web/src/plugins/types.ts` — the `Plugin` interface.
- `web/src/plugins/index.ts` — the plugins array.
- `web/src/plugins/document-index.tsx` — the first plugin.
- `web/src/components/DataTable.tsx` — shared sortable table.
- `web/src/App.tsx`, `web/src/index.tsx` — provider, nav, hash router, refresh button.
- `web/vite.config.ts` — `/api` dev proxy.

## Verification

```bash
cargo test -p l3 2>&1 | tail -8
(cd /home/saxon/code/github/saxonthune/braincrawl/web && vp check && vp build) 2>&1 | tail -20
```

Manual smoke (not required to pass CI, note it in the result): with a local server running
`BRAINCRAWL_AUTH_DISABLED=1 BRAINCRAWL_L3_ROOT=~/code/github/saxonthune/braincrawl-l3`, `just web-dev`
serves the shell; the document index lists all 19 docs with node counts; the Refresh button
re-fetches.

## Out of Scope

- Reading-list plugin, SSE refresh, graph viewer, any L1/L2 view.
- Server-side static hosting of `/web`, production/token auth in the browser.
- Dynamic plugin loading, enable/disable implementation.

## Notes

- Chain position: phase 5 (first of the UI arc). Depends on the landed `/api/l3/graph` endpoint.
- The graph is now real (19 migrated docs, 764 nodes) — the document index has genuine data.

## Surface after this phase

- The Node wire format includes `doc` (source document slug) alongside `id/labels/properties`.
- `web/src/graph.tsx` exports `GraphData`/`GraphNode`/`GraphLink`/`Endpoint` types, a
  `GraphProvider` + `useGraph()` context giving the cached graph resource **and its `refetch()`**,
  and `endpointNodeId()`.
- `web/src/plugins/types.ts` exports the `Plugin` interface (`{name, title, routes, component}`)
  with `PluginProps = {graph, params}`; `web/src/plugins/index.ts` exports the `plugins` array.
- A hash router renders the plugin matching `#/…`; nav is generated from the plugins array; a
  manual Refresh button calls `refetch()`.
- `web/src/components/DataTable.tsx` exports a reusable sortable table.
- `web/vite.config.ts` proxies `/api` → `127.0.0.1:8787`.
- Adding a plugin is: one file + one line in `plugins/index.ts`, no shell changes.
- Deliberately unchanged and relied on by later phases: `braincrawl web` still prints
  `<server>/web` (static hosting not yet wired); no SSE endpoint yet; the graph resource's
  `refetch()` is the hook the live-refresh phase drives.
