import type { Component } from "solid-js";

export interface PluginRoute {
  path: string;
  component: Component;
}

export interface NavEntry {
  label: string;
  path: string;
}

export interface Plugin {
  id: string;
  routes: PluginRoute[];
  nav?: NavEntry;
}
