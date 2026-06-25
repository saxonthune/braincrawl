---
title: Auth & Tenancy
summary: Access is a bearer token in the Authorization header, validated at the entry point against a hashed allowlist in KV. Each token maps to a tenant, and tenant is the isolation key for Research Collections. Core stays auth-blind; the entry points gate every request before any use-case runs.
tags: [architecture, auth, security, tenancy, token]
deps: [doc02.01.03, doc02.02.00]
---

# Auth & Tenancy

Every request carries a **bearer token**; the entry point resolves it to a **tenant**
before any core use-case runs. Authentication answers "is this caller known", tenancy
answers "whose data is this" — the same token settles both.

## The credential

```
Authorization: Bearer <token>
```

A token is an opaque, high-entropy string (`bc_` prefix + 256 bits). It is the only
credential; there are no sessions, cookies, or per-request signing. A consumer stores it
once (the thin client keeps it in `~/.braincrawl/config`) and sends it on every call.

## Where validation lives

Auth is an **entry-point concern, not a core one** (doc02.04). The `core` crate never
sees a token — it operates on a tenant the entry point has already resolved. Both entry
points gate identically:

1. Extract the bearer token; reject with `401` if absent or malformed.
2. Resolve token → tenant via the allowlist; reject with `403` if unknown or revoked.
3. Hand the tenant to the use-case as an ambient parameter.

This keeps the security boundary at the edge and leaves the domain logic testable
without credentials.

## The allowlist

Tokens are checked against a store of **`SHA-256(token) → { tenant, status }`** — the
raw token is never persisted, so a leak of the backing store does not leak credentials.

| Runtime | Backing store |
|---|---|
| Cloudflare | KV (`AUTH_KV`) — same hot-cache tier as id resolution (doc02.02.00) |
| Local | the metadata DB, or a single shared secret from the environment for one-user dev |

A single-user deployment starts as **one shared secret** (`wrangler secret put` /
an env var) mapping to one tenant — no allowlist table needed. Multi-token issuance
(rotate, revoke, name a token) is an additive change: populate the KV allowlist and the
gate already reads it.

## Tenancy and Research Collections

The token's tenant is the **isolation key for Research Collections** — the hint in
doc02.02.00 ("consider per-tenant DB for Research Collection isolation") resolves here.
The Library and Catalog are a **shared substrate**: identity,
metadata, and the citation graph accumulate across all tenants, because facts about a
work are not anyone's private data and the accumulation payoff (doc02.01.03) depends on
sharing them. Only Research Collections — a consumer's curated data — are
partitioned by tenant.

So the gate enforces an asymmetry:

- **Read/write the Library + Catalog** — any valid token; results are tenant-independent.
- **Read/write Research Collections** — scoped to the caller's tenant; one tenant never sees another's
  collections. Per-tenant D1 (doc02.02.00) is the strong form of this boundary.

## API surface

The store API (doc02.01.03, `03-openapi.yaml`) gains one security scheme applied to all
operations:

```yaml
components:
  securitySchemes:
    bearerAuth: { type: http, scheme: bearer }
security:
  - bearerAuth: []
```

Unauthenticated requests get `401`; authenticated-but-unauthorized (e.g. another
tenant's Research Collection) get `403`.

## Local development

The token rides as a bearer header from the start, but the local server may run with auth
**disabled** (a dev flag) so the stack is usable before the gate is wired. With auth
enabled locally, the single-shared-secret path mirrors production without needing KV.
