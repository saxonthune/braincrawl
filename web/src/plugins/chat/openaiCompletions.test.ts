import { describe, expect, it } from "vite-plus/test";
import {
  applyToolCallDeltas,
  finalizeToolCalls,
  fromFinishReason,
  toOpenAiMessages,
  type ToolCallAccumulator,
} from "./openaiCompletions";
import type { ChatMessage, SystemBlockParam } from "./types";

describe("toOpenAiMessages", () => {
  it("round-trips a session with text, tool_use, and tool_result", () => {
    const system: SystemBlockParam[] = [
      { type: "text", text: "core prompt" },
      { type: "text", text: "session context" },
    ];
    const messages: ChatMessage[] = [
      { role: "user", content: [{ type: "text", text: "find works about X" }] },
      {
        role: "assistant",
        content: [
          { type: "text", text: "searching now" },
          { type: "tool_use", id: "call_1", name: "openalex_search", input: { query: "X" } },
        ],
      },
      {
        role: "user",
        content: [{ type: "tool_result", tool_use_id: "call_1", content: '{"count":1}' }],
      },
      { role: "assistant", content: [{ type: "text", text: "found one result" }] },
    ];

    const out = toOpenAiMessages(system, messages);

    expect(out).toEqual([
      { role: "system", content: "core prompt\n\nsession context" },
      { role: "user", content: "find works about X" },
      {
        role: "assistant",
        content: "searching now",
        tool_calls: [
          {
            id: "call_1",
            type: "function",
            function: { name: "openalex_search", arguments: JSON.stringify({ query: "X" }) },
          },
        ],
      },
      { role: "tool", tool_call_id: "call_1", content: '{"count":1}' },
      { role: "assistant", content: "found one result", tool_calls: undefined },
    ]);
  });

  it("emits tool messages before a co-occurring user text block", () => {
    const messages: ChatMessage[] = [
      {
        role: "user",
        content: [
          { type: "tool_result", tool_use_id: "call_1", content: "ok" },
          { type: "text", text: "also please do Y" },
        ],
      },
    ];

    const out = toOpenAiMessages([], messages);

    expect(out).toEqual([
      { role: "tool", tool_call_id: "call_1", content: "ok" },
      { role: "user", content: "also please do Y" },
    ]);
  });
});

describe("tool-call fragment assembly", () => {
  it("assembles a well-formed ToolUseBlock from split fragments", () => {
    const acc = new Map<number, ToolCallAccumulator>();
    applyToolCallDeltas(acc, [{ index: 0, id: "call_1", function: { name: "openalex_search" } }]);
    applyToolCallDeltas(acc, [{ index: 0, function: { arguments: '{"query":' } }]);
    applyToolCallDeltas(acc, [{ index: 0, function: { arguments: '"X"}' } }]);

    const blocks = finalizeToolCalls(acc);

    expect(blocks).toEqual([
      { type: "tool_use", id: "call_1", name: "openalex_search", input: { query: "X" } },
    ]);
  });

  it("falls back to an empty object when arguments JSON is malformed", () => {
    const acc = new Map<number, ToolCallAccumulator>();
    applyToolCallDeltas(acc, [
      { index: 0, id: "call_1", function: { name: "broken", arguments: "{not json" } },
    ]);

    const blocks = finalizeToolCalls(acc);

    expect(blocks).toEqual([{ type: "tool_use", id: "call_1", name: "broken", input: {} }]);
  });
});

describe("fromFinishReason", () => {
  it.each([
    ["tool_calls", "tool_use"],
    ["stop", "end_turn"],
    ["length", "max_tokens"],
    ["content_filter", "other"],
    [undefined, "other"],
    [null, "other"],
  ] as const)("maps %s to %s", (reason: string | null | undefined, expected: string) => {
    expect(fromFinishReason(reason)).toBe(expected);
  });
});
