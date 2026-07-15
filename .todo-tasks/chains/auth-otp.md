# Chain: auth-otp

chain: auth-otp
phases: worker-auth-kv-allowlist,worker-email-otp,pwa-auth-guard
after: pwa-openai-transport
completed: 2026-07-15T08:49:34-04:00

## Phases

- worker-auth-kv-allowlist
- worker-email-otp
- pwa-auth-guard
