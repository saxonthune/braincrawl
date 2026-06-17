# braincrawl Provider Setup

Troubleshooting reference for provider configuration and common errors.

---

## Semantic Scholar

### Getting an API key

1. Go to `https://www.semanticscholar.org/product/api#api-key`
2. Fill in the request form — name, email, and intended use. The key arrives by email
   (usually within a day). It is free.

Without a key you can still call S2, but the **keyless shared pool is throttled to ~1
req/s** and will 429 hard during `--all` snowballs. Get the key first.

### Configuring the key

Two ways, same precedence (env wins):

**Option A — environment variable (session/shell)**

```bash
export BRAINCRAWL_SEMANTICSCHOLAR_API_KEY="your-key-here"
```

Add to `~/.bashrc` / `~/.zshrc` to persist across sessions.

**Option B — config file (permanent)**

```toml
# ~/.config/braincrawl/config.toml
server_url = "http://127.0.0.1:8787"
semanticscholar_api_key = "your-key-here"
```

Override the config path with `BRAINCRAWL_CONFIG=/path/to/config.toml`.

### Verifying it works

```bash
braincrawl --text semanticscholar get DOI:10.1126/science.aaf2654 --skip-push
```

Should return a paper title line. If it returns a 429, the key is missing or the old
keyless quota is still cooling down (wait a minute and retry).

A **429** means rate-limited (key missing or throttled).
A **404** means the ID wasn't found on S2.
A **connection refused** means the braincrawl server is down — check `/health` first:

```bash
curl -fsS http://127.0.0.1:8787/health
```

### Common issues

**Keyless 429 storms during `--all`**
The shared pool is ~1 req/s. A full cited-by snowball on a well-cited paper will 429
repeatedly. Get a key (above), or pass `--limit N` to cap the fetch.

**"Cannot infer ID type" on a bare numeric**
S2 accepts `DOI:…`, `ARXIV:…`, `CorpusId:…`, or a 40-hex `paperId`. A bare number is
ambiguous between CorpusId and other formats. Prefix it: `CorpusId:12345678`.

**ID forms S2 accepts (papers)**
- `DOI:10.xxxx/xxxx`
- `ARXIV:2301.00001`
- `CorpusId:12345678`
- Bare 40-character hex paperId (e.g. `649def34f8be52c8b66281af98ae884c09aef38b`)

**ID forms S2 accepts (authors)**
- Bare authorId (numeric string)
- `ORCID:0000-0000-0000-0000`

**Server down vs. provider down**
Check `curl -fsS http://127.0.0.1:8787/health` first. If the server is up but S2 calls
fail, S2's API may be having an incident — try again in a few minutes.

---

## OpenAlex (parity note)

OpenAlex requires **no key**. The `openalex_api_key` config field / `BRAINCRAWL_OPENALEX_API_KEY`
env var is optional and only needed if you have a polite-pool key to raise the rate limit.
Config and env precedence are identical to S2.
