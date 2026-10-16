mod common;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::*;
use serde_json::json;
use shared::*;
use sqlx::PgPool;

fn close_gt_zero() -> serde_json::Value {
    json!({"conditions": [{"left": {"source": {"indicator": {"kind": "close"}}},
                           "cmp": {"op": "gt"},
                           "right": {"source": {"constant": 0.0}}}]})
}

async fn app_with_data(pool: PgPool) -> TestApp {
    seed_stock(&pool, "AAAA", "Alpha Tbk", &rising_bars(30, 100.0)).await;
    seed_stock(&pool, "BBBB", "Beta Tbk", &rising_bars(30, 200.0)).await;
    let state = state(pool, FakeSource::default());
    state.cache.reload(&state.pool, 300).await.unwrap();
    TestApp::new(state)
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn scan_503_when_cache_empty(pool: PgPool) {
    let app = TestApp::new(state(pool, FakeSource::default()));
    let (status, err): (_, ApiError) = app.json(Method::POST, "/api/scan", Some(close_gt_zero())).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(err.error, "unavailable");
    let (status, _) = app.request(Method::GET, "/api/market/summary", None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn scan_returns_results_after_cache_reload(pool: PgPool) {
    let app = app_with_data(pool).await;
    let (status, result): (_, ScanResultDto) = app.json(Method::POST, "/api/scan", Some(close_gt_zero())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result.total_matched, 2);
    assert_eq!(result.rows[0].ticker, "BBBB", "default urut nilai transaksi terbesar");
    assert_eq!(result.rows[0].name, "Beta Tbk");

    let (status, presets): (_, Vec<PresetDto>) = app.json(Method::GET, "/api/presets", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(presets.len(), 14);
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn scan_invalid_rules_400(pool: PgPool) {
    let app = app_with_data(pool).await;
    let (status, err): (_, ApiError) = app.json(Method::POST, "/api/scan", Some(json!({"conditions": []}))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(err.message.contains("Minimal"));
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn stocks_search_detail_and_candles(pool: PgPool) {
    let app = app_with_data(pool).await;
    let (_, list): (_, Vec<StockDto>) = app.json(Method::GET, "/api/stocks?q=alp", None).await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].ticker, "AAAA");

    let (status, detail): (_, StockDetailDto) = app.json(Method::GET, "/api/stocks/aaaa", None).await;
    assert_eq!(status, StatusCode::OK);
    let stats = detail.stats.unwrap();
    assert_eq!(stats.close, 129.0);
    assert!(stats.change_1m.is_some());
    assert!(detail.signal.is_some(), "detail saham menyertakan rating sinyal");
    let risk = detail.risk.expect("detail saham menyertakan peringatan risiko");
    assert!(risk.stop_loss.is_some() && risk.atr_14.is_some());

    let (status, _) = app.request(Method::GET, "/api/stocks/ZZZZ", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, candles): (_, CandlesDto) =
        app.json(Method::GET, "/api/stocks/AAAA/candles?range=3m&indicators=sma:5,bb:20:20,macd:12:26:9", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(candles.candles.len(), 30);
    assert_eq!(candles.series.len(), 7);
    assert!(candles.series.iter().all(|s| s.values.len() == 30));
    assert_eq!(candles.series[0].values[29], Some(127.0));
    assert_eq!(candles.candles[29].date, "2024-01-30");

    let (_, flow): (_, CandlesDto) =
        app.json(Method::GET, "/api/stocks/AAAA/candles?indicators=obv,ad,cmf:20,mfi:14,rvol:20", None).await;
    assert_eq!(flow.series.len(), 5);

    let (status, _) = app.request(Method::GET, "/api/stocks/AAAA/candles?indicators=foo:1", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = app.request(Method::GET, "/api/stocks/AAAA/candles?range=2y", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn watchlist_crud_and_scan_filter(pool: PgPool) {
    let app = app_with_data(pool).await;
    let (status, wl): (_, WatchlistDto) =
        app.json(Method::POST, "/api/watchlists", Some(json!({"name": "Favorit"}))).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = app.request(Method::POST, &format!("/api/watchlists/{}/items/aaaa", wl.id), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app.request(Method::POST, &format!("/api/watchlists/{}/items/ZZZZ", wl.id), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, lists): (_, Vec<WatchlistDto>) = app.json(Method::GET, "/api/watchlists", None).await;
    assert_eq!(lists[0].items.len(), 1);
    assert_eq!(lists[0].items[0].close, Some(129.0));
    assert!(lists[0].items[0].change_pct.is_some());

    let (status, _) = app.request(Method::POST, "/api/watchlists", Some(json!({"name": "Favorit"}))).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let mut req = close_gt_zero();
    req["universe"] = json!({"watchlist_id": wl.id});
    let (_, result): (_, ScanResultDto) = app.json(Method::POST, "/api/scan", Some(req)).await;
    assert_eq!(result.rows.iter().map(|r| r.ticker.as_str()).collect::<Vec<_>>(), ["AAAA"]);

    let (status, _) = app.request(Method::DELETE, &format!("/api/watchlists/{}/items/AAAA", wl.id), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app.request(Method::DELETE, &format!("/api/watchlists/{}", wl.id), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = app.request(Method::POST, &format!("/api/watchlists/{}/items/AAAA", wl.id), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let mut req = close_gt_zero();
    req["universe"] = json!({"watchlist_id": wl.id});
    let (status, _) = app.request(Method::POST, "/api/scan", Some(req)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn saved_scans_crud(pool: PgPool) {
    let app = app_with_data(pool).await;
    let body = json!({"name": "Close positif", "rules": close_gt_zero()});
    let (status, saved): (_, SavedScanDto) = app.json(Method::POST, "/api/scans", Some(body.clone())).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = app.request(Method::POST, "/api/scans", Some(body)).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (_, list): (_, Vec<SavedScanDto>) = app.json(Method::GET, "/api/scans", None).await;
    assert_eq!(list.len(), 1);

    let updated = json!({"name": "Diubah", "rules": close_gt_zero()});
    let (status, s): (_, SavedScanDto) = app.json(Method::PUT, &format!("/api/scans/{}", saved.id), Some(updated.clone())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(s.name, "Diubah");
    let (status, _) = app.request(Method::PUT, "/api/scans/9999", Some(updated)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let invalid = json!({"name": "Rusak", "rules": {"conditions": []}});
    let (status, _) = app.request(Method::POST, "/api/scans", Some(invalid)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, _) = app.request(Method::DELETE, &format!("/api/scans/{}", saved.id), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn market_summary_and_ingest_status(pool: PgPool) {
    let app = app_with_data(pool).await;
    let (status, summary): (_, MarketSummaryDto) = app.json(Method::GET, "/api/market/summary", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(summary.advancers, 2);
    assert_eq!(summary.as_of, "2024-01-30");

    let (status, ingest): (_, IngestStatusDto) = app.json(Method::GET, "/api/ingest/status", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(ingest.current.is_none());
    assert!(ingest.history.is_empty());

    let (status, _) = app.request(Method::GET, "/api/tidak-ada", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn import_csv(pool: PgPool) {
    let app = TestApp::new(state(pool, FakeSource::default()));
    let boundary = "XBOUNDARY";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"saham.csv\"\r\nContent-Type: text/csv\r\n\r\nKode,Nama Perusahaan\nBBCA,Bank Central Asia Tbk\nTLKM,Telkom Indonesia Tbk\n\r\n--{boundary}--\r\n"
    );
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/stocks/import")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap();
    let (status, bytes) = app.send(req).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&bytes));
    let result: ImportResultDto = serde_json::from_slice(&bytes).unwrap();
    assert_eq!((result.inserted, result.updated), (2, 0));
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn backtest_needs_history_then_reports_all_levels(pool: PgPool) {
    // 30 bar: terlalu pendek untuk backtest (butuh > 200 bar per saham).
    let app = app_with_data(pool.clone()).await;
    let (status, _) = app.request(Method::GET, "/api/backtest", None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);

    seed_stock(&pool, "LONG", "Long Tbk", &rising_bars(260, 2000.0)).await;
    let (status, dto): (_, BacktestDto) = app.json(Method::GET, "/api/backtest?liquid=false", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(dto.tickers, 1);
    assert!(!dto.liquid_only);
    assert_eq!(dto.rows.len(), 7, "6 rating + pembanding");
    assert!(dto.rows.iter().any(|r| r.level.is_none() && r.stats[0].count > 0));
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn brief_summarizes_market(pool: PgPool) {
    let app = TestApp::new(state(pool.clone(), FakeSource::default()));
    let (status, _) = app.request(Method::GET, "/api/brief", None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "belum ada data");

    let app = app_with_data(pool).await;
    let (status, dto): (_, BriefDto) = app.json(Method::GET, "/api/brief", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(dto.headline.contains("Best Buy"), "{}", dto.headline);
    assert_eq!(dto.sections.first().unwrap().title, "Kondisi pasar");
    assert_eq!(dto.sections.last().unwrap().title, "Catatan");
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn fundamentals_feed_quality_detail_and_scan_filter(pool: PgPool) {
    use scanner_core::quality::Fundamentals;
    let app = app_with_data(pool.clone()).await;
    let f = |eps, bv| Fundamentals { eps: Some(eps), book_value: Some(bv), dividend_yield: None, market_cap: None };
    let rows = vec![("AAAA".to_string(), f(20.0, 100.0)), ("BBBB".to_string(), f(-1.0, 50.0)), ("NOPE".to_string(), f(1.0, 1.0))];
    assert_eq!(backend::db::fundamentals::upsert_many(&pool, &rows).await.unwrap(), 2, "emiten tak dikenal dilewati");
    app.state.cache.reload(&pool, 300).await.unwrap();

    let (_, d): (_, StockDetailDto) = app.json(Method::GET, "/api/stocks/AAAA", None).await;
    let q = d.quality.unwrap();
    // ROE 20% (+2) dan PER sama dengan median pasar (+1).
    assert_eq!((q.level, q.score), (Some(QualityLevel::Healthy), 3));
    let (_, d): (_, StockDetailDto) = app.json(Method::GET, "/api/stocks/BBBB", None).await;
    assert_eq!(d.quality.unwrap().level, Some(QualityLevel::Poor));

    let (status, r): (_, ScanResultDto) =
        app.json(Method::POST, "/api/scan", Some(json!({"conditions": [], "universe": {"quality": ["healthy"]}}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(r.rows.iter().map(|r| r.ticker.as_str()).collect::<Vec<_>>(), ["AAAA"]);
    assert_eq!(r.rows[0].quality, Some(QualityLevel::Healthy));
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn server_status_reports_machine_and_database(pool: PgPool) {
    let app = app_with_data(pool).await;
    let (status, s): (_, ServerStatusDto) = app.json(Method::GET, "/api/server", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(s.cpu.cores >= 1);
    assert!(s.memory.total > 0 && s.memory.app_rss > 0);
    assert!(s.disks.first().is_some_and(|d| d.total > 0));
    assert!(s.database.size > 0 && s.database.tables.iter().any(|t| t.name == "daily_prices"));
    assert_eq!((s.cache.stocks, s.cache.bars), (2, 60));
}

#[sqlx::test(migrator = "backend::MIGRATOR")]
async fn win_rates_are_stored_and_filterable(pool: PgPool) {
    let app = app_with_data(pool.clone()).await;
    let rows = std::collections::HashMap::from([
        ("AAAA".to_string(), WinRateDto { signals: 10, wins: 7, avg_return: 2.5 }),
        ("BBBB".to_string(), WinRateDto { signals: 3, wins: 3, avg_return: 5.0 }),
    ]);
    assert_eq!(backend::db::win_rates::replace_all(&pool, &rows).await.unwrap(), 2);
    app.state.cache.reload(&pool, 300).await.unwrap();

    let (_, d): (_, StockDetailDto) = app.json(Method::GET, "/api/stocks/AAAA", None).await;
    assert_eq!(d.win_rate.map(|w| (w.signals, w.wins)), Some((10, 7)));
    // BBBB 3/3 = 100%, tapi kurang dari 5 sinyal: tidak lolos filter.
    let (status, r): (_, ScanResultDto) =
        app.json(Method::POST, "/api/scan", Some(json!({"conditions": [], "universe": {"min_win_rate": 60.0}}))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(r.rows.iter().map(|r| r.ticker.as_str()).collect::<Vec<_>>(), ["AAAA"]);

    // Perhitungan dari riwayat harga (30 bar: terlalu pendek untuk sinyal) mengosongkan tabel.
    backend::ingest::refresh_win_rates(&pool).await.unwrap();
    assert!(backend::db::win_rates::load(&pool).await.unwrap().is_empty());
}
