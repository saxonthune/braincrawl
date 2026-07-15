# Web UI: agent loop, braincrawl tools, reading-companion transport

## Motivation

The chat plugin (merged) renders sessions and calls a stub `Transport`.
This task makes it real: a hand-rolled browser-side agent loop against
the Anthropic API, six tools speaking to the braincrawl store and
OpenAlex, the bundled system prompt plus store-fetched context files, and
persistence after every step so an interrupted turn resumes.

## Do NOT

- Do NOT use the SDK's `toolRunner` or any chat/agent library — the loop
  is hand-rolled so its state round-trips through the session store.
  `@anthropic-ai/sdk` is allowed as transport only (client + streaming +
  types).
- Do NOT add tools beyond the six listed (the rest is a follow-up task).
- Do NOT rewrite the merged chat components — wire into the existing
  `Transport` seam, store actions, and settings accessors; extend types
  only additively.
- Do NOT proxy through any server: the browser calls Anthropic directly
  (`dangerouslyAllowBrowser: true`) and the store directly.
- Do NOT hardcode URLs or models — base URL, tokens, and model come from
  the merged settings module.
- Do NOT invent `^r-` anchors or write them from the client; the store
  normalizes on PUT.

## Plan

### 1. Session model additions (`types.ts`, additive)

- `ChatSession.activeBook?: { workId: string; docSlug: string; title: string }`
- `ChatSession.pendingTurn?: { messagesSnapshotAt: string }` — marker set
  while a turn is in flight, cleared on completion; presence on load
  means the turn was interrupted.
- A `ThinkingBlock`-tolerant message shape is NOT needed — request
  thinking display omitted (default); do not store thinking blocks.

### 2. Tools (`web/src/plugins/chat/tools.ts`)

Each tool = Anthropic tool definition (JSON schema) + async handler.
Handlers use `fetch`; store calls send `Authorization: Bearer <storeToken>`
against `<storeBaseUrl>`; OpenAlex calls hit `https://api.openalex.org`
directly (no key). Tool results are strings (JSON-stringified payloads,
truncated with a note beyond ~20 KB).

1. `openalex_search(query, entity="works", limit=8)` — GET
   `/{entity}?search=...&per-page=...`. For works: also `PUT /works` to
   the store for each result (map id/title/year/authors into the
   `WorkRecord` shape the CLI pushes — mirror the JSON the store's
   `PUT /works` expects; check `apps/cli/src/store_client.rs` for the
   exact field mapping and replicate it).
2. `openalex_cited_by(work_id, concept_filter?, limit=15)` — GET
   `/works?filter=cites:<id>[,concepts.id:<concept>]`; push works AND
   citation edges (`PUT /edges`) like the CLI does.
3. `read_pages(work_id, page_start, page_end)` — GET
   `<base>/works/<work_id>/content/chunks`, cache the parsed chunk array
   in a module-level Map keyed by work id for the app's lifetime, return
   the chunks overlapping the page range (join text, prefix each chunk
   with `[pp.X-Y]`). 404 → tell the model chunks aren't loaded for this
   work and to inform the user (chunking is a desktop step).
4. `graph_neighborhood(seeds, dir, depth=1, max_nodes=40)` — POST
   `<base>/graph/neighborhood`.
5. `l3_read_doc(slug)` — GET `<base>/api/l3/docs/<slug>` (raw markdown).
6. `l3_edit_doc(slug, mode: "append"|"str_replace", text, old_str?)` —
   apply to a locally cached copy (fetch first if not cached): append
   adds `text` at end of file; str_replace requires exactly one match of
   `old_str` (0 or >1 → return an error result without PUTting). Then
   `PUT <base>/api/l3/docs/<slug>`; on 200 adopt the normalized markdown
   into the cache and return a short success summary; on 400 return the
   warnings JSON as the (is_error) tool result so the model corrects
   itself; 409 likewise.

### 3. System prompt assembly (`web/src/plugins/chat/prompt.ts`)

- Import the bundled core: `import corePrompt from "./prompt.md?raw"`
  (vite raw import — confirm it works under vite-plus; if the `?raw`
  suffix fails the build, inline the file as a TS template-literal module
  instead and note it).
