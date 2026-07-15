# Serve the web UI from the worker + iOS installability

## Motivation

"Available anywhere" ends with one URL: the worker serves the built
SolidJS app and the API from the same origin, and the app installs to
the iOS home screen. Same-origin also makes the PWA's store calls
CORS-irrelevant (CORS stays for dev and third-party origins).

## Do NOT

- Do NOT introduce a service worker. iOS installability needs only the
  manifest + apple meta tags, and SW cache-staleness pain isn't worth it
  yet. (Explicitly out: workbox, custom sw.js, anything registering a SW.)
- Do NOT switch to Cloudflare Pages — Workers static assets on the
  existing worker, one deploy unit.
- Do NOT break the server-bin hosting path (`/web` nesting must keep
  working — that's why the vite base goes relative, not absolute-root).
- Do NOT edit `apps/worker/wrangler.toml` (gitignored, user's live file)
  — edit `wrangler.toml.example` and list the mirror step in the result
  notes for the user.

## Plan

### 1. Vite base → relative

`web/vite.config.ts`: `base: "./"` (currently `/web/`). HashRouter means
routing is location-independent; a relative base makes the same `dist`
correct both nested at `/web/` (server-bin) and at `/` (worker). Confirm
built asset URLs in `dist/index.html` are relative after the change.

### 2. Worker assets config

`apps/worker/wrangler.toml.example`:

```toml
[assets]
directory = "../../web/dist"
not_found_handling = "single-page-application"
run_worker_first = ["/api/*", "/works/*", "/graph/*", "/stats", "/edges", "/health"]
```

API paths hit the worker script first; everything else is served from
assets with SPA fallback. Verify the exact `run_worker_first` list
covers every route in `apps/worker/src/lib.rs` dispatch (including
`/works` and `/edges` without trailing segments) — derive it from the
code, not from this snippet. Confirm wrangler 4.92 supports the array
form of `run_worker_first`; if not, use the documented equivalent and
note it.

### 3. Manifest + iOS meta

- `web/public/manifest.webmanifest` — name "braincrawl", short_name,
  `display: "standalone"`, `start_url: "./"`, theme/background colors
  consistent with the app's palette.
- `web/index.html` — link the manifest; add `apple-mobile-web-app-capable`,
  `apple-mobile-web-app-status-bar-style`, and `apple-touch-icon` link.
- Icon: generate a simple flat 180×180 and 512×512 PNG (solid background,
  "bc" glyph) IF ImageMagick (`magick`/`convert`) is available in the
  environment; commit them under `web/public/icons/`. If not available,
  ship the manifest without icons and say so in the result notes — do
  not block on icon tooling.

### 4. Deploy wiring

`justfile`: add a `deploy-worker` recipe = `pnpm -C web build` then
`wrangler deploy` run from `apps/worker` (use make-style `-C`/cd-in-
recipe consistent with existing recipes). The recipe is for the user;
CI/agents don't deploy.

### 5. Dev-mode note

The vite dev proxy currently forwards only `/api`; extend the proxy map
so the chat tools work under `vp dev` against the local server
(`/works`, `/graph`, `/stats`, `/edges`, `/health` → 127.0.0.1:8787).

## Files to Modify

- `web/vite.config.ts` — base + proxy additions
- `web/index.html` — manifest link + apple meta
- `web/public/manifest.webmanifest`, `web/public/icons/*` — new
- `apps/worker/wrangler.toml.example` — [assets] block
- `justfile` — deploy-worker recipe

## Verification

```bash
pnpm -C web install
pnpm -C web build
./scripts/worker-conformance.sh
```

(The conformance run proves the [assets] example change didn't break the
local wrangler-dev flow — its bootstrap copies the example when the real
toml is absent; if `wrangler dev --local` chokes on a missing
`web/dist`, build the web app first in the script or make the assets
directory existence-tolerant and note which.)

## Out of Scope

- Service worker / offline
- Custom domain
- Auth UX changes (token still pasted into settings)

## Notes

- After merge, the user mirrors the `[assets]` block into their live
  `apps/worker/wrangler.toml` and runs `just deploy-worker`.
- `not_found_handling: single-page-application` + HashRouter is belt and
  suspenders; deep links are hash-based anyway.

## Surface after this phase

- `just deploy-worker` builds the web app and deploys worker + assets;
  the app is served at the worker origin root with SPA fallback; API
  routes take precedence via `run_worker_first`.
- The app is installable on iOS (manifest + apple meta; icons if
  tooling allowed).
- The same `web/dist` still works nested at `/web/` on server-bin.
- Negative space: no service worker registered; worker API routes and
  auth unchanged.
