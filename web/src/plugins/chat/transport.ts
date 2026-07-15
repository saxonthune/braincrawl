import Anthropic from "@anthropic-ai/sdk";
import { buildSystemBlocks } from "./prompt";
import { getSetting } from "./settings";
import { appendMessage, finalizeAssistantMessage, getSession, setPendingTurn, updateLastAssistantText } from "./store";
import { findTool, tools } from "./tools";
import type { ChatMessage, ChatSession, ContentBlock, ToolResultBlock, ToolUseBlock, Transport } from "./types";

const MAX_ITERATIONS = 20;

function toApiTools(): Anthropic.Tool[] {
  return tools.map((t) => ({
    name: t.definition.name,
    description: t.definition.description,
    input_schema: t.definition.input_schema as Anthropic.Tool.InputSchema,
  }));
}

function toApiMessages(messages: ChatMessage[]): Anthropic.MessageParam[] {
  return messages.map((m) => ({
    role: m.role,
    content: m.content as unknown as Anthropic.MessageParam["content"],
  }));
}

function toolUseBlocks(message: ChatMessage): ToolUseBlock[] {
  return message.content.filter((b): b is ToolUseBlock => b.type === "tool_use");
}

async function runToolCalls(uses: ToolUseBlock[]): Promise<ToolResultBlock[]> {
  const results = await Promise.all(
    uses.map(async (use): Promise<ToolResultBlock> => {
      const tool = findTool(use.name);
      if (!tool) {
        return { type: "tool_result", tool_use_id: use.id, content: `unknown tool: ${use.name}`, is_error: true };
      }
      try {
        const result = await tool.handler((use.input ?? {}) as Record<string, unknown>);
        return {
          type: "tool_result",
          tool_use_id: use.id,
          content: result.content,
          is_error: result.is_error,
        };
      } catch (err) {
        return {
          type: "tool_result",
          tool_use_id: use.id,
          content: `tool ${use.name} threw: ${String(err)}`,
          is_error: true,
        };
      }
    }),
  );
  return results;
}

// A tool_use message with no matching tool_result yet — the loop must answer it before
// continuing, whether starting fresh or resuming an interrupted turn.
function pendingToolUses(session: ChatSession): ToolUseBlock[] {
  const last = session.messages[session.messages.length - 1];
  if (!last || last.role !== "assistant") return [];
  return toolUseBlocks(last);
}

export function needsResume(session: ChatSession): boolean {
  return !!session.pendingTurn && pendingToolUses(session).length > 0;
}

async function runLoop(sessionId: string, onDelta: (text: string) => void): Promise<ChatMessage[]> {
  const settings = {
    apiKey: getSetting("anthropicKey"),
    model: getSetting("model"),
  };
  const client = new Anthropic({ apiKey: settings.apiKey, dangerouslyAllowBrowser: true });

  setPendingTurn(sessionId, { messagesSnapshotAt: new Date().toISOString() });
  const appended: ChatMessage[] = [];

  const requireSession = (): ChatSession => {
    const session = getSession(sessionId);
    if (!session) throw new Error(`session ${sessionId} disappeared mid-turn`);
    return session;
  };

  try {
    // Resume: answer any unanswered tool_use blocks from a prior interrupted iteration first.
    const initialPending = pendingToolUses(requireSession());
    if (initialPending.length > 0) {
      const resultBlocks = await runToolCalls(initialPending);
      const toolResultMessage: ChatMessage = { role: "user", content: resultBlocks };
      appendMessage(sessionId, toolResultMessage);
      appended.push(toolResultMessage);
    }

    for (let iteration = 0; iteration < MAX_ITERATIONS; iteration++) {
      const session = requireSession();
      const system = await buildSystemBlocks(session.activeBook);
      const apiMessages = toApiMessages(session.messages);

      let finalMessage: Anthropic.Message;
      try {
        const stream = client.messages.stream({
          model: settings.model,
          max_tokens: 8192,
          system: system as unknown as Anthropic.TextBlockParam[],
          tools: toApiTools(),
          messages: apiMessages,
        });
        let accumulated = "";
        stream.on("text", (delta) => {
          onDelta(delta);
          accumulated += delta;
          updateLastAssistantText(sessionId, accumulated);
        });
        finalMessage = await stream.finalMessage();
      } catch (err) {
        const message: ChatMessage = {
          role: "assistant",
          content: [{ type: "text", text: `Turn failed: ${String(err)}` }],
        };
        appendMessage(sessionId, message);
        appended.push(message);
        return appended;
      }

      const assistantMessage: ChatMessage = {
        role: "assistant",
        content: finalMessage.content as unknown as ContentBlock[],
      };
      finalizeAssistantMessage(sessionId, assistantMessage.content);
      appended.push(assistantMessage);

      if (finalMessage.stop_reason === "tool_use") {
        const uses = toolUseBlocks(assistantMessage);
        const resultBlocks = await runToolCalls(uses);
        const toolResultMessage: ChatMessage = { role: "user", content: resultBlocks };
        appendMessage(sessionId, toolResultMessage);
        appended.push(toolResultMessage);
        continue;
      }

      if (finalMessage.stop_reason === "max_tokens") {
        const note: ChatMessage = {
          role: "assistant",
          content: [{ type: "text", text: "[response truncated: max_tokens reached]" }],
        };
        appendMessage(sessionId, note);
        appended.push(note);
      } else if (finalMessage.stop_reason === "refusal") {
        const note: ChatMessage = {
          role: "assistant",
          content: [{ type: "text", text: "[the model declined to continue this response]" }],
        };
        appendMessage(sessionId, note);
        appended.push(note);
      }

      return appended;
    }

    const capNote: ChatMessage = {
      role: "assistant",
      content: [{ type: "text", text: "[stopped: reached the per-turn tool-call iteration cap]" }],
    };
    appendMessage(sessionId, capNote);
    appended.push(capNote);
    return appended;
  } finally {
    setPendingTurn(sessionId, undefined);
  }
}

export const anthropicTransport: Transport = {
  async sendTurn(session: ChatSession, onDelta: (text: string) => void): Promise<ChatMessage[]> {
    return runLoop(session.id, onDelta);
  },
};

/** Resume an interrupted turn without appending a new user message. */
export async function resumeTurn(session: ChatSession, onDelta: (text: string) => void): Promise<ChatMessage[]> {
  return runLoop(session.id, onDelta);
}
