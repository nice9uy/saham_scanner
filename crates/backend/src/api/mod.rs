pub mod backtest;
mod brief;
pub mod chart;
mod ingest;
mod market;
mod scan;
mod scans;
mod server;
mod stocks;
mod watchlists;

use axum::Router;
use axum::routing::{get, post, put};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::AppState;
use crate::error::AppError;

async fn api_not_found() -> AppError {
    AppError::NotFound("Endpoint tidak ditemukan".into())
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/stocks", get(stocks::list))
        .route("/stocks/import", post(stocks::import))
        .route("/stocks/{ticker}", get(stocks::detail))
        .route("/stocks/{ticker}/candles", get(stocks::candles))
        .route("/scan", post(scan::run))
        .route("/presets", get(scan::presets))
        .route("/scans", get(scans::list).post(scans::create))
        .route("/scans/{id}", put(scans::update).delete(scans::delete))
        .route("/watchlists", get(watchlists::list).post(watchlists::create))
        .route("/watchlists/{id}", axum::routing::delete(watchlists::delete))
        .route("/watchlists/{id}/items/{ticker}", post(watchlists::add).delete(watchlists::remove))
        .route("/market/summary", get(market::summary))
        .route("/backtest", get(backtest::run))
        .route("/brief", get(brief::get))
        .route("/server", get(server::get))
        .route("/ingest/run", post(ingest::run))
        .route("/ingest/status", get(ingest::status))
        .fallback(api_not_found);

    let mut app = Router::new().nest("/api", api);
    if let Some(dir) = &state.config.static_dir {
        // Rute frontend (mis. /scanner) jatuh ke index.html supaya router Leptos yang menangani.
        app = app.fallback_service(ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html"))));
    }
    app.layer(CorsLayer::permissive()).layer(TraceLayer::new_for_http()).with_state(state)
}
