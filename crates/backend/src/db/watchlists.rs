use sqlx::PgPool;

pub struct WatchlistRow {
    pub id: i64,
    pub name: String,
    /// `(ticker, nama emiten)` sesuai urutan ditambahkan.
    pub items: Vec<(String, String)>,
}

pub async fn list(pool: &PgPool) -> sqlx::Result<Vec<WatchlistRow>> {
    let lists: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, name FROM watchlists ORDER BY id").fetch_all(pool).await?;
    let items: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT i.watchlist_id, i.ticker, s.name FROM watchlist_items i
         JOIN stocks s ON s.ticker = i.ticker ORDER BY i.added_at, i.ticker",
    )
    .fetch_all(pool)
    .await?;
    Ok(lists
        .into_iter()
        .map(|(id, name)| WatchlistRow {
            id,
            name,
            items: items.iter().filter(|i| i.0 == id).map(|i| (i.1.clone(), i.2.clone())).collect(),
        })
        .collect())
}

pub async fn create(pool: &PgPool, name: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar("INSERT INTO watchlists (name) VALUES ($1) RETURNING id").bind(name).fetch_one(pool).await
}

pub async fn delete(pool: &PgPool, id: i64) -> sqlx::Result<bool> {
    let r = sqlx::query("DELETE FROM watchlists WHERE id = $1").bind(id).execute(pool).await?;
    Ok(r.rows_affected() > 0)
}

pub async fn exists(pool: &PgPool, id: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM watchlists WHERE id = $1)").bind(id).fetch_one(pool).await
}

pub async fn add_item(pool: &PgPool, id: i64, ticker: &str) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO watchlist_items (watchlist_id, ticker) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(id)
        .bind(ticker)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn remove_item(pool: &PgPool, id: i64, ticker: &str) -> sqlx::Result<bool> {
    let r = sqlx::query("DELETE FROM watchlist_items WHERE watchlist_id = $1 AND ticker = $2")
        .bind(id)
        .bind(ticker)
        .execute(pool)
        .await?;
    Ok(r.rows_affected() > 0)
}

/// `None` kalau watchlist tidak ada.
pub async fn tickers(pool: &PgPool, id: i64) -> sqlx::Result<Option<Vec<String>>> {
    if !exists(pool, id).await? {
        return Ok(None);
    }
    let t = sqlx::query_scalar("SELECT ticker FROM watchlist_items WHERE watchlist_id = $1")
        .bind(id)
        .fetch_all(pool)
        .await?;
    Ok(Some(t))
}

/// Semua ticker yang ada di watchlist mana pun, tanpa duplikat.
pub async fn all_tickers(pool: &PgPool) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar("SELECT DISTINCT ticker FROM watchlist_items ORDER BY ticker").fetch_all(pool).await
}
