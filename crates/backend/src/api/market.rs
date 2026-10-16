use axum::Json;
use axum::extract::State;
use shared::MarketSummaryDto;

use super::scan::NO_DATA;
use crate::AppState;
use crate::error::{ApiResult, AppError};

const TOP: usize = 10;

pub async fn summary(State(state): State<AppState>) -> ApiResult<MarketSummaryDto> {
    let data = state.cache.snapshot().ok_or_else(|| AppError::Unavailable(NO_DATA.into()))?;
    let dto = tokio::task::spawn_blocking(move || -> Result<_, AppError> {
        let mut dto = scanner_core::market_summary(&data.prices, &data.meta, TOP)?;
        dto.market_strong = latest_day(&dto.as_of).and_then(|d| data.market.strong_at(d));
        Ok(dto)
    })
    .await??;
    Ok(Json(dto))
}

fn latest_day(iso: &str) -> Option<i32> {
    chrono::NaiveDate::parse_from_str(iso, "%Y-%m-%d").ok().map(crate::frame::days_since_epoch)
}
