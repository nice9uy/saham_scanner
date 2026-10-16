use chrono::{DateTime, Utc};
use shared::{SavedScanDto, ScanRequest};
use sqlx::types::Json;
use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
struct ScanRow {
    id: i64,
    name: String,
    rules: Json<ScanRequest>,
    updated_at: DateTime<Utc>,
}

impl From<ScanRow> for SavedScanDto {
    fn from(r: ScanRow) -> Self {
        Self { id: r.id, name: r.name, rules: r.rules.0, updated_at: r.updated_at.to_rfc3339() }
    }
}

pub async fn list(pool: &PgPool) -> sqlx::Result<Vec<SavedScanDto>> {
    let rows: Vec<ScanRow> =
        sqlx::query_as("SELECT id, name, rules, updated_at FROM saved_scans ORDER BY updated_at DESC, id DESC")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn create(pool: &PgPool, name: &str, rules: &ScanRequest) -> sqlx::Result<SavedScanDto> {
    let row: ScanRow =
        sqlx::query_as("INSERT INTO saved_scans (name, rules) VALUES ($1, $2) RETURNING id, name, rules, updated_at")
            .bind(name)
            .bind(Json(rules))
            .fetch_one(pool)
            .await?;
    Ok(row.into())
}

pub async fn update(pool: &PgPool, id: i64, name: &str, rules: &ScanRequest) -> sqlx::Result<Option<SavedScanDto>> {
    let row: Option<ScanRow> = sqlx::query_as(
        "UPDATE saved_scans SET name = $2, rules = $3, updated_at = now() WHERE id = $1
         RETURNING id, name, rules, updated_at",
    )
    .bind(id)
    .bind(name)
    .bind(Json(rules))
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

pub async fn delete(pool: &PgPool, id: i64) -> sqlx::Result<bool> {
    let r = sqlx::query("DELETE FROM saved_scans WHERE id = $1").bind(id).execute(pool).await?;
    Ok(r.rows_affected() > 0)
}
