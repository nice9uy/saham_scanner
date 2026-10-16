# Scanner Saham IDX — Desain

Tanggal: 2026-10-15
Status: disetujui (brainstorming), menunggu review spec

## 1. Tujuan dan batasan

Aplikasi web fullstack Rust untuk men-scan seluruh saham Bursa Efek Indonesia (±900 emiten) menggunakan analisa teknikal.

- **Pengguna:** pribadi, satu orang, tanpa login/autentikasi.
- **Data:** harian (EOD) dari Yahoo Finance (ticker `KODE.JK`), update otomatis tiap hari bursa pukul 17:00 WIB, ditambah tombol "Update sekarang".
- **Fitur MVP:** screener berbasis aturan, preset strategi, chart candlestick + indikator, watchlist, scan tersimpan, dashboard ringkasan pasar.
- **Stack:** Leptos (CSR) + Tailwind CSS v4, Axum (REST), PostgreSQL (sqlx), Polars (pengolahan data), yata (indikator rekursif).
- **Di luar cakupan:** login multi-user, data intraday/realtime, bursa selain IDX, data fundamental, backtesting, notifikasi/alert.

## 2. Arsitektur

Cargo workspace dengan 4 crate:

```
scanner_saham/
├── Cargo.toml              # [workspace]
├── crates/
│   ├── shared/             # tipe serde bersama (kompatibel WASM)
│   ├── core/               # engine analisa: Polars + yata, tanpa IO
│   ├── backend/            # Axum REST API, sqlx Postgres, updater Yahoo, scheduler
│   └── frontend/           # Leptos CSR, Trunk, Tailwind v4, Lightweight Charts
├── migrations/             # migrasi SQL sqlx
├── data/idx_stocks.csv     # seed daftar emiten
├── docs/architecture.md    # peta arsitektur, diperbarui setiap perubahan kode
├── scripts/db.sh           # PostgreSQL dev/test lewat Podman
└── prod.sh                 # build + jalankan semuanya (Podman + backend + frontend)
```

Nama package: `shared`, `scanner-core` (direktori `crates/core`; tidak bisa diberi nama `core` karena bentrok dengan crate bawaan Rust), `backend`, `frontend`.

Arah dependensi: `frontend → shared`, `backend → core → shared`, `backend → shared`. `shared` tidak boleh bergantung pada Polars, Tokio, atau sqlx agar bisa dikompilasi ke `wasm32-unknown-unknown`.

Alur utama:

```
Yahoo Finance ──(reqwest)──▶ backend::ingest ──▶ PostgreSQL
                                                     │
                                  (startup & setelah ingest)
                                                     ▼
                                        PriceCache (DataFrame di memori)
                                                     │
frontend ──HTTP JSON──▶ backend::api ──▶ core::engine ──▶ ScanResultDto
```

Saat development: `trunk serve` (frontend, port 8080) mem-proxy `/api` ke backend (port 3000). Di produksi, backend menyajikan file statis hasil `trunk build` dari `crates/frontend/dist`, jadi cukup satu proses.

### 2.1 `shared`

Hanya struct/enum dengan `serde::{Serialize, Deserialize}`:

- DTO: `StockDto`, `CandleDto`, `IndicatorSeriesDto`, `ScanResultDto`, `ScanRowDto`, `WatchlistDto`, `SavedScanDto`, `IngestRunDto`, `MarketSummaryDto`, `ApiError`.
- Model aturan: `ScanRequest`, `Condition`, `Operand`, `OperandSource`, `Indicator`, `Comparator`, `Logic`, `Universe`, `PresetDto`.
- `Indicator::key()` menghasilkan nama kolom yang stabil (mis. `sma_50`, `macd_signal_12_26_9`), dipakai backend untuk nama kolom dan frontend untuk header tabel.

### 2.2 `core`

Murni komputasi. Input `DataFrame` OHLCV, output kolom indikator atau hasil scan.

