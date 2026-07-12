---
title: Web UI
summary: The Web UI is a feature of braincrawl providing read-only views of everything it holds, across all three layers; its views are provided by Web UI plugins.
tags: [product, web-ui, read-only, plugin]
deps: [doc01.01]
---

# Web UI

The Web UI is a feature of braincrawl. It provides read-only views of everything
braincrawl holds, across all three layers (Library, Catalog, Research
Collections). It performs no write operations.

## Web UI plugins

The Web UI's views are provided by **Web UI plugins**.

- A plugin provides views of the Web UI.
- A plugin author writes a component and decides the data needed to get it.
- A plugin has full read access to the user's research graph.
- The author decides what data to look for.
- The author decides the markup and interactivity of the plugin's SolidJS
  components.
- A plugin performs queries; a plugin performs no writes.
- The reading list ships as a default plugin.
