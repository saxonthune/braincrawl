import { tools } from "./tools";
import type {
  ChatMessage,
  ContentBlock,
  ModelCall,
  SystemBlockParam,
  TextBlock,
  ToolResultBlock,
  ToolUseBlock,
} from "./types";

// ── OpenAI chat-completions wire types (hand-written, no SDK dependency) ────

interface OpenAiToolCall {
  id: string;
  type: "function";
  function: { name: string; arguments: string };
}

interface OpenAiMessage {
  role: "system" | "user" | "assistant" | "tool";
  content: string | null;
  tool_calls?: OpenAiToolCall[];
  tool_call_id?: string;
}

interface OpenAiTool {
  type: "function";
  function: { name: string; description: string; parameters: unknown };
}

interface OpenAiToolCallDelta {
  index: number;
  id?: string;
  function?: { name?: string; arguments?: string };
}

// ── request mapping ──────────────────────────────────────────────────────────

function joinText(blocks: ContentBlock[]): string {
  return blocks
    .filter((b): b is TextBlock => b.type === "text")
    .map((b) => b.text)
    .join("\n\n");
}

export function toOpenAiMessages(
  system: SystemBlockParam[],
  messages: ChatMessage[],
): OpenAiMessage[] {
  const out: OpenAiMessage[] = [];

  const systemText = system.map((s) => s.text).join("\n\n");
  if (systemText) out.push({ role: "system", content: systemText });

  for (const message of messages) {
    if (message.role === "assistant") {
      const text = joinText(message.content);
      const uses = message.content.filter((b): b is ToolUseBlock => b.type === "tool_use");
      const tool_calls: OpenAiToolCall[] | undefined =
        uses.length > 0
          ? uses.map((u) => ({
              id: u.id,
              type: "function",
              function: { name: u.name, arguments: JSON.stringify(u.input ?? {}) },
            }))
          : undefined;
      out.push({ role: "assistant", content: text || null, tool_calls });
      continue;
    }

    const toolResults = message.content.filter(
      (b): b is ToolResultBlock => b.type === "tool_result",
    );
    for (const result of toolResults) {
      out.push({ role: "tool", tool_call_id: result.tool_use_id, content: result.content });
    }
    const text = joinText(message.content);
    if (text) out.push({ role: "user", content: text });
  }

  return out;
}

export function toOpenAiTools(toolList: typeof tools): OpenAiTool[] {
  return toolList.map((t) => ({
    type: "function",
    function: {
      name: t.definition.name,
      description: t.definition.description,
      parameters: t.definition.input_schema,
    },
  }));
}

export function fromFinishReason(reason: string | null | undefined): ModelTurnResultStopReason {
  if (reason === "tool_calls") return "tool_use";
  if (reason === "stop") return "end_turn";
  if (reason === "length") return "max_tokens";
  return "other";
}

type ModelTurnResultStopReason = "end_turn" | "tool_use" | "max_tokens" | "refusal" | "other";

// ── streaming tool-call fragment assembly ───────────────────────────────────

export interface ToolCallAccumulator {
  id?: string;
  name?: string;
  arguments: string;
}

export function applyToolCallDeltas(
  acc: Map<number, ToolCallAccumulator>,
  deltas: OpenAiToolCallDelta[] | undefined,
): void {
  if (!deltas) return;
  for (const delta of deltas) {
    const entry = acc.get(delta.index) ?? { arguments: "" };
    if (delta.id) entry.id = delta.id;
    if (delta.function?.name) entry.name = delta.function.name;
    if (delta.function?.arguments) entry.arguments += delta.function.arguments;
    acc.set(delta.index, entry);
  }
}

export function finalizeToolCalls(acc: Map<number, ToolCallAccumulator>): ToolUseBlock[] {
  return Array.from(acc.entries())
    .sort(([a], [b]) => a - b)
    .map(([, entry]) => {
      let input: unknown = {};
      try {
        input = entry.arguments ? JSON.parse(entry.arguments) : {};
      } catch {
        input = {};
      }
      return {
        type: "tool_use" as const,
        id: entry.id ?? "",
        name: entry.name ?? "",
        input,
      };
    });
}

// ── SSE streaming call ───────────────────────────────────────────────────────

interface OpenAiStreamChunk {
  choices?: {
    delta?: { content?: string; tool_calls?: OpenAiToolCallDelta[] };
    finish_reason?: string | null;
  }[];
}

export function callOpenAiFormat(endpoint: string, authHeaderValue: string): ModelCall {
  return async ({ model, system, messages, onText }) => {
    const res = await fetch(endpoint, {
      method: "POST",
      headers: {
        Authorization: authHeaderValue,
        "Content-Type": "application/json",
        "X-Title": "braincrawl",
      },
      body: JSON.stringify({
        model,
        messages: toOpenAiMessages(system, messages),
        tools: toOpenAiTools(tools),
        stream: true,
        max_tokens: 8192,
      }),
    });

    if (!res.ok || !res.body) {
      throw new Error(`OpenRouter request failed: ${res.status} ${await res.text()}`);
    }

    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    let accumulatedText = "";
    const toolCallAcc = new Map<number, ToolCallAccumulator>();
    let finishReason: string | null | undefined;

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });

      let newlineIndex: number;
      while ((newlineIndex = buffer.indexOf("\n")) !== -1) {
        const line = buffer.slice(0, newlineIndex).trim();
        buffer = buffer.slice(newlineIndex + 1);
        if (!line.startsWith("data:")) continue;
        const payload = line.slice("data:".length).trim();
        if (payload === "[DONE]") continue;
        let chunk: OpenAiStreamChunk;
        try {
          chunk = JSON.parse(payload) as OpenAiStreamChunk;
        } catch {
          continue;
        }
        const choice = chunk.choices?.[0];
        if (!choice) continue;
        if (choice.delta?.content) {
          accumulatedText += choice.delta.content;
          onText(accumulatedText);
        }
        applyToolCallDeltas(toolCallAcc, choice.delta?.tool_calls);
        if (choice.finish_reason) finishReason = choice.finish_reason;
      }
    }

    const content: ContentBlock[] = [];
    if (accumulatedText) content.push({ type: "text", text: accumulatedText });
    content.push(...finalizeToolCalls(toolCallAcc));

    return { content, stopReason: fromFinishReason(finishReason) };
  };
}

export function callOpenRouter(apiKey: string): ModelCall {
  return callOpenAiFormat("https://openrouter.ai/api/v1/chat/completions", `Bearer ${apiKey}`);
}
