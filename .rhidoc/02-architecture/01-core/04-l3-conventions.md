---
title: L3 Conventions
summary: The L3 Research Collection body has no formalized contract yet — only the envelope (doc/schema/updated) is frozen. Candidate body conventions are collected and churned in an experimental working ledger inside the braincrawl Claude skill, not here, until one earns its way into the spec. This doc is the stable pointer to that live space.
tags: [architecture, core, l3, research-collection, conventions, experimental]
deps: [doc02.01.01]
---

# L3 Conventions

The L3 Research Collection's **envelope** is frozen — three frontmatter keys (`doc`,
`schema`, `updated`), enforced by `braincrawl l3 check`. The **body** is deliberately not.
Body conventions — what belongs in an L3 doc and what must stay queryable from the Library
and Catalog (`doc02.01.01`) instead — are still being worked out, and formalizing them here
prematurely would freeze leans that real sessions haven't tested.

So the live work does **not** live in this spec doc. It lives in an experimental **working
ledger** inside the braincrawl Claude skill:

> `.claude/skills/braincrawl/l3-conventions.md` (repo-relative; the user-level
> `~/.claude/skills/braincrawl` is a symlink to this same file)

That ledger is a scratch space: each candidate convention carries a *lean*, not a law, and
may be reversed or discarded at no cost. Candidates arrive from `l3-feedback-*` reports
(filed by the `braincrawl-l3-feedback` skill into the braincrawl todo-tasks inbox); a
braincrawl session reads each report and records a lean.

A candidate is promoted out of the ledger only when it has earned formalization — at which
point it graduates into the braincrawl skill's `SKILL.md` §5 and/or a `braincrawl l3 check`
per-schema body lint. **When that happens, this doc is where the landed convention is
written up as spec.** Until the first graduation, this doc carries nothing but the pointer
above.
