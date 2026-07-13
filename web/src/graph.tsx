import {
  createContext,
  createResource,
  onCleanup,
  onMount,
  useContext,
  type JSX,
  type Resource,
} from "solid-js";

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
  const res = await fetch("/api/l3/graph");
  if (!res.ok) {
    throw new Error(`GET /api/l3/graph failed: ${res.status} ${res.statusText}`);
  }
  return res.json() as Promise<GraphData>;
}

/**
 * Subscribes to the server's SSE change feed and calls `refetch` on each
 * "changed" signal. On a static/edge deploy `/api/events` doesn't exist, so
 * `EventSource` fails to connect — that's expected, and we close quietly
 * rather than retrying: the manual Refresh button remains the fallback.
 */
function watchForChanges(refetch: () => void): void {
  const source = new EventSource("/api/events");
  source.onmessage = () => refetch();
  source.onerror = () => source.close();
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

/** `node:<id>` → `<id>`; any other endpoint (a catalog id) → null. */
export function endpointNodeId(e: Endpoint): string | null {
  return e.startsWith("node:") ? e.slice("node:".length) : null;
}
