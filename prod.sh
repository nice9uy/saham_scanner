#!/usr/bin/env bash
# Menjalankan Scanner Saham sepenuhnya lewat Podman:
#   container scanner-saham-db  (PostgreSQL 16)
#   container scanner-saham-app (backend + frontend)
# Binary backend dan frontend di-build di host, lalu dikemas ke image (lihat Containerfile).
# Pemakaian: ./prod.sh [start|stop|restart|status|logs]
set -euo pipefail
cd "$(dirname "$0")"

if [ -f .env ]; then
  set -a
  # shellcheck disable=SC1091
  source .env
  set +a
fi

# Semua variabel yang dipakai compose.yaml harus terisi: podman-compose 1.3 membiarkan
# `${VAR}` apa adanya kalau variabel tidak di-set.
export DB_PORT="${DB_PORT:-5440}"
export DB_PASSWORD="${DB_PASSWORD:-scanner}"
export APP_PORT="${APP_PORT:-3000}"
export INGEST_AT="${INGEST_AT:-17:00}"
export INGEST_CONCURRENCY="${INGEST_CONCURRENCY:-4}"
export BACKFILL_YEARS="${BACKFILL_YEARS:-5}"
export CACHE_BARS="${CACHE_BARS:-300}"
export LIVE_UPDATE="${LIVE_UPDATE:-1}"
export LIVE_GAP_SECS="${LIVE_GAP_SECS:-3}"
export LIVE_INTERVAL_MIN="${LIVE_INTERVAL_MIN:-2}"
export RUST_LOG="${RUST_LOG:-info,sqlx=warn,tower_http=warn}"
URL="http://localhost:${APP_PORT}"

info() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m!!\033[0m %s\n' "$*" >&2; }
die() {
  warn "$*"
  exit 1
}

require() {
  command -v podman >/dev/null 2>&1 || die "podman tidak ditemukan"
  podman compose version >/dev/null 2>&1 || die "podman compose tidak tersedia (sudo apt install podman-compose)"
}

# Backend versi lama yang berjalan langsung di host (sebelum memakai container).
stop_legacy_backend() {
  local pid_file=.run/backend.pid
  if [ -f "$pid_file" ] && kill -0 "$(cat "$pid_file")" 2>/dev/null; then
    info "Menghentikan backend lama yang berjalan di host"
    kill "$(cat "$pid_file")" || true
    sleep 1
  fi
  rm -f "$pid_file"
}

build() {
  command -v trunk >/dev/null 2>&1 || die "trunk tidak ditemukan (cargo install trunk --locked)"
  info "Build frontend (trunk, release)"
  (cd crates/frontend && trunk build --release)
  info "Build backend (cargo, release)"
  cargo build --release -p backend
}

start() {
  require
  stop_legacy_backend
  build
  info "Menyiapkan database (container scanner-saham-db, port $DB_PORT)"
  scripts/db.sh start
  info "Membuat image dan menjalankan container scanner-saham-app"
  podman compose up -d --build --force-recreate --no-deps app
  for _ in $(seq 1 60); do
    if curl -fsS "$URL/api/ingest/status" >/dev/null 2>&1; then
      info "Scanner Saham berjalan di $URL"
      info "Log: ./prod.sh logs   ·   Status: ./prod.sh status   ·   Berhenti: ./prod.sh stop"
      return
    fi
    if [ "$(podman inspect -f '{{.State.Running}}' scanner-saham-app 2>/dev/null)" != "true" ]; then
      warn "Container aplikasi berhenti saat startup. Log terakhir:"
      podman logs --tail 30 scanner-saham-app >&2 || true
      exit 1
    fi
    sleep 0.5
  done
  die "Aplikasi belum merespons setelah 30 detik; cek ./prod.sh logs"
}

case "${1:-start}" in
  start | restart) start ;;
  stop)
    require
    stop_legacy_backend
    info "Menghentikan semua container (data database tetap tersimpan)"
    podman compose down
    ;;
  status) podman compose ps ;;
  logs) podman logs -f --tail 100 scanner-saham-app ;;
  *)
    echo "Pemakaian: $0 [start|stop|restart|status|logs]" >&2
    exit 2
    ;;
esac
