# Worker: OpenRouter proxy so devices never hold the LLM key

## Motivation

The OpenRouter key currently lives in each browser's localStorage — the
most exposed secret in the system. Holding it as a worker secret and
proxying LLM calls means a phone session needs only its email-minted
session token: sign in, chat, no key paste. Revoking a session then also
revokes LLM spend.

## Do NOT

- Do NOT proxy arbitrary OpenRouter paths — allowlist exactly
  `v1/messages` and `v1/chat/completions`, POST only. Anything else
  under `/api/llm/` → 404. (Prevents the proxy reaching OpenRouter
  account/credits endpoints.)
- Do NOT buffer or transform response bodies — return the upstream
  `Response` so streaming (SSE) passes through untouched.
- Do NOT log request or response bodies (they contain conversation
  content).
- Do NOT remove the client-side key path in the PWA — a pasted key
  still overrides the proxy (direct Anthropic or direct OpenRouter,
  current behavior unchanged).
- Do NOT touch the native server, CLI, tools, or session storage.

## Plan

### 1. Worker route

In `apps/worker/src/lib.rs`, inside the authed dispatch (behind the
existing KV-allowlist gate): paths starting with `/api/llm/` go to a new
`apps/worker/src/llm_proxy.rs`:

- Strip the `/api/llm/` prefix; require the remainder to be exactly
  `v1/messages` or `v1/chat/completions` and method POST, else 404.
- Read `env.secret("OPENROUTER_API_KEY")`; missing → 503 JSON
  `{"error":"llm proxy not configured"}`.
- Forward to `https://openrouter.ai/api/<remainder>`: pass through the
  request body and `Content-Type` plus the `anthropic-version` header if
  present; set `Authorization: Bearer <secret>` (the client's own
  Authorization header must NOT be forwarded); add `X-Title: braincrawl`.
- Return the upstream response as-is (status, headers, streaming body).
  Check what the `worker` crate needs to pass a streaming body through —
  use `Fetch::Request`/raw response passthrough rather than `.text()`.

### 2. PWA transports

- `web/src/plugins/chat/transport.ts`: when the API key setting is
  empty, run in proxy mode — Anthropic SDK client constructed with
  `baseURL: <origin>/api/llm` (derive absolute URL from
  `window.location.origin` + the storeBaseUrl setting when set) and
  `authToken: getSetting("storeToken")`, `dangerouslyAllowBrowser`.
  When the key is set, current key-prefix behavior is unchanged.
- Endpoint/format selection in proxy mode is slug-based: model starting
  with `anthropic/` or `~anthropic/` → Anthropic messages path; any
  other slug containing `/` → the OpenAI-format caller pointed at
  `<origin>/api/llm/v1/chat/completions` with
  `Authorization: Bearer <storeToken>`; a bare model id without `/` in
  proxy mode → the existing in-chat error note pattern, updated to say
  proxy mode expects OpenRouter slugs.
- `web/src/plugins/chat/openaiCompletions.ts`: parameterize the endpoint
  URL and auth header value (currently hardcoded to openrouter.ai +
  the key) so both direct and proxy modes share it.
- `web/src/plugins/chat/Settings.tsx`: API key field hint becomes
  "optional — leave empty to use the server's key"; empty is now the
  recommended default.
- Empty key + empty model stays sensible: make the model default
  `~anthropic/claude-sonnet-latest` in `settings.ts` DEFAULTS (proxy
  mode makes OpenRouter slugs the norm; document in the hint that a
  pasted `sk-ant-` key needs a plain Anthropic model id instead).

### 3. Conformance

`crates/conformance`: `check_llm_proxy(base_url, token)` — authed POST
to `/api/llm/v1/chat/completions` with a trivial body returns 503 (no
secret configured in the dev environment, proving the route exists,
is authed, and fails closed); unauthenticated POST → 401; authed POST to
`/api/llm/v1/other` → 404. Invoke from the worker external test.

### 4. Docs touchpoint

`apps/worker/wrangler.toml.example`: comment documenting
`wrangler secret put OPENROUTER_API_KEY` as optional — absent means the
proxy answers 503 and clients must bring their own key.

## Files to Modify

- `apps/worker/src/llm_proxy.rs` — new
- `apps/worker/src/lib.rs` — route
- `web/src/plugins/chat/transport.ts`, `openaiCompletions.ts`,
  `Settings.tsx`, `settings.ts` — proxy mode
- `crates/conformance/src/lib.rs` + worker external test
- `apps/worker/wrangler.toml.example` — secret comment

## Verification

```bash
cargo test --workspace
./scripts/worker-conformance.sh
CI=true pnpm -C web install
pnpm -C web build
pnpm -C web test
```

## Out of Scope

- Per-session spend metering/quotas (single user; OpenRouter credit
  limit is the backstop)
- Anthropic-key proxying (OpenRouter covers Anthropic models via its
  Anthropic-compatible endpoint)
- Prompt-cache verification through the proxy

## Notes

- User runbook after merge: `wrangler secret put OPENROUTER_API_KEY`,
  `just deploy-worker`, then clear the API key field in the PWA settings
  on each device (or just on new devices — pasted keys keep working).
- The proxy binds LLM spend to store sessions: a leaked session token
  can now spend OpenRouter credits until revoked — acceptable for one
  user with a credit limit set; revisit if tokens ever multiply.
