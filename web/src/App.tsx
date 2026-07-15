import { createSignal, For, Show, type JSX } from "solid-js";
import { A, HashRouter, Navigate, Route } from "@solidjs/router";
import { GraphProvider, useGraph } from "./graph";
import { isAuthed, probeAuth } from "./services/auth";
import { plugins } from "./plugins";
import "./App.css";

const [probed, setProbed] = createSignal(false);
void probeAuth().then(() => setProbed(true));

function RequireAuth(props: { children?: JSX.Element }): JSX.Element {
  return (
    <Show when={probed()} fallback={<p>Checking…</p>}>
      <Show when={isAuthed()} fallback={<Navigate href="/auth" />}>
        {props.children}
      </Show>
    </Show>
  );
}

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
  return (
    <>
      <header>
        <h1>braincrawl</h1>
        <Nav />
      </header>
      <main>{props.children}</main>
    </>
  );
}

// Paths reachable without an authed session: the auth flow itself, and Settings
// (the manual-token escape hatch the auth page links to when locked out).
const UNGUARDED_PATHS = new Set(["/auth", "/settings"]);

function App(): JSX.Element {
  const allRoutes = plugins.flatMap((plugin) => plugin.routes);
  const guardedRoutes = allRoutes
    .filter((route) => !UNGUARDED_PATHS.has(route.path))
    .map((route) => <Route path={route.path} component={route.component} />);
  const unguardedRoutes = allRoutes
    .filter((route) => UNGUARDED_PATHS.has(route.path))
    .map((route) => <Route path={route.path} component={route.component} />);
  return (
    <GraphProvider>
      <HashRouter root={Shell}>
        <Route component={RequireAuth}>{guardedRoutes}</Route>
        {unguardedRoutes}
        <Route path="*" component={() => <p>Not found: {window.location.hash}</p>} />
      </HashRouter>
    </GraphProvider>
  );
}

export default App;
