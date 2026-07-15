import { createMemo, type JSX } from "solid-js";
import { useNavigate } from "@solidjs/router";
import { DataTable } from "../../components/DataTable";
import { createSession, deleteSession, useChatStore } from "./store";
import type { ChatSession } from "./types";

export function SessionList(): JSX.Element {
  const navigate = useNavigate();
  const store = useChatStore();
  const rows = createMemo(() => store.order.map((id) => store.sessions[id]).filter(Boolean));

  const handleNew = () => {
    const session = createSession();
    navigate(`/chat/${session.id}`);
  };

  return (
    <div>
      <header class="chat-session-list-header">
        <h2>Chat sessions</h2>
        <div>
          <button type="button" onClick={handleNew}>
            New session
          </button>
          <a href="#/settings">Settings</a>
        </div>
      </header>
      <DataTable
        columns={[
          {
            key: "title",
            header: "Title",
            sortValue: (row: ChatSession) => row.title,
            render: (row: ChatSession) => <a href={`#/chat/${row.id}`}>{row.title}</a>,
          },
          {
            key: "updatedAt",
            header: "Updated",
            sortValue: (row: ChatSession) => row.updatedAt,
            render: (row: ChatSession) => new Date(row.updatedAt).toLocaleString(),
          },
          {
            key: "messages",
            header: "Messages",
            sortValue: (row: ChatSession) => row.messages.length,
            render: (row: ChatSession) => row.messages.length,
          },
          {
            key: "actions",
            header: "",
            render: (row: ChatSession) => (
              <button type="button" onClick={() => deleteSession(row.id)}>
                Delete
              </button>
            ),
          },
        ]}
        rows={rows()}
      />
    </div>
  );
}
