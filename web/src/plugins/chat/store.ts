import { createStore, produce } from "solid-js/store";
import type { ActiveBook, ChatMessage, ChatSession, PendingTurn } from "./types";

const SESSION_KEY_PREFIX = "bc.chat.session.";
const SESSIONS_INDEX_KEY = "bc.chat.sessions";

interface ChatStoreState {
  sessions: Record<string, ChatSession>;
  order: string[];
}

const [state, setState] = createStore<ChatStoreState>({ sessions: {}, order: [] });

let hydrated = false;

function readIndex(): string[] {
  try {
    const raw = localStorage.getItem(SESSIONS_INDEX_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    return Array.isArray(parsed) ? (parsed as string[]) : [];
  } catch (err) {
    console.warn("chat store: failed to read session index", err);
    return [];
  }
}

function readSession(id: string): ChatSession | null {
  try {
    const raw = localStorage.getItem(SESSION_KEY_PREFIX + id);
    if (!raw) return null;
    return JSON.parse(raw) as ChatSession;
  } catch (err) {
    console.warn(`chat store: failed to read session ${id}`, err);
    return null;
  }
}

function writeIndex(order: string[]): void {
  try {
    localStorage.setItem(SESSIONS_INDEX_KEY, JSON.stringify(order));
  } catch (err) {
    console.warn("chat store: failed to persist session index", err);
  }
}

function writeSession(session: ChatSession): void {
  try {
    localStorage.setItem(SESSION_KEY_PREFIX + session.id, JSON.stringify(session));
  } catch (err) {
    console.warn(`chat store: failed to persist session ${session.id}`, err);
  }
}

function removeSession(id: string): void {
  try {
    localStorage.removeItem(SESSION_KEY_PREFIX + id);
  } catch (err) {
    console.warn(`chat store: failed to remove session ${id}`, err);
  }
}

function hydrate(): void {
  if (hydrated) return;
  hydrated = true;
  const order = readIndex();
  const sessions: Record<string, ChatSession> = {};
  const validOrder: string[] = [];
  for (const id of order) {
    const session = readSession(id);
    if (session) {
      sessions[id] = session;
      validOrder.push(id);
    }
  }
  setState({ sessions, order: validOrder });
}

function newId(): string {
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

function nowIso(): string {
  return new Date().toISOString();
}

export function useChatStore(): {
  sessions: Record<string, ChatSession>;
  order: string[];
} {
  hydrate();
  return state;
}

/** Live snapshot of one session, read fresh from the store each call. */
export function getSession(id: string): ChatSession | undefined {
  hydrate();
  return state.sessions[id];
}

export function createSession(title = "New session"): ChatSession {
  hydrate();
  const id = newId();
  const timestamp = nowIso();
  const session: ChatSession = {
    id,
    title,
    createdAt: timestamp,
    updatedAt: timestamp,
    messages: [],
  };
  setState(
    produce((s) => {
      s.sessions[id] = session;
      s.order.unshift(id);
    }),
  );
  writeSession(session);
  writeIndex(state.order);
  return session;
}

export function deleteSession(id: string): void {
  hydrate();
  setState(
    produce((s) => {
      delete s.sessions[id];
      s.order = s.order.filter((sid) => sid !== id);
    }),
  );
  removeSession(id);
  writeIndex(state.order);
}

export function renameSession(id: string, title: string): void {
  hydrate();
  if (!state.sessions[id]) return;
  setState(
    produce((s) => {
      s.sessions[id].title = title;
      s.sessions[id].updatedAt = nowIso();
    }),
  );
  writeSession(state.sessions[id]);
}

export function appendMessage(id: string, msg: ChatMessage): void {
  hydrate();
  if (!state.sessions[id]) return;
  setState(
    produce((s) => {
      s.sessions[id].messages.push(msg);
      s.sessions[id].updatedAt = nowIso();
    }),
  );
  writeSession(state.sessions[id]);
}

export function setActiveBook(id: string, activeBook: ActiveBook): void {
  hydrate();
  if (!state.sessions[id]) return;
  setState(
    produce((s) => {
      s.sessions[id].activeBook = activeBook;
      s.sessions[id].updatedAt = nowIso();
    }),
  );
  writeSession(state.sessions[id]);
}

export function setPendingTurn(id: string, pendingTurn: PendingTurn | undefined): void {
  hydrate();
  if (!state.sessions[id]) return;
  setState(
    produce((s) => {
      s.sessions[id].pendingTurn = pendingTurn;
    }),
  );
  writeSession(state.sessions[id]);
}

/**
 * Replace the streaming-in-progress assistant message (created by
 * `updateLastAssistantText`) with the API's final content blocks, or append a new
 * message if none was started (e.g. a turn that produced no text deltas).
 */
export function finalizeAssistantMessage(id: string, content: ChatMessage["content"]): void {
  hydrate();
  if (!state.sessions[id]) return;
  setState(
    produce((s) => {
      const messages = s.sessions[id].messages;
      const last = messages[messages.length - 1];
      if (last && last.role === "assistant") {
        last.content = content;
      } else {
        messages.push({ role: "assistant", content });
      }
      s.sessions[id].updatedAt = nowIso();
    }),
  );
  writeSession(state.sessions[id]);
}

export function updateLastAssistantText(id: string, text: string): void {
  hydrate();
  const session = state.sessions[id];
  if (!session) return;
  setState(
    produce((s) => {
      const messages = s.sessions[id].messages;
      let last = messages[messages.length - 1];
      if (!last || last.role !== "assistant") {
        last = { role: "assistant", content: [{ type: "text", text: "" }] };
        messages.push(last);
      }
      const block = last.content[0];
      if (block && block.type === "text") {
        block.text = text;
      } else {
        last.content.unshift({ type: "text", text });
      }
      s.sessions[id].updatedAt = nowIso();
    }),
  );
  writeSession(state.sessions[id]);
}
