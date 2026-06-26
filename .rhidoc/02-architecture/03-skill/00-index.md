---
title: Skill
summary: The agent-facing surface — how a consuming agent drives the graph API. Gathers works once via the Library/Catalog, then reads and judges at query time over a Research Collection.
tags: [architecture, skill, agent, consumer]
deps: []
---

# Skill — agent-facing surface

One concrete consumer of the core API (`doc02.01`). The skill is how a research agent
drives braincrawl, embodying the project thesis: **the research funnel is a graph
traversal**, so separate *gathering works* (build) from *reading and judging* (query).

## Workflow shape

1. **Find a starting work** — search or semantic entry to find landmark works (you
   don't know the entry point when entering a new field).
2. **Get more catalog entries** — follow citations forward through the citation graph
   in the Catalog (`doc02.01.01`), gating expansion by concept/topic to keep the
   citations from leading off-topic. Inventory-shaped, non-decisive on purpose.
3. **Project** — maintain a Research Collection (annotation-over-reference) of the
   selection set with domain annotations and edges.
4. **Read and judge at query time** — verification stops being an up-front kill-gate
   and becomes a lens chosen per query (e.g. judge claims by where they sit in the
   citation network).

> Open: define the skill's command/tool vocabulary against the core API surface
> sketched in the architecture docs (works:resolve, graph/neighborhood, collections, expand).
