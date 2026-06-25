---
title: Skill
summary: The agent-facing surface — how a consuming agent drives the graph API. Builds coverage once via the Library/Catalog, then applies decisiveness as a query-time lens over a Research Collection.
tags: [architecture, skill, agent, consumer]
deps: []
---

# Skill — agent-facing surface

One concrete consumer of the core API (`doc02.01`). The skill is how a research agent
drives braincrawl, embodying the project thesis: **the research funnel is a graph
traversal**, so separate *coverage* (build) from *decisiveness* (query).

## Workflow shape

1. **Seed** — search / semantic entry to find landmark works (you don't know the
   seeds when entering a new field).
2. **Build coverage** — snowball forward through the citation graph in the Catalog
   (`doc02.01.01`), gating expansion by concept/topic to control citation drift.
   Inventory-shaped, non-decisive on purpose.
3. **Project** — maintain a Research Collection (annotation-over-reference) of the
   selection set with domain annotations and edges.
4. **Decide at query time** — verification stops being an up-front kill-gate and
   becomes a lens chosen per query (e.g. judge claims by where they sit in the
   citation network).

> Open: define the skill's command/tool vocabulary against the core API surface
> sketched in the architecture docs (works:resolve, graph/neighborhood, collections, expand).
