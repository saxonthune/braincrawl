---
title: Providers
summary: The provider-fork namespaces — each talks straight to one external API, bypassing routing. One provider today (OpenAlex) has been joined by Semantic Scholar as a second.
tags: [cli, providers, fork, external]
deps: [doc02.06]
---

# Providers

A **provider** is an external scholarly data source braincrawl can read from.
Each provider gets its own CLI namespace, `braincrawl <provider> …`, which is the
**deliberate fork** described in doc02.06: it bypasses routing and queries that
provider's API directly.

## Why a fork exists

The smart path (plain `braincrawl` verbs) decides for the caller whether an
answer comes from the metadata server or from upstream. The provider namespace
removes that decision: it always goes upstream, so the caller sees exactly what
the provider returns. Push-by-default still applies — fetched records persist to
the metadata server unless `--skip-push` is passed.

## Shared shape

Every provider namespace exposes the **same flexible base vocabulary** — a small
set of orthogonal verbs (lookup, search, filter, autocomplete, and citation
traversal) that map onto that provider's primitives. Richer workflows are built
*on top* of this vocabulary, not baked into it. Each provider doc records how the
base verbs bind to that provider's API and which fields the trimmed records keep.

## Contents

- **doc02.06.01.01 — OpenAlex**: the `braincrawl openalex` namespace.
- **doc02.06.01.02 — Semantic Scholar**: the `braincrawl semanticscholar` namespace.
