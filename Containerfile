# Image runtime Scanner Saham. Binary dan frontend di-build di host oleh ./prod.sh
# (host dan image sama-sama Debian trixie, jadi glibc dan arsitekturnya cocok).
FROM docker.io/library/debian:trixie-slim

RUN useradd --system --uid 10001 --home-dir /app scanner
WORKDIR /app

COPY --chown=scanner target/release/backend /app/backend
COPY --chown=scanner crates/frontend/dist /app/dist
COPY --chown=scanner data/idx_stocks.csv /app/data/idx_stocks.csv

USER scanner
EXPOSE 3000
CMD ["/app/backend"]
