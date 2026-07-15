# Worker: email OTP endpoints minting session tokens

## Motivation

Browser users should sign in with a one-time code emailed to an
allowlisted address instead of pasting the shared secret. Verified codes
mint session tokens into the AUTH_KV allowlist the previous phase wired
into the gate. The allowlist of addresses is a worker secret — the
client never reads it, and both auth endpoints answer uniformly so
membership cannot be probed.

## Do NOT

- Do NOT expose any endpoint that returns the allowlist or reveals
  whether an email is on it — `request-code` returns the same 200 body
  for allowlisted and unknown addresses.
- Do NOT store plaintext codes or tokens in KV — store SHA-256 hashes
  (reuse `braincrawl_auth::hash_token`).
- Do NOT put these endpoints behind the auth gate — they are the way in;
  mount them before the gate like `/health`.
- Do NOT add an email-provider SDK — Resend is one `fetch` call.
- Do NOT touch the native server or the CLI.
- Do NOT let a missing `RESEND_API_KEY`/`ALLOWED_EMAILS` secret panic —
  treat missing secrets as "nothing sent / nobody allowlisted", still
  answering uniformly (keeps local wrangler-dev and conformance green
  with no secrets configured).

## Plan

### 1. Pure logic in crates/auth (testable off-worker)

Add to `crates/auth`:

- `pub fn normalize_email(raw: &str) -> String` — trim + lowercase.
- `pub fn email_allowed(allowlist_csv: &str, email: &str) -> bool` —
  split the secret on commas, normalize both sides, exact match.
- `pub struct OtpRecord { pub code_hash: String, pub attempts: u32 }`
  with serde, plus `parse`/`to_json` helpers (mirror `KvEntry` style).

Unit tests: case/space-insensitive matching, empty csv matches nothing,
record round-trip.

### 2. Worker module

New `apps/worker/src/auth_otp.rs`, routes mounted in `lib.rs` BEFORE the
auth gate:

- `POST /api/auth/request-code` — body `{"email": ...}`:
  1. Normalize; rate-limit via KV key `otp-rl/<sha256(email)>`
     (integer count, TTL 3600s, max 5/hour) — over limit → same uniform
     200.
  2. If `ALLOWED_EMAILS` secret contains the address: generate a 6-digit
     code from crypto-grade randomness (`getrandom`/worker crypto — match
     what the workspace already uses for randomness, e.g. `crates/l3`'s
     id generation), store `otp/<sha256(email)>` →
     `OtpRecord { code_hash: sha256(code), attempts: 0 }` TTL 600s,
     and send the email via Resend:
     `POST https://api.resend.com/emails` with bearer `RESEND_API_KEY`,
     from `EMAIL_FROM` (a `[vars]` entry), subject/body containing the
     code and a 10-minute expiry note.
  3. Respond `200 {"ok": true}` in every branch (unknown email, rate
     limited, send failure — log failures via `console_log!`).
- `POST /api/auth/verify` — body `{"email":..., "code":...}`:
  1. Load `otp/<sha256(email)>`; absent → `401 {"error":"invalid"}`.
  2. `attempts >= 5` → delete record, 401. Hash mismatch → increment
     attempts, write back (keep TTL short — rewrite with 600s), 401.
  3. Match → delete the OTP record, mint a session token (32 random
     bytes, hex), write `AUTH_KV` key `sha256(token)` →
     `{"tenant":"default","status":"active"}` with TTL 30 days
     (2592000s), return `200 {"token": <plaintext>}`.

Note KV keys: OTP + rate-limit records live in the same AUTH_KV
namespace under the `otp/`/`otp-rl/` prefixes — distinct from token
hashes, which are bare hex and can never collide with prefixed keys.

### 3. Config

- `apps/worker/wrangler.toml.example` — `[vars] EMAIL_FROM =
  "braincrawl <auth@example.com>"` plus a comment documenting the two
  secrets (`ALLOWED_EMAILS` comma-separated, `RESEND_API_KEY`) and that
  both are optional for local dev.

### 4. Conformance-adjacent check

Extend `crates/conformance` with `check_auth_otp_uniform(base_url)`:
`request-code` for an arbitrary address returns 200 `{"ok":true}` (no
secrets configured in the dev environment, so this exercises the
uniform-response branch), and `verify` with a bogus code returns 401.
Invoke it from the worker external test next to the existing checks.

## Files to Modify

- `crates/auth/src/lib.rs` — email + OTP record helpers
- `apps/worker/src/auth_otp.rs` — new
- `apps/worker/src/lib.rs` — route mounting before the gate
- `apps/worker/wrangler.toml.example` — vars + secret docs
- `crates/conformance/src/lib.rs` + worker external test — uniform check

## Verification

```bash
cargo test -p braincrawl-auth
cargo test --workspace
./scripts/worker-conformance.sh
```

## Out of Scope

- Session listing/revocation UI (revoke = delete the KV key by hand)
- SMS or any second channel
- Native server OTP (documented parity exception)

## Notes

- User-side runbook after merge: create a Resend account, verify the
  sending domain (SPF/DKIM records on the zone), then
  `wrangler secret put RESEND_API_KEY` and
  `wrangler secret put ALLOWED_EMAILS`, set `EMAIL_FROM` in the live
  toml, redeploy.

## Surface after this phase

- `POST /api/auth/request-code {email}` → always `200 {"ok":true}`;
  sends a 6-digit code (TTL 10 min) when the address is in the
  `ALLOWED_EMAILS` secret; ≤5 requests/address/hour.
- `POST /api/auth/verify {email, code}` → `200 {"token": ...}` minting a
  30-day session token accepted by the auth gate (previous phase), or
  `401 {"error":"invalid"}`; ≤5 attempts per code.
- Both endpoints are unauthenticated and mounted before the gate.
- Negative space: every authed route, the shared secret, CLI, and native
  server unchanged; no allowlist read endpoint exists.
