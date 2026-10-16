use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use shared::IngestStatusDto;

use crate::AppState;
use crate::db::ingest_runs;
use crate::error::{ApiResult, AppError};

const HISTORY: i64 = 20;

async fn current(state: &AppState) -> Result<IngestStatusDto, AppError> {
    Ok(IngestStatusDto {
        current: state.ingest.progress(),
        history: ingest_runs::recent(&state.pool, HISTORY).await?,
        live: state.config.live_enabled.then(|| state.live.status()),
    })
}

pub async fn status(State(state): State<AppState>) -> ApiResult<IngestStatusDto> {
    Ok(Json(current(&state).await?))
}

/// Memulai update di background; 202 dengan status terkini, atau 409 kalau sedang berjalan.
pub async fn run(State(state): State<AppState>) -> Result<(StatusCode, Json<IngestStatusDto>), AppError> {
    crate::ingest::trigger(&state, "manual")?;
    Ok((StatusCode::ACCEPTED, Json(current(&state).await?)))
}
