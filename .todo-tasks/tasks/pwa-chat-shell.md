# Web UI: chat session plugin (shell + state, stub transport)

## Motivation

The reading-companion needs a chat surface in the existing SolidJS web UI.
Decision on record: SolidJS, NO chat-management libraries — a hand-rolled
Solid store owns session state. The LLM transport (Anthropic API + tool
loop) lands in a follow-up task; this task builds the UI, the session
store with persistence, and a stub transport interface the next task
implements.

## Do NOT

- Do NOT add any chat/LLM library (no Vercel `ai`, no deep-chat, and not
  `@anthropic-ai/sdk` yet — the transport here is a stub interface).
- Do NOT make any network calls to Anthropic or the braincrawl API from
  this task's code.
- Do NOT touch anything outside `web/`.
- Do NOT restructure the router or the plugin API (`web/src/plugins/types.ts`
  stays as-is: `Plugin { id, routes, nav? }`).
- Do NOT break the two existing plugins (document-index, reading-list) —
  they must render exactly as before when the graph loads.

## Plan

### 1. Un-gate the Shell from the graph load

`web/src/App.tsx` `Shell` currently wraps ALL routes in a `Switch` that
blocks until `/api/l3/graph` loads (and shows an error page if it fails).
The chat plugin must work even when the graph endpoint is absent (future
worker deploy). Refactor:

- Add `web/src/components/RequireGraph.tsx`: a component that renders the
  current Loading/error/success `Switch` (move that logic here from
  `Shell`) around its `props.children`, using `useGraph()`.
- `Shell` renders `props.children` directly (keep header/nav/Refresh as-is).
- Wrap the route components of `document-index.tsx` and `reading-list.tsx`
  in `RequireGraph` so their behavior is unchanged.

### 2. Chat data model — `web/src/plugins/chat/types.ts`

Define local types structurally compatible with the Anthropic Messages API
content blocks (the next task will feed real API data through them):

- `TextBlock { type: "text"; text: string }`
- `ToolUseBlock { type: "tool_use"; id: string; name: string; input: unknown }`
- `ToolResultBlock { type: "tool_result"; tool_use_id: string; content: string; is_error?: boolean }`
- `ContentBlock = TextBlock | ToolUseBlock | ToolResultBlock`
- `ChatMessage { role: "user" | "assistant"; content: ContentBlock[] }`
- `ChatSession { id: string; title: string; createdAt: string; updatedAt: string; messages: ChatMessage[] }`
- `Transport { sendTurn(session: ChatSession, onDelta: (text: string) => void): Promise<ChatMessage[]> }`
  — the seam the agent-loop task implements. Ship a `stubTransport` that
  resolves one assistant message: "Transport not configured — agent loop
  arrives in a later task."

### 3. Session store — `web/src/plugins/chat/store.ts`

Hand-rolled Solid store (use `createStore` from solid-js/store):

- State: `sessions: Record<string, ChatSession>`, `order: string[]`.
- Actions: `createSession(title?)`, `deleteSession(id)`, `renameSession`,
  `appendMessage(id, msg)`, `updateLastAssistantText(id, text)` (for
  streaming append later).
- Persistence: localStorage, one key per session
  (`bc.chat.session.<id>`) plus an index key (`bc.chat.sessions`). Persist
  on every mutation. Hydrate lazily on first access. Wrap all storage
  access in try/catch so a quota error degrades to in-memory (console.warn,
  don't crash).
- Settings module (same file or `settings.ts`): typed get/set over
  localStorage keys `bc.settings.anthropicKey`, `bc.settings.storeToken`,
  `bc.settings.storeBaseUrl` (default `""` = same origin),
  `bc.settings.model` (default `"claude-opus-4-8"`).

### 4. UI — `web/src/plugins/chat/` components

- `SessionList.tsx` (route `/chat`): list sessions (title, updatedAt,
  message count), "New session" button → navigates to `/chat/:id`, delete
  button per row. Reuse existing table styling (`DataTable.tsx`) where it
  fits; plain markup is fine.
- `ChatView.tsx` (route `/chat/:id`): message list rendering each block
  type — text as paragraphs; tool_use as a collapsed one-liner
  ("⚙ tool_name(...)" expandable to pretty-printed JSON input); tool_result
  collapsed similarly, red-tinted when `is_error`. Textarea + Send button:
  appends the user message, calls the transport, appends the returned
  assistant message(s); disable input while a turn is in flight. Enter
  sends, Shift+Enter newlines. Auto-scroll to bottom on new content.
- `Settings.tsx` (route `/settings`): form fields for the four settings;
  the key fields use `type="password"` with a show toggle; saved on change;
  a note that keys live in this browser's localStorage only.

### 5. Plugin registration

- `web/src/plugins/chat/index.tsx` exports `chat: Plugin` with routes
  `/chat`, `/chat/:id`, `/settings` and nav entry `{ label: "Chat",
  path: "/chat" }`. (Settings gets no nav entry of its own; link to it from
  the SessionList header.)
- Register in `web/src/plugins/index.ts`.

## Files to Modify

- `web/src/App.tsx` — Shell un-gating
- `web/src/components/RequireGraph.tsx` — new
- `web/src/plugins/document-index.tsx`, `web/src/plugins/reading-list.tsx`
  — wrap in RequireGraph
- `web/src/plugins/chat/{types.ts,store.ts,index.tsx,SessionList.tsx,ChatView.tsx,Settings.tsx}` — new
- `web/src/plugins/index.ts` — register plugin
- `web/src/App.css` — minimal styles for chat bubbles/blocks if needed

## Verification

```bash
pnpm -C web install
pnpm -C web build
```

## Out of Scope

- Real Anthropic transport, tool loop, system prompt (task pwa-agent-loop)
- PWA manifest / service worker (task pwa-hosting)
- IndexedDB (localStorage is the minimal viable store; revisit if sessions
  outgrow quota)

## Notes

- `vp` is the Vite+ CLI; `pnpm -C web build` runs `tsc -b && vp build`, so
  the build doubles as the typecheck gate.
- Keep the content-block types local (no SDK import) but field-for-field
  compatible with Anthropic's wire shapes — the next task must not need to
  remodel stored sessions.

## Surface after this phase

- Plugin `chat` registered with routes `/chat`, `/chat/:id`, `/settings`
  and nav entry "Chat".
- `web/src/plugins/chat/types.ts` exports `TextBlock`, `ToolUseBlock`,
  `ToolResultBlock`, `ContentBlock`, `ChatMessage`, `ChatSession`,
  `Transport`, `stubTransport`.
- `web/src/plugins/chat/store.ts` exports the session store with actions
  `createSession`, `deleteSession`, `renameSession`, `appendMessage`,
  `updateLastAssistantText`, all persisting to localStorage
  (`bc.chat.*` keys), plus settings accessors for
  `bc.settings.{anthropicKey,storeToken,storeBaseUrl,model}`.
- `ChatView` calls whatever `Transport` it is given; swapping
  `stubTransport` for a real one is the only integration point the
  agent-loop task needs.
- `web/src/components/RequireGraph.tsx` exists; `Shell` no longer blocks
  routes on the graph load; document-index and reading-list behave as
  before.
- Negative space: plugin API (`types.ts`), router setup, vite config, and
  everything outside `web/` unchanged.
