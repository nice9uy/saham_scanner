# Arsitektur Scanner Saham

> Diperbarui setiap kali kode berubah, bersama graph graphify (`graphify update . --force` →
> `graphify-out/graph.html`, `graphify-out/GRAPH_REPORT.md`).
> Terakhir diperbarui: 2026-10-15, setelah frontend selesai.

## Status

| Crate | Status | Verifikasi |
|---|---|---|
| `shared` | selesai | 7 test, kompilasi wasm32 (37 indikator, DTO risiko) |
| `scanner-core` | selesai | 43 test (termasuk `risk.rs`: peringatan risiko + stop loss ATR) |
| `backend` | selesai | 19 test (8 unit + 3 db/ingest + 8 API, butuh PostgreSQL) |
| `frontend` | selesai | clippy wasm32 bersih; dicek manual lewat screenshot Chrome headless |

## Menjalankan

Semua berjalan lewat Podman (`podman compose`, stack `scanner-saham` di `compose.yaml`):

```mermaid
graph LR
    host["./prod.sh (host)"] -->|trunk build + cargo build --release| artefak["target/release/backend + crates/frontend/dist"]
    artefak -->|Containerfile, debian:trixie-slim| image["image localhost/scanner-saham"]
    image --> app["container scanner-saham-app :3000 (user scanner)"]
    app -->|postgres://db:5432| db["container scanner-saham-db :5440 → 5432"]
    db --> vol[("volume scanner-saham-pgdata")]
    app --> yahoo[(Yahoo Finance)]
```

| Perintah | Fungsi |
|---|---|
| `./prod.sh` / `restart` | build frontend + backend di host → `scripts/db.sh start` → `podman compose up -d --build --force-recreate --no-deps app` → tunggu `http://localhost:3000` siap |
| `./prod.sh stop` | `podman compose down` (volume data tetap) |
| `./prod.sh status` · `logs` | `podman compose ps` · `podman logs -f scanner-saham-app` |
| `scripts/db.sh start\|stop\|status\|psql` | hanya service `db` (untuk dev dan test); mengganti container lama hasil `podman run` tanpa menyentuh data |
| `DATABASE_URL=postgres://scanner:scanner@localhost:5440/scanner cargo test` | semua test (80) |

Catatan: podman-compose 1.3 tidak mengisi `${VAR:-default}` yang default-nya kosong atau mengandung titik dua, jadi `prod.sh` meng-export semua variabel compose beserta default-nya. Backend juga menganggap variabel kosong sebagai tidak di-set.

## Crate dan dependensi

```mermaid
graph LR
    frontend["frontend (Leptos 0.8 CSR + Tailwind v4)"] --> shared
    frontend -->|HTTP /api| backend
    frontend --> lwc[(Lightweight Charts 5.0.8)]
    backend["backend (Axum + sqlx)"] --> core
    backend --> shared
    core["scanner-core"] --> shared
    core --> polars[(polars 0.55)]
    core --> yata[(yata 0.7)]
    backend --> pg[(PostgreSQL 16 · Podman)]
    backend --> yahoo[(Yahoo Finance)]
    shared --> serde[(serde)]
```

## Komponen backend

```mermaid
graph TD
    main["main.rs → lib::run"] --> migrate["MIGRATOR (migrations/)"]
    main --> seed["seed_if_empty (data/idx_stocks.csv)"]
    main --> cache["PriceCache.reload"]
    main --> sched["ingest::schedule::spawn_scheduler (Sen–Jum 17:00 WIB)"]
    main --> live["ingest::live::spawn (jam bursa, 100 saham / request)"]
    main --> router["api::router"]
    sched --> trigger["ingest::trigger"]
    router --> trigger
    trigger --> run["ingest::run (retry, buffer_unordered)"]
    run --> source["PriceSource → YahooSource"]
    live --> source
    live --> db
    live --> cache
    run --> db
    trigger --> cache
    router --> handlers["api/* handler"]
    handlers --> db["db/* (stocks, prices, watchlists, scans, ingest_runs)"]
    handlers --> cache
    handlers --> core["scanner-core: run_scan, market_summary, stock_stats, add_indicators"]
    cache --> frame["frame::prices_frame (OHLC × adj_close/close)"]
```

