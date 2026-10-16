use std::collections::HashMap;
use std::io::Read;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use scanner_core::StockMeta;
use shared::{ImportResultDto, StockDto};
use sqlx::{FromRow, PgPool};

/// Satu baris impor emiten. Nama kosong diganti kode saham (diisi dari Yahoo saat ingest).
#[derive(Debug, Clone, PartialEq)]
pub struct StockRecord {
    pub ticker: String,
    pub name: String,
    pub sector: Option<String>,
    pub board: Option<String>,
}

impl StockRecord {
    pub fn new(ticker: &str, name: &str, sector: Option<&str>, board: Option<&str>) -> Option<Self> {
        let ticker = ticker.trim().to_uppercase().trim_end_matches(".JK").to_string();
        if ticker.is_empty() || !ticker.chars().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
        let clean = |v: Option<&str>| v.map(str::trim).filter(|v| !v.is_empty()).map(String::from);
        let name = name.trim();
        Some(Self {
            name: if name.is_empty() { ticker.clone() } else { name.to_string() },
            ticker,
            sector: clean(sector),
            board: clean(board),
        })
    }
}

#[derive(FromRow)]
struct StockRow {
    ticker: String,
    name: String,
    sector: Option<String>,
    board: Option<String>,
    is_active: bool,
}

impl From<StockRow> for StockDto {
    fn from(r: StockRow) -> Self {
        Self { ticker: r.ticker, name: r.name, sector: r.sector, board: r.board, is_active: r.is_active }
    }
}

/// Membaca CSV emiten. Header yang dikenali (tidak peka huruf besar/kecil):
/// `ticker|kode|code`, `name|nama|nama perusahaan`, `sector|sektor`, `board|papan|papan pencatatan`.
pub fn parse_csv(input: impl Read) -> Result<Vec<StockRecord>> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).trim(csv::Trim::All).from_reader(input);
    let headers: Vec<String> = reader.headers()?.iter().map(|h| h.to_lowercase()).collect();
    let find = |names: &[&str]| headers.iter().position(|h| names.contains(&h.as_str()));
    let Some(ticker_idx) = find(&["ticker", "kode", "code", "kode saham"]) else {
        bail!("CSV harus punya kolom ticker/kode");
    };
    let name_idx = find(&["name", "nama", "nama perusahaan"]);
    let sector_idx = find(&["sector", "sektor"]);
    let board_idx = find(&["board", "papan", "papan pencatatan"]);

    let mut out = Vec::new();
    for (line, rec) in reader.records().enumerate() {
        let rec = rec.with_context(|| format!("baris {} tidak valid", line + 2))?;
        let get = |idx: Option<usize>| idx.and_then(|i| rec.get(i));
        if let Some(r) = StockRecord::new(rec.get(ticker_idx).unwrap_or(""), get(name_idx).unwrap_or(""), get(sector_idx), get(board_idx)) {
            out.push(r);
        }
    }
    Ok(out)
}

pub async fn upsert_many(pool: &PgPool, records: &[StockRecord]) -> sqlx::Result<ImportResultDto> {
    let mut tx = pool.begin().await?;
    let (mut inserted, mut updated) = (0, 0);
    for r in records {
        let was_insert: bool = sqlx::query_scalar(
            "INSERT INTO stocks (ticker, name, sector, board) VALUES ($1, $2, $3, $4)
             ON CONFLICT (ticker) DO UPDATE SET
                name = CASE WHEN EXCLUDED.name = EXCLUDED.ticker THEN stocks.name ELSE EXCLUDED.name END,
                sector = COALESCE(EXCLUDED.sector, stocks.sector),
                board = COALESCE(EXCLUDED.board, stocks.board),
                is_active = TRUE,
                updated_at = now()
             RETURNING (xmax = 0)",
        )
        .bind(&r.ticker)
        .bind(&r.name)
        .bind(&r.sector)
        .bind(&r.board)
        .fetch_one(&mut *tx)
        .await?;
        if was_insert { inserted += 1 } else { updated += 1 }
    }
    tx.commit().await?;
    Ok(ImportResultDto { inserted, updated })
}

/// Hasil sinkronisasi daftar emiten dari sumber otomatis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncResult {
    pub inserted: u64,
    pub reactivated: u64,
    pub deactivated: u64,
}

/// Jumlah minimum emiten dalam daftar sumber sebelum emiten yang tidak tercantum dinonaktifkan.
/// Melindungi dari daftar parsial saat sumber sedang bermasalah.
pub const MIN_FULL_LISTING: usize = 500;

