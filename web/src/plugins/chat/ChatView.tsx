import {
  createEffect,
  createMemo,
  createSignal,
  For,
  Match,
  Show,
  Switch,
  type JSX,
} from "solid-js";
import { useParams } from "@solidjs/router";
import { storeFetch } from "../../services/store";
import { appendMessage, setActiveBook, useChatStore } from "./store";
import { anthropicTransport, needsResume, resumeTurn } from "./transport";
import { toDisplayItems, type DisplayItem } from "./display";
import { type ChatSession, type Transport } from "./types";

function DisplayItemView(props: { item: DisplayItem }): JSX.Element {
  const [expanded, setExpanded] = createSignal(false);

  return (
    <Switch>
      <Match when={props.item.kind === "user-text"}>
        <div class="chat-message chat-message-user">
          <div class="chat-message-role">user</div>
          <p class="chat-block-text">{(props.item as { text: string }).text}</p>
        </div>
      </Match>
      <Match when={props.item.kind === "assistant-text"}>
        {(() => {
          const item = props.item as { text: string; streaming: boolean };
          return (
            <div class="chat-message chat-message-assistant">
              <div class="chat-message-role">assistant</div>
              <p class="chat-block-text">
                {item.text}
                <Show when={item.streaming}>
                  <span class="chat-stream-caret" />
                </Show>
              </p>
            </div>
          );
        })()}
      </Match>
      <Match when={props.item.kind === "tool-activity"}>
        {(() => {
          const item = props.item as Extract<DisplayItem, { kind: "tool-activity" }>;
          return (
            <div
              classList={{
                "chat-block-tool": true,
                "chat-block-error": item.state.status === "error",
              }}
            >
              <button type="button" onClick={() => setExpanded((v) => !v)}>
                ⚙ {item.toolName}(...)
                <Switch>
                  <Match when={item.state.status === "running"}>
                    <span class="chat-tool-spinner"> …running</span>
                  </Match>
                  <Match when={item.state.status === "error"}>
                    <span> (error)</span>
                  </Match>
                </Switch>
              </button>
              <Show when={expanded()}>
                <pre>{JSON.stringify(item.input, null, 2)}</pre>
                <Show when={item.state.status !== "running"}>
                  <pre>{(item.state as { output: string }).output}</pre>
                </Show>
              </Show>
            </div>
          );
        })()}
      </Match>
      <Match when={props.item.kind === "notice"}>
        {(() => {
          const item = props.item as { text: string; tone: "error" | "info" };
          return (
            <div
              classList={{
                "chat-block-tool-result": true,
                "chat-block-error": item.tone === "error",
              }}
            >
              {item.text}
            </div>
          );
        })()}
      </Match>
    </Switch>
  );
}

function ActiveBookHeader(props: { session: ChatSession }): JSX.Element {
  const [editing, setEditing] = createSignal(!props.session.activeBook);
  const [workId, setWorkId] = createSignal(props.session.activeBook?.workId ?? "");
  const [docSlug, setDocSlug] = createSignal(props.session.activeBook?.docSlug ?? "");

  const save = async () => {
    const id = workId().trim();
    const slug = docSlug().trim();
    if (!id || !slug) return;
    let title = id;
    try {
      const res = await storeFetch(`/works/${encodeURIComponent(id)}`);
      if (res.ok) {
        const work = (await res.json()) as { attrs?: { display_name?: string; title?: string } };
        title = work.attrs?.display_name ?? work.attrs?.title ?? id;
      }
    } catch {
      // best-effort title lookup; fall back to the work id
    }
    setActiveBook(props.session.id, { workId: id, docSlug: slug, title });
    setEditing(false);
  };

  return (
    <div class="chat-active-book">
      <Show
        when={!editing()}
        fallback={
          <div class="chat-active-book-form">
            <input
              type="text"
              placeholder="Work id"
              value={workId()}
              onInput={(e) => setWorkId(e.currentTarget.value)}
            />
            <input
              type="text"
              placeholder="Doc slug"
              value={docSlug()}
              onInput={(e) => setDocSlug(e.currentTarget.value)}
            />
            <button
              type="button"
              onClick={() => void save()}
              disabled={!workId().trim() || !docSlug().trim()}
            >
              Set
            </button>
          </div>
        }
      >
        <span class="chat-active-book-summary">
          📖 {props.session.activeBook?.title} ({props.session.activeBook?.docSlug})
        </span>
        <button type="button" onClick={() => setEditing(true)}>
          Edit
        </button>
      </Show>
    </div>
  );
}

export function ChatView(): JSX.Element {
  const params = useParams();
  const store = useChatStore();
  const [draft, setDraft] = createSignal("");
  const [sending, setSending] = createSignal(false);
  let scrollRef: HTMLDivElement | undefined;

  const session = () => store.sessions[params.id ?? ""];

  const displayItems = createMemo(() => {
    const current = session();
    if (!current) return [];
    return toDisplayItems(current, { streaming: sending() });
  });

  const turnStatus = createMemo<"idle" | "streaming" | "running-tools">(() => {
    if (!sending()) return "idle";
    const items = displayItems();
    const last = items[items.length - 1];
    if (last?.kind === "tool-activity" && last.state.status === "running") {
      return "running-tools";
    }
    return "streaming";
  });

  createEffect(() => {
    session()?.messages.length;
    if (scrollRef) scrollRef.scrollTop = scrollRef.scrollHeight;
  });

  // Empty key field is proxy mode, not "unconfigured" — anthropicTransport's
  // resolveModelCall handles the no-key path (and reports a bad model slug itself).
  const transport = (): Transport => anthropicTransport;

  const send = async () => {
    const id = params.id;
    const text = draft().trim();
    if (!id || !text || sending()) return;
    setDraft("");
    appendMessage(id, { role: "user", content: [{ type: "text", text }] });
    setSending(true);
    try {
      const current = store.sessions[id];
      await transport().sendTurn(current, () => {});
    } finally {
      setSending(false);
    }
  };

  const resume = async () => {
    const id = params.id;
    if (!id || sending()) return;
    setSending(true);
    try {
      const current = store.sessions[id];
      await resumeTurn(current, () => {});
    } finally {
      setSending(false);
    }
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void send();
    }
  };

  return (
    <Show when={session()} fallback={<p>Session not found.</p>}>
      <div class="chat-view">
        <p>
          <a href="#/chat">&larr; Sessions</a>
        </p>
        <h2>{session().title}</h2>
        <ActiveBookHeader session={session()} />
        <div class="chat-messages" ref={scrollRef}>
          <For each={displayItems()}>{(item) => <DisplayItemView item={item} />}</For>
        </div>
        <Show when={turnStatus() !== "idle"}>
          <div class="chat-turn-status">
            {turnStatus() === "running-tools" ? "running tool…" : "thinking…"}
          </div>
        </Show>
        <Show when={needsResume(session()) && !sending()}>
          <div class="chat-resume">
            <p>This turn was interrupted before it finished.</p>
            <button type="button" onClick={() => void resume()}>
              Resume turn
            </button>
          </div>
        </Show>
        <div class="chat-input">
          <textarea
            value={draft()}
            onInput={(e) => setDraft(e.currentTarget.value)}
            onKeyDown={onKeyDown}
            disabled={sending()}
            placeholder="Send a message…"
          />
          <button type="button" onClick={() => void send()} disabled={sending() || !draft().trim()}>
            Send
          </button>
        </div>
      </div>
    </Show>
  );
}