## File

| File | Isi |
|---|---|
| `crates/shared/src/indicator.rs` | `enum Indicator` (32 varian) + `key()`, `label()`, `lookback()`, `periods()`, `is_recursive()` |
| `crates/shared/src/scan.rs` | `ScanRequest`, `Condition`, `Operand`, `Comparator`, `Universe`, `ScanResultDto`, `PresetDto` |
| `crates/shared/src/api.rs` | DTO REST: `ApiError`, `StockDto`, `StockDetailDto`, `CandlesDto`, `WatchlistDto`, `SavedScanDto`, `MarketSummaryDto`, `IngestStatusDto`, … |
| `crates/core/src/indicators/{vector,recursive,mod}.rs` | indikator Polars per ticker; Supertrend manual + yata; `add_indicators` |
| `crates/core/src/{validate,engine,presets}.rs` | validasi; `run_scan` / `run_scan_annotated` dengan `Annotations` (risiko + rating, `guard_best_buy`); filter `universe.signals`; 13 preset |
| `crates/core/src/signal.rs` | `signals`: rating Best Buy…Best Sell dari 12 suara indikator + pemicu 3 hari; `snapshots` (bar terakhir atau semua baris), `level_from_score`, `BLOCKS_BEST_BUY` |
| `crates/backend/src/system.rs` + `api/server.rs` + `db/stats.rs` | Halaman Server: `/proc` (CPU per core disampel tiap 3 detik oleh `SystemMonitor`, memori, beban, uptime), `statvfs` disk, ukuran database dan tabel dari PostgreSQL; `GET /server` |
| `backtest::win_rates` + `migrations/0004_win_rates.sql` + `db/win_rates.rs` | Win rate sinyal beli per saham dari seluruh riwayat; `ingest::refresh_win_rates` setelah update harian dan saat start; dimuat ke `Annotations::win_rates`; filter `Universe::min_win_rate` / `min_win_signals` |
| `crates/core/src/quality.rs` | `quality(funds, meta, closes)` → `QualityDto` per saham (Sehat/Cukup/Lemah/Buruk dari laba, ekuitas, ROE, PER vs median sektor, dividen); disimpan di `Annotations::quality`, dipakai filter `Universe::quality` dan kolom scan. Data dari `migrations/0003_fundamentals.sql` + `db/fundamentals.rs`, diisi `PriceSource::fetch_fundamentals` (Yahoo v7, `parse_fundamentals` dengan kurs `IDR=X`) di `ingest::run` dan `ensure_index` |
| `crates/core/src/brief.rs` | `brief(BriefInput)` → `BriefDto`: ringkasan harian berbentuk tulisan (kondisi pasar, sektor, peluang beli, hindari/jual, waspada, watchlist) dari harga, `Annotations`, IHSG; endpoint `GET /brief` di `api/brief.rs`, halaman `pages/news.rs` |
| `crates/core/src/market.rs` | `MarketRegime`: IHSG di atas SMA 50 per tanggal (`strong_at` memakai hari bursa terakhir); dipakai `Annotations::compute` dan backtest untuk membatalkan Best Buy saat pasar lemah |
| `crates/core/src/backtest.rs` | `Backtest` (`add` per kelompok saham, `finish` → `BacktestDto`): rating setiap hari historis, masuk di open besok, hasil 5/10/20 hari vs pembanding semua hari, rincian per tahun, perbandingan cara keluar setelah Best Buy (`trail_exit`) |
| `crates/core/src/risk.rs` | `risk_reports` / `flag_codes` (per baris, untuk backtest): trailing stop chandelier (`TRAIL_DAYS` 22, `TRAIL_ATR_MULT` 3) + peringatan risiko (pergerakan tidak wajar, riwayat volatilitas ekstrem 60 hari, likuiditas tidak konsisten, kurang likuid, harga < Rp100, distribusi, tren turun, jenuh beli) + stop loss 2 × ATR(14) |
| `crates/core/src/{stats,summary,frame}.rs` | `stock_stats`, `market_summary`, pembaca kolom per ticker |
| `crates/backend/src/lib.rs` | `AppState`, `MIGRATOR`, `seed_if_empty`, `run` |
| `crates/backend/src/config.rs` | `Config::from_env` (`DATABASE_URL`, `BIND_ADDR`, `INGEST_AT`, `INGEST_CONCURRENCY`, `BACKFILL_YEARS`, `CACHE_BARS`, `LIVE_UPDATE`, `LIVE_GAP_SECS`, `LIVE_INTERVAL_MIN`, `STATIC_DIR`, `SEED_CSV`) |
| `crates/backend/src/error.rs` | `AppError` → `ApiError` JSON + status |
| `crates/backend/src/cache.rs` | `PriceCache` (DataFrame 300 bar terakhir + meta + bar terakhir + `Annotations` yang dihitung sekali per reload) |
| `crates/backend/src/db/*.rs` | query runtime sqlx per tabel; `stocks::parse_csv`, `stocks::sync_listing` (tambah emiten baru, nonaktifkan yang hilang kalau daftar ≥ 500) |
| `crates/backend/src/ingest/live.rs` | Update intraday jam bursa: `spawn`, `market_open`, `cooldown` (jeda 429 berlipat), `no_data_means_holiday`; semua saham via `PriceSource::fetch_today` (`/v7/finance/quote`, `BATCH` 100, IHSG di request pertama); status lewat `LiveHandle` → `IngestStatusDto.live` |
| `crates/backend/src/ingest/{mod,yahoo,schedule,listing}.rs` | `PriceSource` (+ `listing()`, `fetch_index()`, `fetch_today()`), `IngestHandle`, `run`, `trigger`; parser chart Yahoo + `parse_quotes` (v7, sesi cookie + crumb); `next_run`; daftar emiten TradingView + pemetaan sektor |
| `crates/backend/src/api/*.rs` | handler REST; `chart::parse_indicators` |
| `migrations/0001_init.sql` | tabel `stocks`, `daily_prices`, `watchlists`, `watchlist_items`, `saved_scans`, `ingest_runs` |
| `migrations/0002_index_prices.sql` + `crates/backend/src/db/index_prices.rs` | penutupan IHSG (`^JKSE`); diisi `ingest::run`, putaran intraday, dan `ingest::ensure_index` saat start; `market_regime` → `CacheData::market` |

