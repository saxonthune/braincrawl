# braincrawl — task runner. One systemd user service (braincrawl-server.service)
# serves the L1/L2 API and the built web UI (/web); these recipes drive it via
# `systemctl --user`.
# The service is enabled with lingering, so it also starts at boot.

# List available recipes
default:
    @just --list

# Two things other agents depend on go stale independently — the `braincrawl` CLI
# on your PATH (refreshed by `install`) and the *running* shared server, which keeps
# executing its old binary until `restart` rebuilds and relaunches it.
#
# One-stop: refresh the PATH CLI + rebuild & restart the server. Run after pulling/changes.
upgrade: install restart

# Start the shared server (systemd user service)
systemd-start:
    systemctl --user start braincrawl-server

# Stop the shared server
systemd-stop:
    systemctl --user stop braincrawl-server

# Bounce the service (does NOT rebuild — use `just restart` to pick up code changes)
systemd-restart:
    systemctl --user restart braincrawl-server

# Show whether the server is up + /health
systemd-status:
    systemctl --user status braincrawl-server --no-pager
    @curl -fsS http://127.0.0.1:8787/health && echo

# Tail the server log (journald)
systemd-logs:
    journalctl --user -u braincrawl-server -f

# NB: this only refreshes the running server. To update the `braincrawl` CLI on
# your PATH after CLI changes, run `just install` (the service never touches it).
#
# Rebuild the release binaries + web UI, then bounce the service (picks up code changes)
restart: build-release web-build
    systemctl --user restart braincrawl-server

# Build the server + CLI binaries (debug)
build:
    cargo build --bin braincrawl-server --bin braincrawl

# Build optimized release binaries (server script + consumers point here)
build-release:
    cargo build --release --bin braincrawl-server --bin braincrawl

# Install the systemd *user* service from scripts/braincrawl-server.service, wiring
# ExecStart to THIS repo's release binary. Enables lingering so it starts at boot.
# Idempotent — re-run after editing the template. Undo: `just systemd-uninstall`.
systemd-install: build-release web-build
    #!/usr/bin/env bash
    set -euo pipefail
    unit_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
    mkdir -p "$unit_dir"
    sed -e "s#@EXEC@#{{justfile_directory()}}/target/release/braincrawl-server#" \
        -e "s#@WEBROOT@#{{justfile_directory()}}/web/dist#" \
        scripts/braincrawl-server.service > "$unit_dir/braincrawl-server.service"
    env_file="${XDG_CONFIG_HOME:-$HOME/.config}/braincrawl/server.env"
    if [ ! -f "$env_file" ]; then
        mkdir -p "$(dirname "$env_file")"
        cp scripts/server.env.example "$env_file"
        echo "scaffolded $env_file (personal overrides — edit and re-run 'just systemd-restart')"
    fi
    systemctl --user daemon-reload
    loginctl enable-linger "$USER"
    systemctl --user enable --now braincrawl-server
    echo "installed + started braincrawl-server.service (lingering enabled)"

# Remove the systemd user service (stops it, disables boot start, drops the unit)
systemd-uninstall:
    #!/usr/bin/env bash
    set -euo pipefail
    systemctl --user disable --now braincrawl-server || true
    rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/braincrawl-server.service"
    systemctl --user daemon-reload
    echo "removed braincrawl-server.service"

# Install/update the `braincrawl` CLI into ~/.cargo/bin as a standalone compiled
# binary. Re-run after changing the CLI to push a new build. (For hands-off dev,
# `just watch` rebuilds on save instead; `just install` is also how you undo it.)
install:
    cargo install --path apps/cli --force

# Auto-rebuild the CLI on every source save (needs `watchexec`). Points the PATH
# `braincrawl` at target/release via symlink, so all shells/sessions run the freshly
# built binary with no manual reinstall. Run in a spare terminal while developing the
# CLI. Undo the symlink (restore a standalone binary) with `just install`.
watch:
    #!/usr/bin/env bash
    set -euo pipefail
    bin="${CARGO_HOME:-$HOME/.cargo}/bin/braincrawl"
    cargo build --release --bin braincrawl
    ln -sf "{{justfile_directory()}}/target/release/braincrawl" "$bin"
    echo "linked $bin -> target/release/braincrawl; watching apps/cli/src ..."
    exec watchexec --watch apps/cli/src --exts rs --debounce 300ms --restart -- \
        cargo build --release --bin braincrawl

# Run the workspace test suite
test:
    cargo test

# Edge/CI gate: requires wrangler + network. Boots the Worker under wrangler dev
# --local (emulated D1/R2/KV) and runs the shared conformance suite against it.
worker-test:
    ./scripts/worker-conformance.sh

# Regenerate the Luminous CLI-grammar canvas in .luminous/ from the clap definition
luminous-cli:
    cargo run -p braincrawl-cli --example luminous_cli_grammar

# Regenerate the Atlas Data File from the CLI grammar signal (refreshes the signal first)
luminous-atlas: luminous-cli
    python3 .luminous/atlas/gen-atlasdata.py

# ── Web UI (web/ — its own Vite+ workspace; vp runs from inside it) ──────────

# Dev server for the Web UI
web-dev:
    #!/usr/bin/env bash
    set -euo pipefail
    cd {{justfile_directory()}}/web && vp dev

# Production build of the Web UI (web/dist)
web-build:
    #!/usr/bin/env bash
    set -euo pipefail
    cd {{justfile_directory()}}/web && vp build

# Format + lint + type-check the Web UI in one pass
web-check:
    #!/usr/bin/env bash
    set -euo pipefail
    cd {{justfile_directory()}}/web && vp check

# Web UI tests (Vitest)
web-test:
    #!/usr/bin/env bash
    set -euo pipefail
    cd {{justfile_directory()}}/web && vp test

# Run only the golden-turn harness tests (the fixed offline eval set)
web-eval:
    #!/usr/bin/env bash
    set -euo pipefail
    cd {{justfile_directory()}}/web && vp test golden

# Bring the local dev stack current with the working tree: rebuild the server
# binary + web dist the systemd user service serves, restart it. The production
# deploy is deliberately a separate command (deploy-worker).
update-local: web-build
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release -p braincrawl-server --manifest-path {{justfile_directory()}}/Cargo.toml
    systemctl --user restart braincrawl-server
    systemctl --user is-active braincrawl-server

# Build the web app + deploy the worker (assets + API) as one unit. Requires
# apps/worker/wrangler.toml (mirror the [assets] block from wrangler.toml.example
# if you haven't already).
deploy-worker: web-build
    #!/usr/bin/env bash
    set -euo pipefail
    cd {{justfile_directory()}}/apps/worker && wrangler deploy

# Generate a new long-lived AUTH_KV token for the CLI. Revoke by deleting the
# KV key (no TTL is intentional — CLI tokens are meant to persist).
generate-worker-token:
    #!/usr/bin/env bash
    set -euo pipefail
    cd {{justfile_directory()}}/apps/worker
    TOKEN=$(openssl rand -hex 32)
    HASH=$(printf '%s' "$TOKEN" | sha256sum | cut -d' ' -f1)
    wrangler kv key put "$HASH" '{"tenant":"default","status":"active"}' --binding AUTH_KV --remote
    echo ""
    echo "Token: $TOKEN"
    echo "Store it somewhere durable and in config.toml's auth_token — it will not be shown again."
