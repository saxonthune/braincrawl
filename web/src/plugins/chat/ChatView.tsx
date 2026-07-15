import { createEffect, createSignal, For, Match, Show, Switch, type JSX } from "solid-js";
import { useParams } from "@solidjs/router";
import { appendMessage, useChatStore } from "./store";
import { stubTransport, type ChatMessage, type ContentBlock } from "./types";

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
            <div classList={{ "chat-block-tool-result": true, "chat-block-error": !!block.is_error }}>
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

  const send = async () => {
    const id = params.id;
    const text = draft().trim();
    if (!id || !text || sending()) return;
    setDraft("");
    appendMessage(id, { role: "user", content: [{ type: "text", text }] });
    setSending(true);
    try {
      const current = store.sessions[id];
      const replies = await stubTransport.sendTurn(current, () => {});
      for (const reply of replies) {
        appendMessage(id, reply);
      }
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
        <div class="chat-messages" ref={scrollRef}>
          <For each={session().messages}>{(message) => <MessageView message={message} />}</For>
        </div>
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
