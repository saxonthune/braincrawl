# `restricted` rights stores-and-holds bytes instead of rejecting them

## Motivation

Today `restricted` is a tombstone: `put_content` rejects the push with
`DomainError::RightsViolation` and stores **zero bytes**
(`crates/core/src/usecases.rs:199`), and `get_content` maps it to a 451
`ContentOutcome::Restricted` (`:257`). So the only way to hold bytes you can later
`extract-text` is `--rights open`, which forces mislabeling a copyrighted personal
copy as openly redistributable.

This task redefines `restricted` to be an honest **redistribution marker on stored
bytes**: the payload is stored and readable by the local owner exactly like `open`,
but the `rights=restricted` value rides on the payload row as the durable "do not
ship this if the corpus is ever served/shared" signal. The shared/served store does
not exist yet, so for now restricted reads simply return the bytes — there is no
non-owner to gate against. The 451 reject/serve path is **removed entirely** (per
the user's decision), not kept dormant.

`link_only` (genuine pointer-only, no bytes) is unchanged.

## Do NOT

- Do NOT add a new rights tier (`held`, `private`, etc.). Redefine the existing
  `Restricted` variant. Keep `enum Rights { Open, LinkOnly, Restricted }` exactly
  as three variants.
- Do NOT keep any 451 / `ContentOutcome::Restricted` / `DomainError::RightsViolation`
  scaffolding "for the future." Strip it out completely.
- Do NOT change `link_only` semantics or the `Open` path's behavior.
- Do NOT touch the alias/edge/resolver code, blob backends, or anything outside the
  rights-handling surface listed below.
- Do NOT remove the `Restricted` value from the rights string parsers/serializers
  in the store backends (`store-sqlite`, `store-d1`) — `restricted` is still a valid,
  persisted rights value; it just now means "stored, non-redistributable."

## Plan

### 1. Core types — remove the 451 enum surface (`crates/core/src/types.rs`)

- Delete the `RightsViolation(Rights)` variant from `enum DomainError` (line ~166-167,
  including its `#[error(...)]` attribute). `Rights` is still used elsewhere, so leave
  the `Rights` enum intact.
- Delete the `Restricted` variant from `enum ContentOutcome` (line ~250) and its
  `- Restricted → 451 ...` doc-comment line (~240).

### 2. Core usecases — store and serve restricted bytes (`crates/core/src/usecases.rs`)

- In `put_content` (~line 188): delete the early-reject block
  ```rust
  if rights == Rights::Restricted {
      return Err(DomainError::RightsViolation(Rights::Restricted));
  }
  ```
- Change the blob-write condition so bytes are stored for both `Open` and `Restricted`,
  i.e. store the blob for every rights value except `LinkOnly`. Replace
  `if rights == Rights::Open` (~line 213) with `if rights != Rights::LinkOnly`.
- In `get_content` (~line 256): collapse the `Restricted` arm into the `Open` arm so
  restricted reads return the stored bytes. The match becomes:
  ```rust
  match descriptor.rights {
      Rights::LinkOnly => match descriptor.source_url {
          Some(url) => Ok(ContentOutcome::RedirectUrl(url)),
          None => Ok(ContentOutcome::Pending),
      },
      Rights::Open | Rights::Restricted => match self.blob.get(&descriptor.r2_key).await? {
          None => Ok(ContentOutcome::Pending),
          Some(b) => Ok(ContentOutcome::Bytes {
              bytes: b.bytes,
              mime: b.mime,
              content_hash: b.content_hash,
          }),
      },
  }
  ```
- Update the doc comments on `put_content` (~lines 182-186) to describe the new
  behavior: `Restricted` and `Open` both store + serve bytes; `LinkOnly` stores
  descriptor only. Drop the "451 semantics" line.

### 3. Server runtime (`apps/server/src/lib.rs`)

- In `domain_status` (~line 194): delete the `DomainError::RightsViolation(_) => ...`
  arm. The `_ => INTERNAL_SERVER_ERROR` catch-all remains, so the match stays valid.
- In the GET-content response match (~line 310): delete the
  `Ok(ContentOutcome::Restricted) => StatusCode::from_u16(451)...` arm. The match is
  now exhaustive over the remaining `ContentOutcome` variants.

### 4. Worker runtime (`apps/worker/src/lib.rs`)

- In `domain_status` (~line 205): delete the `DomainError::RightsViolation(_) => 451`
  arm (the `_ => 500` catch-all remains).
- In the GET-content response match (~line 339): delete the
  `Ok(ContentOutcome::Restricted) => Response::error("unavailable for legal reasons", 451)`
  arm.

### 5. CLI store client (`apps/cli/src/store_client.rs`)

- Delete the `Restricted` variant from the CLI-local `enum ContentOutcome` (~line 20).
- In `get_content`'s status match (~line 212): delete the `451 => Ok(ContentOutcome::Restricted)`
  arm. A 451 should no longer occur; let it fall through to the existing
  `_ => Err(ClientError::Server { status, body })` arm.
- Update the doc comment listing the status→outcome mapping (~line 192) to drop
  `451 → Restricted`.

### 6. CLI commands (`apps/cli/src/main.rs`)

- Delete the `ContentOutcome::Restricted => { ... }` arm in the `extract-text` handler
  (~line 284) and in the `get-pdf` handler (~line 405). Both matches become exhaustive
  over the remaining variants (`Bytes`, `Redirect`, `Pending`, `Absent`).

### 7. Tests — assert restricted now stores and reads back

- `crates/core/tests/engine.rs` (~lines 446-464): the block labeled "Restricted
  content — put_content must error" must be rewritten. Instead of asserting
  `RightsViolation`, push a payload with `Rights::Restricted`, assert `put_content`
  succeeds, then assert `get_content` returns `ContentOutcome::Bytes { .. }` with the
  same bytes. Follow the style of the existing `Open`-path assertions in the same file.
- `crates/backends/store-sqlite/tests/engine.rs` (~lines 271-290): same rewrite —
  restricted push succeeds and the bytes read back via `get_content`.

### 8. Docs

- `.rhidoc/02-architecture/01-core/03-openapi.yaml`: in the get_content responses
  (~lines 36-41) remove the `"451": { description: restricted — never persisted }`
  line. Update the `"200"` description so it covers `rights=open` **and**
  `rights=restricted` (both return bytes). The PUT request `rights` enum
  (line ~97) stays `[open, link_only, restricted]`.
- `.rhidoc/02-architecture/01-core/01-layers.md` (~line 49): the `rights` row stays,
  but if there is any prose nearby implying restricted is not persisted, correct it
  to "restricted = stored but non-redistributable."
- `.rhidoc/02-architecture/01-core/03-api.md`: if the `get_content` line (~33) or
  surrounding prose describes restricted as 451/never-persisted, update it to reflect
  that restricted bytes are stored and served to the owner.

## Files to Modify

- `crates/core/src/types.rs` — drop `RightsViolation` and `ContentOutcome::Restricted`
- `crates/core/src/usecases.rs` — store + serve restricted bytes; update doc comments
- `apps/server/src/lib.rs` — drop 451 arms in `domain_status` + content GET
- `apps/worker/src/lib.rs` — drop 451 arms in `domain_status` + content GET
- `apps/cli/src/store_client.rs` — drop `Restricted` variant + 451 mapping
- `apps/cli/src/main.rs` — drop `Restricted` arms in `extract-text` and `get-pdf`
- `crates/core/tests/engine.rs` — rewrite restricted test to assert stored+readable
- `crates/backends/store-sqlite/tests/engine.rs` — same rewrite
- `.rhidoc/02-architecture/01-core/03-openapi.yaml` — remove 451; fix 200 description
- `.rhidoc/02-architecture/01-core/01-layers.md`, `.../03-api.md` — correct prose if present

## Verification

```bash
cargo build --bin braincrawl-server --bin braincrawl
cargo test
# worker targets wasm only; check it still compiles if the target is installed
rustup target list --installed | grep -q wasm32-unknown-unknown && \
  cargo check -p braincrawl-worker --target wasm32-unknown-unknown || \
  echo "skip worker check: wasm32 target not installed"
# sanity: no 451 / RightsViolation logic survives
! grep -rn "RightsViolation\|ContentOutcome::Restricted\|451" crates/ apps/ --include=*.rs
```

## Out of Scope

- The shared/served multi-user store and any owner-vs-non-owner read gating — does
  not exist yet. This task only stops throwing the bytes away.
- `link_only` semantics — unchanged.
- Any rights-related UI/CLI flag changes beyond the behavior described.

## Notes

- The whole policy change lives in `crates/core` (`put_content` + `get_content`); the
  server/worker/cli are thin mappers, so their edits are pure deletions of the now-dead
  451 arms. Expect the compiler to point at every leftover reference once the two
  core enum variants are removed — follow the errors.
- `DomainError::RightsViolation` was the *only* producer of 451; after removal nothing
  returns it, which is why the enum variant goes too.
- `restricted` remains a fully valid, round-trippable rights value in the sqlite/d1
  string maps — do not touch those parse/serialize arms.