- `indicators::polars` — ekspresi Polars per ticker (`.over("ticker")`).
- `indicators::recursive` — indikator rekursif yang dijalankan per grup ticker, lalu hasilnya digabung kembali sebagai kolom. ADX/+DI/−DI, Ichimoku, dan Parabolic SAR memakai yata 0.7 (fitur `period_type_u16`). Supertrend ditulis sendiri karena yata 0.7 tidak menyediakannya.
- `engine` — validasi `ScanRequest`, kompilasi kondisi menjadi `Expr`, eksekusi.
- `presets` — fungsi `presets() -> Vec<PresetDto>`.

Skema `DataFrame` input: `ticker: str, date: date, open, high, low, close, volume: f64`, terurut per `(ticker, date)`, dengan harga yang sudah disesuaikan (lihat 3.3).

### 2.3 `backend`

- `axum` (router + handler), `tower-http` (CORS untuk dev, file statis, tracing), `sqlx` (Postgres, migrasi), `reqwest` (Yahoo), `tracing`. Jadwal update ditulis sendiri (loop tokio yang menghitung waktu jalan berikutnya), tanpa library cron.
- Modul: `api/` (satu file per resource), `db/` (query per tabel), `ingest/` (klien Yahoo, parser, orkestrasi), `cache.rs` (`PriceCache`), `error.rs` (`AppError → IntoResponse`), `config.rs` (env: `DATABASE_URL`, `BIND_ADDR`, `INGEST_AT`, `INGEST_CONCURRENCY`, `BACKFILL_YEARS`).

### 2.4 `frontend`

- Leptos CSR + `leptos_router`, build dengan Trunk, Tailwind CSS v4 lewat dukungan bawaan Trunk.
- HTTP lewat `gloo-net`.
- Chart: TradingView Lightweight Charts (UMD dari CDN atau file vendor lokal), dipanggil lewat binding `wasm-bindgen` di modul `chart/`.

## 3. Data

### 3.1 Skema PostgreSQL

```sql
CREATE TABLE stocks (
    ticker      TEXT PRIMARY KEY,          -- 'BBCA' (tanpa .JK)
    name        TEXT NOT NULL,
    sector      TEXT,
    board       TEXT,                      -- Utama / Pengembangan / Akselerasi / ...
    is_active   BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE daily_prices (
    ticker     TEXT NOT NULL REFERENCES stocks(ticker) ON DELETE CASCADE,
    date       DATE NOT NULL,
    open       DOUBLE PRECISION NOT NULL,
    high       DOUBLE PRECISION NOT NULL,
    low        DOUBLE PRECISION NOT NULL,
    close      DOUBLE PRECISION NOT NULL,
    adj_close  DOUBLE PRECISION NOT NULL,
    volume     BIGINT NOT NULL,
    PRIMARY KEY (ticker, date)
);
CREATE INDEX daily_prices_date_idx ON daily_prices (date);

CREATE TABLE watchlists (
    id          BIGSERIAL PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE watchlist_items (
    watchlist_id BIGINT NOT NULL REFERENCES watchlists(id) ON DELETE CASCADE,
    ticker       TEXT NOT NULL REFERENCES stocks(ticker) ON DELETE CASCADE,
    added_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (watchlist_id, ticker)
);

CREATE TABLE saved_scans (
    id          BIGSERIAL PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    rules       JSONB NOT NULL,            -- serde_json dari shared::ScanRequest
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE ingest_runs (
    id              BIGSERIAL PRIMARY KEY,
    started_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at     TIMESTAMPTZ,
    status          TEXT NOT NULL,         -- running | success | partial | failed
    trigger         TEXT NOT NULL,         -- schedule | manual | backfill
    ok_count        INTEGER NOT NULL DEFAULT 0,
    failed_tickers  TEXT[] NOT NULL DEFAULT '{}',
    error           TEXT
);
```

### 3.2 Daftar emiten

