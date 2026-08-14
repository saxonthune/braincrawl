---
title: Architecture
summary: 
tags: []
deps: []
---

# Architecture


| Ref | Item | Kind | Summary | Tags |
|-----|------|------|---------|------|

| doc02.01 | Core | group (5) | — | — |
| doc02.02 | Cloudflare | group (0) | — | — |
| doc02.03 | Skill | group (0) | — | — |
| doc02.04 | Monorepo & Runtimes | doc | A single Cargo workspace in Rust. The platform-agnostic core defines traits; concrete backends bind them to Cloudflare or to a local stack. Two entry points — a Wasm Worker and a native dev server — wire the backends, and are the only crates that name infrastructure. | architecture, monorepo, rust, cloudflare, runtime, traits-and-backends, separation |
| doc02.05 | Auth & Tenancy | doc | Access is a bearer token in the Authorization header, validated at the entry point against a hashed allowlist in KV. Each token maps to a tenant, and tenant is the isolation key for Research Collections. Core never sees a token; the entry points run every request through the gate before any use-case runs. | architecture, auth, security, tenancy, token |
| doc02.06 | CLI | group (2) | — | — |

Topics: architecture, auth, cloudflare, monorepo, runtime, rust, security, separation, tenancy, token, traits-and-backends
