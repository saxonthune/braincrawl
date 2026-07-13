import {
  createMemo,
  createSignal,
  For,
  Match,
  onCleanup,
  onMount,
  Switch,
  type JSX,
} from "solid-js";
import { GraphProvider, useGraph } from "./graph";
import { plugins } from "./plugins";
import type { Plugin } from "./plugins/types";
import "./App.css";

function currentHash(): string {
  const hash = window.location.hash.slice(1);
  return hash === "" ? "/" : hash;
}

interface RouteMatch {
  plugin: Plugin;
  params: Record<string, string>;
}

function matchRoute(path: string): RouteMatch | null {
  const pathSegments = path.split("/").filter((s) => s !== "");
  for (const plugin of plugins) {
    for (const route of plugin.routes) {
      const routeSegments = route.split("/").filter((s) => s !== "");
      if (routeSegments.length !== pathSegments.length) continue;
      const params: Record<string, string> = {};
      let matched = true;
      for (let i = 0; i < routeSegments.length; i++) {
        const routeSeg = routeSegments[i];
        const pathSeg = pathSegments[i];
        if (routeSeg.startsWith(":")) {
          params[routeSeg.slice(1)] = pathSeg;
        } else if (routeSeg !== pathSeg) {
          matched = false;
          break;
        }
      }
      if (matched) return { plugin, params };
    }
  }
  return null;
}

function Router(): JSX.Element {
  const { graph } = useGraph();
  const [hash, setHash] = createSignal(currentHash());

  const onHashChange = () => setHash(currentHash());
  onMount(() => window.addEventListener("hashchange", onHashChange));
  onCleanup(() => window.removeEventListener("hashchange", onHashChange));

  const route = createMemo(() => matchRoute(hash()));

  return (
    <Switch fallback={<p>Loading graph…</p>}>
      <Match when={graph.error}>
        <p class="error">Failed to load the graph: {String(graph.error?.message ?? graph.error)}</p>
      </Match>
      <Match when={graph()}>
        {(data) => (
          <Switch fallback={<p>Not found: {hash()}</p>}>
            <Match when={route()}>
              {(m) => {
                const Component = m().plugin.component;
                return <Component graph={data()} params={m().params} />;
              }}
            </Match>
          </Switch>
        )}
      </Match>
    </Switch>
  );
}

function Nav(): JSX.Element {
  const { refetch } = useGraph();
  return (
    <nav>
      <For each={plugins}>{(plugin) => <a href={`#${plugin.routes[0]}`}>{plugin.title}</a>}</For>
      <button type="button" onClick={() => refetch()}>
        Refresh
      </button>
    </nav>
  );
}

function App(): JSX.Element {
  return (
    <GraphProvider>
      <header>
        <h1>braincrawl</h1>
        <Nav />
      </header>
      <main>
        <Router />
      </main>
    </GraphProvider>
  );
}

export default App;
