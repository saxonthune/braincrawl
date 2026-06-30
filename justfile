# braincrawl — task runner. The server lifecycle is a thin wrapper around
# scripts/braincrawl-server.sh (the shared local L1/L2 server other repos point at).

# List available recipes
default:
    @just --list

# Two things other agents depend on go stale independently — the `braincrawl` CLI
# on your PATH (refreshed by `install`) and the *running* shared server, which keeps
# executing its old binary until `restart` rebuilds and relaunches it.
#
# One-stop: refresh the PATH CLI + rebuild & restart the server. Run after pulling/changes.
upgrade: install restart

# Build (if needed) and launch the shared server in the background
server-start:
    ./scripts/braincrawl-server.sh start

# Stop the shared server
server-stop:
    ./scripts/braincrawl-server.sh stop

# Restart the shared server
server-restart:
    ./scripts/braincrawl-server.sh restart

# NB: this only refreshes the running server. To update the `braincrawl` CLI on
# your PATH after CLI changes, run `just install` (the server script never touches it).
#
# Rebuild the release binaries, then restart the server (picks up code changes)
restart: build-release
    ./scripts/braincrawl-server.sh restart

# Show whether the server is up + /health
server-status:
    ./scripts/braincrawl-server.sh status

# Tail the server log
server-logs:
    ./scripts/braincrawl-server.sh logs

# Build the server + CLI binaries (debug)
build:
    cargo build --bin braincrawl-server --bin braincrawl

# Build optimized release binaries (server script + consumers point here)
build-release:
    cargo build --release --bin braincrawl-server --bin braincrawl

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
