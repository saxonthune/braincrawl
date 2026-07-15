import {
  createContext,
  createResource,
  onCleanup,
  onMount,
  useContext,
  type JSX,
  type Resource,
} from "solid-js";
import { storeFetch } from "./services/store";

export type Endpoint = string;

export interface GraphNode {
  id: string | null;
  doc: string;
  labels: string[];
  properties: Record<string, unknown> & { title?: string };
}

export interface GraphLink {
  source: Endpoint;
  target: Endpoint;
  type: string;
  properties: Record<string, unknown>;
}

export interface GraphData {
  nodes: GraphNode[];
  links: GraphLink[];
}

interface GraphContextValue {
  graph: Resource<GraphData>;
  refetch: () => void;
}

const GraphContext = createContext<GraphContextValue>();

async function fetchGraph(): Promise<GraphData> {
  const res = await storeFetch("/api/l3/graph");
  if (!res.ok) {
    throw new Error(`GET /api/l3/graph failed: ${res.status} ${res.statusText}`);
  }
  return res.json() as Promise<GraphData>;
}

/**
 * Subscribes to the server's SSE change feed and calls `refetch` on each
 * "changed" signal. On a static/edge deploy (the worker) `/api/events`
 * doesn't exist, so `EventSource` retries the connection forever by default —
 * close it for good the first time it errors before ever opening, leaving the
 * manual Refresh button as the fallback there.
 */
function watchForChanges(refetch: () => void): void {
  const source = new EventSource("/api/events");
  let opened = false;
  source.onopen = () => {
    opened = true;
  };
  source.onmessage = () => refetch();
  source.onerror = () => {
    if (!opened) source.close();
  };
  onCleanup(() => source.close());
}

export function GraphProvider(props: { children: JSX.Element }): JSX.Element {
  const [graph, { refetch }] = createResource(fetchGraph);
  const value: GraphContextValue = { graph, refetch: () => refetch() };
  onMount(() => watchForChanges(value.refetch));
  return <GraphContext.Provider value={value}>{props.children}</GraphContext.Provider>;
}

export function useGraph(): GraphContextValue {
  const ctx = useContext(GraphContext);
  if (!ctx) {
    throw new Error("useGraph() must be called within a GraphProvider");
  }
  return ctx;
}

/**
 * The loaded graph as a plain accessor. Route components mount only after the
 * shell has gated on a loaded resource, so this never sees `undefined` in
 * practice — the throw guards against a component rendered outside that gate.
 */
export function useGraphData(): () => GraphData {
  const { graph } = useGraph();
  return () => {
    const data = graph();
    if (!data) throw new Error("useGraphData() read before the graph loaded");
    return data;
  };
}

/** `node:<id>` → `<id>`; any other endpoint (a catalog id) → null. */
export function endpointNodeId(e: Endpoint): string | null {
  return e.startsWith("node:") ? e.slice("node:".length) : null;
}
