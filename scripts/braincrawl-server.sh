#!/usr/bin/env bash
#
# braincrawl-server.sh — start/stop/status the shared local braincrawl server.
#
# One server owns the accumulating L1/L2 corpus; every consumer repo points at it
# (BRAINCRAWL_SERVER_URL) so coverage compounds instead of fragmenting per-repo.
# Manual lifecycle: start it when you sit down to research, stop it when done.
#
#   ./scripts/braincrawl-server.sh start    # build (if needed) + launch in background
#   ./scripts/braincrawl-server.sh stop
#   ./scripts/braincrawl-server.sh status
#   ./scripts/braincrawl-server.sh restart
#   ./scripts/braincrawl-server.sh logs     # tail -f the server log
#
# Overridable via env: BRAINCRAWL_BIND, BRAINCRAWL_DB, BRAINCRAWL_BLOB_ROOT,
# BRAINCRAWL_AUTH_TOKEN (omit to run with auth disabled — dev/localhost only).

set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE_DIR="${BRAINCRAWL_STATE_DIR:-$HOME/.local/share/braincrawl}"
BIND="${BRAINCRAWL_BIND:-127.0.0.1:8787}"
DB="${BRAINCRAWL_DB:-$STATE_DIR/braincrawl.db}"
BLOB_ROOT="${BRAINCRAWL_BLOB_ROOT:-$STATE_DIR/blobs}"
PIDFILE="$STATE_DIR/server.pid"
LOGFILE="$STATE_DIR/server.log"
BIN="$REPO_DIR/target/release/braincrawl-server"
HEALTH_URL="http://$BIND/health"

mkdir -p "$STATE_DIR" "$BLOB_ROOT"

running_pid() {
  # Echo the live PID if the recorded process is still alive, else nothing.
  [ -f "$PIDFILE" ] || return 0
  local pid
  pid="$(cat "$PIDFILE")"
  if kill -0 "$pid" 2>/dev/null; then
    echo "$pid"
  fi
}

cmd_start() {
  local pid
  pid="$(running_pid)"
  if [ -n "$pid" ]; then
    echo "braincrawl-server already running (pid $pid) on $BIND"
    return 0
  fi

  if [ ! -x "$BIN" ]; then
    echo "building braincrawl-server (release)…"
    cargo build --release --bin braincrawl-server --manifest-path "$REPO_DIR/Cargo.toml"
  fi

  local auth_env=()
  if [ -n "${BRAINCRAWL_AUTH_TOKEN:-}" ]; then
    auth_env=(BRAINCRAWL_AUTH_TOKEN="$BRAINCRAWL_AUTH_TOKEN")
    echo "auth: enabled (bearer token)"
  else
    auth_env=(BRAINCRAWL_AUTH_DISABLED=1)
    echo "auth: DISABLED (localhost dev mode)"
  fi

  env "${auth_env[@]}" \
    BRAINCRAWL_DB="$DB" \
    BRAINCRAWL_BLOB_ROOT="$BLOB_ROOT" \
    BRAINCRAWL_BIND="$BIND" \
    nohup "$BIN" >>"$LOGFILE" 2>&1 &
  echo $! >"$PIDFILE"
  echo "braincrawl-server started (pid $(cat "$PIDFILE")) on $BIND"
  echo "  db:    $DB"
  echo "  blobs: $BLOB_ROOT"
  echo "  log:   $LOGFILE"
  echo "  check: curl -s $HEALTH_URL"
}

cmd_stop() {
  local pid
  pid="$(running_pid)"
  if [ -z "$pid" ]; then
    echo "braincrawl-server not running"
    rm -f "$PIDFILE"
    return 0
  fi
  kill "$pid"
  rm -f "$PIDFILE"
  echo "braincrawl-server stopped (pid $pid)"
}

cmd_status() {
  local pid
  pid="$(running_pid)"
  if [ -z "$pid" ]; then
    echo "down — not running"
    return 1
  fi
  echo "up — pid $pid, bind $BIND"
  echo -n "health: "
  curl -fsS "$HEALTH_URL" || echo "(no response — server may be starting)"
  echo
}

case "${1:-}" in
  start)   cmd_start ;;
  stop)    cmd_stop ;;
  restart) cmd_stop; cmd_start ;;
  status)  cmd_status ;;
  logs)    tail -f "$LOGFILE" ;;
  *)
    echo "usage: $0 {start|stop|restart|status|logs}" >&2
    exit 2
    ;;
esac
