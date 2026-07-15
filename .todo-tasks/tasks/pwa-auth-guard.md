# Web UI: shared services layer, auth guard, and email sign-in page

## Motivation

The cross-cutting concerns (settings, authorized store fetch) grew
inside the chat plugin; `graph.tsx` fetches without auth and 401s
against the deployed worker. Formalize the plugin-shell shape: lift
services out of chat, guard plugin routes behind an auth check with a
declarative redirect, and add a sign-in page over the worker's new email
OTP endpoints. Absorbs the `web-graph-fetch-auth` fix.

## Do NOT

- Do NOT change the `Plugin` contract in `web/src/plugins/types.ts`
  beyond what the auth route needs — no speculative slots.
- Do NOT remove the manual token paste in Settings — it stays as the
  dev/shared-secret path alongside OTP sign-in.
- Do NOT introduce cookies or change how the token is stored — the
  session token from OTP lands in the same `bc.settings.storeToken`
  localStorage slot the manual path uses.
- Do NOT alter chat behavior, tools, transports, or session storage.
  The transport work merged just before this chain — imports may
  reference `./settings` inside plugins/chat; update import paths
  mechanically, do not rewrite logic.
- Do NOT add a client-side "security" measure beyond the redirect — the
  worker's 401 is the boundary; the guard is UX.

## Plan

### 1. Services lift

New `web/src/services/`:

- `settings.ts` — MOVE `web/src/plugins/chat/settings.ts` here
  unchanged (same localStorage keys, so existing browsers keep their
  values). Update all importers (`plugins/chat/*`); leave no re-export
  shim.
- `store.ts` — one `storeFetch(path, init?)` used by everything that
  talks to the store: prefixes `storeBaseUrl`, attaches
  `Authorization: Bearer` when `storeToken` is set. Port the chat
  plugin's `storeUrl`/header logic (in `tools.ts` and `prompt.ts`) onto
  it and update `graph.tsx` to use it — that fixes the unauthenticated
  graph fetch. On any 401 response, call `setUnauthed()` (below).
- `auth.ts` — a module-level signal:
  `isAuthed()`, `setUnauthed()`, and `probeAuth()` which GETs `/stats`
  via `storeFetch` and sets the signal from the result. Probe once at
  app start; empty `storeToken` → unauthed without probing.

### 2. Guard

`web/src/App.tsx`: wrap plugin routes in a layout route

```tsx
<Route component={RequireAuth}>{pluginRoutes}</Route>
<Route path="/auth" component={AuthPage} />
```

`RequireAuth` renders children when `isAuthed()`, else
`<Navigate href="/auth" />` (declarative; from `@solidjs/router`).
While the initial probe is in flight, render a minimal "checking…"
placeholder rather than redirecting (avoid a flash of the auth page on
every reload).

### 3. Auth page

New `web/src/plugins/auth/` plugin (`id: "auth"`, one route `/auth`, no
nav entry): two-step form —

1. Email field → `POST /api/auth/request-code` via `storeFetch` (no
   token needed; endpoint is public). Always advance to step 2 with the
   neutral message "If that address is allowed, a code was sent."
2. Code field → `POST /api/auth/verify`; on 200 store `token` into
   `setSetting("storeToken", ...)`, `probeAuth()`, navigate to `/`.
   On 401 show "invalid or expired code" and stay.

Include a link to `#/settings` ("have a token? paste it instead").

### 4. Graph SSE quieting

`web/src/graph.tsx`: the `EventSource("/api/events")` reconnect-loops
against the worker (which has no events endpoint). On `error` before any
`open`, close the source and do not retry (server-bin still gets live
refresh; the worker path degrades to manual Refresh).

### 5. Tests

Extend the vitest suite (test script exists from the transport task):
unit tests for `storeFetch` URL/header assembly and for the auth-signal
transitions (`probeAuth` with mocked fetch: 200 → authed, 401 →
unauthed). Keep DOM out of it — test the services, not components.

## Files to Modify

- `web/src/services/settings.ts` (moved), `store.ts`, `auth.ts` — new
  layer
- `web/src/plugins/chat/{settings.ts → delete}, tools.ts, prompt.ts,
  transport*.ts, ChatView.tsx, Settings.tsx` — import updates only
- `web/src/plugins/auth/index.tsx` — new plugin
- `web/src/plugins/index.ts` — register auth plugin
- `web/src/App.tsx` — RequireAuth wrapper
- `web/src/graph.tsx` — storeFetch + SSE quieting
- `web/src/services/*.test.ts` — new tests

## Verification

```bash
CI=true pnpm -C web install
pnpm -C web build
pnpm -C web test
```

## Out of Scope

- Cookie sessions, logout/revocation UI
- Documenting the plugin contract in .rhidoc (user-driven, separate)
- Any worker change

## Surface after this phase

- `web/src/services/{settings,store,auth}.ts` exist; every store call in
  the app goes through `storeFetch`; a 401 anywhere flips `isAuthed`
  false and the router lands on `/auth`.
- `/auth` performs the email → code → token flow against
  `/api/auth/request-code` + `/api/auth/verify` and stores the session
  token in the existing `bc.settings.storeToken` slot.
- Manual token paste in Settings still works; localStorage keys
  unchanged, so existing installs stay signed in.
- Negative space: `Plugin` contract shape, chat plugin behavior, and
  worker API usage are unchanged.
