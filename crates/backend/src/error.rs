use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use scanner_core::CoreError;
use shared::ApiError;

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    NotFound(String),
    Conflict(String),
    Unavailable(String),
    Internal(anyhow::Error),
}

pub type ApiResult<T> = Result<Json<T>, AppError>;

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m),
            AppError::Conflict(m) => (StatusCode::CONFLICT, "conflict", m),
            AppError::Unavailable(m) => (StatusCode::SERVICE_UNAVAILABLE, "unavailable", m),
            AppError::Internal(e) => {
                tracing::error!("{e:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", "Terjadi kesalahan di server".to_string())
            }
        };
        (status, Json(ApiError { error: code.into(), message })).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &e
            && db.is_unique_violation()
        {
            return AppError::Conflict("Nama sudah dipakai".into());
        }
        AppError::Internal(e.into())
    }
}

impl From<CoreError> for AppError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::Invalid(m) => AppError::BadRequest(m),
            other => AppError::Internal(other.into()),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e)
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(e: tokio::task::JoinError) -> Self {
        AppError::Internal(e.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn body(resp: Response) -> (StatusCode, ApiError) {
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn maps_status_and_body() {
        let (status, err) = body(AppError::NotFound("Saham tidak ditemukan".into()).into_response()).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(err, ApiError { error: "not_found".into(), message: "Saham tidak ditemukan".into() });

        let (status, err) = body(AppError::Internal(anyhow::anyhow!("rahasia")).into_response()).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.error, "internal");
        assert!(!err.message.contains("rahasia"));
    }
}
