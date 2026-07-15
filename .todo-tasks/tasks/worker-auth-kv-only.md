# Worker auth: drop the shared secret — KV allowlist tokens only

## Motivation

DECIDED: the worker authenticates exclusively against the KV allowlist
(email-OTP sessions and explicitly minted tokens). The `AUTH_TOKEN`
shared secret is an immortal, unrevocable credential; every remaining
consumer can hold a revocable KV token instead. The CLI gets a minting
recipe; conformance seeds a KV entry into its isolated local state.

## Do NOT

- Do NOT touch the native server — it keeps `SharedSecret` over its own
  local token (`server.env`); this is the established parity exception
  for auth.
- Do NOT remove `SharedSecret` from `crates/auth` — the server uses it.
- Do NOT change the OTP endpoints or session semantics from the
  auth-otp chain.
- Do NOT weaken any 401 behavior — no header, unknown hash, revoked or
  malformed entry all still 401.
- Do NOT print a minted token's hash and value into files — the recipe
  echoes the token to stdout once and writes nothing to disk.

## Plan

### 1. Worker gate

`apps/worker/src/lib.rs` (~line 453): delete the `env.secret("AUTH_TOKEN")`
+ `SharedSecret` path entirely. The gate becomes: parse bearer →
`hash_token` → AUTH_KV lookup → `parse_kv_entry` → `is_active()` →
authorized with the entry's tenant; anything else 401. Remove the
now-unused `braincrawl_auth::SharedSecret`/`authorize` imports from the
worker (they remain used by the server).

### 2. Conformance script

`scripts/worker-conformance.sh`:
- Drop the `.dev.vars` injection (step 2) and its cleanup line.
- After the D1 migrations step, seed the allowlist into the isolated
  state: compute `HASH=$(printf '%s' "$TEST_TOKEN" | sha256sum | cut -d' ' -f1)`
  then
  `wrangler kv key put --binding AUTH_KV "$HASH" '{"tenant":"default","status":"active"}' --local --persist-to "$PERSIST_DIR"`
  (run from `$WORKER_DIR` like the migrations step; verify the exact
  wrangler 4.x `kv key put` syntax against `wrangler kv key put --help`
  before relying on it).
- Everything else (readiness probe with bearer, suite, negative 401
  check) is unchanged and must pass as-is.

### 3. Mint recipe

`justfile`: add `mint-worker-token`:
1. `TOKEN=$(openssl rand -hex 32)`
2. `HASH=$(printf '%s' "$TOKEN" | sha256sum | cut -d' ' -f1)`
3. `wrangler kv key put "$HASH" '{"tenant":"default","status":"active"}' --binding AUTH_KV --remote` from `apps/worker`
4. echo the token with a "store it in your password manager; config.toml
   auth_token" note.
Follow the justfile's existing shebang-recipe style (see
`deploy-worker`). Long-lived (no TTL) is intentional for CLI tokens;
revoke = delete the KV key.

### 4. PWA

- `web/src/plugins/chat/Settings.tsx`: remove the "Store token"
  PasswordField (the `bc.settings.storeToken` slot itself stays — OTP
  sessions land there via the auth page; services read it unchanged).
- `web/src/plugins/auth/`: remove the "have a token? paste it instead"
  link/text if present.
- Leave everything else (services, guard, transports) untouched.

### 5. Docs touchpoint

`apps/worker/wrangler.toml.example`: update the AUTH_KV comment — the
shared secret is gone; tokens come from email OTP or
`just mint-worker-token`; delete the stale
"Provision the shared secret via: wrangler secret put AUTH_TOKEN" line.

## Files to Modify

- `apps/worker/src/lib.rs` — gate
- `scripts/worker-conformance.sh` — KV seed instead of .dev.vars
- `justfile` — mint-worker-token
- `web/src/plugins/chat/Settings.tsx`, `web/src/plugins/auth/*` — remove
  paste affordances
- `apps/worker/wrangler.toml.example` — comment update

## Verification

```bash
cargo test --workspace
./scripts/worker-conformance.sh
CI=true pnpm -C web install
pnpm -C web build
pnpm -C web test
```

## Out of Scope

- Native server auth
- Tenancy threading (`worker-auth-kv-tenancy` draft)
- Session management UI

## Notes

- User runbook after merge: `just mint-worker-token` → put the token in
  the password manager and in `~/.config/braincrawl/config.toml`
  `auth_token` → `just deploy-worker` → `wrangler secret delete
  AUTH_TOKEN`. Browsers holding the old shared secret will 401 into the
  email sign-in flow — expected.
- Dev escape hatch: with the paste field gone, setting a token in a
  browser without email requires devtools
  (`localStorage.setItem("bc.settings.storeToken", ...)`) — acceptable;
  note it in the result summary.
