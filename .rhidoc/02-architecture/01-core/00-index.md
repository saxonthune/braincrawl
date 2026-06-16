---
title: Core
summary: The platform-agnostic knowledge-graph engine, decomposed into the three goals layers — corpus (identity + payloads), neutral metadata graph, and per-consumer projection graphs.
tags: [architecture, core, layers]
deps: []
---

# Core

The core engine is the platform-agnostic heart of braincrawl. Its storage separation
is documented in **Layers** (`doc02.01.01`): Layer 1 is the raw blob store (R2),
Layer 2 is the metadata database (D1) that holds every queryable fact, and identity
is the shared spine both depend on.
