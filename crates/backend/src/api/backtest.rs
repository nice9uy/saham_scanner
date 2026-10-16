//! Uji historis rating sinyal atas seluruh riwayat harga. Perhitungannya berat (semua saham,
//! 5 tahun), jadi hasilnya disimpan dan baru dihitung ulang setelah update harian berikutnya.

use std::collections::HashMap;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Query, State};
use scanner_core::{Backtest, CoreError};
use serde::Deserialize;
use shared::BacktestDto;
use tokio::sync::Mutex;

use crate::AppState;
use crate::db::prices;
use crate::error::{ApiResult, AppError};
use crate::frame::prices_frame;

/// Jumlah ticker yang dimuat sekaligus, supaya memori tetap kecil.
const BATCH: usize = 100;

/// `liquid_only` → (versi data, hasil).
type Results = HashMap<bool, (u64, Arc<BacktestDto>)>;

#[derive(Clone, Default)]
pub struct BacktestCache {
    /// Mutex juga mencegah dua perhitungan berjalan bersamaan.
    inner: Arc<Mutex<Results>>,
}

#[derive(Deserialize)]
pub struct Params {
    liquid: Option<bool>,
}

async fn compute(state: &AppState, liquid_only: bool) -> Result<BacktestDto, AppError> {
    let started = std::time::Instant::now();
    let tickers = prices::tickers_with_prices(&state.pool).await?;
    let market = crate::db::index_prices::market_regime(&state.pool).await?;
    let mut bt = Backtest::new(liquid_only, market);
    for batch in tickers.chunks(BATCH) {
        let rows = prices::history_many(&state.pool, batch).await?;
        bt = tokio::task::spawn_blocking(move || -> Result<Backtest, CoreError> {
            bt.add(&prices_frame(&rows)?)?;
            Ok(bt)
        })
        .await??;
    }
    let dto = bt.finish();
    tracing::info!(saham = dto.tickers, likuid = liquid_only, detik = started.elapsed().as_secs(), "backtest selesai");
    Ok(dto)
}

/// `GET /backtest?liquid=true` (default: hanya hari saat saham likuid).
pub async fn run(State(state): State<AppState>, Query(p): Query<Params>) -> ApiResult<Arc<BacktestDto>> {
    let liquid_only = p.liquid.unwrap_or(true);
    let version = state.ingest.version();
    let mut cache = state.backtest.inner.lock().await;
    if let Some((v, dto)) = cache.get(&liquid_only)
        && *v == version
    {
        return Ok(Json(dto.clone()));
    }
    let dto = Arc::new(compute(&state, liquid_only).await?);
    if dto.tickers == 0 {
        return Err(AppError::Unavailable("Belum ada riwayat harga yang cukup untuk backtest".into()));
    }
    cache.insert(liquid_only, (version, dto.clone()));
    Ok(Json(dto))
}
