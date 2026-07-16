# L3 catalog references: any alias namespace + honest Alias type

## Motivation

An L3 catalog reference (`- catalog [[openalex:W1]]`) is parsed by
`crates/l3/src/model.rs::Endpoint::resolve`, which today:

1. **Hardcodes a two-namespace allowlist** (`openalex:`/`doi:`). Anything else —
   `isbn:`, `oclc:`, `pmid:` — falls through to the bare-slug branch and is silently
   misclassified as a research node (`doc:isbn:...`). Books referenced by ISBN/OCLC
   cannot be catalog references at all.
2. **Wraps the raw alias string in `Endpoint::Catalog(CanonicalId(...))`.** The type
   claims a resolved canonical id but holds an unresolved provider alias — the exact
   OpenAlex-as-canonical conflation the glossary (`doc01.01`) and id-resolution doc
   (`doc02.01.02`) now explicitly rule out: the canonical id is a braincrawl-minted
   UUID; every external id is an alias that resolves to it.

This task fixes the parsing and the type, and adds `uuid:` as the reserved reference
form for a work that has no external id (e.g. a book) — you reference it by its
canonical id directly.

## Do NOT

- Do NOT change the on-disk `.l3.md` reference syntax. `- catalog [[openalex:W1]]`
  still parses; `ns:value` must round-trip through serialize/deserialize unchanged.
- Do NOT resolve aliases to the canonical UUID in this task. No `StoreClient` calls, no
  network. `Endpoint::Catalog` holds the alias as authored; resolution is Part B (a
  separate follow-up). `uuid:` is parsed as an ordinary `Alias { namespace: "uuid",
  value }` — its "already canonical, skip the store" meaning belongs to Part B.
- Do NOT keep calling the Work index a "canonical id" index or "split-proof join key"
  in generated output — it is still keyed by the alias string until Part B. Describe it
  as a work-reference index.
- Do NOT special-case `uuid:` at parse time beyond it being a valid namespace.
- Do NOT reintroduce a namespace allowlist — accept any `[a-z0-9_]+` namespace.

## Plan

### 1. `crates/l3/src/model.rs` — the type and parsing

- Import `Alias`: `use braincrawl_core::types::{Alias, CanonicalId};` (keep
  `CanonicalId` — still used elsewhere in the crate).
- Change the enum:
  ```rust
  pub enum Endpoint {
      Node(NodeId),
      Catalog(Alias),
  }
  ```
- Add a shared catalog-parse helper and rewrite `resolve` to decide node-vs-catalog by
  shape, with no namespace allowlist:
  ```rust
  fn parse_alias(s: &str) -> Option<Alias> {
      let (ns, value) = s.split_once(':')?;
      if value.is_empty() || !ns.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_')) {
          return None;
      }
      Some(Alias { namespace: ns.to_string(), value: value.to_string() })
  }

  pub fn resolve(target: &str) -> Endpoint {
      if let Some(anchor) = target.strip_prefix('^') {
          return Endpoint::Node(NodeId(anchor.to_string()));
      }
      if let Some(alias) = parse_alias(target) {
          return Endpoint::Catalog(alias);
      }
      Endpoint::Node(NodeId(format!("doc:{target}")))
  }
  ```
  Note the order: `^anchor` is checked first so an anchor never parses as an alias. A
  colon-free doc slug has no `:` so `parse_alias` returns `None` → doc node. Update the
  doc-comment to describe "any `namespace:value` → a catalog reference" and name `uuid:`
  as the canonical-id form.
- Serialize the catalog arm as `ns:value`:
  ```rust
  Endpoint::Catalog(a) => format!("{}:{}", a.namespace, a.value),
  ```
- Deserialize: `node:` prefix → `Node`; otherwise parse via `parse_alias`, and if that
  returns `None` (no colon — should not occur for a well-formed catalog endpoint) fall
  back to `Alias { namespace: String::new(), value: s }` so deserialize is total.

### 2. `crates/l3/src/parse.rs` — update tests to the new type

