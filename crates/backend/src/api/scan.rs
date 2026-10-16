use std::collections::HashSet;

use axum::Json;
use axum::extract::State;
use shared::{PresetDto, ScanRequest, ScanResultDto};

use crate::AppState;
use crate::db::watchlists;
use crate::error::{ApiResult, AppError};

pub const NO_DATA: &str = "Data belum tersedia, jalankan update data";

pub async fn run(State(state): State<AppState>, Json(req): Json<ScanRequest>) -> ApiResult<ScanResultDto> {
    let data = state.cache.snapshot().ok_or_else(|| AppError::Unavailable(NO_DATA.into()))?;
    let allowed: Option<HashSet<String>> = match req.universe.watchlist_id {
        Some(id) => Some(
            watchlists::tickers(&state.pool, id)
                .await?
                .ok_or_else(|| AppError::NotFound("Watchlist tidak ditemukan".into()))?
                .into_iter()
                .collect(),
        ),
        None => None,
    };
    let result = tokio::task::spawn_blocking(move || {
        scanner_core::run_scan_annotated(&data.prices, &data.meta, &req, allowed.as_ref(), Some(&data.annotations))
    })
    .await??;
    Ok(Json(result))
}

pub async fn presets() -> Json<Vec<PresetDto>> {
    Json(scanner_core::presets())
}