Yahoo tidak menyediakan daftar emiten IDX, dan idx.co.id memblokir scraping (Cloudflare). Daftar emiten di-seed dari `data/idx_stocks.csv` (kolom: `ticker,name,sector,board`) saat startup kalau tabel `stocks` masih kosong. `POST /api/stocks/import` (multipart CSV) melakukan upsert. Emiten yang tidak ada di CSV baru **tidak** dihapus (tetap tersimpan beserta riwayatnya).

### 3.3 Ingest dari Yahoo

- Endpoint: `https://query1.finance.yahoo.com/v8/finance/chart/{TICKER}.JK?interval=1d&period1={unix}&period2={unix}&events=split,div`, dengan header `User-Agent` mirip browser.
- **Backfill:** ticker yang belum punya data diambil `BACKFILL_YEARS` (default 5) tahun ke belakang.
- **Incremental:** mulai dari `max(date) - 5 hari` per ticker sampai hari ini, upsert dengan `INSERT ... ON CONFLICT (ticker, date) DO UPDATE`.
- **Konkurensi:** `INGEST_CONCURRENCY` (default 4) dengan `tokio::sync::Semaphore`. Retry 3× dengan exponential backoff (1s, 2s, 4s) untuk error jaringan, 5xx, dan 429.
- Bar yang salah satu nilai OHLC-nya `null` di-skip. Bar dengan volume 0 tetap disimpan (hari tanpa transaksi).
- Kegagalan per ticker dicatat di `failed_tickers` dan tidak menghentikan run. Status akhir: `success` (0 gagal), `partial` (sebagian gagal), `failed` (semua gagal atau error fatal).
- Hanya satu run boleh berjalan dalam satu waktu (`AtomicBool`/`Mutex`). Trigger manual saat ada run berjalan mengembalikan `409 Conflict`.
- **Jadwal:** `INGEST_AT` (default `17:00`) setiap Senin–Jumat, zona waktu WIB (UTC+7). Saat startup, kalau tabel `daily_prices` masih kosong, backfill langsung dijalankan.
- **Nama emiten:** kalau nama di `stocks` masih sama dengan kodenya (seed tanpa nama), nama diisi dari `meta.longName` Yahoo.
- **Penyesuaian harga:** sebelum masuk ke `core`, `factor = adj_close / close`, lalu `open/high/low/close × factor`. Volume tidak diubah.

### 3.4 PriceCache

- `Arc<RwLock<Option<DataFrame>>>` berisi `CACHE_BARS` (default 300) bar terakhir untuk semua ticker aktif, dengan harga sudah disesuaikan.
- Dimuat saat startup dan dimuat ulang setelah setiap ingest yang selesai.
- Scan membaca cache, tidak menyentuh database untuk data harga. Kalau cache kosong (belum pernah ingest), `POST /api/scan` mengembalikan `503` dengan pesan "Data belum tersedia, jalankan update data".
- Batasan: indikator yang butuh lebih dari 300 bar (mis. `SMA(500)`) akan bernilai null, sehingga kondisinya tidak lolos. Validasi menolak periode > 500, dan UI menampilkan peringatan untuk periode > 250.

## 4. Engine scan

### 4.1 Model aturan (`shared`)

