# Worker auth: accept KV allowlist tokens alongside the shared secret

## Motivation

Email OTP needs somewhere to put minted session tokens. `doc02.05` already
specifies the shape — a hashed-token allowlist in KV, `SHA-256(token) →
{tenant, status}` — and the worker's `AUTH_KV` binding is declared but
unwired. This phase wires the lookup into the auth gate. Tenant
*threading* (isolation of per-tenant state) stays out; tenant is still
`"default"` everywhere.

## Do NOT

- Do NOT remove or weaken the `AUTH_TOKEN` shared-secret path — the CLI
  and conformance suite depend on it; KV lookup is an additional
  acceptance path, tried only when the shared secret does not match.
- Do NOT thread tenant into usecases/stores — the gate resolves a tenant
  string and the worker keeps using it exactly as it uses `"default"`
  today. Tenancy isolation is a separate deferred task.
- Do NOT touch the native server — this is a deliberate, documented
  parity exception: session tokens are a worker concern (the PWA's
  backend); the server keeps shared-secret-only auth.
- Do NOT make `crates/auth` depend on worker/KV/async runtimes — pure
  string/JSON logic only.

## Plan

### 1. crates/auth: allowlist entry type

`crates/auth/src/lib.rs` already has `hash_token`, `parse_bearer`,
`Allowlist`, `authorize`. Add:

```rust
#[derive(serde::Serialize, serde::Deserialize)]
pub struct KvEntry { pub tenant: String, pub status: String }
pub fn parse_kv_entry(json: &str) -> Option<KvEntry>   // None on bad JSON
impl KvEntry { pub fn is_active(&self) -> bool }        // status == "active"
```

Unit tests: active entry parses, revoked entry rejected by
`is_active`, malformed JSON → None. (Add serde/serde_json to the crate
if absent — check first.)

### 2. Worker gate

`apps/worker/src/lib.rs` around line 453: after the existing
shared-secret `authorize` fails (and only then), take the bearer token
from the header, compute `hash_token`, `env.kv("AUTH_KV")?.get(&hash)`
(async), parse with `parse_kv_entry`, and authorize when `is_active()`,
using the entry's tenant where `"default"` is used today. No header, no
KV hit, inactive, or malformed → the existing 401. The KV read runs only
on shared-secret miss, so the common CLI path costs no KV operation.

### 3. Config note

`apps/worker/wrangler.toml.example`: update the AUTH_KV comment block —
it is now wired: entries are `SHA-256(token) → {"tenant":...,
"status":"active"|"revoked"}`; the shared secret remains valid
alongside.

## Files to Modify

- `crates/auth/src/lib.rs` (+ Cargo.toml if serde is new there)
- `apps/worker/src/lib.rs` — gate extension
- `apps/worker/wrangler.toml.example` — comment update

## Verification

```bash
cargo test -p braincrawl-auth
cargo test --workspace
./scripts/worker-conformance.sh
```

(Conformance must pass unchanged — it authenticates via the shared
secret, which this phase must not disturb.)

## Out of Scope

- Tenant isolation threading (inbox draft `worker-auth-kv-tenancy`
  keeps the remainder)
- Any endpoint additions (next phase)
- Native server changes

## Surface after this phase

- Worker auth gate accepts EITHER the `AUTH_TOKEN` shared secret OR a
  bearer token whose `SHA-256` hex hash is an `AUTH_KV` key holding
  `{"tenant": <t>, "status": "active"}`; revoked/absent/malformed → 401.
- `braincrawl_auth` exports `KvEntry { tenant, status }`,
  `parse_kv_entry(&str) -> Option<KvEntry>`, `KvEntry::is_active()`,
  alongside the existing `hash_token`/`parse_bearer`/`authorize`.
- Negative space: shared-secret auth, all routes, and the native server
  are byte-identical in behavior; nothing writes to AUTH_KV yet.
