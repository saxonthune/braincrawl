---
title: Architecture
summary: Monorepo split into three deliverables — core (the layered knowledge-graph engine), cloudflare (the deployment target), and skill (the agent-facing surface).
tags: [architecture, monorepo, overview]
deps: []
---

# Architecture

braincrawl is organized as a **monorepo with three deliverables**:

| Layer | Ref | What it is |
|---|---|---|
| **Core** | `doc02.01` | The platform-agnostic knowledge-graph engine. Itself decomposes into the three layers from the project goals: Library, Catalog, and Research Collections. |
| **Cloudflare** | `doc02.02` | The deployment target — Workers, D1, R2, KV, Durable Objects, Queues, Vectorize. Maps core abstractions onto edge primitives. |
| **Skill** | `doc02.03` | The agent-facing surface — how a consuming agent (e.g. a research skill) drives the graph: build coverage once, choose decisiveness as a query-time lens. |
| **CLI** | `doc02.06` | The `braincrawl` script — a monolithic CLI that routes between the metadata server and external providers, plus the deliberate provider forks that bypass routing. |

The seam: **core** holds the domain logic and is the source of truth for the data
model; **cloudflare** is one concrete binding of that model to infrastructure;
**skill** is one concrete consumer of the resulting API.

See the README for the full motivation — separating *coverage*
(build, expensive, done once) from *decisiveness* (query, cheap, repeatable).
