use shared::{DatabaseDto, TableDto};
use sqlx::PgPool;

/// Ukuran database, jumlah koneksi, dan ukuran tiap tabel (terbesar dulu).
pub async fn database(pool: &PgPool) -> sqlx::Result<DatabaseDto> {
    let (version, size, connections): (String, i64, i64) = sqlx::query_as(
        "SELECT current_setting('server_version'), pg_database_size(current_database()),
                (SELECT count(*) FROM pg_stat_activity WHERE datname = current_database())",
    )
    .fetch_one(pool)
    .await?;
    let tables: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT relname::text, n_live_tup, pg_total_relation_size(relid)
         FROM pg_stat_user_tables ORDER BY pg_total_relation_size(relid) DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(DatabaseDto {
        version,
        size: size.max(0) as u64,
        connections,
        tables: tables.into_iter().map(|(name, rows, size)| TableDto { name, rows, size: size.max(0) as u64 }).collect(),
    })
}
