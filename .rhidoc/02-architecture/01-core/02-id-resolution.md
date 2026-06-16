---
title: Id Resolution
summary: A core braincrawl feature — consumers hand in any external id and braincrawl routes every id of the same resource to one canonical GUID. Resolution is incremental union-find over the alias table; convergence is guaranteed for any record that co-asserts two ids, and merges are confluent.
tags: [architecture, core, identity, id-resolution, union-find]
deps: [doc02.01.01, doc02.01.03]
---

# Id Resolution

Identity resolution is a **core feature**, not a consumer concern. A consumer hands
braincrawl any external id (DOI, ISBN, OCLC, PMID, OpenAlex `W…`, …) and braincrawl
routes every id naming the same resource to the same canonical node.

## Canonical id

The canonical id is a braincrawl-minted **GUID** — opaque and provider-neutral. No
external scheme (not even OpenAlex `W…`) is the canonical key; every external id is an
alias pointing at the GUID.

## The model — incremental union-find

Each external id is an element, each `node` is an equivalence class, and the GUID is
the class representative. The `alias` table (`doc02.01.01`) is the union-find parent
map, and `UNIQUE (namespace, value)` is the structural invariant: one external id
belongs to exactly one class and can never fork.

Resolution operates on the **id bundle** a provider record carries — not a bare id.
A provider record (an OpenAlex work, say) names several ids at once
(`{openalex:W…, doi:…, pmid:…}`); that co-assertion is the evidence that links them.

`resolve(record) → guid`:

1. Extract the bundle `{(namespace, value), …}` from the record.
2. Look each up in `alias`; collect the set `G` of distinct GUIDs hit.
3. `|G| == 0` → mint a GUID, insert the node and all aliases.
4. `|G| == 1` → use it; insert any aliases not yet present.
5. `|G| ≥ 2` → the bundle witnesses that these classes are one resource → **merge**.

As long as id co-occurrence is truthful, this produces correct equivalence classes
**regardless of record arrival order**.

## Concurrency

Stateless workers race: two resolving the same id could both miss and both mint. Two
mechanisms serialize them:

- **Get-or-create on the unique constraint.** Alias writes are
  `INSERT … ON CONFLICT DO NOTHING` followed by `SELECT`. The constraint is the
  serialization point — the losing writer reads the winner's GUID. This alone
  guarantees no fork per single alias.
- **Single-writer coordinator keyed by the strongest id in the bundle** (deterministic
  priority `doi > pmid > openalex > …`). A per-id coordinator runs resolution of a
  multi-id bundle as a critical section, so concurrent resolutions of one work
  serialize across all their alias rows. This is the same per-work coordinator that
  guards upstream fetches (see Layers, `doc02.01.01`).

## Merge — confluent by construction

When step 5 (or a later bridging record) establishes that two GUIDs are one resource:

1. **Choose a deterministic survivor** (e.g. the lexicographically smaller GUID).
   Deterministic selection makes merges **commutative**: whatever order workers apply
   them, the surviving representative is identical.
2. **Repoint** `alias`, `node_assertion`, `edge.src_id`/`edge.dst_id`,
   `edge_assertion`, and `payloads` from the loser to the survivor, folding any
   primary-key collisions (duplicate edges collapse; same-source assertions keep the
   newest `fetched_at`).
3. **Tombstone the loser with a `merged_into` pointer** (loser GUID → survivor GUID).
   The tombstone is never deleted, because consumers may still hold the old GUID.
   Resolution follows the `merged_into` chain (with path compression) to the live
   representative. This redirect is the union-find forest persisted to the metadata DB.

Merges are therefore idempotent and order-independent: the moment a bridging record
appears, two classes converge permanently and every consumer holding the old GUID
auto-resolves through the tombstone.

## The convergence boundary

Convergence is guaranteed **for any record that co-asserts two ids**. It is *not*
derivable from two naked ids that never co-occur in any record — if one session pushes
only `openalex:W…` and another only `doi:…` with no bridging record, no algorithm can
relate them, because no evidence exists.

Two structural choices keep that case rare and recoverable:

- **Never ingest naked.** `put_work` (`doc02.01.03`) takes the provider record and
  extracts the full alias bundle, so a consumer cannot register an id stripped of its
  crosswalk. With OpenAlex as the bundle-rich hub, nearly every record bridges, and the
  bridge is usually present before the second push arrives.
- **Healing is confluent.** A genuine fork resolves whenever a later record carrying
  both ids arrives, via the merge above — safely, in any order.

The shippable guarantee is thus: **no fork survives a bridging record, and naked
ingestion is impossible** — not the unachievable "no fork ever."

## Fuzzy fallback — a separate, non-guaranteed path

Records with no shared strong id (humanities books without DOI/ORCID) fall outside
union-find. They are matched by probabilistic record linkage — blocking on a
normalized `title+author+year` key (or MinHash/LSH), then a similarity threshold. This
path produces **merge proposals** that an agent or user confirms before any union is
committed; it never auto-merges. Keeping it as a distinct path ensures probabilistic
matching can never corrupt the deterministic core.