/// Menyamakan tabel `stocks` dengan daftar emiten dari sumber otomatis.
/// Emiten baru ditambahkan; nama dan sektor yang sudah ada tidak ditimpa (hanya yang kosong
/// diisi); emiten yang tidak tercantum dinonaktifkan kalau daftarnya lengkap.
pub async fn sync_listing(pool: &PgPool, records: &[StockRecord]) -> sqlx::Result<SyncResult> {
    let mut tx = pool.begin().await?;
    let mut result = SyncResult::default();
    for r in records {
        let (inserted, reactivated): (bool, bool) = sqlx::query_as(
            "WITH prev AS (SELECT is_active FROM stocks WHERE ticker = $1)
             INSERT INTO stocks (ticker, name, sector, board) VALUES ($1, $2, $3, $4)
             ON CONFLICT (ticker) DO UPDATE SET
                name = CASE WHEN stocks.name = stocks.ticker THEN EXCLUDED.name ELSE stocks.name END,
                sector = COALESCE(stocks.sector, EXCLUDED.sector),
                board = COALESCE(stocks.board, EXCLUDED.board),
                is_active = TRUE,
                updated_at = now()
             RETURNING (xmax = 0), COALESCE((SELECT NOT is_active FROM prev), FALSE)",
        )
        .bind(&r.ticker)
        .bind(&r.name)
        .bind(&r.sector)
        .bind(&r.board)
        .fetch_one(&mut *tx)
        .await?;
        result.inserted += u64::from(inserted);
        result.reactivated += u64::from(reactivated);
    }
    if records.len() >= MIN_FULL_LISTING {
        let tickers: Vec<&str> = records.iter().map(|r| r.ticker.as_str()).collect();
        result.deactivated = sqlx::query(
            "UPDATE stocks SET is_active = FALSE, updated_at = now()
             WHERE is_active AND NOT (ticker = ANY($1))",
        )
        .bind(&tickers)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    }
    tx.commit().await?;
    Ok(result)
}

/// Cari berdasarkan awalan kode atau potongan nama (tidak peka huruf besar/kecil).
pub async fn list(pool: &PgPool, q: Option<&str>, limit: i64) -> sqlx::Result<Vec<StockDto>> {
    let q = q.map(|q| q.trim().replace(['%', '_'], "")).filter(|q| !q.is_empty());
    let rows: Vec<StockRow> = sqlx::query_as(
        "SELECT ticker, name, sector, board, is_active FROM stocks
         WHERE $1::text IS NULL OR ticker ILIKE $1 || '%' OR name ILIKE '%' || $1 || '%'
         ORDER BY (ticker ILIKE COALESCE($1, '') || '%') DESC, ticker
         LIMIT $2",
    )
    .bind(q)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn get(pool: &PgPool, ticker: &str) -> sqlx::Result<Option<StockDto>> {
    let row: Option<StockRow> =
        sqlx::query_as("SELECT ticker, name, sector, board, is_active FROM stocks WHERE ticker = $1")
            .bind(ticker.to_uppercase())
            .fetch_optional(pool)
            .await?;
    Ok(row.map(Into::into))
}

pub async fn active_tickers_with_last_date(pool: &PgPool) -> sqlx::Result<Vec<(String, Option<NaiveDate>)>> {
    sqlx::query_as(
        "SELECT s.ticker, max(p.date) FROM stocks s
         LEFT JOIN daily_prices p ON p.ticker = s.ticker
         WHERE s.is_active GROUP BY s.ticker ORDER BY s.ticker",
    )
    .fetch_all(pool)
    .await
}

/// Mengisi nama hanya kalau nama masih sama dengan kode (hasil seed tanpa nama).
pub async fn fill_name_if_missing(pool: &PgPool, ticker: &str, name: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE stocks SET name = $2, updated_at = now() WHERE ticker = $1 AND name = ticker")
        .bind(ticker)
        .bind(name.trim())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn meta_map(pool: &PgPool) -> sqlx::Result<HashMap<String, StockMeta>> {
    let rows: Vec<StockRow> =
        sqlx::query_as("SELECT ticker, name, sector, board, is_active FROM stocks").fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.ticker, StockMeta { name: r.name, sector: r.sector, board: r.board }))
        .collect())
}

pub async fn count(pool: &PgPool) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM stocks").fetch_one(pool).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_csv_with_aliases() {
        let csv = "Kode,Nama Perusahaan,Papan Pencatatan\nbbca.jk,Bank Central Asia Tbk,Utama\n,kosong,\nTLKM,,Utama\n";
        let rows = parse_csv(csv.as_bytes()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], StockRecord {
            ticker: "BBCA".into(),
            name: "Bank Central Asia Tbk".into(),
            sector: None,
            board: Some("Utama".into()),
        });
        assert_eq!(rows[1].name, "TLKM");
        assert!(parse_csv("foo,bar\n1,2\n".as_bytes()).is_err());
    }
}
