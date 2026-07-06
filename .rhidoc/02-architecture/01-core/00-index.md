---
title: Core
summary: 
tags: []
deps: []
---

# Core


| Ref | Item | Kind | Summary | Tags |
|-----|------|------|---------|------|

| doc02.01.01 | Layers | doc | Core splits into two storage worlds — the Library (Layer 1) is raw bytes in a blob store keyed by UUID, the Catalog (Layer 2) is the metadata database that holds every fact, including the facts about the bytes. Identity is the shared spine both depend on. | architecture, core, layers, storage, separation |
| doc02.01.02 | Id Resolution | doc | A core braincrawl feature — consumers hand in any external id and braincrawl routes every id of the same resource to one UUID. Resolution is incremental union-find over the alias table; convergence is guaranteed for any record that co-asserts two ids, and merges are confluent. | architecture, core, identity, id-resolution, union-find |
| doc02.01.03 | API | doc | The consumer-facing store API — upsert and read for works, content, and citation edges. Every id parameter accepts any external identifier; braincrawl resolves it to a UUID internally, so consumers never resolve identity themselves. | architecture, core, api, contract |
| doc02.01.04 | L3 Conventions | doc | The L3 Research Collection body has no formalized contract yet — only the envelope (doc/schema/updated) is frozen. Candidate body conventions are collected and churned in an experimental working ledger inside the braincrawl Claude skill, not here, until one earns its way into the spec. This doc is the stable pointer to that live space. | architecture, core, l3, research-collection, conventions, experimental |

Topics: api, architecture, contract, conventions, core, experimental, id-resolution, identity, l3, layers, research-collection, separation, storage, union-find
