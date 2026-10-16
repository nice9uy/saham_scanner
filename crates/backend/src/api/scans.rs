use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use shared::{SaveScanDto, SavedScanDto};

use crate::AppState;
use crate::db::scans;
use crate::error::{ApiResult, AppError};

fn checked(body: &SaveScanDto) -> Result<String, AppError> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Nama scan wajib diisi".into()));
    }
    scanner_core::validate(&body.rules)?;
    Ok(name.to_string())
}

pub async fn list(State(state): State<AppState>) -> ApiResult<Vec<SavedScanDto>> {
    Ok(Json(scans::list(&state.pool).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<SaveScanDto>,
) -> Result<(StatusCode, Json<SavedScanDto>), AppError> {
    let name = checked(&body)?;
    Ok((StatusCode::CREATED, Json(scans::create(&state.pool, &name, &body.rules).await?)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<SaveScanDto>,
) -> ApiResult<SavedScanDto> {
    let name = checked(&body)?;
    scans::update(&state.pool, id, &name, &body.rules)
        .await?
        .map(Json)
        .ok_or_else(|| AppError::NotFound("Scan tidak ditemukan".into()))
}

pub async fn delete(State(state): State<AppState>, Path(id): Path<i64>) -> Result<StatusCode, AppError> {
    if scans::delete(&state.pool, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound("Scan tidak ditemukan".into()))
    }
}
