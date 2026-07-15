# Chat display model — parts projection, streaming, thinking indicator

## Motivation

The chat UI renders the Anthropic Messages wire format directly, causing three problems:

1. **Tool results are labeled "USER".** The Messages API requires `tool_result` blocks to
   live inside a `user`-role message. `MessageView` in `ChatView.tsx` switches on
   `message.role`, so those blocks inherit the "USER" label. Tool activity should render on
   its own visual track, not attributed to the user.
2. **Streaming does not surface.** `updateLastAssistantText` (store.ts) already creates a
   placeholder assistant message when none exists, and the transport calls it on every text
   delta — so the text reaches the store. The task is to make it visibly stream and to give
   feedback during the non-text phases of the loop (tool execution, between iterations).
3. **No thinking indicator.** `ChatView` has a `sending()` signal but never renders it.

The fix borrows the Vercel AI SDK's core idea — a message is a list of typed *parts*,
rendered by part type rather than wire role — hand-rolled for SolidJS, no dependency. The
design principle: **loop state is a pure function of the transcript.** A `tool_use` block
with no matching `tool_result` yet IS the "running" state; you derive it, you don't store it.

## Do NOT

- Do NOT move the agent loop server-side (Worker or native Rust). Out of scope; a separate
  project. This task is client-side display only.
- Do NOT change the wire message shape stored in `ChatSession.messages` or the
  `ContentBlock` union in `types.ts` — the transport (`transport.ts`, `openaiCompletions.ts`)
  depends on that exact shape. The display model is a *projection over* the wire messages,
  never a replacement for them.
- Do NOT add `@ai-sdk/solid`, assistant-ui, CopilotKit, or any chat component library.
- Do NOT alter provider/transport logic in `transport.ts` or `openaiCompletions.ts` beyond
  what is strictly needed to expose streaming/turn state to the view (prefer exposing state
  via the store + a signal rather than editing the transport at all).
- Do NOT render reasoning/thinking tokens — the models over these paths do not emit them.

## Plan

### 1. Add the display-model types and projection (`web/src/plugins/chat/display.ts`, new)

Define a discriminated union and a pure projection:

```ts
export type ToolState =
  | { status: "running"; input: unknown }
  | { status: "ok"; input: unknown; output: string }
  | { status: "error"; input: unknown; output: string };

export type DisplayItem =
  | { kind: "user-text"; text: string }
  | { kind: "assistant-text"; text: string; streaming: boolean }
  | { kind: "tool-activity"; toolName: string; input: unknown; state: ToolState }
  | { kind: "notice"; text: string; tone: "error" | "info" };

export type TurnStatus = "idle" | "streaming" | "running-tools" | "error";

export function toDisplayItems(
  session: ChatSession,
  opts: { streaming: boolean },
): DisplayItem[];
```

`toDisplayItems` walks `session.messages` in order and emits items:

- `user` message text blocks → `user-text` (skip a `user` message that is purely tool
  results — those become `tool-activity`, see below).
- `assistant` message text blocks → `assistant-text`. Mark `streaming: true` only on the
  **last** item of the **last** assistant message when `opts.streaming` is true and that
  message is the final message in the session.
- Each `tool_use` block (in an assistant message) → a `tool-activity` item. Pair it with its
  `tool_result` by matching `ToolUseBlock.id === ToolResultBlock.tool_use_id`, searching the
  subsequent `user` message(s). Map to `ToolState`: no result found → `running`; result with
  `is_error` truthy → `error` (output = result.content); otherwise → `ok` (output =
  result.content). Emit the `tool-activity` item at the position of the `tool_use` so call and
  result render together on one track, in loop order.
- Any assistant text that is a bracketed system notice already emitted by the loop
  (e.g. `[response truncated: max_tokens reached]`, `[stopped: reached the per-turn tool-call
  iteration cap]`, `Turn failed: ...`) → `notice`. Keep this heuristic small and documented;
  it only needs to catch the notice strings the loop actually produces (grep `transport.ts`
  for the literals). If unsure, leave it as `assistant-text` rather than mis-classifying.

