use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use shared::{CreateWatchlistDto, WatchlistDto, WatchlistItemDto};

use crate::AppState;
use crate::db::{stocks, watchlists};
use crate::error::{ApiResult, AppError};

fn not_found() -> AppError {
    AppError::NotFound("Watchlist tidak ditemukan".into())
}

pub async fn list(State(state): State<AppState>) -> ApiResult<Vec<WatchlistDto>> {
    let rows = watchlists::list(&state.pool).await?;
    let cache = state.cache.snapshot();
    let dto = rows
        .into_iter()
        .map(|w| WatchlistDto {
            id: w.id,
            name: w.name,
            items: w
                .items
                .into_iter()
                .map(|(ticker, name)| {
                    let last = cache.as_ref().and_then(|c| c.last.get(&ticker).copied());
                    let signal = cache.as_ref().and_then(|c| c.annotations.signals.get(&ticker)).map(|s| s.level);
                    let trailing_stop = cache.as_ref().and_then(|c| c.annotations.risks.get(&ticker)).and_then(|r| r.trailing_stop);
                    WatchlistItemDto {
                        ticker,
                        name,
                        close: last.map(|l| l.close),
                        change_pct: last.and_then(|l| l.change_pct),
                        volume: last.map(|l| l.volume),
                        signal,
                        trailing_stop,
                    }
                })
                .collect(),
        })
        .collect();
    Ok(Json(dto))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateWatchlistDto>,
) -> Result<(StatusCode, Json<WatchlistDto>), AppError> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Nama watchlist wajib diisi".into()));
    }
    let id = watchlists::create(&state.pool, name).await?;
    Ok((StatusCode::CREATED, Json(WatchlistDto { id, name: name.into(), items: vec![] })))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<i64>) -> Result<StatusCode, AppError> {
    if watchlists::delete(&state.pool, id).await? { Ok(StatusCode::NO_CONTENT) } else { Err(not_found()) }
}

pub async fn add(
    State(state): State<AppState>,
    Path((id, ticker)): Path<(i64, String)>,
) -> Result<StatusCode, AppError> {
    if !watchlists::exists(&state.pool, id).await? {
        return Err(not_found());
    }
    let stock = stocks::get(&state.pool, &ticker)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Saham {} tidak ditemukan", ticker.to_uppercase())))?;
    watchlists::add_item(&state.pool, id, &stock.ticker).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove(
    State(state): State<AppState>,
    Path((id, ticker)): Path<(i64, String)>,
) -> Result<StatusCode, AppError> {
    if !watchlists::exists(&state.pool, id).await? {
        return Err(not_found());
    }
    if watchlists::remove_item(&state.pool, id, &ticker.to_uppercase()).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound("Saham tidak ada di watchlist".into()))
    }
}
