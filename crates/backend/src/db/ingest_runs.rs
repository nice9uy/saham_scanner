use chrono::{DateTime, Utc};
use shared::IngestRunDto;
use sqlx::{FromRow, PgPool};

#[derive(FromRow)]
struct RunRow {
    id: i64,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    status: String,
    trigger: String,
    ok_count: i32,
    failed_tickers: Vec<String>,
    error: Option<String>,
}

impl From<RunRow> for IngestRunDto {
    fn from(r: RunRow) -> Self {
        Self {
            id: r.id,
            started_at: r.started_at.to_rfc3339(),
            finished_at: r.finished_at.map(|t| t.to_rfc3339()),
            status: r.status,
            trigger: r.trigger,
            ok_count: r.ok_count,
            failed_tickers: r.failed_tickers,
            error: r.error,
        }
    }
}

const COLUMNS: &str = "id, started_at, finished_at, status, trigger, ok_count, failed_tickers, error";

pub async fn start(pool: &PgPool, trigger: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar("INSERT INTO ingest_runs (status, trigger) VALUES ('running', $1) RETURNING id")
        .bind(trigger)
        .fetch_one(pool)
        .await
}

pub async fn finish(
    pool: &PgPool,
    id: i64,
    status: &str,
    ok_count: i32,
    failed: &[String],
    error: Option<&str>,
) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE ingest_runs SET finished_at = now(), status = $2, ok_count = $3, failed_tickers = $4, error = $5
         WHERE id = $1",
    )
    .bind(id)
    .bind(status)
    .bind(ok_count)
    .bind(failed)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get(pool: &PgPool, id: i64) -> sqlx::Result<IngestRunDto> {
    let row: RunRow = sqlx::query_as(&format!("SELECT {COLUMNS} FROM ingest_runs WHERE id = $1"))
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(row.into())
}

pub async fn recent(pool: &PgPool, n: i64) -> sqlx::Result<Vec<IngestRunDto>> {
    let rows: Vec<RunRow> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM ingest_runs ORDER BY id DESC LIMIT $1"))
        .bind(n)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

/// Run yang masih `running` saat server mati ditandai gagal ketika startup.
pub async fn fail_abandoned(pool: &PgPool) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE ingest_runs SET status = 'failed', finished_at = now(), error = 'Server berhenti saat update berjalan'
         WHERE status = 'running'",
    )
    .execute(pool)
    .await?;
    Ok(())
}
