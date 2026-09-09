import { createMemo, For, Show, type JSX } from "solid-js";
import { useParams } from "@solidjs/router";
import { RequireGraph } from "../components/RequireGraph";
import { WorkLink } from "../components/WorkLink";
import { useGraphData, type Endpoint, type GraphLink, type GraphNode } from "../graph";
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

const LIST_TAG_LIMIT = 8;

function topLabels(labelCounts: Map<string, number>): [string, number][] {
  return [...labelCounts.entries()].sort((a, b) => b[1] - a[1]);
}

function DocumentList(): JSX.Element {
  const graph = useGraphData();
  const summaries = createMemo(() => summarizeDocs(graph().nodes));

  return (
    <div class="doc-page">
      <Show when={summaries().length > 0} fallback={<p>No documents in the store yet.</p>}>
        <ul class="node-cards stack gap-4">
          <For each={summaries()}>
            {(row) => (
              <li>
                <article class="card node-card">
                  <div class="card-section stack gap-2">
                    <h2 class="doc-list-title">
                      <a href={`#/doc/${row.doc}`}>{row.doc}</a>
                    </h2>
                    <p class="doc-facts">{row.nodeCount} nodes</p>
                    <div class="cluster gap-2">
                      <For each={topLabels(row.labelCounts).slice(0, LIST_TAG_LIMIT)}>
                        {([label, count]) => (
                          <span class="tag">
                            #{label} <span class="tag-count">{count}</span>
                          </span>
                        )}
                      </For>
                      <Show when={row.labelCounts.size > LIST_TAG_LIMIT}>
                        <span class="tag-count">+{row.labelCounts.size - LIST_TAG_LIMIT} more</span>
                      </Show>
                    </div>
                  </div>
                </article>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
}

function nodeTitle(n: GraphNode): string {
  return (n.properties.title as string | undefined) ?? n.id ?? "(untitled)";
}

function nodeRemarks(n: GraphNode): string | undefined {
  return n.properties.remarks as string | undefined;
}

function readingRole(n: GraphNode): string | undefined {
  const reading = n.properties.reading as { role?: string } | undefined;
  return reading?.role;
}

function scrollToNode(id: string | null): void {
  if (id) document.getElementById(id)?.scrollIntoView({ behavior: "smooth", block: "start" });
}

function cardRemarks(n: GraphNode): string | undefined {
  const remarks = nodeRemarks(n);
  return remarks && remarks.trim() !== nodeTitle(n).trim() ? remarks : undefined;
}

function DocumentDetail(): JSX.Element {
  const graph = useGraphData();
  const params = useParams();
  const slug = () => params.slug ?? "";
  const nodes = createMemo(() => graph().nodes.filter((n) => n.doc === slug()));

  const byEndpoint = createMemo(() => {
    const m = new Map<Endpoint, GraphNode>();
    for (const n of graph().nodes) if (n.id) m.set(`node:${n.id}`, n);
    return m;
  });
  const outgoing = (n: GraphNode): GraphLink[] =>
    n.id ? graph().links.filter((l) => l.source === `node:${n.id}`) : [];

  const workCount = createMemo(() => {
    const works = new Set<Endpoint>();
    for (const n of nodes()) {
      for (const l of outgoing(n)) if (!l.target.startsWith("node:")) works.add(l.target);
    }
    return works.size;
  });
  const labelCounts = createMemo(() => {
    const counts = new Map<string, number>();
    for (const n of nodes()) {
      for (const label of n.labels) counts.set(label, (counts.get(label) ?? 0) + 1);
    }
    return [...counts.entries()].sort((a, b) => b[1] - a[1]);
  });

  const docSlugs = createMemo(() => new Set(graph().nodes.map((n) => n.doc)));

  const DOC_PREFIX = "node:doc:";
  const EdgeTarget = (props: { target: Endpoint }): JSX.Element => {
    const node = () => byEndpoint().get(props.target);
    const docRef = () =>
      props.target.startsWith(DOC_PREFIX) ? props.target.slice(DOC_PREFIX.length) : null;
    return (
      <Show
        when={node()}
        fallback={
          <Show when={docRef()} fallback={<WorkLink target={props.target} />}>
            {(ref) => (
              <Show when={docSlugs().has(ref())} fallback={<span>{ref()} ?</span>}>
                <a href={`#/doc/${ref()}`}>{ref()}</a>
              </Show>
            )}
          </Show>
        }
      >
        {(n) => (
          <Show
            when={n().doc === slug()}
            fallback={<a href={`#/doc/${n().doc}`}>{nodeTitle(n())}</a>}
          >
            <button type="button" class="linklike" onClick={() => scrollToNode(n().id)}>
              {nodeTitle(n())}
            </button>
          </Show>
        )}
      </Show>
    );
  };

  return (
    <div class="doc-page stack gap-5">
      <p class="doc-breadcrumb">
        <a href="#/documents">&larr; All documents</a>
      </p>
      <Show when={nodes().length > 0} fallback={<p>No nodes found for this document.</p>}>
        <header class="stack gap-2">
          <h2 class="doc-title">{slug()}</h2>
          <p class="doc-facts">
            {nodes().length} nodes &middot; {workCount()} works referenced
          </p>
          <div class="cluster gap-2">
            <For each={labelCounts()}>
              {([label, count]) => (
                <span class="tag">
                  #{label} <span class="tag-count">{count}</span>
                </span>
              )}
            </For>
          </div>
        </header>
        <ul class="node-cards stack gap-4">
          <For each={nodes()}>
            {(n) => (
              <li>
                <article
                  class="card node-card"
                  id={n.id ?? undefined}
                  data-labels={n.labels.join(" ")}
                >
                  <header class="card-section stack gap-2">
                    <div class="cluster gap-2">
                      <h3 class="node-card-title">{nodeTitle(n)}</h3>
                      <For each={n.labels}>{(label) => <span class="tag">#{label}</span>}</For>
                      <Show when={readingRole(n)}>
                        {(role) => <span class="tag tag-reading">{role()}</span>}
                      </Show>
                    </div>
                    <Show when={outgoing(n).length > 0}>
                      <div class="cluster gap-2">
                        <For each={outgoing(n)}>
                          {(l) => (
                            <span class="edge">
                              <span class="edge-type">{l.type}</span>{" "}
                              <EdgeTarget target={l.target} />
                            </span>
                          )}
                        </For>
                      </div>
                    </Show>
                  </header>
                  <Show when={cardRemarks(n)}>
                    {(remarks) => (
                      <div class="card-section node-card-remarks">
                        <p>{remarks()}</p>
                      </div>
                    )}
                  </Show>
                </article>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </div>
  );
}

export const documentIndex: Plugin = {
  id: "document-index",
  routes: [
    {
      path: "/documents",
      component: () => (
        <RequireGraph>
          <DocumentList />
        </RequireGraph>
      ),
    },
    {
      path: "/doc/:slug",
      component: () => (
        <RequireGraph>
          <DocumentDetail />
        </RequireGraph>
      ),
    },
  ],
  nav: { label: "Documents", path: "/documents" },
};
