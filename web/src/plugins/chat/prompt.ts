import corePrompt from "./prompt.md?raw";
import { getSetting } from "./settings";
import type { ActiveBook } from "./types";

export interface SystemBlock {
  type: "text";
  text: string;
  cache_control?: { type: "ephemeral" };
}

function storeUrl(path: string): string {
  const base = getSetting("storeBaseUrl").replace(/\/$/, "");
  return `${base}${path}`;
}

function storeHeaders(): Record<string, string> {
  const token = getSetting("storeToken");
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function fetchAgentFile(name: string): Promise<string | null> {
  try {
    const res = await fetch(storeUrl(`/api/l3/agent/${name}`), { headers: storeHeaders() });
    if (!res.ok) return null;
    return await res.text();
  } catch {
    return null;
  }
}

async function fetchDoc(slug: string): Promise<string | null> {
  try {
    const res = await fetch(storeUrl(`/api/l3/docs/${encodeURIComponent(slug)}`), {
      headers: storeHeaders(),
    });
    if (!res.ok) return null;
    return await res.text();
  } catch {
    return null;
  }
}

export async function buildSystemBlocks(activeBook: ActiveBook | undefined): Promise<SystemBlock[]> {
  const blocks: SystemBlock[] = [{ type: "text", text: corePrompt, cache_control: { type: "ephemeral" } }];

  const [principles, memory] = await Promise.all([
    fetchAgentFile("principles"),
    fetchAgentFile("memory"),
  ]);
  if (principles) blocks.push({ type: "text", text: `## Principles\n\n${principles}` });
  if (memory) blocks.push({ type: "text", text: `## Memory\n\n${memory}` });

  const today = new Date().toISOString().slice(0, 10);
  let sessionContext = `## Session context\n\nToday's date: ${today}.`;
  if (activeBook) {
    sessionContext += `\nActive book: "${activeBook.title}" (work id ${activeBook.workId}, doc slug ${activeBook.docSlug}).`;
    const doc = await fetchDoc(activeBook.docSlug);
    if (doc) sessionContext += `\n\n### Active research doc (${activeBook.docSlug})\n\n${doc}`;
  } else {
    sessionContext += "\nNo active book is set for this session.";
  }
  blocks.push({ type: "text", text: sessionContext });

  return blocks;
}
