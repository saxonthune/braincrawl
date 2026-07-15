import { createEffect, createSignal, For, Match, Show, Switch, type JSX } from "solid-js";
import { useParams } from "@solidjs/router";
import { storeFetch } from "../../services/store";
import { appendMessage, setActiveBook, useChatStore } from "./store";
import { anthropicTransport, needsResume, resumeTurn } from "./transport";
import { type ChatMessage, type ChatSession, type ContentBlock, type Transport } from "./types";

function ContentBlockView(props: { block: ContentBlock }): JSX.Element {
  const [expanded, setExpanded] = createSignal(false);

  return (
    <Switch>
      <Match when={props.block.type === "text"}>
        <p class="chat-block-text">{(props.block as { text: string }).text}</p>
      </Match>
      <Match when={props.block.type === "tool_use"}>
        {(() => {
          const block = props.block as { name: string; input: unknown };
          return (
            <div class="chat-block-tool">
              <button type="button" onClick={() => setExpanded((v) => !v)}>
                ⚙ {block.name}(...)
              </button>
              <Show when={expanded()}>
                <pre>{JSON.stringify(block.input, null, 2)}</pre>
              </Show>
            </div>
          );
        })()}
      </Match>
      <Match when={props.block.type === "tool_result"}>
        {(() => {
          const block = props.block as { content: string; is_error?: boolean };
          return (
            <div
              classList={{ "chat-block-tool-result": true, "chat-block-error": !!block.is_error }}
            >
              <button type="button" onClick={() => setExpanded((v) => !v)}>
                ⚙ tool result{block.is_error ? " (error)" : ""}
              </button>
              <Show when={expanded()}>
                <pre>{block.content}</pre>
              </Show>
            </div>
          );
        })()}
      </Match>
    </Switch>
  );
}

function MessageView(props: { message: ChatMessage }): JSX.Element {
  return (
    <div classList={{ "chat-message": true, [`chat-message-${props.message.role}`]: true }}>
      <div class="chat-message-role">{props.message.role}</div>
      <For each={props.message.content}>{(block) => <ContentBlockView block={block} />}</For>
    </div>
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
          <For each={session().messages}>{(message) => <MessageView message={message} />}</For>
        </div>
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
