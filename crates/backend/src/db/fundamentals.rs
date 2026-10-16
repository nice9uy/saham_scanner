use std::collections::HashMap;

use scanner_core::quality::Fundamentals;
use sqlx::PgPool;

pub async fn upsert_many(pool: &PgPool, rows: &[(String, Fundamentals)]) -> sqlx::Result<u64> {
    if rows.is_empty() {
        return Ok(0);
    }
    let col = |f: fn(&Fundamentals) -> Option<f64>| rows.iter().map(|(_, x)| f(x)).collect::<Vec<_>>();
    let r = sqlx::query(
        "INSERT INTO fundamentals (ticker, eps, book_value, dividend_yield, market_cap)
         SELECT u.ticker, u.eps, u.book_value, u.dividend_yield, u.market_cap
         FROM UNNEST($1::text[], $2::float8[], $3::float8[], $4::float8[], $5::float8[])
              AS u(ticker, eps, book_value, dividend_yield, market_cap)
         JOIN stocks s ON s.ticker = u.ticker
         ON CONFLICT (ticker) DO UPDATE SET
            eps = EXCLUDED.eps, book_value = EXCLUDED.book_value, dividend_yield = EXCLUDED.dividend_yield,
            market_cap = EXCLUDED.market_cap, updated_at = now()",
    )
    .bind(rows.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>())
    .bind(col(|f| f.eps))
    .bind(col(|f| f.book_value))
    .bind(col(|f| f.dividend_yield))
    .bind(col(|f| f.market_cap))
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

/// `(ticker, eps, book_value, dividend_yield, market_cap)`
type Row = (String, Option<f64>, Option<f64>, Option<f64>, Option<f64>);

pub async fn load(pool: &PgPool) -> sqlx::Result<HashMap<String, Fundamentals>> {
    let rows: Vec<Row> =
        sqlx::query_as("SELECT ticker, eps, book_value, dividend_yield, market_cap FROM fundamentals")
            .fetch_all(pool)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(t, eps, book_value, dividend_yield, market_cap)| {
            (t, Fundamentals { eps, book_value, dividend_yield, market_cap })
        })
        .collect())
}

pub async fn is_empty(pool: &PgPool) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT NOT EXISTS (SELECT 1 FROM fundamentals)").fetch_one(pool).await
}
