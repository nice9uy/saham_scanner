//! Update data harga: backfill untuk saham tanpa data, incremental untuk sisanya.

pub mod listing;
pub mod live;
pub mod schedule;
pub mod yahoo;

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{Days, NaiveDate};
use futures::{StreamExt, stream};
use shared::{IngestProgressDto, IngestRunDto};
use sqlx::PgPool;

use crate::db::prices::PriceBar;
use crate::db::stocks::StockRecord;
use crate::db::{fundamentals, index_prices, ingest_runs, prices, stocks, win_rates};
use crate::error::AppError;
use crate::AppState;

/// Hari ke belakang yang diambil ulang saat update incremental (menangkap koreksi data).
const OVERLAP_DAYS: u64 = 5;

#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    pub name: Option<String>,
    pub bars: Vec<PriceBar>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FetchError {
    NotFound,
    RateLimited,
    Transient(String),
    Invalid(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::NotFound => write!(f, "saham tidak ditemukan di sumber data"),
            FetchError::RateLimited => write!(f, "dibatasi sumber data (429)"),
            FetchError::Transient(m) => write!(f, "gangguan sementara: {m}"),
            FetchError::Invalid(m) => write!(f, "respons tidak valid: {m}"),
        }
    }
}

#[async_trait]
pub trait PriceSource: Send + Sync {
    async fn fetch(&self, ticker: &str, from: NaiveDate, to: NaiveDate) -> Result<Fetched, FetchError>;

    /// Harga indeks (simbol Yahoo apa adanya, mis. `^JKSE`). Default: tidak ada data.
    async fn fetch_index(&self, _symbol: &str, _from: NaiveDate, _to: NaiveDate) -> Result<Fetched, FetchError> {
        Ok(Fetched { name: None, bars: vec![] })
    }

    /// Candle hari ini (`today`, tanggal bursa) untuk banyak simbol dalam satu request. Kode saham
    /// tanpa `.JK`; indeks apa adanya (mis. `^JKSE`). Simbol yang belum bertransaksi hari ini
    /// tidak dikembalikan. Default: tidak didukung (kosong).
    async fn fetch_today(&self, _symbols: &[String], _today: NaiveDate) -> Result<Vec<(String, PriceBar)>, FetchError> {
        Ok(vec![])
    }

    /// Data fundamental terkini (rupiah per saham) untuk banyak saham. Default: tidak didukung.
    async fn fetch_fundamentals(
        &self,
        _tickers: &[String],
    ) -> Result<Vec<(String, scanner_core::quality::Fundamentals)>, FetchError> {
        Ok(vec![])
    }

    /// Daftar seluruh emiten untuk disinkronkan sebelum update harga. Kosong = lewati sinkronisasi.
    async fn listing(&self) -> Result<Vec<StockRecord>, FetchError> {
        Ok(vec![])
    }
}

#[derive(Default)]
struct HandleInner {
    running: AtomicBool,
    done: AtomicUsize,
    total: AtomicUsize,
    failed: AtomicUsize,
    /// Naik setiap update harian selesai; hasil yang bergantung pada seluruh riwayat
    /// (mis. backtest) dihitung ulang kalau versinya berubah.
    version: AtomicU64,
}

/// Menjamin hanya satu update berjalan dalam satu waktu, sekaligus menyimpan progresnya.
#[derive(Clone, Default)]
pub struct IngestHandle {
    inner: Arc<HandleInner>,
}

/// Selama guard ini hidup, update dianggap sedang berjalan.
pub struct IngestGuard {
    inner: Arc<HandleInner>,
}

impl Drop for IngestGuard {
    fn drop(&mut self) {
        self.inner.running.store(false, Ordering::SeqCst);
    }
}

impl IngestGuard {
    fn set_total(&self, total: usize) {
        self.inner.total.store(total, Ordering::SeqCst);
    }

