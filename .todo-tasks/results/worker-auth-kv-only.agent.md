# Agent Result: worker-auth-kv-only

date: 2026-07-15T10:04:44-04:00
session: completed
verification: passed
commits: 3
branch: feat/milestone01_claude_worker-auth-kv-only
surface deviations: none
turns: 42/100
cost: $1.2637353999999998/$5.00
uncommitted: none
session id: 9aaa1eae-9404-4af2-9570-4c37336cf48d


## Summary

None. The plan had no explicit `## Surface after this phase` block.

## Commits

```
287007d worker: update wrangler.toml.example AUTH_KV comment for shared-secret removal
c6b5427 pwa: remove manual token paste affordances
05ad2f4 worker: gate on AUTH_KV allowlist only, drop AUTH_TOKEN shared secret
```

## Build & Test Output (last 30 lines)

```
vite v8.1.3 building client environment for production...
[2Ktransforming...[plugin rolldown:vite-resolve] Module "node:path" has been externalized for browser compatibility, imported by "/home/saxon/code/github/saxonthune/agent-braincrawl-worker-auth-kv-only/web/node_modules/.pnpm/@anthropic-ai+sdk@0.111.0/node_modules/@anthropic-ai/sdk/lib/credentials/types.mjs". See https://vite.dev/guide/troubleshooting.html#module-externalized-for-browser-compatibility for more details.
[plugin rolldown:vite-resolve] Module "node:fs" has been externalized for browser compatibility, imported by "/home/saxon/code/github/saxonthune/agent-braincrawl-worker-auth-kv-only/web/node_modules/.pnpm/@anthropic-ai+sdk@0.111.0/node_modules/@anthropic-ai/sdk/lib/credentials/types.mjs". See https://vite.dev/guide/troubleshooting.html#module-externalized-for-browser-compatibility for more details.
[plugin rolldown:vite-resolve] Module "node:fs" has been externalized for browser compatibility, imported by "/home/saxon/code/github/saxonthune/agent-braincrawl-worker-auth-kv-only/web/node_modules/.pnpm/@anthropic-ai+sdk@0.111.0/node_modules/@anthropic-ai/sdk/lib/credentials/credential-chain.mjs". See https://vite.dev/guide/troubleshooting.html#module-externalized-for-browser-compatibility for more details.
[plugin rolldown:vite-resolve] Module "node:fs" has been externalized for browser compatibility, imported by "/home/saxon/code/github/saxonthune/agent-braincrawl-worker-auth-kv-only/web/node_modules/.pnpm/@anthropic-ai+sdk@0.111.0/node_modules/@anthropic-ai/sdk/lib/credentials/identity-token.mjs". See https://vite.dev/guide/troubleshooting.html#module-externalized-for-browser-compatibility for more details.
[plugin rolldown:vite-resolve] Module "node:path" has been externalized for browser compatibility, imported by "/home/saxon/code/github/saxonthune/agent-braincrawl-worker-auth-kv-only/web/node_modules/.pnpm/@anthropic-ai+sdk@0.111.0/node_modules/@anthropic-ai/sdk/core/credentials.mjs". See https://vite.dev/guide/troubleshooting.html#module-externalized-for-browser-compatibility for more details.
[plugin rolldown:vite-resolve] Module "node:fs" has been externalized for browser compatibility, imported by "/home/saxon/code/github/saxonthune/agent-braincrawl-worker-auth-kv-only/web/node_modules/.pnpm/@anthropic-ai+sdk@0.111.0/node_modules/@anthropic-ai/sdk/core/credentials.mjs". See https://vite.dev/guide/troubleshooting.html#module-externalized-for-browser-compatibility for more details.
[plugin rolldown:vite-resolve] Module "node:fs" has been externalized for browser compatibility, imported by "/home/saxon/code/github/saxonthune/agent-braincrawl-worker-auth-kv-only/web/node_modules/.pnpm/@anthropic-ai+sdk@0.111.0/node_modules/@anthropic-ai/sdk/lib/credentials/user-oauth.mjs". See https://vite.dev/guide/troubleshooting.html#module-externalized-for-browser-compatibility for more details.
✓ 153 modules transformed.
rendering chunks...
computing gzip size...
dist/index.html                                    0.84 kB │ gzip:  0.41 kB
dist/assets/index-CBo7zMh5.css                     3.46 kB │ gzip:  1.30 kB
dist/assets/__vite-browser-external-DS6vgwWv.js    0.09 kB │ gzip:  0.10 kB
dist/assets/node.browser-C_yFqa3t.js               0.97 kB │ gzip:  0.40 kB
dist/assets/index-C8iFWK7E.js                    244.00 kB │ gzip: 71.19 kB

✓ built in 294ms

> web@0.0.0 test /home/saxon/code/github/saxonthune/agent-braincrawl-worker-auth-kv-only/web
> vp test


 RUN  v4.1.10 /home/saxon/code/github/saxonthune/agent-braincrawl-worker-auth-kv-only/web


 Test Files  3 passed (3)
      Tests  18 passed (18)
   Start at  10:04:43
   Duration  827ms (transform 145ms, setup 0ms, import 210ms, tests 22ms, environment 1.67s)
```
