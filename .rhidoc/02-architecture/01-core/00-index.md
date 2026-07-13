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
| doc02.01.04 | L3 Conventions | doc | An L3 Research Document is markdown with two required frontmatter fields (doc, updated) and a body of research nodes — headings carrying property and link lines. Node anchors are store-global; reference ids, never copy metadata. A worked example ships with the braincrawl skill. | architecture, core, l3, research-collection, conventions, node-grammar |

Topics: api, architecture, contract, conventions, core, id-resolution, identity, l3, layers, node-grammar, research-collection, separation, storage, union-find
