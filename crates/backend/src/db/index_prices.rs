use chrono::NaiveDate;
use sqlx::PgPool;

use crate::db::prices::PriceBar;

/// Simbol Yahoo untuk IHSG.
pub const IHSG: &str = "^JKSE";

pub async fn upsert(pool: &PgPool, symbol: &str, bars: &[PriceBar]) -> sqlx::Result<u64> {
    if bars.is_empty() {
        return Ok(0);
    }
    let r = sqlx::query(
        "INSERT INTO index_prices (symbol, date, close)
         SELECT $1, * FROM UNNEST($2::date[], $3::float8[])
         ON CONFLICT (symbol, date) DO UPDATE SET close = EXCLUDED.close",
    )
    .bind(symbol)
    .bind(bars.iter().map(|b| b.date).collect::<Vec<_>>())
    .bind(bars.iter().map(|b| b.close).collect::<Vec<_>>())
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

pub async fn closes(pool: &PgPool, symbol: &str) -> sqlx::Result<Vec<(NaiveDate, f64)>> {
    sqlx::query_as("SELECT date, close FROM index_prices WHERE symbol = $1 ORDER BY date").bind(symbol).fetch_all(pool).await
}

pub async fn last_date(pool: &PgPool, symbol: &str) -> sqlx::Result<Option<NaiveDate>> {
    sqlx::query_scalar("SELECT max(date) FROM index_prices WHERE symbol = $1").bind(symbol).fetch_one(pool).await
}

/// Penutupan terakhir dan penutupan hari bursa sebelumnya.
pub async fn last_two(pool: &PgPool, symbol: &str) -> sqlx::Result<Option<(f64, f64)>> {
    let rows: Vec<f64> =
        sqlx::query_scalar("SELECT close FROM index_prices WHERE symbol = $1 ORDER BY date DESC LIMIT 2")
            .bind(symbol)
            .fetch_all(pool)
            .await?;
    Ok(match rows[..] {
        [last, prev, ..] if prev != 0.0 => Some((last, prev)),
        _ => None,
    })
}

/// Kondisi pasar (IHSG di atas SMA 50) dari data tersimpan.
pub async fn market_regime(pool: &PgPool) -> sqlx::Result<scanner_core::MarketRegime> {
    let closes: Vec<(i32, f64)> =
        closes(pool, IHSG).await?.into_iter().map(|(d, c)| (crate::frame::days_since_epoch(d), c)).collect();
    Ok(scanner_core::MarketRegime::from_closes(&closes))
}