## Komponen frontend

```mermaid
graph TD
    main["main.rs (Router)"] --> shell["components/layout.rs: Shell, Tape (running trade), StockSearch"]
    main --> dash["pages/dashboard.rs: /"]
    main --> scan["pages/scanner.rs: /scanner"]
    main --> stock["pages/stock.rs: /stock/:ticker"]
    main --> wl["pages/watchlist.rs: /watchlist"]
    main --> data["pages/data.rs: /data"]
    scan --> builder["scanner/builder.rs: ConditionRow, OperandEditor"]
    builder --> catalog["scanner/catalog.rs: KINDS, build, decompose"]
    stock --> chart["components/chart.rs: PriceChart"]
    chart -->|wasm-bindgen| chartjs["public/chart.js: ScannerChart.render"]
    chartjs --> lwc[(vendor/lightweight-charts.js)]
    dash & scan & stock & wl & data & shell --> api["api.rs (gloo-net)"]
    api -->|/api| backend[(backend)]
```

| File | Isi |
|---|---|
| `crates/frontend/index.html`, `Trunk.toml` | entry Trunk; Tailwind 4.1.13; proxy `/api` → `:3000` untuk `trunk serve` |
| `crates/frontend/style/input.css` | token desain (`@theme`): ink `#0E1420`, panel, line, paper `#E6E1D3`, brass `#D4A657`, up `#4FBF8B`, down `#E5675E`; font Bricolage Grotesque / IBM Plex Sans / IBM Plex Mono; animasi tape |
| `crates/frontend/src/api.rs` | klien REST bertipe → `Res<T>` dengan `ApiFailure { message, status }` |
| `crates/frontend/src/format.rs` | angka gaya Indonesia, Rupiah ringkas (rb/jt/M/T), tanggal, `short_name` |
| `crates/frontend/src/scanner/catalog.rs` | 32 jenis indikator untuk UI ↔ `shared::Indicator` |
| `crates/frontend/src/scanner/builder.rs` | penyusun kondisi berbentuk kalimat; parameter disimpan saat `change` agar fokus tidak hilang |
| `crates/frontend/src/pages/*.rs` | lima halaman; state scanner bisa di-bookmark lewat `?q=<json>`, `?preset=<id>`, `?watchlist=<id>` |
| `crates/frontend/public/chart.js` | candlestick + volume + overlay di panel harga; RSI/MACD/Stoch/ADX di panel bawah (stretch factor 3:1) |

