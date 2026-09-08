# braincrawl Web UI

The Web UI is a Solid application served by the braincrawl server at `/web`.
The workspace uses Vite+ (`vp`) for package management, development, checks, and
tests.

## Development

From this directory:

```sh
vp install
vp dev
```

Open <http://localhost:5173> while the development server is running. The app
expects the braincrawl API at the configured origin; the native server serves
the same UI from `/web` in production.

## Checks and tests

```sh
vp check
vp test
```

From the repository root, the equivalent recipes are `just web-check` and
`just web-test`.

## Production build

```sh
vp build
```

The output is written to `dist/`. The repository-level `just web-build` recipe
builds this directory for the native server and deployment workflows.