- At turn start, fetch `<base>/api/l3/agent/principles` and
  `.../agent/memory` (tolerate 404/network-fail → skip silently), and the
  active book's doc via `l3_read_doc` machinery.
- Build the `system` array as blocks, in order: core prompt (with
  `cache_control: {type: "ephemeral"}` on this stable block), then
  principles, then memory, then a short session-context block naming the
  active book (title, work id, doc slug) and today's date. Volatile
  blocks come after the cached one.

### 4. The loop (`web/src/plugins/chat/transport.ts`)

`anthropicTransport: Transport` replacing the stub when an API key is
set (ChatView picks stub vs real based on settings — smallest possible
wiring change):

- Client: `new Anthropic({ apiKey, dangerouslyAllowBrowser: true })`.
- Per turn: build system blocks, map the session's stored messages into
  API messages verbatim (they're already block-shaped), then iterate:
  `client.messages.stream({model, max_tokens: 8192, system, tools,
  messages})`; stream text deltas through `onDelta` AND into the store
  via `updateLastAssistantText`; on final message, append the assistant
  message (full content blocks) to the store; if `stop_reason ===
  "tool_use"`, run all tool calls (parallel-safe with `Promise.all`),
  append ONE user message containing all `tool_result` blocks to the
  store, and continue the loop. Stop on `end_turn` (or `max_tokens` —
  append a note), hard cap 20 iterations per turn.
- Set `pendingTurn` at turn start, clear on completion — every append
  already persists via the store, so an interrupted turn's partial state
  is durable by construction.
- Resume: in ChatView, if a loaded session has `pendingTurn` set and its
  last message is a tool_result user message (or an assistant tool_use
  missing results), show a "Resume turn" button that re-enters the loop
  without a new user message (executing any unanswered tool_use blocks
  first).
- Errors (network, 401, refusal stop_reason): append an assistant text
  message describing the failure plainly; never lose the history.

### 5. Active-book picker (minimal)

In ChatView's header: if `session.activeBook` is unset show two text
inputs (work id, doc slug) + a set button; when set, show title/slug with
an edit affordance. No search UI — that's later. Title may be fetched
from `GET <base>/works/<id>` best-effort.

### 6. Dependency

Add `@anthropic-ai/sdk` to `web/package.json` (regular dependency,
current version).

## Files to Modify

- `web/package.json` — add SDK
- `web/src/plugins/chat/types.ts` — additive session fields
- `web/src/plugins/chat/tools.ts` — new
- `web/src/plugins/chat/prompt.ts` — new (assembly + context-file fetch)
- `web/src/plugins/chat/transport.ts` — new (loop)
- `web/src/plugins/chat/ChatView.tsx` — transport selection, resume
  button, active-book header
- `web/src/plugins/chat/store.ts` — only if a small mutation helper is
  missing (e.g. setting activeBook/pendingTurn)

## Verification

```bash
pnpm -C web install
pnpm -C web build
```

## Out of Scope

- Extended tool inventory (pwa-tools-extended)
- PWA manifest/hosting; prompt-caching beyond the single breakpoint;
  thinking-block storage; search-based book picker

## Notes

- The worker L3 + agent-file surfaces this builds on are the merged
  l3-anywhere chain plus the worker-l3-agent-files phase preceding this
  one in the chain (Surface: GET/PUT /api/l3/docs/{slug} with
  normalization + warnings/409, GET /api/l3/agent/{name} | 404).
- Store CORS is already deployed. OpenAlex sends
  `Access-Control-Allow-Origin: *`.
- Keep tool result strings compact — they are re-sent every turn.

## Surface after this phase

- `anthropicTransport` implements the chat plugin's `Transport`; ChatView
  uses it when an API key is configured, stub otherwise.
- Sessions carry `activeBook` and survive mid-turn interruption with a
  working resume path.
- The six tools exist with the exact names/schemas above; system prompt =
  bundled core + fetched principles/memory + session context.
- Negative space: chat shell UI structure, settings keys, store
  persistence format for existing fields — unchanged; no server/worker
  changes in this phase.
