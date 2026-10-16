#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use backend::config::Config;
use backend::db::prices::{self, PriceBar};
use backend::db::stocks::{self, StockRecord};
use backend::ingest::{FetchError, Fetched, PriceSource};
use backend::{AppState, build_app};
use chrono::{Days, NaiveDate};
use http_body_util::BodyExt;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::PgPool;
use tower::ServiceExt;

/// Sumber harga palsu: setiap ticker punya antrean respons; antrean habis → NotFound.
#[derive(Default)]
pub struct FakeSource {
    responses: Mutex<HashMap<String, VecDeque<Result<Fetched, FetchError>>>>,
    listing: Vec<StockRecord>,
}

impl FakeSource {
    pub fn with(self, ticker: &str, responses: Vec<Result<Fetched, FetchError>>) -> Self {
        self.responses.lock().unwrap().insert(ticker.to_string(), responses.into());
        self
    }

    pub fn with_listing(mut self, listing: Vec<StockRecord>) -> Self {
        self.listing = listing;
        self
    }
}

#[async_trait]
impl PriceSource for FakeSource {
    async fn fetch(&self, ticker: &str, _from: NaiveDate, _to: NaiveDate) -> Result<Fetched, FetchError> {
        self.responses
            .lock()
            .unwrap()
            .get_mut(ticker)
            .and_then(VecDeque::pop_front)
            .unwrap_or(Err(FetchError::NotFound))
    }

    async fn listing(&self) -> Result<Vec<StockRecord>, FetchError> {
        Ok(self.listing.clone())
    }
}

pub fn day(i: u64) -> NaiveDate {
    NaiveDate::from_ymd_opt(2024, 1, 1).unwrap().checked_add_days(Days::new(i)).unwrap()
}

/// `n` bar naik dari `start`.
pub fn rising_bars(n: u64, start: f64) -> Vec<PriceBar> {
    (0..n)
        .map(|i| {
            let c = start + i as f64;
            PriceBar { date: day(i), open: c, high: c + 1.0, low: c - 1.0, close: c, adj_close: c, volume: 1_000_000 }
        })
        .collect()
}

pub async fn seed_stock(pool: &PgPool, ticker: &str, name: &str, bars: &[PriceBar]) {
    let rec = StockRecord::new(ticker, name, Some("Keuangan"), Some("Utama")).unwrap();
    stocks::upsert_many(pool, &[rec]).await.unwrap();
    prices::upsert_bars(pool, ticker, bars).await.unwrap();
}

pub fn state(pool: PgPool, source: FakeSource) -> AppState {
    AppState::new(pool, Arc::new(source), Config::for_tests(""))
}

pub struct TestApp {
    pub state: AppState,
    router: Router,
}

impl TestApp {
    pub fn new(state: AppState) -> Self {
        Self { router: build_app(state.clone()), state }
    }

    pub async fn request(&self, method: Method, uri: &str, body: Option<serde_json::Value>) -> (StatusCode, Vec<u8>) {
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(json) => {
                req = req.header(header::CONTENT_TYPE, "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };
        let resp = self.router.clone().oneshot(req.body(body).unwrap()).await.unwrap();
        let status = resp.status();
        (status, resp.into_body().collect().await.unwrap().to_bytes().to_vec())
    }

    pub async fn json<T: DeserializeOwned>(&self, method: Method, uri: &str, body: Option<serde_json::Value>) -> (StatusCode, T) {
        let (status, bytes) = self.request(method, uri, body).await;
        let parsed = serde_json::from_slice(&bytes)
            .unwrap_or_else(|e| panic!("{uri}: {e}: {}", String::from_utf8_lossy(&bytes)));
        (status, parsed)
    }

    pub async fn send(&self, req: Request<Body>) -> (StatusCode, Vec<u8>) {
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        (status, resp.into_body().collect().await.unwrap().to_bytes().to_vec())
    }
}

pub fn to_json<T: Serialize>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).unwrap()
}