    fn record(&self, ok: bool) {
        self.inner.done.fetch_add(1, Ordering::SeqCst);
        if !ok {
            self.inner.failed.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl IngestHandle {
    pub fn try_begin(&self) -> Option<IngestGuard> {
        self.inner.running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).ok()?;
        for counter in [&self.inner.done, &self.inner.total, &self.inner.failed] {
            counter.store(0, Ordering::SeqCst);
        }
        Some(IngestGuard { inner: self.inner.clone() })
    }

    pub fn version(&self) -> u64 {
        self.inner.version.load(Ordering::SeqCst)
    }

    pub fn progress(&self) -> Option<IngestProgressDto> {
        self.inner.running.load(Ordering::SeqCst).then(|| IngestProgressDto {
            done: self.inner.done.load(Ordering::SeqCst),
            total: self.inner.total.load(Ordering::SeqCst),
            failed: self.inner.failed.load(Ordering::SeqCst),
        })
    }
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub concurrency: usize,
    pub backfill_years: u32,
    pub today: NaiveDate,
    /// Jeda sebelum setiap percobaan ulang (default 1s, 2s, 4s).
    pub retry_delays: Vec<Duration>,
}

async fn fetch_with_retry(
    source: &dyn PriceSource,
    ticker: &str,
    from: NaiveDate,
    to: NaiveDate,
    delays: &[Duration],
) -> Result<Fetched, FetchError> {
    let mut attempt = 0;
    loop {
        match source.fetch(ticker, from, to).await {
            Err(FetchError::RateLimited | FetchError::Transient(_)) if attempt < delays.len() => {
                tokio::time::sleep(delays[attempt]).await;
                attempt += 1;
            }
            other => return other,
        }
    }
}

async fn ingest_one(
    pool: &PgPool,
    source: &dyn PriceSource,
    ticker: &str,
    last: Option<NaiveDate>,
    opts: &RunOptions,
) -> Result<(), String> {
    let from = match last {
        Some(d) => d.checked_sub_days(Days::new(OVERLAP_DAYS)).unwrap_or(d),
        None => opts
            .today
            .checked_sub_days(Days::new(u64::from(opts.backfill_years) * 366))
            .unwrap_or(opts.today),
    };
    let fetched = fetch_with_retry(source, ticker, from, opts.today, &opts.retry_delays)
        .await
        .map_err(|e| e.to_string())?;
    prices::upsert_bars(pool, ticker, &fetched.bars).await.map_err(|e| format!("gagal menyimpan: {e}"))?;
    if let Some(name) = &fetched.name {
        stocks::fill_name_if_missing(pool, ticker, name).await.map_err(|e| format!("gagal menyimpan nama: {e}"))?;
    }
    Ok(())
}

/// IHSG untuk kondisi pasar. Kegagalan tidak menghentikan update harga saham.
async fn ingest_index(pool: &PgPool, source: &dyn PriceSource, opts: &RunOptions) -> Result<(), String> {
    let last = index_prices::last_date(pool, index_prices::IHSG).await.map_err(|e| e.to_string())?;
    let from = match last {
        Some(d) => d.checked_sub_days(Days::new(OVERLAP_DAYS)).unwrap_or(d),
        None => opts.today.checked_sub_days(Days::new(u64::from(opts.backfill_years) * 366)).unwrap_or(opts.today),
    };
    let mut attempt = 0;
    let fetched = loop {
        match source.fetch_index(index_prices::IHSG, from, opts.today).await {
            Err(FetchError::RateLimited | FetchError::Transient(_)) if attempt < opts.retry_delays.len() => {
                tokio::time::sleep(opts.retry_delays[attempt]).await;
                attempt += 1;
            }
            other => break other.map_err(|e| e.to_string())?,
        }
    };
    index_prices::upsert(pool, index_prices::IHSG, &fetched.bars).await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Win rate sinyal beli per saham dari seluruh riwayat (dimuat 100 ticker sekaligus).
pub async fn refresh_win_rates(pool: &PgPool) -> anyhow::Result<()> {
    let started = std::time::Instant::now();
    let market = index_prices::market_regime(pool).await?;
    let mut all = std::collections::HashMap::new();
    for batch in prices::tickers_with_prices(pool).await?.chunks(100) {
        let rows = prices::history_many(pool, batch).await?;
        let market = market.clone();
        let part = tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
            Ok(scanner_core::backtest::win_rates(&crate::frame::prices_frame(&rows)?, &market)?)
        })
        .await??;
        all.extend(part);
    }
    let n = win_rates::replace_all(pool, &all).await?;
    tracing::info!(saham = n, detik = started.elapsed().as_secs(), "win rate diperbarui");
    Ok(())
}

/// Data fundamental semua emiten aktif. Kegagalan tidak menghentikan update harga.
async fn ingest_fundamentals(pool: &PgPool, source: &dyn PriceSource) -> Result<(), String> {
    let tickers: Vec<String> =
        stocks::active_tickers_with_last_date(pool).await.map_err(|e| e.to_string())?.into_iter().map(|(t, _)| t).collect();
    let rows = source.fetch_fundamentals(&tickers).await.map_err(|e| e.to_string())?;
    let n = fundamentals::upsert_many(pool, &rows).await.map_err(|e| e.to_string())?;
    tracing::info!(saham = n, "data fundamental diperbarui");
    Ok(())
}

/// Saat start: kalau data IHSG atau fundamental belum ada (mis. baru upgrade), ambil sekarang lalu
/// muat ulang cache, supaya keduanya langsung berlaku tanpa menunggu update harian.
pub fn ensure_index(state: &AppState) {
    let state = state.clone();
    tokio::spawn(async move {
        let mut fetched = false;
        if matches!(index_prices::last_date(&state.pool, index_prices::IHSG).await, Ok(None)) {
            match ingest_index(&state.pool, state.source.as_ref(), &state.run_options()).await {
                Ok(()) => {
                    tracing::info!("data IHSG diambil");
                    fetched = true;
                }
                Err(e) => tracing::warn!("IHSG gagal diambil, dicoba lagi saat update harian: {e}"),
            }
        }
        if matches!(fundamentals::is_empty(&state.pool).await, Ok(true)) {
            match ingest_fundamentals(&state.pool, state.source.as_ref()).await {
                Ok(()) => fetched = true,
                Err(e) => tracing::warn!("data fundamental gagal diambil, dicoba lagi saat update harian: {e}"),
            }
        }
        if matches!(win_rates::is_empty(&state.pool).await, Ok(true)) && matches!(prices::is_empty(&state.pool).await, Ok(false)) {
            match refresh_win_rates(&state.pool).await {
                Ok(()) => fetched = true,
                Err(e) => tracing::warn!("win rate gagal dihitung, dicoba lagi saat update harian: {e:#}"),
            }
        }
        if fetched && let Err(e) = state.cache.reload(&state.pool, state.config.cache_bars).await {
            tracing::error!("gagal memuat ulang cache: {e:#}");
        }
    });
}

/// Menjalankan satu update penuh dan mencatatnya di `ingest_runs`.
pub async fn run(
    pool: &PgPool,
    source: Arc<dyn PriceSource>,
    guard: &IngestGuard,
    trigger: &str,
    opts: &RunOptions,
) -> anyhow::Result<IngestRunDto> {
    let id = ingest_runs::start(pool, trigger).await?;
    if let Err(e) = ingest_index(pool, source.as_ref(), opts).await {
        tracing::warn!("IHSG gagal diperbarui: {e}");
    }
    let outcome: anyhow::Result<(i32, Vec<String>)> = async {
        sync_listing(pool, source.as_ref()).await;
        let tickers = stocks::active_tickers_with_last_date(pool).await?;
        guard.set_total(tickers.len());
        let results: Vec<(String, Result<(), String>)> = stream::iter(tickers)
            .map(|(ticker, last)| {
                let source = source.clone();
                async move {
                    let r = ingest_one(pool, source.as_ref(), &ticker, last, opts).await;
                    if let Err(e) = &r {
                        tracing::warn!(%ticker, "update gagal: {e}");
                    }
                    guard.record(r.is_ok());
                    (ticker, r)
                }
            })
            .buffer_unordered(opts.concurrency.max(1))
            .collect()
            .await;
        if let Err(e) = ingest_fundamentals(pool, source.as_ref()).await {
            tracing::warn!("data fundamental gagal diperbarui: {e}");
        }
        let ok = results.iter().filter(|(_, r)| r.is_ok()).count() as i32;
        let mut failed: Vec<String> = results.into_iter().filter(|(_, r)| r.is_err()).map(|(t, _)| t).collect();
        failed.sort();
        Ok((ok, failed))
    }
    .await;

    match outcome {
        Ok((ok, failed)) => {
            let status = match (ok, failed.is_empty()) {
                (_, true) => "success",
                (0, false) => "failed",
                _ => "partial",
            };
            ingest_runs::finish(pool, id, status, ok, &failed, None).await?;
        }
        Err(e) => ingest_runs::finish(pool, id, "failed", 0, &[], Some(&format!("{e:#}"))).await?,
    }
    Ok(ingest_runs::get(pool, id).await?)
}

/// Memulai update di background lalu memuat ulang cache. 409 kalau sedang berjalan.
pub fn trigger(state: &AppState, trigger: &'static str) -> Result<(), AppError> {
    let guard = state
        .ingest
        .try_begin()
        .ok_or_else(|| AppError::Conflict("Update data sedang berjalan".into()))?;
    let state = state.clone();
    tokio::spawn(async move {
        let opts = state.run_options();
        match run(&state.pool, state.source.clone(), &guard, trigger, &opts).await {
            Ok(r) => tracing::info!(status = %r.status, ok = r.ok_count, gagal = r.failed_tickers.len(), "update data selesai"),
            Err(e) => tracing::error!("update data gagal: {e:#}"),
        }
        if let Err(e) = refresh_win_rates(&state.pool).await {
            tracing::warn!("win rate gagal dihitung: {e:#}");
        }
        if let Err(e) = state.cache.reload(&state.pool, state.config.cache_bars).await {
            tracing::error!("gagal memuat ulang cache: {e:#}");
        }
        guard.inner.version.fetch_add(1, Ordering::SeqCst);
        drop(guard);
    });
    Ok(())
}

/// Menyamakan daftar emiten dengan sumber. Kegagalan tidak menghentikan update:
/// harga tetap diambil untuk emiten yang sudah tersimpan.
async fn sync_listing(pool: &PgPool, source: &dyn PriceSource) {
    match source.listing().await {
        Ok(list) if list.is_empty() => {}
        Ok(list) => match stocks::sync_listing(pool, &list).await {
            Ok(r) => tracing::info!(
                emiten = list.len(),
                baru = r.inserted,
                aktif_lagi = r.reactivated,
                nonaktif = r.deactivated,
                "daftar emiten disinkronkan"
            ),
            Err(e) => tracing::warn!("gagal menyimpan daftar emiten: {e}"),
        },
        Err(e) => tracing::warn!("daftar emiten tidak bisa diambil, memakai daftar tersimpan: {e}"),
    }
}
