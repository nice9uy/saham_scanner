//! Backend Scanner Saham: REST API Axum di atas PostgreSQL dan `scanner-core`.

pub mod api;
pub mod cache;
pub mod config;
pub mod db;
pub mod error;
pub mod frame;
pub mod ingest;
pub mod system;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{FixedOffset, Utc};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub use api::router as build_app;
use cache::PriceCache;
use config::Config;
use ingest::live::LiveHandle;
use ingest::{IngestHandle, PriceSource, RunOptions};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub cache: PriceCache,
    pub ingest: IngestHandle,
    pub live: LiveHandle,
    pub backtest: api::backtest::BacktestCache,
    pub system: system::SystemMonitor,
    pub source: Arc<dyn PriceSource>,
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(pool: PgPool, source: Arc<dyn PriceSource>, config: Config) -> Self {
        Self { pool, cache: PriceCache::default(), ingest: IngestHandle::default(), live: LiveHandle::default(),
            backtest: Default::default(),
            system: Default::default(),
            source, config: Arc::new(config) }
    }

    pub fn run_options(&self) -> RunOptions {
        let wib = FixedOffset::east_opt(7 * 3600).expect("offset WIB valid");
        RunOptions {
            concurrency: self.config.ingest_concurrency,
            backfill_years: self.config.backfill_years,
            today: Utc::now().with_timezone(&wib).date_naive(),
            retry_delays: [1, 2, 4].map(Duration::from_secs).to_vec(),
        }
    }
}

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

/// Mengisi tabel `stocks` dari CSV seed kalau masih kosong.
pub async fn seed_if_empty(pool: &PgPool, path: &std::path::Path) -> Result<()> {
    if db::stocks::count(pool).await? > 0 || !path.exists() {
        return Ok(());
    }
    let file = std::fs::File::open(path).with_context(|| format!("membuka {}", path.display()))?;
    let records = db::stocks::parse_csv(file)?;
    let r = db::stocks::upsert_many(pool, &records).await?;
    tracing::info!(emiten = r.inserted, "seed daftar emiten dari {}", path.display());
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async { tokio::signal::ctrl_c().await.ok() };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("server berhenti");
}

pub async fn run(config: Config) -> Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&config.database_url)
        .await
        .with_context(|| "tidak bisa terhubung ke PostgreSQL (sudah menjalankan scripts/db.sh start?)")?;
    MIGRATOR.run(&pool).await.context("migrasi database gagal")?;
    db::ingest_runs::fail_abandoned(&pool).await?;
    seed_if_empty(&pool, &config.seed_csv).await?;

    let source = Arc::new(ingest::yahoo::YahooSource::new()?);
    let bind = config.bind_addr.clone();
    let static_dir = config.static_dir.clone();
    let state = AppState::new(pool, source, config);
    state.cache.reload(&state.pool, state.config.cache_bars).await?;

    state.system.spawn();
    ingest::schedule::spawn_scheduler(state.clone());
    ingest::ensure_index(&state);
    if state.config.live_enabled {
        ingest::live::spawn(state.clone());
    }
    if db::prices::is_empty(&state.pool).await? {
        tracing::info!("belum ada data harga, memulai backfill");
        ingest::trigger(&state, "backfill").map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }

    let listener = tokio::net::TcpListener::bind(&bind).await.with_context(|| format!("bind {bind}"))?;
    match &static_dir {
        Some(dir) => tracing::info!("frontend disajikan dari {}", dir.display()),
        None => tracing::info!("frontend belum di-build; hanya /api yang tersedia"),
    }
    tracing::info!("server berjalan di http://{bind}");
    axum::serve(listener, build_app(state)).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}
