#!/usr/bin/env bash
#
# braincrawl-server.sh — start/stop/status the shared local braincrawl server.
#
# The server runs as a systemd *user* service (braincrawl-server.service), enabled
# with lingering so it also starts at boot. This script is a thin shim over
# `systemctl --user` so existing callers keep working; systemd owns the process,
# the restart policy, and the logs (journald). Configuration (bind, db, blob root,
# auth) lives in the unit file: ~/.config/systemd/user/braincrawl-server.service.
#
#   ./scripts/braincrawl-server.sh start
#   ./scripts/braincrawl-server.sh stop
#   ./scripts/braincrawl-server.sh status
#   ./scripts/braincrawl-server.sh restart   # bounce only; does NOT rebuild
#   ./scripts/braincrawl-server.sh logs      # follow journald
#
# To rebuild the release binary and then bounce the service, use `just restart`.

set -euo pipefail

UNIT=braincrawl-server
BIND="${BRAINCRAWL_BIND:-127.0.0.1:8787}"
HEALTH_URL="http://$BIND/health"

case "${1:-}" in
  start)   systemctl --user start "$UNIT" ;;
  stop)    systemctl --user stop "$UNIT" ;;
  restart) systemctl --user restart "$UNIT" ;;
  status)
    systemctl --user status "$UNIT" --no-pager || true
    echo -n "health: "
    curl -fsS "$HEALTH_URL" || echo "(no response — server may be starting)"
    echo
    ;;
  logs)    journalctl --user -u "$UNIT" -f ;;
  *)
    echo "usage: $0 {start|stop|restart|status|logs}" >&2
    exit 2
    ;;
esac
