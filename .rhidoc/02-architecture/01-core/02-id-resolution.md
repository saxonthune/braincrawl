---
title: Id Resolution
summary: A core braincrawl feature — consumers hand in any external id and braincrawl routes every id of the same resource to one canonical GUID. Consumers never resolve identity themselves.
tags: [architecture, core, identity, id-resolution]
deps: [doc02.01.01]
---

# Id Resolution

Identity resolution is a **core feature**, not a consumer concern. A consumer hands
braincrawl any external id (DOI, ISBN, OCLC, PMID, OpenAlex `W…`, …) and braincrawl
guarantees that every id naming the same resource routes to the same place.

## Canonical id

The canonical id is a braincrawl-minted **GUID**. It is opaque and provider-neutral:
no external scheme (not even OpenAlex `W…`) is the canonical key. Every external id —
including OpenAlex's — is an **alias** pointing at the GUID.

## The job

Given `{namespace, value}`, return the canonical GUID:

- **Known alias** → its GUID.
- **Unknown alias that resolves to a known resource** → the existing GUID (different
  ids of one resource must converge, never fork).
- **Genuinely new resource** → mint a new GUID and record the alias.

This convergence guarantee is the contract: consumers store and query by whatever id
they hold, and braincrawl makes them all land on one node. The alias multimap and KV
resolver projection that back this live in the metadata DB (see Layers, `doc02.01.01`).

> A later session fleshes out merge/split handling, no-strong-id fallback, and the
> resolution API surface.