```rust
pub struct ScanRequest {
    pub conditions: Vec<Condition>,   // 1..=10
    pub logic: Logic,                 // All | Any
    pub universe: Universe,
    pub sort_by: Option<SortSpec>,    // { operand: Operand, descending: bool }
    pub limit: u32,                   // 1..=500, default 100
}

pub struct Universe {
    pub min_price: Option<f64>,
    pub max_price: Option<f64>,
    pub min_avg_value_20d: Option<f64>, // rata-rata close*volume 20 hari, dalam Rupiah
    pub sectors: Vec<String>,           // kosong = semua
    pub boards: Vec<String>,            // kosong = semua
    pub watchlist_id: Option<i64>,
}

pub struct Condition { pub left: Operand, pub cmp: Comparator, pub right: Operand }

pub struct Operand { pub source: OperandSource, pub multiplier: f64 /* default 1.0 */, pub offset: u16 /* bar ke belakang, default 0 */ }

pub enum OperandSource { Indicator(Indicator), Constant(f64) }

pub enum Comparator {
    Gt, Gte, Lt, Lte,
    CrossAbove { within: u8 },        // 1..=20, default 1
    CrossBelow { within: u8 },
}

pub enum Indicator {
    Open, High, Low, Close, Volume,
    ChangePct { period: u16 },
    Sma { period: u16 }, Ema { period: u16 }, VolumeSma { period: u16 },
    Rsi { period: u16 },
    MacdLine { fast: u16, slow: u16, signal: u16 },
    MacdSignal { fast: u16, slow: u16, signal: u16 },
    MacdHist { fast: u16, slow: u16, signal: u16 },
    BbUpper { period: u16, std_x10: u16 },   // std_x10 = 20 → 2.0σ
    BbMiddle { period: u16, std_x10: u16 },
    BbLower { period: u16, std_x10: u16 },
    BbWidth { period: u16, std_x10: u16 },   // (upper - lower) / middle
    Atr { period: u16 },
    StochK { period: u16, smooth: u16 }, StochD { period: u16, smooth: u16, d: u16 },
    HighestHigh { period: u16 }, LowestLow { period: u16 },
    LowestOf { inner: Box<Indicator>, period: u16 }, // nilai minimum indikator dalam N bar
    // yata
    Supertrend { period: u16, mult_x10: u16 },
    Adx { period: u16 }, PlusDi { period: u16 }, MinusDi { period: u16 },
    // Ketiga parameter Ichimoku selalu disertakan (satu pass yata menghitung keempat garis).
    // Displacement Senkou = kijun. Syarat: tenkan < kijun < senkou_b.
    IchimokuTenkan { tenkan: u16, kijun: u16, senkou_b: u16 },
    IchimokuKijun { tenkan: u16, kijun: u16, senkou_b: u16 },
    IchimokuSenkouA { tenkan: u16, kijun: u16, senkou_b: u16 },
    IchimokuSenkouB { tenkan: u16, kijun: u16, senkou_b: u16 },
    ParabolicSar { step_x1000: u16, max_x1000: u16 },
}
```

Parameter pecahan disimpan sebagai integer berskala (`std_x10`, `mult_x10`, `step_x1000`) supaya `Indicator` bisa `Eq + Hash` (untuk deduplikasi) dan key kolomnya stabil.

### 4.2 Implementasi indikator

| Polars (vektor) | Rekursif, per ticker |
|---|---|
| OHLCV, ChangePct, SMA, EMA (`ewm_mean`, `adjust=false`), VolumeSMA | Supertrend (manual, ATR Wilder dengan seed SMA) |
| RSI (Wilder: `ewm_mean` dengan `alpha = 1/n`) | ADX, +DI, −DI (yata, skala 0–100, periode ≥ 2) |
| MACD line/signal/hist | Ichimoku Tenkan/Kijun/Senkou A/B (yata) |
| Bollinger (std populasi, `ddof=0`), BbWidth, ATR (Wilder) | Parabolic SAR (yata, step < max) |
| Stochastic %K/%D, HighestHigh, LowestLow, LowestOf | |

Semua window per ticker (`.over("ticker")`). Bar awal yang belum cukup data bernilai null.

### 4.3 Eksekusi

