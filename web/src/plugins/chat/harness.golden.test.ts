import { beforeEach, describe, expect, it, vi } from "vite-plus/test";
import { appendMessage, createSession, getSession } from "./store";
import { runLoop } from "./transport";
import { scriptedModelCall } from "./testing/scriptedModel";
import type { ContentBlock } from "./types";

const CANNED_STATS = { works: 42, edges: 7 };

vi.mock("../../services/store", () => ({
  storeFetch: vi.fn(async (path: string) => {
    if (path === "/stats") {
      return new Response(JSON.stringify(CANNED_STATS), { status: 200 });
    }
    return new Response("not found", { status: 404 });
  }),
}));

function startSession(userText: string) {
  const session = createSession("golden");
  appendMessage(session.id, { role: "user", content: [{ type: "text", text: userText }] });
  return session;
}

describe("runLoop golden turns", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("completes a single tool round-trip", async () => {
    const session = startSession("what's in the store?");
    const toolUse: ContentBlock = { type: "tool_use", id: "t1", name: "store_stats", input: {} };
    const model = scriptedModelCall([
      { content: [toolUse], stopReason: "tool_use" },
      {
        content: [{ type: "text", text: "there are 42 works" }],
        stopReason: "end_turn",
        text: "there are 42 works",
      },
    ]);

    await runLoop(session.id, () => {}, model);

    const final = getSession(session.id);
    expect(final?.messages.map((m) => m.role)).toEqual(["user", "assistant", "user", "assistant"]);
    expect(final?.messages[1]).toEqual({ role: "assistant", content: [toolUse] });
    expect(final?.messages[2]).toEqual({
      role: "user",
      content: [{ type: "tool_result", tool_use_id: "t1", content: JSON.stringify(CANNED_STATS) }],
    });
    expect((final?.messages[2].content[0] as { is_error?: boolean }).is_error).toBeFalsy();
    expect(final?.messages[3]).toEqual({
      role: "assistant",
      content: [{ type: "text", text: "there are 42 works" }],
    });
  });

  it("appends an is_error tool_result for an unknown tool, then completes", async () => {
    const session = startSession("do something weird");
    const model = scriptedModelCall([
      {
        content: [{ type: "tool_use", id: "t1", name: "not_a_real_tool", input: {} }],
        stopReason: "tool_use",
      },
      { content: [{ type: "text", text: "done" }], stopReason: "end_turn", text: "done" },
    ]);

    await runLoop(session.id, () => {}, model);

    const final = getSession(session.id);
    expect(final?.messages[2]).toEqual({
      role: "user",
      content: [
        {
          type: "tool_result",
          tool_use_id: "t1",
          content: "unknown tool: not_a_real_tool",
          is_error: true,
        },
      ],
    });
    expect(final?.messages.at(-1)).toEqual({
      role: "assistant",
      content: [{ type: "text", text: "done" }],
    });
  });

  it("stops at the iteration cap instead of looping forever", async () => {
    const session = startSession("loop forever");
    const turns = Array.from({ length: 20 }, (_, i) => ({
      content: [{ type: "tool_use" as const, id: `t${i}`, name: "store_stats", input: {} }],
      stopReason: "tool_use" as const,
    }));
    const model = scriptedModelCall(turns);

    await runLoop(session.id, () => {}, model);

    const final = getSession(session.id);
    expect(final?.messages.at(-1)).toEqual({
      role: "assistant",
      content: [{ type: "text", text: "[stopped: reached the per-turn tool-call iteration cap]" }],
    });
  });
});
