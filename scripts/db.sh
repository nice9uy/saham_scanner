#!/usr/bin/env bash
# PostgreSQL Scanner Saham (service `db` di compose.yaml), untuk dev dan `cargo test`.
# Pemakaian: scripts/db.sh start|stop|status|psql
set -euo pipefail
cd "$(dirname "$0")/.."

NAME=scanner-saham-db
PORT="${DB_PORT:-5440}"

running() { [ "$(podman inspect -f '{{.State.Running}}' "$NAME" 2>/dev/null)" = "true" ]; }

# Container lama dari versi script sebelumnya (dibuat dengan `podman run`) diganti
# container compose. Datanya aman karena tersimpan di volume scanner-saham-pgdata.
remove_legacy() {
  if podman container exists "$NAME" &&
    [ -z "$(podman inspect -f '{{index .Config.Labels "com.docker.compose.project"}}' "$NAME" 2>/dev/null)" ]; then
    echo "Mengganti container database lama dengan versi compose (data tetap)"
    podman rm -f "$NAME" >/dev/null
  fi
}

wait_ready() {
  for _ in $(seq 1 60); do
    podman exec "$NAME" pg_isready -U scanner -d scanner >/dev/null 2>&1 && return 0
    sleep 1
  done
  echo "PostgreSQL tidak siap dalam 60 detik" >&2
  exit 1
}

case "${1:-start}" in
  start)
    remove_legacy
    podman compose up -d db >/dev/null 2>&1 || podman compose up -d db
    wait_ready
    echo "Database berjalan (port $PORT)"
    ;;
  stop) podman compose stop db >/dev/null && echo "Database dihentikan" ;;
  status)
    if running; then echo "running (port $PORT)"; else echo "stopped"; exit 1; fi
    ;;
  psql) podman exec -it "$NAME" psql -U scanner -d scanner ;;
  *)
    echo "Pemakaian: $0 start|stop|status|psql" >&2
    exit 2
    ;;
esac