1. **Validasi:** periode 1–500, jumlah kondisi 1–10, `limit` 1–500, `within` 1–20, `offset` 0–20, `multiplier` finite. Error → `CoreError::Invalid(String)` → HTTP 400.
2. **Kumpulkan** semua `Indicator` unik dari kondisi dan `sort_by`, ditambah kolom bawaan (`close`, `change_pct_1`, `volume`, `avg_value_20`).
3. **Hitung kolom:** indikator Polars ditambahkan lewat `with_columns`. Indikator yata dihitung per partisi ticker lalu di-`hstack` (urutan baris dijaga).
4. **Kompilasi kondisi** ke `Expr`. Operand → `col(key).shift(offset).over("ticker") * multiplier`, atau `lit(c)`.
   - `Gt/Gte/Lt/Lte` → perbandingan langsung.
   - `CrossAbove{within: w}` → `cross = (L > R) & (L.shift(1) <= R.shift(1))`, lolos kalau `cross` true di salah satu dari `w` bar terakhir (`rolling_max` atas cross berbentuk int, window `w`, per ticker).
   - `CrossBelow` simetris.
   - Gabungkan dengan `all_horizontal` (All) atau `any_horizontal` (Any). Null dianggap false.
5. **Ambil bar terakhir** per ticker. Buang ticker yang `date` terakhirnya < `max(date)` global (suspend atau tidak ada transaksi).
6. **Filter universe** (harga, `avg_value_20`, sektor dan papan dari join metadata `stocks`, watchlist dari daftar ticker yang diberikan backend), lalu filter kondisi.
7. **Urutkan** berdasarkan `sort_by` (default `avg_value_20` desc), lalu `limit`.
8. **Output** `ScanResultDto { as_of: String /* YYYY-MM-DD */, total_matched, rows: Vec<ScanRowDto>, columns: Vec<ColumnDto { key, label }> }`. Tiap baris: `ticker, name, close, change_pct, volume, avg_value_20, values: BTreeMap<String, Option<f64>>` untuk setiap indikator yang dipakai di aturan.

### 4.4 Preset

Semua preset memakai `Universe { min_avg_value_20d: Some(1_000_000_000.0), ..Default }`:

| id | Nama | Kondisi |
|---|---|---|
| `golden-cross` | Golden Cross | SMA50 CrossAbove{within:3} SMA200 |
| `rsi-oversold-uptrend` | RSI Oversold dalam Uptrend | RSI14 < 30 AND Close > SMA200 |
| `macd-bullish` | MACD Bullish Cross | MACD line(12,26,9) CrossAbove{within:2} MACD signal(12,26,9) |
| `breakout-52w` | Breakout 52 Minggu | Close > HighestHigh(250) dengan offset 1 |
| `volume-spike` | Volume Spike | Volume > 2× VolumeSMA20 AND ChangePct(1) > 0 |
| `bb-squeeze` | Bollinger Squeeze | BbWidth(20,2.0) <= LowestOf(BbWidth(20,2.0), 120) |
| `supertrend-flip` | Supertrend Flip Bullish | Close CrossAbove{within:2} Supertrend(10,3.0) |

## 5. REST API

Semua di bawah `/api`, JSON. Error: `{ "error": "<kode>", "message": "<pesan dalam Bahasa Indonesia>" }`, dengan kode `bad_request` (400), `not_found` (404), `conflict` (409), `unavailable` (503), dan `internal` (500, detail hanya di log).

| Method & path | Request | Response |
|---|---|---|
| `GET /stocks?q=&limit=` | — | `Vec<StockDto>` (cari prefix ticker atau substring nama, case-insensitive) |
| `GET /stocks/{ticker}` | — | `StockDetailDto` (metadata + harga terakhir, perubahan 1H/1M/3B/1T, high/low 52 minggu, avg value 20 hari) |
| `GET /stocks/{ticker}/candles?range=6m&indicators=sma:50,ema:20,bb:20:20,rsi:14,macd:12:26:9,supertrend:10:30` | — | `{ candles: Vec<CandleDto>, series: Vec<IndicatorSeriesDto> }`. Indikator dihitung dari riwayat penuh lalu dipotong sesuai `range` (`3m, 6m, 1y, 3y, 5y`) |
| `POST /stocks/import` | multipart `file` CSV | `{ inserted, updated }` |
| `POST /scan` | `ScanRequest` | `ScanResultDto` |
| `GET /presets` | — | `Vec<PresetDto>` |
| `GET /scans` · `POST /scans` · `PUT /scans/{id}` · `DELETE /scans/{id}` | `{ name, rules: ScanRequest }` | `SavedScanDto` / 204 |
| `GET /watchlists` | — | `Vec<WatchlistDto>` (beserta item + harga terakhir + perubahan %) |
| `POST /watchlists` · `DELETE /watchlists/{id}` | `{ name }` | `WatchlistDto` / 204 |
| `POST /watchlists/{id}/items/{ticker}` · `DELETE ...` | — | 204 |
| `GET /market/summary` | — | `MarketSummaryDto { as_of, advancers, decliners, unchanged, top_gainers[10], top_losers[10], top_value[10] }` (gainers/losers hanya saham dengan avg value ≥ Rp1 miliar) |
| `POST /ingest/run` | — | 202 + `IngestRunDto`, atau 409 kalau sedang berjalan |
| `GET /ingest/status` | — | `{ current: Option<IngestProgressDto { done, total, failed }>, history: Vec<IngestRunDto> /* 20 terakhir */ }` |

