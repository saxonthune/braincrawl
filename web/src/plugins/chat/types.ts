import { appendMessage } from "./store";

export interface TextBlock {
  type: "text";
  text: string;
}

export interface ToolUseBlock {
  type: "tool_use";
  id: string;
  name: string;
  input: unknown;
}

export interface ToolResultBlock {
  type: "tool_result";
  tool_use_id: string;
  content: string;
  is_error?: boolean;
}

export type ContentBlock = TextBlock | ToolUseBlock | ToolResultBlock;

export interface ChatMessage {
  role: "user" | "assistant";
  content: ContentBlock[];
}

export interface ActiveBook {
  workId: string;
  docSlug: string;
  title: string;
}

export interface PendingTurn {
  messagesSnapshotAt: string;
}

export interface ChatSession {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  messages: ChatMessage[];
  activeBook?: ActiveBook;
  pendingTurn?: PendingTurn;
}

/**
 * A turn appends every message it produces to the store itself (so an interrupted
 * turn's partial state is durable) and returns those same messages for convenience.
 */
export interface Transport {
  sendTurn(session: ChatSession, onDelta: (text: string) => void): Promise<ChatMessage[]>;
}

export const stubTransport: Transport = {
  async sendTurn(session: ChatSession): Promise<ChatMessage[]> {
    const reply: ChatMessage = {
      role: "assistant",
      content: [
        {
          type: "text",
          text: "Transport not configured — set an Anthropic API key in Settings to enable the agent loop.",
        },
      ],
    };
    appendMessage(session.id, reply);
    return [reply];
  },
};
