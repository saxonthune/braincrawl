import { createMemo, Show, type JSX } from "solid-js";
import { useParams } from "@solidjs/router";
import { DataTable } from "../components/DataTable";
import { RequireGraph } from "../components/RequireGraph";
import { useGraphData, type GraphNode } from "../graph";
import type { Plugin } from "./types";

interface DocSummary {
  doc: string;
  nodeCount: number;
  labelCounts: Map<string, number>;
}

function summarizeDocs(nodes: GraphNode[]): DocSummary[] {
  const byDoc = new Map<string, DocSummary>();
  for (const node of nodes) {
    let summary = byDoc.get(node.doc);
    if (!summary) {
      summary = { doc: node.doc, nodeCount: 0, labelCounts: new Map() };
      byDoc.set(node.doc, summary);
    }
    summary.nodeCount += 1;
    for (const label of node.labels) {
      summary.labelCounts.set(label, (summary.labelCounts.get(label) ?? 0) + 1);
    }
  }
  return [...byDoc.values()].sort((a, b) => a.doc.localeCompare(b.doc));
}

function formatLabelCounts(labelCounts: Map<string, number>): string {
  return [...labelCounts.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([label, count]) => `#${label} ${count}`)
    .join(", ");
}

function DocumentList(): JSX.Element {
  const graph = useGraphData();
  const summaries = createMemo(() => summarizeDocs(graph().nodes));

  return (
    <DataTable
      columns={[
        {
          key: "doc",
          header: "Document",
          sortValue: (row: DocSummary) => row.doc,
          render: (row: DocSummary) => <a href={`#/doc/${row.doc}`}>{row.doc}</a>,
        },
        {
          key: "nodeCount",
          header: "Nodes",
          sortValue: (row: DocSummary) => row.nodeCount,
          render: (row: DocSummary) => row.nodeCount,
        },
        {
          key: "labels",
          header: "Labels",
          render: (row: DocSummary) => formatLabelCounts(row.labelCounts),
        },
      ]}
      rows={summaries()}
    />
  );
}

function DocumentDetail(): JSX.Element {
  const graph = useGraphData();
  const params = useParams();
  const slug = () => params.slug ?? "";
  const nodes = createMemo(() => graph().nodes.filter((n) => n.doc === slug()));

  return (
    <div>
      <p>
        <a href="#/">&larr; All documents</a>
      </p>
      <h2>{slug()}</h2>
      <Show when={nodes().length > 0} fallback={<p>No nodes found for this document.</p>}>
        <DataTable
          columns={[
            {
              key: "title",
              header: "Title",
              render: (row: GraphNode) =>
                (row.properties.title as string | undefined) ?? row.id ?? "",
            },
            {
              key: "labels",
              header: "Labels",
              render: (row: GraphNode) => row.labels.map((l) => `#${l}`).join(" "),
            },
            {
              key: "links",
              header: "Outgoing links",
              render: (row: GraphNode) => {
                const nodeEndpoint = row.id ? `node:${row.id}` : null;
                const outgoing = graph().links.filter((l) => l.source === nodeEndpoint);
                return outgoing.map((l) => `${l.type}→${l.target}`).join(", ");
              },
            },
          ]}
          rows={nodes()}
        />
      </Show>
    </div>
  );
}

export const documentIndex: Plugin = {
  id: "document-index",
  routes: [
    { path: "/", component: () => <RequireGraph><DocumentList /></RequireGraph> },
    { path: "/doc/:slug", component: () => <RequireGraph><DocumentDetail /></RequireGraph> },
  ],
  nav: { label: "Documents", path: "/" },
};