Nama di `POST /scans` yang sudah dipakai → 409. Ticker yang tidak dikenal → 404.

## 6. Frontend

### 6.1 Gaya visual

- Tema gelap: latar `zinc-950`, kartu `zinc-900/60` + `backdrop-blur` + border `white/5`, aksen `indigo-400`.
- Warna harga: naik `emerald-400`, turun `rose-400`, stagnan `zinc-400`.
- Font: Inter untuk teks, JetBrains Mono untuk angka (`tabular-nums`).
- Layout: sidebar kiri (ikon + label, menyempit di layar kecil menjadi bottom bar), konten dengan max-width.
- State loading memakai skeleton, state error memakai banner dengan pesan dari `ApiError.message`.

### 6.2 Halaman

1. **Dashboard `/`:** kartu breadth (naik/turun/stagnan + bar proporsi), tiga tabel ringkas (Top Gainers, Top Losers, Top Nilai Transaksi), kartu status data (tanggal terakhir, status run terakhir), grid tombol preset yang membuka `/scanner?preset=<id>` dan langsung menjalankannya.
2. **Scanner `/scanner`:**
   - Panel kiri (atau atas di layar kecil): chip preset; filter universe (harga min/max, nilai transaksi minimum, multi-select sektor dan papan, pilihan watchlist); toggle logika AND/OR; daftar kondisi.
   - Setiap kondisi: `[Indikator ▾][param…] [×mult] [operator ▾ (+within)] [Indikator|Angka ▾][param…] [hapus]`. Field parameter muncul sesuai jenis indikator.
   - Tombol **Jalankan**, **Simpan**, **Muat scan tersimpan**.
   - Panel kanan: header `N saham cocok · per <as_of>`; tabel dengan kolom tetap (Kode, Nama, Harga, %, Volume, Nilai) ditambah kolom dinamis dari `columns`; sorting di sisi klien per kolom; klik baris → `/stock/:ticker`.
   - State `ScanRequest` di-serialize ke query string (`?q=<base64 json>`) supaya bisa di-bookmark.
3. **Detail saham `/stock/:ticker`:**
   - Header: kode, nama, sektor, harga, perubahan, tombol ★ tambah ke watchlist (dropdown pilih watchlist).
   - Chart utama candlestick + histogram volume. Overlay bisa di-toggle: SMA20/50/200, EMA20, Bollinger(20,2), Supertrend(10,3).
   - Panel bawah: RSI14 (garis 30/70) dan MACD(12,26,9).
   - Pilihan range 3B/6B/1T/3T/5T. Kartu statistik di samping (52w high/low, avg value, perubahan multi-periode).
