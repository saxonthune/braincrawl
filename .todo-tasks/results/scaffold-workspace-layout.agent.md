# Agent Result: scaffold-workspace-layout

date: 2026-06-16T12:14:22-04:00
session: completed
verification: passed
commits: 1
branch: feat/milestone01_claude_scaffold-workspace-layout
surface deviations: declared
session id: 14546ae8-3e24-4ff3-9945-6a2157cc4f23


## Summary

Commit confirmed. Implementation complete.

---

## Implementation Summary

The Cargo workspace skeleton is fully in place. `cargo check --workspace` and `cargo fmt --all -- --check` both pass. All 29 new files committed in one atomic commit (`a9a613f`).

**What was built:**
- `Cargo.toml` workspace root with 11 member crates
- `.gitignore` (`/target`)
- `crates/core`: domain types + 4 port traits (`BlobStore`, `PayloadsRepo`, `MetadataStore`, `IdResolver`) with `#[async_trait(?Send)]`
- 7 adapter crates, each implementing their port with `todo!()` bodies
- `crates/sql`: shared query home placeholder
- `apps/server`: native binary stub, `apps/worker`: `cdylib`+`rlib` stub with `wrangler.toml`
- `migrations/0001_init.sql`: `payloads` table DDL

## Notes

- **Workspace dep alias renamed:** The plan specified `core = { path = "crates/core", package = "braincrawl-core" }` in `[workspace.dependencies]`, but naming a dep `core` shadows Rust's built-in `core` crate and breaks `async_trait`'s macro-generated code (`::core::pin`, `::core::future`, etc.). The key was renamed to `braincrawl-core` throughout. This is the only structural deviation from the plan's exact text, and it was necessary for `cargo check` to pass.
- `Rights` needed `#[derive(Debug)]` to satisfy `thiserror`'s `{0:?}` format in `DomainError::RightsViolation` — minor but required.
- No infrastructure dependencies anywhere; all adapter bodies are `todo!()`.

## Surface Deviations

- **Workspace dependency key renamed from `core` to `braincrawl-core`** (plan said `core = { path = "crates/core", package = "braincrawl-core" }`). The external contract — crate name `braincrawl-core`, all types and traits exported — is unchanged. Only the Cargo dep alias inside member `Cargo.toml` files changed (from `core` to `braincrawl-core`), which means in Rust source the import prefix is `braincrawl_core::` rather than `core::`. The Surface's named symbols (`WorkId`, `Kind`, `Rights`, `StoredBlob`, `PayloadDescriptor`, `DomainError`, `BlobStore`, `PayloadsRepo`, `MetadataStore`, `IdResolver`) are all present and exported correctly.

## Commits

```
a9a613f feat: scaffold Cargo workspace layout
```

## Build & Test Output (last 30 lines)

```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.01s
OK
```
