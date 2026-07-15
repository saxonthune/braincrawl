import { describe, expect, it } from "vite-plus/test";
import { toDisplayItems } from "./display";
import type { ChatSession } from "./types";

function session(messages: ChatSession["messages"]): ChatSession {
  return {
    id: "s1",
    title: "Test",
    createdAt: "2026-01-01T00:00:00.000Z",
    updatedAt: "2026-01-01T00:00:00.000Z",
    messages,
  };
}

describe("toDisplayItems", () => {
  it("projects a tool_use with no matching result as running", () => {
    const s = session([
      {
        role: "assistant",
        content: [{ type: "tool_use", id: "t1", name: "search", input: { q: "x" } }],
      },
    ]);
    const items = toDisplayItems(s, { streaming: false });
    expect(items).toEqual([
      {
        kind: "tool-activity",
        toolName: "search",
        input: { q: "x" },
        state: { status: "running", input: { q: "x" } },
      },
    ]);
  });

  it("projects a tool_use paired with a non-error result as ok", () => {
    const s = session([
      {
        role: "assistant",
        content: [{ type: "tool_use", id: "t1", name: "search", input: { q: "x" } }],
      },
      {
        role: "user",
        content: [{ type: "tool_result", tool_use_id: "t1", content: "found it" }],
      },
    ]);
    const items = toDisplayItems(s, { streaming: false });
    expect(items).toEqual([
      {
        kind: "tool-activity",
        toolName: "search",
        input: { q: "x" },
        state: { status: "ok", input: { q: "x" }, output: "found it" },
      },
    ]);
  });

  it("projects a tool_use paired with an is_error result as error", () => {
    const s = session([
      {
        role: "assistant",
        content: [{ type: "tool_use", id: "t1", name: "search", input: {} }],
      },
      {
        role: "user",
        content: [{ type: "tool_result", tool_use_id: "t1", content: "boom", is_error: true }],
      },
    ]);
    const items = toDisplayItems(s, { streaming: false });
    expect(items).toEqual([
      {
        kind: "tool-activity",
        toolName: "search",
        input: {},
        state: { status: "error", input: {}, output: "boom" },
      },
    ]);
  });

  it("does not emit user-text for a tool-result-only user message", () => {
    const s = session([
      {
        role: "assistant",
        content: [{ type: "tool_use", id: "t1", name: "search", input: {} }],
      },
      {
        role: "user",
        content: [{ type: "tool_result", tool_use_id: "t1", content: "ok" }],
      },
    ]);
    const items = toDisplayItems(s, { streaming: false });
    expect(items.some((i) => i.kind === "user-text")).toBe(false);
  });

  it("marks streaming true only on the final assistant text item of the last message", () => {
    const s = session([
      { role: "user", content: [{ type: "text", text: "hi" }] },
      { role: "assistant", content: [{ type: "text", text: "partial" }] },
    ]);
    const items = toDisplayItems(s, { streaming: true });
    expect(items).toEqual([
      { kind: "user-text", text: "hi" },
      { kind: "assistant-text", text: "partial", streaming: true },
    ]);
  });

  it("does not mark streaming on an earlier assistant message", () => {
    const s = session([
      { role: "assistant", content: [{ type: "text", text: "first" }] },
      { role: "user", content: [{ type: "text", text: "hi" }] },
      { role: "assistant", content: [{ type: "text", text: "second" }] },
    ]);
    const items = toDisplayItems(s, { streaming: true });
    expect(items).toEqual([
      { kind: "assistant-text", text: "first", streaming: false },
      { kind: "user-text", text: "hi" },
      { kind: "assistant-text", text: "second", streaming: true },
    ]);
  });

  it("orders assistant text then tool-activity items, never emitting user-text for the result message", () => {
    const s = session([
      { role: "user", content: [{ type: "text", text: "do it" }] },
      {
        role: "assistant",
        content: [
          { type: "text", text: "on it" },
          { type: "tool_use", id: "t1", name: "a", input: {} },
          { type: "tool_use", id: "t2", name: "b", input: {} },
        ],
      },
      {
        role: "user",
        content: [
          { type: "tool_result", tool_use_id: "t1", content: "res1" },
          { type: "tool_result", tool_use_id: "t2", content: "res2" },
        ],
      },
    ]);
    const items = toDisplayItems(s, { streaming: false });
    expect(items.map((i) => i.kind)).toEqual([
      "user-text",
      "assistant-text",
      "tool-activity",
      "tool-activity",
    ]);
  });

  it("classifies known loop notice strings as notice items", () => {
    const s = session([
      {
        role: "assistant",
        content: [{ type: "text", text: "[stopped: reached the per-turn tool-call iteration cap]" }],
      },
    ]);
    const items = toDisplayItems(s, { streaming: false });
    expect(items).toEqual([
      {
        kind: "notice",
        text: "[stopped: reached the per-turn tool-call iteration cap]",
        tone: "error",
      },
    ]);
  });
});
