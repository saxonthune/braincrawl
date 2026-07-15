import type { ChatMessage, ChatSession, TextBlock, ToolResultBlock, ToolUseBlock } from "./types";

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

// Literals the loop appends as assistant text notices — see transport.ts (runLoop).
const NOTICE_PREFIXES = [
  "[response truncated: max_tokens reached]",
  "[stopped: reached the per-turn tool-call iteration cap]",
  "[the model declined to continue this response]",
];

function isNotice(text: string): boolean {
  return NOTICE_PREFIXES.includes(text) || text.startsWith("Turn failed: ");
}

function findToolResult(
  messages: ChatMessage[],
  fromIndex: number,
  toolUseId: string,
): ToolResultBlock | undefined {
  for (let i = fromIndex; i < messages.length; i++) {
    const message = messages[i];
    if (message.role !== "user") continue;
    for (const block of message.content) {
      if (block.type === "tool_result" && block.tool_use_id === toolUseId) {
        return block;
      }
    }
  }
  return undefined;
}

function toolState(result: ToolResultBlock | undefined, input: unknown): ToolState {
  if (!result) return { status: "running", input };
  if (result.is_error) return { status: "error", input, output: result.content };
  return { status: "ok", input, output: result.content };
}

export function toDisplayItems(session: ChatSession, opts: { streaming: boolean }): DisplayItem[] {
  const messages = session.messages;
  const items: DisplayItem[] = [];

  messages.forEach((message, messageIndex) => {
    const isLastMessage = messageIndex === messages.length - 1;

    if (message.role === "user") {
      const hasToolResult = message.content.some((b) => b.type === "tool_result");
      if (hasToolResult) return;
      for (const block of message.content) {
        if (block.type === "text") {
          items.push({ kind: "user-text", text: (block as TextBlock).text });
        }
      }
      return;
    }

    // assistant message
    const textBlocks = message.content.filter((b): b is TextBlock => b.type === "text");
    textBlocks.forEach((block, blockIndex) => {
      const isLastBlock = blockIndex === textBlocks.length - 1;
      if (isNotice(block.text)) {
        items.push({ kind: "notice", text: block.text, tone: "error" });
        return;
      }
      const streaming = opts.streaming && isLastMessage && isLastBlock;
      items.push({ kind: "assistant-text", text: block.text, streaming });
    });

    for (const block of message.content) {
      if (block.type !== "tool_use") continue;
      const use = block as ToolUseBlock;
      const result = findToolResult(messages, messageIndex + 1, use.id);
      items.push({
        kind: "tool-activity",
        toolName: use.name,
        input: use.input,
        state: toolState(result, use.input),
      });
    }
  });

  return items;
}