4. **Watchlist `/watchlist`:** tab per watchlist, tabel (Kode, Nama, Harga, %, Volume), tombol hapus item, buat/hapus watchlist, tombol "Scan watchlist ini" → `/scanner` dengan `universe.watchlist_id` terisi.
5. **Data `/data`:** tombol "Update sekarang" dengan progres bar (polling `GET /ingest/status` tiap 2 detik saat berjalan), tabel riwayat run (waktu, trigger, status, ok/gagal; ticker gagal bisa diklik untuk melihat daftarnya), dan form upload CSV emiten.

### 6.3 Modul

`api.rs` (klien HTTP bertipe untuk setiap endpoint), `pages/` (satu file per halaman), `components/` (tabel, kartu, condition row, select, toast, layout), `chart/` (binding Lightweight Charts dan komponen `<PriceChart>`, `<IndicatorPane>`).

## 7. Penanganan error

- `core`: `CoreError { Invalid(String), Compute(PolarsError) }`.
- `backend`: `AppError { BadRequest, NotFound, Conflict, Unavailable, Internal(anyhow::Error) }` → `IntoResponse` dengan body `ApiError`. Error internal di-log via `tracing` dan hanya mengembalikan pesan generik.
- Ingest: error per ticker tidak fatal (lihat 3.3). Error koneksi database saat ingest → run `failed` dan `error` diisi.
- Frontend: setiap pemanggilan API mengembalikan `Result<T, ApiError>`. Error ditampilkan sebagai banner/toast. Error jaringan dipetakan ke "Tidak dapat terhubung ke server".

## 8. Pengujian

- **`core`:**
  - Unit test tiap indikator Polars terhadap nilai referensi yang dihitung manual atau dengan yata pada seri kecil (toleransi 1e-6).
  - Test bahwa nilai bar awal null.
  - Test engine dengan `DataFrame` sintetis multi-ticker: perbandingan, cross dengan `within`, offset, multiplier, AND/OR, pembuangan ticker basi, filter universe, sorting, dan limit.
  - Test validasi menolak input di luar batas.
  - Test setiap preset lolos validasi.
- **`backend`:**
  - Parser Yahoo dites dengan fixture JSON (`tests/fixtures/`), termasuk kasus null dan split.
  - Integration test endpoint memakai `#[sqlx::test]` dengan database Postgres test: CRUD watchlist dan scan tersimpan, 404/409, `POST /scan` dengan data seed kecil, `503` saat cache kosong.
  - Klien Yahoo di balik trait `PriceSource` supaya orkestrasi ingest bisa dites dengan implementasi palsu.
- **`frontend`:** `cargo check --target wasm32-unknown-unknown`, unit test fungsi murni (format angka Rupiah, serialisasi query string), dan verifikasi manual di browser.

## 9. Konfigurasi dan menjalankan

- **Podman compose** (`compose.yaml`, stack `scanner-saham`): container `scanner-saham-db` (`postgres:16`, volume `scanner-saham-pgdata`, port host `5440`) dan `scanner-saham-app` (image `debian:trixie-slim` berisi binary backend + `dist` frontend yang di-build di host, port `3000`, user non-root). `./prod.sh` build di host lalu menjalankan keduanya; `./prod.sh stop` = `podman compose down` (data tetap). `scripts/db.sh` hanya menjalankan service `db` untuk dev dan test.
- `.env.example`: `DATABASE_URL`, `BIND_ADDR=0.0.0.0:3000`, `INGEST_AT=17:00`, `INGEST_CONCURRENCY=4`, `BACKFILL_YEARS=5`, `CACHE_BARS=300`, `STATIC_DIR=crates/frontend/dist`.
- Dev: `scripts/db.sh start`, `cargo run -p backend` (migrasi otomatis saat startup), `trunk serve` di `crates/frontend`.
- README berisi langkah setup, termasuk `rustup target add wasm32-unknown-unknown` dan `cargo install trunk`.

## 10. Dokumentasi arsitektur

`docs/architecture.md` berisi diagram Mermaid (crate dan dependensinya, modul per crate, tipe utama, alur data ingest dan scan). File ini diperbarui setiap kali kode berubah, sebagai pengganti graphify yang tidak terpasang.
