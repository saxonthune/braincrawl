import Anthropic from "@anthropic-ai/sdk";
import { callOpenRouter } from "./openaiCompletions";
import { buildSystemBlocks } from "./prompt";
import { getSetting } from "./settings";
import { appendMessage, finalizeAssistantMessage, getSession, setPendingTurn, updateLastAssistantText } from "./store";
import { findTool, tools } from "./tools";
import type { ChatMessage, ChatSession, ContentBlock, ModelCall, ToolResultBlock, ToolUseBlock, Transport } from "./types";

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

// OpenRouter's Anthropic-compatible endpoint takes the sk-or- key as a Bearer
// token (SDK authToken) and only guarantees Anthropic models, addressed by
// OpenRouter slug (anthropic/... or ~anthropic/...-latest).
function makeClient(apiKey: string): Anthropic {
  if (apiKey.startsWith("sk-or-")) {
    return new Anthropic({ baseURL: "https://openrouter.ai/api", authToken: apiKey, dangerouslyAllowBrowser: true });
  }
  return new Anthropic({ apiKey, dangerouslyAllowBrowser: true });
}

function anthropicModelCall(apiKey: string): ModelCall {
  const client = makeClient(apiKey);
  return async ({ model, system, messages, onText }) => {
    const stream = client.messages.stream({
      model,
      max_tokens: 8192,
      system: system as unknown as Anthropic.TextBlockParam[],
      tools: toApiTools(),
      messages: toApiMessages(messages),
    });
    let accumulated = "";
    stream.on("text", (delta) => {
      accumulated += delta;
      onText(accumulated);
    });
    const finalMessage = await stream.finalMessage();
    const stopReason =
      finalMessage.stop_reason === "tool_use" ||
      finalMessage.stop_reason === "max_tokens" ||
      finalMessage.stop_reason === "refusal" ||
      finalMessage.stop_reason === "end_turn"
        ? finalMessage.stop_reason
        : "other";
    return {
      content: finalMessage.content as unknown as ContentBlock[],
      stopReason,
    };
  };
}

// Three-way provider selection on (apiKey, model): sk-ant- goes direct to Anthropic,
// sk-or- + anthropic/~anthropic model uses OpenRouter's Anthropic-compatible endpoint
// (still the Anthropic SDK path), sk-or- + any other slug uses the OpenAI-format call.
function resolveModelCall(apiKey: string, model: string): ModelCall | ChatMessage {
  if (!apiKey.startsWith("sk-or-")) {
    return anthropicModelCall(apiKey);
  }
  if (model.startsWith("anthropic/") || model.startsWith("~anthropic/")) {
    return anthropicModelCall(apiKey);
  }
  if (model.includes("/")) {
    return callOpenRouter(apiKey);
  }
  return {
    role: "assistant",
    content: [
      {
        type: "text",
        text: `OpenRouter key detected, but model "${model}" is not a valid OpenRouter slug — set Model in Settings to any OpenRouter slug, e.g. "anthropic/claude-sonnet-4.6" or "deepseek/deepseek-v4-flash".`,
      },
    ],
  };
}

async function runLoop(sessionId: string, onDelta: (text: string) => void): Promise<ChatMessage[]> {
  const settings = {
    apiKey: getSetting("anthropicKey"),
    model: getSetting("model"),
  };
  const modelCallOrNote = resolveModelCall(settings.apiKey, settings.model);
  if (!(typeof modelCallOrNote === "function")) {
    appendMessage(sessionId, modelCallOrNote);
    return [modelCallOrNote];
  }
  const modelCall = modelCallOrNote;

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

      let result: Awaited<ReturnType<ModelCall>>;
      let previousText = "";
      try {
        result = await modelCall({
          model: settings.model,
          system,
          messages: session.messages,
          onText: (accumulatedSoFar) => {
            onDelta(accumulatedSoFar.slice(previousText.length));
            previousText = accumulatedSoFar;
            updateLastAssistantText(sessionId, accumulatedSoFar);
          },
        });
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
        content: result.content,
      };
      finalizeAssistantMessage(sessionId, assistantMessage.content);
      appended.push(assistantMessage);

      if (result.stopReason === "tool_use") {
        const uses = toolUseBlocks(assistantMessage);
        const resultBlocks = await runToolCalls(uses);
        const toolResultMessage: ChatMessage = { role: "user", content: resultBlocks };
        appendMessage(sessionId, toolResultMessage);
        appended.push(toolResultMessage);
        continue;
      }

      if (result.stopReason === "max_tokens") {
        const note: ChatMessage = {
          role: "assistant",
          content: [{ type: "text", text: "[response truncated: max_tokens reached]" }],
        };
        appendMessage(sessionId, note);
        appended.push(note);
      } else if (result.stopReason === "refusal") {
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
