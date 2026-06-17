# braincrawl — task runner. The server lifecycle is a thin wrapper around
# scripts/braincrawl-server.sh (the shared local L1/L2 server other repos point at).

# List available recipes
default:
    @just --list

# Build (if needed) and launch the shared server in the background
server-start:
    ./scripts/braincrawl-server.sh start

# Stop the shared server
server-stop:
    ./scripts/braincrawl-server.sh stop

# Restart the shared server
server-restart:
    ./scripts/braincrawl-server.sh restart

# Show whether the server is up + /health
server-status:
    ./scripts/braincrawl-server.sh status

# Tail the server log
server-logs:
    ./scripts/braincrawl-server.sh logs

# Build the server + CLI binaries
build:
    cargo build --bin braincrawl-server --bin braincrawl

# Run the workspace test suite
test:
    cargo test
