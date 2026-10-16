//! Ringkasan pasar harian (halaman Berita), disusun dari cache harga dan data IHSG.

use axum::Json;
use axum::extract::State;
use polars::prelude::*;
use scanner_core::brief::{BriefInput, brief};
use shared::BriefDto;

use super::scan::NO_DATA;
use crate::AppState;
use crate::db::{index_prices, watchlists};
use crate::error::{ApiResult, AppError};

pub async fn get(State(state): State<AppState>) -> ApiResult<BriefDto> {
    let data = state.cache.snapshot().ok_or_else(|| AppError::Unavailable(NO_DATA.into()))?;
    let ihsg = index_prices::last_two(&state.pool, index_prices::IHSG).await?;
    let watchlist = watchlists::all_tickers(&state.pool).await?;
    let dto = tokio::task::spawn_blocking(move || -> Result<BriefDto, AppError> {
        let latest = data.prices.column("date").and_then(|c| c.cast(&DataType::Int32)).ok();
        let latest = latest.and_then(|c| c.i32().ok().and_then(|c| c.max()));
        Ok(brief(&BriefInput {
            prices: &data.prices,
            meta: &data.meta,
            annotations: &data.annotations,
            ihsg,
            market_strong: latest.and_then(|d| data.market.strong_at(d)),
            watchlist: &watchlist,
        })?)
    })
    .await??;
    Ok(Json(dto))
}