## REST API (`/api`)

| Endpoint | Handler |
|---|---|
| `GET /stocks?q=&limit=` · `GET /stocks/{ticker}` · `GET /stocks/{ticker}/candles?range=&indicators=` · `POST /stocks/import` | `api/stocks.rs` |
| `POST /scan` · `GET /presets` | `api/scan.rs` |
| `GET/POST /scans` · `PUT/DELETE /scans/{id}` | `api/scans.rs` |
| `GET/POST /watchlists` · `DELETE /watchlists/{id}` · `POST/DELETE /watchlists/{id}/items/{ticker}` | `api/watchlists.rs` |
| `GET /market/summary` | `api/market.rs` |
| `GET /backtest?liquid=` | `api/backtest.rs` (`BacktestCache`, dihitung ulang saat `IngestHandle::version` berubah; data dimuat 100 ticker sekaligus via `db::prices::history_many`) |
| `POST /ingest/run` (202 / 409) · `GET /ingest/status` | `api/ingest.rs` |

## Alur update data

```mermaid
sequenceDiagram
    participant T as trigger (jadwal / manual / backfill)
    participant I as ingest::run
    participant Y as YahooSource
    participant TV as TradingView
    participant DB as PostgreSQL
    participant C as PriceCache
    T->>I: IngestGuard (satu run sekaligus)
    I->>DB: ingest_runs.start
    I->>TV: daftar seluruh emiten IDX (scanner.tradingview.com)
    I->>DB: sync_listing (tambah baru, nonaktifkan yang hilang; gagal → pakai daftar tersimpan)
    I->>DB: daftar emiten aktif + tanggal terakhir
    loop tiap saham (4 paralel)
        I->>Y: chart dari (terakhir − 5 hari) atau 5 tahun ke belakang
        Y-->>I: bar harian + nama (retry 1s/2s/4s untuk 429/5xx)
        I->>DB: upsert daily_prices, isi nama kalau masih kosong
    end
    I->>DB: ingest_runs.finish (success / partial / failed)
    T->>C: reload (latest_bars 300 → DataFrame disesuaikan)
```

## Alur `POST /api/scan`

```mermaid
sequenceDiagram
    participant F as Frontend
    participant A as api::scan
    participant C as PriceCache
    participant E as scanner_core::run_scan
    F->>A: ScanRequest
    A->>C: snapshot (503 kalau kosong)
    A->>A: universe.watchlist_id → allowed (404 kalau tidak ada)
    A->>E: spawn_blocking(prices, meta, req, allowed)
    E-->>A: ScanResultDto (atau Invalid → 400)
    A-->>F: JSON
```
