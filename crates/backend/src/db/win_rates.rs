use std::collections::HashMap;

use shared::WinRateDto;
use sqlx::PgPool;

/// Mengganti seluruh isi tabel dengan hasil perhitungan terbaru.
pub async fn replace_all(pool: &PgPool, rows: &HashMap<String, WinRateDto>) -> sqlx::Result<u64> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM win_rates").execute(&mut *tx).await?;
    let (tickers, rest): (Vec<String>, Vec<WinRateDto>) = rows.iter().map(|(t, w)| (t.clone(), *w)).unzip();
    let r = sqlx::query(
        "INSERT INTO win_rates (ticker, signals, wins, avg_return)
         SELECT u.ticker, u.signals, u.wins, u.avg_return
         FROM UNNEST($1::text[], $2::int4[], $3::int4[], $4::float8[]) AS u(ticker, signals, wins, avg_return)
         JOIN stocks s ON s.ticker = u.ticker",
    )
    .bind(&tickers)
    .bind(rest.iter().map(|w| w.signals as i32).collect::<Vec<_>>())
    .bind(rest.iter().map(|w| w.wins as i32).collect::<Vec<_>>())
    .bind(rest.iter().map(|w| w.avg_return).collect::<Vec<_>>())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(r.rows_affected())
}

pub async fn load(pool: &PgPool) -> sqlx::Result<HashMap<String, WinRateDto>> {
    let rows: Vec<(String, i32, i32, f64)> =
        sqlx::query_as("SELECT ticker, signals, wins, avg_return FROM win_rates").fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(t, s, w, a)| (t, WinRateDto { signals: s.max(0) as u32, wins: w.max(0) as u32, avg_return: a }))
        .collect())
}

pub async fn is_empty(pool: &PgPool) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT NOT EXISTS (SELECT 1 FROM win_rates)").fetch_one(pool).await
}
