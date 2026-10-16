mod common;

use std::sync::Arc;
use std::time::Duration;

use backend::db::{ingest_runs, prices, stocks};
use backend::ingest::{self, FetchError, Fetched, IngestHandle, RunOptions};
use common::*;
use sqlx::PgPool;

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn stocks_upsert_and_search(pool: PgPool) {
    let recs = [
        stocks::StockRecord::new("BBCA", "Bank Central Asia Tbk", Some("Keuangan"), None).unwrap(),
        stocks::StockRecord::new("BBRI", "Bank Rakyat Indonesia Tbk", None, None).unwrap(),
        stocks::StockRecord::new("TLKM", "", None, None).unwrap(),
    ];
    let r = stocks::upsert_many(&pool, &recs).await.unwrap();
    assert_eq!((r.inserted, r.updated), (3, 0));

    let again = stocks::StockRecord::new("BBCA", "", Some("Perbankan"), Some("Utama")).unwrap();
    let r = stocks::upsert_many(&pool, &[again]).await.unwrap();
    assert_eq!((r.inserted, r.updated), (0, 1));
    let bbca = stocks::get(&pool, "bbca").await.unwrap().unwrap();
    assert_eq!(bbca.name, "Bank Central Asia Tbk", "nama tidak ditimpa oleh nama kosong");
    assert_eq!(bbca.sector.as_deref(), Some("Perbankan"));

    let found: Vec<String> = stocks::list(&pool, Some("bb"), 10).await.unwrap().into_iter().map(|s| s.ticker).collect();
    assert_eq!(found, ["BBCA", "BBRI"]);
    let by_name: Vec<String> = stocks::list(&pool, Some("rakyat"), 10).await.unwrap().into_iter().map(|s| s.ticker).collect();
    assert_eq!(by_name, ["BBRI"]);

    stocks::fill_name_if_missing(&pool, "TLKM", "Telkom Indonesia").await.unwrap();
    stocks::fill_name_if_missing(&pool, "BBCA", "Nama Lain").await.unwrap();
    assert_eq!(stocks::get(&pool, "TLKM").await.unwrap().unwrap().name, "Telkom Indonesia");
    assert_eq!(stocks::get(&pool, "BBCA").await.unwrap().unwrap().name, "Bank Central Asia Tbk");
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn prices_upsert_is_idempotent_and_latest_limits(pool: PgPool) {
    seed_stock(&pool, "AAAA", "A", &rising_bars(10, 100.0)).await;
    seed_stock(&pool, "BBBB", "B", &rising_bars(5, 50.0)).await;
    prices::upsert_bars(&pool, "AAAA", &rising_bars(10, 100.0)).await.unwrap();
    assert_eq!(prices::history(&pool, "AAAA").await.unwrap().len(), 10);

    let latest = prices::latest_bars(&pool, 3).await.unwrap();
    assert_eq!(latest.len(), 6);
    let a: Vec<f64> = latest.iter().filter(|r| r.ticker == "AAAA").map(|r| r.close).collect();
    assert_eq!(a, [107.0, 108.0, 109.0]);
    assert!(!prices::is_empty(&pool).await.unwrap());
}

fn ok(name: &str) -> Result<Fetched, FetchError> {
    Ok(Fetched { name: Some(name.into()), bars: rising_bars(30, 1000.0) })
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn ingest_run_with_fake_source(pool: PgPool) {
    for t in ["AAAA", "BBBB", "XXXX"] {
        stocks::upsert_many(&pool, &[stocks::StockRecord::new(t, "", None, None).unwrap()]).await.unwrap();
    }
    let source = FakeSource::default()
        .with("AAAA", vec![ok("Alpha Tbk")])
        .with("BBBB", vec![Err(FetchError::Transient("timeout".into())), ok("Beta Tbk")]);
    let handle = IngestHandle::default();
    let guard = handle.try_begin().unwrap();
    assert!(handle.try_begin().is_none(), "tidak boleh ada dua update bersamaan");

    let opts = RunOptions { concurrency: 2, backfill_years: 1, today: day(40), retry_delays: vec![Duration::ZERO; 3] };
    let run = ingest::run(&pool, Arc::new(source), &guard, "manual", &opts).await.unwrap();
    assert_eq!(run.status, "partial");
    assert_eq!(run.ok_count, 2);
    assert_eq!(run.failed_tickers, ["XXXX"]);
    assert!(run.finished_at.is_some());
    assert_eq!(handle.progress().unwrap().done, 3);

    assert_eq!(prices::history(&pool, "BBBB").await.unwrap().len(), 30);
    assert_eq!(stocks::get(&pool, "AAAA").await.unwrap().unwrap().name, "Alpha Tbk");
    assert_eq!(ingest_runs::recent(&pool, 5).await.unwrap().len(), 1);

    drop(guard);
    assert!(handle.progress().is_none());
    assert!(handle.try_begin().is_some());
}

fn rec(ticker: &str, name: &str, sector: Option<&str>) -> stocks::StockRecord {
    stocks::StockRecord::new(ticker, name, sector, None).unwrap()
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn sync_listing_fills_without_overwriting(pool: PgPool) {
    stocks::upsert_many(&pool, &[rec("BBCA", "Bank Central Asia Tbk", Some("Keuangan")), rec("TLKM", "", None)])
        .await
        .unwrap();

    // Daftar pendek: tambah dan isi yang kosong, tapi jangan nonaktifkan yang tidak tercantum.
    let short = [rec("BBCA", "Nama Lain", Some("Finance")), rec("AADI", "PT Adaro Andalan", Some("Energi"))];
    let r = stocks::sync_listing(&pool, &short).await.unwrap();
    assert_eq!((r.inserted, r.deactivated), (1, 0));
    let bbca = stocks::get(&pool, "BBCA").await.unwrap().unwrap();
    assert_eq!((bbca.name.as_str(), bbca.sector.as_deref()), ("Bank Central Asia Tbk", Some("Keuangan")));
    assert!(stocks::get(&pool, "TLKM").await.unwrap().unwrap().is_active);

    // Daftar lengkap (≥ MIN_FULL_LISTING): emiten yang tidak tercantum dinonaktifkan.
    let mut full: Vec<stocks::StockRecord> =
        (0..stocks::MIN_FULL_LISTING).map(|i| rec(&format!("X{i:03}"), "", None)).collect();
    full.push(rec("BBCA", "", None));
    full.push(rec("AADI", "", None));
    let r = stocks::sync_listing(&pool, &full).await.unwrap();
    assert_eq!(r.deactivated, 1);
    assert!(!stocks::get(&pool, "TLKM").await.unwrap().unwrap().is_active);

    // Muncul lagi di daftar → aktif lagi, namanya terisi.
    let r = stocks::sync_listing(&pool, &[rec("TLKM", "PT Telkom Indonesia Tbk", Some("Infrastruktur"))]).await.unwrap();
    assert_eq!(r.reactivated, 1);
    let tlkm = stocks::get(&pool, "TLKM").await.unwrap().unwrap();
    assert!(tlkm.is_active);
    assert_eq!(tlkm.name, "PT Telkom Indonesia Tbk");
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn ingest_downloads_newly_listed_stocks(pool: PgPool) {
    let source = FakeSource::default()
        .with_listing(vec![rec("NEWW", "PT Emiten Baru Tbk", Some("Teknologi"))])
        .with("NEWW", vec![ok("PT Emiten Baru Tbk")]);
    let handle = IngestHandle::default();
    let guard = handle.try_begin().unwrap();
    let opts = RunOptions { concurrency: 1, backfill_years: 1, today: day(40), retry_delays: vec![] };
    let run = ingest::run(&pool, Arc::new(source), &guard, "manual", &opts).await.unwrap();
    assert_eq!((run.status.as_str(), run.ok_count), ("success", 1));
    assert_eq!(prices::history(&pool, "NEWW").await.unwrap().len(), 30);
}
