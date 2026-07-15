import { ChatView } from "./ChatView";
import { SessionList } from "./SessionList";
import { Settings } from "./Settings";
import type { Plugin } from "../types";

export const chat: Plugin = {
  id: "chat",
  routes: [
    { path: "/chat", component: SessionList },
    { path: "/chat/:id", component: ChatView },
    { path: "/settings", component: Settings },
  ],
  nav: { label: "Chat", path: "/chat" },
};
