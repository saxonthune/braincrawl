# Live refresh: watch the store, push to the browser

## Motivation

Edit an L3 doc, see the browser view update in place. Solid re-renders automatically when a
resource refetches — the page never reloads; the only missing piece is the change signal. This
phase adds a file watcher on the server that emits Server-Sent Events, and wires the shell's
`EventSource` to call the graph resource's `refetch()`.

## Do NOT

- Do NOT add any write path — this is a change *notification*, not a mutation.
- Do NOT block or branch the edge/Cloudflare backend on this. On a static/edge deploy the events
  endpoint simply never exists or never fires (data changes only at deploy); the browser code must
  degrade silently when `EventSource` fails to connect, not error.
- Do NOT re-parse or diff the graph on the server for the event — the event is a bare "changed"
  signal; the browser refetches `/api/l3/graph` (which already parses fresh per request).
- Do NOT change the plugin interface, the graph resource's public shape, or `/api/l3/graph`.
- Do NOT remove or bypass the manual Refresh button — it stays as the explicit control.

## Plan

### 1. File watcher + SSE endpoint (`apps/server`)

- Add the `notify` crate (workspace dep) to the server. On startup, if `l3_root` is `Some`, spawn
  a watcher over that directory (recursive, `.l3.md` files).
- Debounce bursts: an agent session writes several files in quick succession. Coalesce events
  within a short window (e.g. 300–500ms) into a single "changed" notification. Use a
  `tokio::sync::broadcast` channel: the watcher task sends on change (post-debounce); each SSE
  connection subscribes.
- Add `GET /api/events` (SSE, axum `Sse` + `KeepAlive`) on the public router (read-only signal, no
  body). Each connected client receives a `data: changed` event per debounced change. If `l3_root`
  is `None`, the route may 404 or simply never emit.
- Keep it off the auth gate for parity with the local read-only flow (same posture as the dev
  proxy assumption), OR behind the same gate as `/api/l3/graph` — match whichever router
  `/api/l3/graph` ended up on so the browser reaches both the same way.

### 2. EventSource wiring (`web/`)

- In the shell (or the graph context), open an `EventSource("/api/events")`. On each message, call
  the graph resource's `refetch()`. On `error`/failed connection, close quietly and do not retry
  aggressively — a static deploy has no such endpoint; the app must stay fully functional without it.
- If auto-refetch proves jumpy mid-read, the fallback (a "data changed" toast that refetches on
  click) is a UX swap on the same mechanism — implement the direct refetch first; note the toast
  option in the result.

### 3. Optional cheap version signal

If straightforward, add `GET /api/version` returning a short content hash (e.g. the same ETag the
graph endpoint computes) for polling clients that can't use SSE. Optional; skip if it adds friction.

## Files to Modify

- `apps/server/Cargo.toml`, root `Cargo.toml` — add `notify`.
- `apps/server/src/lib.rs` — the watcher task, broadcast channel, `/api/events` route (+ optional
  `/api/version`).
- `apps/server/src/handlers/` — the SSE handler (new file or in `l3.rs`).
- `apps/server-bin/src/main.rs` — spawn the watcher if `l3_root` is set (wire the channel into app state).
- `web/src/graph.tsx` (or `App.tsx`) — the `EventSource` + `refetch()` wiring.

## Verification

```bash
cargo build --workspace 2>&1 | tail -5
cargo test -p braincrawl-server 2>&1 | tail -8
(cd /home/saxon/code/github/saxonthune/braincrawl/web && vp check && vp build) 2>&1 | tail -20
```

Manual smoke (note in result): with the local server + `just web-dev` running, edit a node's
remarks in a `.l3.md` file and confirm the open browser view updates within the debounce window
without a manual refresh.

## Out of Scope

- Any write path or edit-from-browser.
- Edge/Cloudflare event delivery (the endpoint just doesn't fire there).
- Per-plugin granular invalidation — one "changed" signal refetches the whole graph.

## Notes

- Chain position: phase 7 (last of the UI arc). Depends on the web-ui-shell Surface (the graph
  resource's `refetch()`).
- Debounce matters: agent sessions write files in bursts; a single coalesced signal per burst.

## Surface after this phase

- The server watches the L3 root and emits `GET /api/events` (SSE) with a debounced "changed"
  signal per edit burst.
- The shell's `EventSource` calls `refetch()` on each signal, updating open views in place; the
  app degrades silently to manual-refresh-only when the endpoint is absent (edge deploy).
