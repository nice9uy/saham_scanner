use chrono::NaiveDate;
use sqlx::{FromRow, PgPool};

/// Satu bar harian dari sumber data (harga mentah + adj_close).
#[derive(Debug, Clone, PartialEq)]
pub struct PriceBar {
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub adj_close: f64,
    pub volume: i64,
}

#[derive(Debug, Clone, PartialEq, FromRow)]
pub struct PriceRow {
    pub ticker: String,
    pub date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub adj_close: f64,
    pub volume: i64,
}

const COLUMNS: &str = "ticker, date, open, high, low, close, adj_close, volume";

pub async fn upsert_bars(pool: &PgPool, ticker: &str, bars: &[PriceBar]) -> sqlx::Result<u64> {
    if bars.is_empty() {
        return Ok(0);
    }
    let col = |f: fn(&PriceBar) -> f64| bars.iter().map(f).collect::<Vec<f64>>();
    let result = sqlx::query(
        "INSERT INTO daily_prices (ticker, date, open, high, low, close, adj_close, volume)
         SELECT $1, * FROM UNNEST($2::date[], $3::float8[], $4::float8[], $5::float8[], $6::float8[], $7::float8[], $8::int8[])
         ON CONFLICT (ticker, date) DO UPDATE SET
            open = EXCLUDED.open, high = EXCLUDED.high, low = EXCLUDED.low,
            close = EXCLUDED.close, adj_close = EXCLUDED.adj_close, volume = EXCLUDED.volume",
    )
    .bind(ticker)
    .bind(bars.iter().map(|b| b.date).collect::<Vec<_>>())
    .bind(col(|b| b.open))
    .bind(col(|b| b.high))
    .bind(col(|b| b.low))
    .bind(col(|b| b.close))
    .bind(col(|b| b.adj_close))
    .bind(bars.iter().map(|b| b.volume).collect::<Vec<_>>())
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn history(pool: &PgPool, ticker: &str) -> sqlx::Result<Vec<PriceRow>> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM daily_prices WHERE ticker = $1 ORDER BY date"))
        .bind(ticker)
        .fetch_all(pool)
        .await
}

/// `n` bar terakhir untuk setiap emiten aktif, terurut per (ticker, date).
pub async fn latest_bars(pool: &PgPool, n: usize) -> sqlx::Result<Vec<PriceRow>> {
    sqlx::query_as(&format!(
        "SELECT {COLUMNS} FROM (
            SELECT p.*, row_number() OVER (PARTITION BY p.ticker ORDER BY p.date DESC) AS rn
            FROM daily_prices p JOIN stocks s ON s.ticker = p.ticker
            WHERE s.is_active
         ) t WHERE rn <= $1 ORDER BY ticker, date"
    ))
    .bind(n as i64)
    .fetch_all(pool)
    .await
}

pub async fn is_empty(pool: &PgPool) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT NOT EXISTS (SELECT 1 FROM daily_prices)").fetch_one(pool).await
}

/// Semua ticker yang punya data harga, termasuk emiten nonaktif (untuk backtest tanpa bias
/// hanya saham yang bertahan).
pub async fn tickers_with_prices(pool: &PgPool) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT DISTINCT ticker FROM daily_prices ORDER BY ticker").fetch_all(pool).await
}

/// Seluruh riwayat beberapa ticker, terurut per (ticker, date).
pub async fn history_many(pool: &PgPool, tickers: &[String]) -> sqlx::Result<Vec<PriceRow>> {
    sqlx::query_as(&format!("SELECT {COLUMNS} FROM daily_prices WHERE ticker = ANY($1) ORDER BY ticker, date"))
        .bind(tickers)
        .fetch_all(pool)
        .await
}