`resolve` call sites (lines ~249, ~251, ~266) are unchanged. The unit tests that
assert on `Endpoint::Catalog(id) if id.0 == "openalex:W1"` (lines ~554, ~563) must
change to match on the `Alias` fields, e.g.
`Endpoint::Catalog(a) if a.namespace == "openalex" && a.value == "W1"`. Add cases:
- `resolve("isbn:9780521179799")` → `Catalog(Alias{namespace:"isbn", ...})`.
- `resolve("uuid:0f9a1b2c-...")` → `Catalog(Alias{namespace:"uuid", ...})`.
- `resolve("some-doc-slug")` → `Node(doc:some-doc-slug)` (colon-free stays a node).
- `resolve("^r-aaa1")` → `Node(r-aaa1)`.

### 3. `apps/cli/src/l3.rs` — update the two consumers of `Catalog`

- Reading-list `work_id` extraction (~287–293): both arms return `dst.0.clone()` /
  `src.0.clone()`. Replace with `format!("{}:{}", a.namespace, a.value)` on the bound
  `Alias`. The `work_id` stays a display string in the same `ns:value` form.
- Work-index builder (~824): `work_index.entry(id.0.clone())` → key by
  `format!("{}:{}", id.namespace, id.value)` on the bound `Alias` (still an alias-string
  key in this task).
- Index header text (~890): replace
  `"Canonical ids (`openalex:`/`doi:`) → docs that reference them — the split-proof join key."`
  with an accurate line, e.g.
  `"Work references (`openalex:`/`doi:`/`isbn:`/…/`uuid:`) → docs that reference them."`

### 4. Docs — list the recognized reference forms (land with the code)

- `.rhidoc/02-architecture/01-core/04-l3-conventions.md`: the link-line bullet says a
  target is `^r-…`, a bare `slug`, or `openalex:…`/`doi:…`. Broaden to "or any
  `namespace:value` catalog reference (`openalex:`/`doi:`/`isbn:`/`pmid:`/…, or `uuid:`
  for a work referenced by its canonical id directly)."
- `.claude/skills/braincrawl/SKILL.md`: the node-grammar link-line bullet (~line 251)
  has the same enumeration — broaden it the same way, and note `uuid:` is how you
  reference a work (e.g. a book) that has no external id.

## Files to Modify

- `crates/l3/src/model.rs` — `Endpoint::Catalog(Alias)`, `parse_alias`, `resolve`, serde.
- `crates/l3/src/parse.rs` — test assertions + new resolve cases.
- `apps/cli/src/l3.rs` — reading-list `work_id`, work-index key, index header text.
- `.rhidoc/02-architecture/01-core/04-l3-conventions.md` — reference-forms enumeration.
- `.claude/skills/braincrawl/SKILL.md` — reference-forms enumeration.

## Verification

```bash
cargo test -p l3
cargo test -p braincrawl-cli l3
cargo build -p braincrawl-cli
```

The new `resolve` unit tests must show `isbn:`/`uuid:` targets becoming `Catalog`
endpoints and a colon-free slug staying a `Node`. `cargo test -p braincrawl-cli l3`
must keep the existing work-index reindex test green (its key is still the `ns:value`
string).

## Out of Scope

- **Part B (follow-up task):** the Work-index builder resolves each catalog `Alias` to
  its canonical UUID via `StoreClient` (skipping the call for the `uuid:` namespace,
  whose value already IS the canonical id), keys the index by UUID for a genuinely
  split-proof join, and falls back to the `ns:value` string when the server is
  unreachable or the work is not yet stored. Depends on this task's `Alias` type.
- Fuzzy/probabilistic matching for no-id works (`doc02.01.02` fuzzy fallback).

## Surface after this phase

- `Endpoint::Catalog` carries `braincrawl_core::types::Alias { namespace, value }` — an
  unresolved external reference exactly as authored, never a resolved UUID.
- `Endpoint::resolve` maps any `namespace:value` (`[a-z0-9_]+` namespace, non-empty
  value) to a `Catalog` endpoint; `^anchor` and colon-free slugs stay `Node` endpoints.
- `uuid:` is a valid namespace like any other at parse time; the convention is that its
  value is the work's canonical id. No resolution happens in this phase.
- The Work index is still keyed by the `ns:value` alias string (NOT the UUID). Part B
  changes this key to the resolved canonical UUID.
- On-disk `.l3.md` reference syntax is unchanged; `ns:value` round-trips through serde.
