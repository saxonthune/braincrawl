# braincrawl Provider Setup

Troubleshooting reference for provider configuration and common errors.

---

## Config precedence

All provider config follows the same precedence: **env > `~/.config/braincrawl/config.toml`
> unset**. Override the config path with `BRAINCRAWL_CONFIG=/path/to/config.toml`.

```toml
# ~/.config/braincrawl/config.toml
active_store = "local"                # which [stores.<name>] every command targets
unpaywall_email = "you@example.com"   # contact email Unpaywall requires for fulltext lookups
crossref_mailto = "you@example.com"   # polite-pool contact for Crossref reference fetches

[stores.local]
url = "http://127.0.0.1:8787"
```

Check the server is up before debugging provider calls:

```bash
curl -fsS http://127.0.0.1:8787/health
```

A **connection refused** means the braincrawl server is down — start it first. If the
server is up but a provider call fails, that provider's API may be having an incident —
try again in a few minutes.

## OpenAlex

OpenAlex requires **no key**. The `openalex_api_key` config field / `BRAINCRAWL_OPENALEX_API_KEY`
env var is optional and only needed if you have a polite-pool key to raise the rate limit.

## Unpaywall & Crossref

`fetch-content --from unpaywall` (and `--from auto`) needs a contact email — Unpaywall
requires one by policy. Set `unpaywall_email` in `config.toml` (above) and it just works,
no per-command prefix. The `BRAINCRAWL_UNPAYWALL_EMAIL` env var overrides the config value.
`crossref_mailto` / `BRAINCRAWL_CROSSREF_MAILTO` behaves the same way for Crossref reference
fetches.
