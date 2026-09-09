import { Match, Switch, type JSX } from "solid-js";
import { useGraph } from "../graph";

export function RequireGraph(props: { children: JSX.Element }): JSX.Element {
  const { graph } = useGraph();
  return (
    <Switch fallback={<p>Loading graph…</p>}>
      <Match when={graph.error}>
        <p class="error">Failed to load the graph: {String(graph.error?.message ?? graph.error)}</p>
      </Match>
      <Match when={graph()}>{props.children}</Match>
    </Switch>
  );
}
