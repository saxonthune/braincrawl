import { createMemo, For, type JSX } from "solid-js";
import { DataTable } from "../components/DataTable";
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
    <div>
      <For each={[...groups().entries()]}>
        {([role, rows]) => (
          <section>
            <h2>{role}</h2>
            <DataTable
              columns={[
                {
                  key: "work",
                  header: "Work",
                  sortValue: (row: ReadingRow) => row.workId ?? "",
                  render: (row: ReadingRow) => row.workId ?? "—",
                },
                {
                  key: "why",
                  header: "Why",
                  render: (row: ReadingRow) => row.why,
                },
                {
                  key: "doc",
                  header: "Source",
                  sortValue: (row: ReadingRow) => row.doc,
                  render: (row: ReadingRow) => <a href={`#/doc/${row.doc}`}>{row.doc}</a>,
                },
              ]}
              rows={rows}
            />
          </section>
        )}
      </For>
    </div>
  );
}

export const readingList: Plugin = {
  id: "reading-list",
  routes: [{ path: "/reading-list", component: ReadingListComponent }],
  nav: { label: "Reading list", path: "/reading-list" },
};
