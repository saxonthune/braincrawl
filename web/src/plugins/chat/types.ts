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

export interface ChatSession {
  id: string;
  title: string;
  createdAt: string;
  updatedAt: string;
  messages: ChatMessage[];
}

export interface Transport {
  sendTurn(session: ChatSession, onDelta: (text: string) => void): Promise<ChatMessage[]>;
}

export const stubTransport: Transport = {
  async sendTurn(): Promise<ChatMessage[]> {
    return [
      {
        role: "assistant",
        content: [
          {
            type: "text",
            text: "Transport not configured — agent loop arrives in a later task.",
          },
        ],
      },
    ];
  },
};
