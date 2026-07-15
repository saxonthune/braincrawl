import { createMemo, For, Show, type JSX } from "solid-js";
import { RequireGraph } from "../components/RequireGraph";
import { WorkLink } from "../components/WorkLink";
import { endpointNodeId, useGraphData, type GraphData } from "../graph";
import type { Plugin } from "./types";

const BLESSED_ROLES = ["start-here", "core", "rigor", "reference"];

interface ReadingRow {
  doc: string;
  role: string;
  why: string;
  workId: string | null;
}

function roleRank(role: string): number {
  const i = BLESSED_ROLES.indexOf(role);
  return i === -1 ? BLESSED_ROLES.length : i;
}

function readingRows(graph: GraphData): ReadingRow[] {
  const rows: ReadingRow[] = [];
  for (const node of graph.nodes) {
    const reading = node.properties.reading as { role?: string; why?: string } | undefined;
    if (!reading || typeof reading !== "object") continue;
    const catalogLink = graph.links.find(
      (l) => l.type === "catalog" && node.id !== null && endpointNodeId(l.source) === node.id,
    );
    rows.push({
      doc: node.doc,
      role: reading.role ?? "",
      why: reading.why ?? "",
      workId: catalogLink ? catalogLink.target : null,
    });
  }
  rows.sort((a, b) => roleRank(a.role) - roleRank(b.role) || a.role.localeCompare(b.role));
  return rows;
}

function groupByRole(rows: ReadingRow[]): Map<string, ReadingRow[]> {
  const groups = new Map<string, ReadingRow[]>();
  for (const row of rows) {
    const key = row.role || "(no role)";
    let group = groups.get(key);
    if (!group) {
      group = [];
      groups.set(key, group);
    }
    group.push(row);
  }
  return groups;
}

function ReadingListComponent(): JSX.Element {
  const graph = useGraphData();
  const groups = createMemo(() => groupByRole(readingRows(graph())));

  return (
    <div class="doc-page stack gap-5">
      <Show when={groups().size > 0} fallback={<p>No reading entries yet.</p>}>
        <For each={[...groups().entries()]}>
          {([role, rows]) => (
            <section class="stack gap-2">
              <h2 class="section-label">{role}</h2>
              <div class="card node-card">
                <div class="card-section">
                  <ul class="row-list">
                    <For each={rows}>
                      {(row) => (
                        <li class="stack gap-1">
                          <div class="cluster gap-2">
                            <Show when={row.workId} fallback={<span>—</span>}>
                              {(workId) => (
                                <span class="edge">
                                  <WorkLink target={workId()} />
                                </span>
                              )}
                            </Show>
                            <span class="edge">
                              in <a href={`#/doc/${row.doc}`}>{row.doc}</a>
                            </span>
                          </div>
                          <Show when={row.why}>
                            <p class="why">{row.why}</p>
                          </Show>
                        </li>
                      )}
                    </For>
                  </ul>
                </div>
              </div>
            </section>
          )}
        </For>
      </Show>
    </div>
  );
}

export const readingList: Plugin = {
  id: "reading-list",
  routes: [
    {
      path: "/reading-list",
      component: () => (
        <RequireGraph>
          <ReadingListComponent />
        </RequireGraph>
      ),
    },
  ],
  nav: { label: "Reading list", path: "/reading-list" },
};
