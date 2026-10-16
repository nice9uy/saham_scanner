use axum::Json;
use axum::extract::{Multipart, Path, Query, State};
use polars::prelude::*;
use scanner_core::{Annotations, format_day, indicators::add_indicators, stock_stats};
use serde::Deserialize;
use shared::{CandleDto, CandlesDto, ImportResultDto, IndicatorSeriesDto, StockDetailDto, StockDto};

use super::chart::{parse_indicators, range_bars};
use crate::AppState;
use crate::db::{prices, stocks};
use crate::error::{ApiResult, AppError};
use crate::frame::prices_frame;

#[derive(Deserialize)]
pub struct ListQuery {
    q: Option<String>,
    limit: Option<i64>,
}

pub async fn list(State(state): State<AppState>, Query(q): Query<ListQuery>) -> ApiResult<Vec<StockDto>> {
    let limit = q.limit.unwrap_or(20).clamp(1, 1000);
    Ok(Json(stocks::list(&state.pool, q.q.as_deref(), limit).await?))
}

async fn find(state: &AppState, ticker: &str) -> Result<StockDto, AppError> {
    stocks::get(&state.pool, ticker)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Saham {} tidak ditemukan", ticker.to_uppercase())))
}

pub async fn detail(State(state): State<AppState>, Path(ticker): Path<String>) -> ApiResult<StockDetailDto> {
    let stock = find(&state, &ticker).await?;
    let rows = prices::history(&state.pool, &stock.ticker).await?;
    let ticker = stock.ticker.clone();
    let snapshot = state.cache.snapshot();
    let market = snapshot.as_ref().map(|d| d.market.clone()).unwrap_or_default();
    let quality = snapshot.as_ref().and_then(|d| d.annotations.quality.get(&stock.ticker).cloned());
    let win_rate = snapshot.as_ref().and_then(|d| d.annotations.win_rates.get(&stock.ticker).copied());
    let (stats, risk, signal) = tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let df = prices_frame(&rows).map_err(anyhow::Error::from)?;
        // Sama dengan scan: Best Buy dibatalkan oleh peringatan risiko dan kondisi pasar.
        let mut ann = Annotations::compute(&df, &market)?;
        Ok((stock_stats(&df)?, ann.risks.remove(&ticker), ann.signals.remove(&ticker)))
    })
    .await??;
    Ok(Json(StockDetailDto { stock, stats, risk, signal, quality, win_rate }))
}

#[derive(Deserialize)]
pub struct CandlesQuery {
    range: Option<String>,
    indicators: Option<String>,
}

pub async fn candles(
    State(state): State<AppState>,
    Path(ticker): Path<String>,
    Query(q): Query<CandlesQuery>,
) -> ApiResult<CandlesDto> {
    let bars = range_bars(q.range.as_deref()).map_err(AppError::BadRequest)?;
    let inds = parse_indicators(q.indicators.as_deref().unwrap_or("")).map_err(AppError::BadRequest)?;
    let stock = find(&state, &ticker).await?;
    let rows = prices::history(&state.pool, &stock.ticker).await?;

    let dto = tokio::task::spawn_blocking(move || -> Result<CandlesDto, AppError> {
        let df = add_indicators(prices_frame(&rows).map_err(anyhow::Error::from)?, &inds)?;
        let start = df.height().saturating_sub(bars);
        let df = df.slice(start as i64, bars);
        let f64s = |name: &str| -> Result<Vec<Option<f64>>, AppError> {
            let c = df.column(name).map_err(anyhow::Error::from)?;
            Ok(c.f64().map_err(anyhow::Error::from)?.iter().map(|v| v.filter(|x| x.is_finite())).collect())
        };
        let days = df.column("date").and_then(|c| c.cast(&DataType::Int32)).map_err(anyhow::Error::from)?;
        let days: Vec<i32> = days.i32().map_err(anyhow::Error::from)?.iter().map(|d| d.unwrap_or_default()).collect();
        let (open, high, low, close, volume) = (f64s("open")?, f64s("high")?, f64s("low")?, f64s("close")?, f64s("volume")?);
        let candles = (0..df.height())
            .map(|i| CandleDto {
                date: format_day(days[i]),
                open: open[i].unwrap_or_default(),
                high: high[i].unwrap_or_default(),
                low: low[i].unwrap_or_default(),
                close: close[i].unwrap_or_default(),
                volume: volume[i].unwrap_or_default(),
            })
            .collect();
        let series = inds
            .iter()
            .map(|ind| Ok(IndicatorSeriesDto { key: ind.key(), label: ind.label(), values: f64s(&ind.key())? }))
            .collect::<Result<_, AppError>>()?;
        Ok(CandlesDto { candles, series })
    })
    .await??;
    Ok(Json(dto))
}

/// Upload CSV daftar emiten (field multipart `file`).
pub async fn import(State(state): State<AppState>, mut form: Multipart) -> ApiResult<ImportResultDto> {
    let mut data = None;
    while let Some(field) = form.next_field().await.map_err(|e| AppError::BadRequest(e.to_string()))? {
        if field.name() == Some("file") {
            data = Some(field.bytes().await.map_err(|e| AppError::BadRequest(e.to_string()))?);
        }
    }
    let data = data.ok_or_else(|| AppError::BadRequest("Field 'file' tidak ada".into()))?;
    let records = stocks::parse_csv(data.as_ref()).map_err(|e| AppError::BadRequest(format!("{e:#}")))?;
    if records.is_empty() {
        return Err(AppError::BadRequest("CSV tidak berisi emiten".into()));
    }
    let result = stocks::upsert_many(&state.pool, &records).await?;
    state.cache.reload(&state.pool, state.config.cache_bars).await?;
    Ok(Json(result))
}
