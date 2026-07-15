# Web UI: OpenAI-format transport for non-Anthropic models via OpenRouter

## Motivation

The chat agent can now bill through OpenRouter, but only for Anthropic
models — the transport speaks the Anthropic Messages format, and
OpenRouter's Anthropic-compatible endpoint guarantees Anthropic models
only. Cheap models (first target: `deepseek/deepseek-v4-flash`, ~30x
cheaper than Sonnet) need the OpenAI chat-completions format. The
`Transport` seam in `web/src/plugins/chat/` was designed for exactly
this addition.

## Do NOT

- Do NOT add an SDK dependency (no `openai` package) — the project rule
  is no LLM libraries beyond the existing Anthropic SDK; the new path is
  hand-rolled `fetch` + SSE parsing.
- Do NOT change the session storage format. `ChatMessage`/`ContentBlock`
  in `types.ts` (Anthropic block shapes) stay the canonical on-disk
  form; the new transport converts at the edge, both directions.
- Do NOT break the two existing model paths: `sk-ant-…` key → direct
  Anthropic, `sk-or-…` key + `anthropic/`- or `~anthropic/`-prefixed
  model → OpenRouter's Anthropic-compatible endpoint (see `makeClient`
  in `transport.ts`). Byte-identical behavior for those.
- Do NOT touch `tools.ts`, `store.ts`, `prompt.md`, or the store/worker.
- Do NOT restructure the agent loop's semantics — resume handling,
  MAX_ITERATIONS, pendingTurn, and the error-note messages in
  `transport.ts` keep working identically for all providers.

## Plan

### 1. Extract the per-provider model call from the loop

`web/src/plugins/chat/transport.ts` `runLoop` currently constructs an
Anthropic client and streams inline. Extract the single model call
behind a narrow internal type so the loop is provider-agnostic:

```ts
interface ModelTurnResult {
  content: ContentBlock[];
  stopReason: "end_turn" | "tool_use" | "max_tokens" | "refusal" | "other";
}
type ModelCall = (args: {
  model: string;
  system: SystemBlockParam[];   // whatever buildSystemBlocks returns
  messages: ChatMessage[];
  onText: (accumulatedSoFar: string) => void;
}) => Promise<ModelTurnResult>;
```

The loop keeps: iteration cap, resume of pending tool_use, tool
execution, stop_reason branching (map the existing branches onto
`stopReason`), error-note append on throw, pendingTurn lifecycle. The
Anthropic implementation wraps the existing `client.messages.stream`
code unchanged (including the `refusal` branch, which only Anthropic
reports).

### 2. Provider selection

In `runLoop`, replace the current bare-slug error with three-way
selection on `(apiKey, model)`:

- `sk-ant-…` → Anthropic direct (existing).
- `sk-or-…` and model starts with `anthropic/` or `~anthropic/` →
  Anthropic SDK against `https://openrouter.ai/api` (existing
  `makeClient` branch).
- `sk-or-…` and model contains `/` otherwise → new OpenAI-format call.
- `sk-or-…` and model has no `/` → keep the existing in-chat error note
  (update its wording to mention any OpenRouter slug now works, e.g.
  `deepseek/deepseek-v4-flash`).

### 3. New module: OpenAI chat-completions caller

New file `web/src/plugins/chat/openaiCompletions.ts`. Export the pure
mapping functions (for tests) and the caller:

- `toOpenAiMessages(system, messages)`:
  - system blocks → one leading `{role:"system", content}` message
    (concatenate the text of the system blocks with "\n\n"; drop
    cache_control — OpenRouter caches automatically where the provider
    supports it).
  - assistant `ChatMessage` → `{role:"assistant", content: <joined text
    blocks or null>, tool_calls?: [{id, type:"function",
    function:{name, arguments: JSON.stringify(input)}}]}` from its
    tool_use blocks.
  - user message whose blocks are tool_result → one `{role:"tool",
    tool_call_id, content}` message **per block**, in order (the store
    never mixes tool_result with user text; if a text block does
    co-occur, emit the tool messages first, then a user message).
  - plain user message → `{role:"user", content: <joined text>}`.
- `toOpenAiTools(tools)` → `[{type:"function", function:{name,
  description, parameters: input_schema}}]`.
- `fromFinishReason(reason)` → `"tool_calls"→"tool_use"`,
  `"stop"→"end_turn"`, `"length"→"max_tokens"`, anything else →
  `"other"`.
- `callOpenRouter(args) : ModelCall` — POST
  `https://openrouter.ai/api/v1/chat/completions` with headers
  `Authorization: Bearer <key>`, `Content-Type: application/json`,
  `X-Title: braincrawl`; body `{model, messages, tools, stream: true,
  max_tokens: 8192}`. Parse the SSE stream (`data: {...}` lines,
  terminated by `data: [DONE]`):
  - `delta.content` fragments accumulate into one TextBlock; call
    `onText(accumulated)` per fragment.
  - `delta.tool_calls` fragments accumulate **keyed by `index`** —
    `id` and `function.name` arrive on the first fragment,
    `function.arguments` arrives as string fragments to concatenate.
  - On stream end, assemble `content`: TextBlock (if any text) followed
    by one ToolUseBlock per accumulated call — `{type:"tool_use", id,
    name, input: JSON.parse(arguments)}`; if arguments fail to parse,
    use `{}` as input (the tool handler's own validation will surface
    the problem as a tool_result error).
  - Non-2xx response → throw with status + response body text (the
    loop's existing catch turns it into a "Turn failed" chat note).

Keep the OpenAI wire types local to this module (hand-written minimal
interfaces) — no dependency.

### 4. Settings hint

`web/src/plugins/chat/Settings.tsx` — extend the model-field hint: with
an OpenRouter key, any OpenRouter slug works; non-Anthropic example
`deepseek/deepseek-v4-flash`.

### 5. Tests

Add `"test": "vp test"` to `web/package.json` scripts. New
`web/src/plugins/chat/openaiCompletions.test.ts` (vitest, zero-config
default glob) covering:

- Round-trip mapping: a session with user text → assistant text +
  tool_use → user tool_result → assistant text maps to the expected
  OpenAI message array (system first, tool role messages with matching
  tool_call_id).
- Tool-call fragment assembly: feed the accumulator the split-fragment
  pattern (`id`+`name` first, then two `arguments` halves) and assert
  one well-formed ToolUseBlock; malformed arguments JSON yields
  `input: {}`.
- `fromFinishReason` mapping table.

Structure the SSE accumulation so the fragment-assembly logic is a pure
exported function testable without a network stream.

## Files to Modify

- `web/src/plugins/chat/transport.ts` — extract ModelCall, three-way
  provider selection
- `web/src/plugins/chat/openaiCompletions.ts` — new
- `web/src/plugins/chat/openaiCompletions.test.ts` — new
- `web/src/plugins/chat/Settings.tsx` — model hint text
- `web/package.json` — test script

## Verification

```bash
CI=true pnpm -C web install
pnpm -C web build
pnpm -C web test
```

## Out of Scope

- Reasoning/thinking parameters for reasoning models
- Provider-specific pricing display or usage tracking
- Any change to which model is "default" — the user picks slugs in
  Settings

## Notes

- OpenRouter's chat-completions endpoint is the documented OpenAI
  superset: https://openrouter.ai/docs. Streaming tool_calls follow the
  standard OpenAI delta shape.
- The existing Anthropic-path behavior after refactor is guarded only
  by the build + manual use; keep the extraction mechanical (move code,
  don't rewrite it).