Keep the function pure (no store access, no side effects) so it is unit-testable.

### 2. Add a co-located unit test (`web/src/plugins/chat/display.test.ts`, new)

Follow the existing vitest style in `web/src` (see `web/src/services/*.test.ts`). Cover:

- A `tool_use` with no matching result projects to `tool-activity` with `state.status === "running"`.
- A `tool_use` paired with a non-error `tool_result` → `ok` with the output text.
- A `tool_use` paired with an `is_error` result → `error`.
- A `user` message that is only `tool_result` blocks does NOT emit a `user-text` item.
- `streaming: true` marks only the final assistant text item, and only when it is the last message.
- Ordering: for the screenshot case (assistant text, then two tool_use, then a user message
  with two tool_results), items come out as user-text?/assistant-text, tool-activity,
  tool-activity — never a `user-text` for the tool-result-only message.

### 3. Rewrite the view to render display items (`web/src/plugins/chat/ChatView.tsx`)

- Replace the `MessageView` / `ContentBlockView` role-based rendering with a render over
  `toDisplayItems(session(), { streaming: sending() })`. Render by `item.kind`:
  - `user-text` → the user bubble.
  - `assistant-text` → the assistant bubble; when `streaming` is true, show a caret/pulse.
  - `tool-activity` → its own track (reuse the existing `.chat-block-tool` /
    `.chat-block-tool-result` styles), with an expand toggle showing `input` and, when
    present, `output`. A `running` state shows a spinner/"…" instead of a result.
  - `notice` → the existing error/notice styling (`.chat-block-error` for `tone === "error"`).
- Derive a `TurnStatus` for the top-level indicator: while `sending()` is true, show a
  "thinking…" indicator; if the last display item is a `tool-activity` in `running` state,
  prefer "running tool…". Render this near the input or above the message list.
- Keep the existing resume banner and input behavior unchanged.

### 4. CSS (`web/src/App.css`)

Add minimal styles for the streaming caret and the thinking indicator. Match the existing
`.chat-*` conventions and CSS variables. Do NOT restructure existing chat styles.

## Files to Modify

- `web/src/plugins/chat/display.ts` — NEW: types + `toDisplayItems` projection.
- `web/src/plugins/chat/display.test.ts` — NEW: unit tests for the projection.
- `web/src/plugins/chat/ChatView.tsx` — render by display item; add thinking/status indicator.
- `web/src/App.css` — streaming caret + thinking indicator styles.

## Verification

```bash
cd web && vp check && vp test run
```

All type-checks pass, formatting clean, and the new `display.test.ts` cases pass alongside
the existing suite. Manually confirm in the running app (`http://localhost:8787`) that a
tool-heavy turn shows tool activity on its own track (no "USER" label), a thinking indicator
appears while a turn is in flight, and assistant text streams in.

## Out of Scope

- Server-side agent loop (Worker or native).
- Durable/resumable server-persisted transcripts.
- Reasoning-token rendering.

## Notes

- Wire field names (from `types.ts`): `ToolUseBlock` = `{ type, id, name, input }`;
  `ToolResultBlock` = `{ type, tool_use_id, content, is_error? }`; `TextBlock` = `{ type, text }`;
  `ChatMessage` = `{ role, content: ContentBlock[] }`. Pairing key: `tool_use.id` ===
  `tool_result.tool_use_id`.
- The loop's notice literals live in `transport.ts` (`runLoop`): "[response truncated: …]",
  "[stopped: reached the per-turn tool-call iteration cap]", "Turn failed: …", "[the model
  declined to continue this response]".
- If, while verifying streaming, text still does not visibly update, the likely cause is the
  per-delta `writeSession` (full-session `JSON.stringify` to localStorage on every token) in
  `updateLastAssistantText` — note it for a follow-up; do not rearchitect persistence here.
- A concurrent session has uncommitted work in `web/`. This task runs in its own worktree,
  so that is isolated, but expect a merge afterward.
