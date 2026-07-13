import type { Component } from "solid-js";
import type { GraphData } from "../graph";

export interface PluginProps {
  graph: GraphData;
  params: Record<string, string>;
}

export interface Plugin {
  name: string;
  title: string;
  routes: string[];
  component: Component<PluginProps>;
}
