import { For, Match, Switch, type JSX } from "solid-js";
import { A, HashRouter, Route } from "@solidjs/router";
import { GraphProvider, useGraph } from "./graph";
import { plugins } from "./plugins";
import "./App.css";

function Nav(): JSX.Element {
  const { refetch } = useGraph();
  return (
    <nav>
      <For each={plugins.filter((p) => p.nav)}>
        {(p) => (
          <A href={p.nav!.path} end>
            {p.nav!.label}
          </A>
        )}
      </For>
      <button type="button" onClick={() => refetch()}>
        Refresh
      </button>
    </nav>
  );
}

function Shell(props: { children?: JSX.Element }): JSX.Element {
  const { graph } = useGraph();
  return (
    <>
      <header>
        <h1>braincrawl</h1>
        <Nav />
      </header>
      <main>
        <Switch fallback={<p>Loading graph…</p>}>
          <Match when={graph.error}>
            <p class="error">
              Failed to load the graph: {String(graph.error?.message ?? graph.error)}
            </p>
          </Match>
          <Match when={graph()}>{props.children}</Match>
        </Switch>
      </main>
    </>
  );
}

function App(): JSX.Element {
  const routes = plugins.flatMap((plugin) =>
    plugin.routes.map((route) => <Route path={route.path} component={route.component} />),
  );
  return (
    <GraphProvider>
      <HashRouter root={Shell}>
        {routes}
        <Route path="*" component={() => <p>Not found: {window.location.hash}</p>} />
      </HashRouter>
    </GraphProvider>
  );
}

export default App;
